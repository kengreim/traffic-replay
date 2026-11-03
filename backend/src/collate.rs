//! Collating raw datafeeds into an event capture, and publishing it to R2.

use anyhow::{Context, bail};
use aws_sdk_s3::primitives::ByteStream;
use chrono::{DateTime, Utc};
use flate2::Compression;
use flate2::write::GzEncoder;
use futures::TryStreamExt;
use serde::Serialize;
use sqlx::PgPool;
use std::collections::HashMap;
use std::io::Write;
use std::time::Duration;
use tracing::{info, warn};
use vatsim_utils::models::Pilot;

use crate::{
    Airport, CAPTURE_RANGE_NM, EVENT_POST_TIME_MINUTES, EVENT_PRE_TIME_MINUTES, EventConfig,
    FlightPlan, IcaoId, OptimizedEventCapture, PUBLIC_DATA_URL, PilotStatic, calculate_centroid,
    event_slug, filter_pilots_by_distance_and_field, pilots_to_optimized_feature_collection,
};

const EVENTS_JSON_KEY: &str = "events.json";

#[derive(Default)]
struct CountingWriter {
    bytes: usize,
}

impl Write for CountingWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.bytes += buf.len();
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

pub struct CollatedEvent {
    pub slug: String,
    pub gzipped: Vec<u8>,
}

/// Window of datafeeds collated for an event, including the pre/post buffers.
#[must_use]
pub fn collation_window(event: &EventConfig) -> (DateTime<Utc>, DateTime<Utc>) {
    (
        event.advertised_start_time - Duration::from_secs(60 * EVENT_PRE_TIME_MINUTES),
        event.advertised_end_time + Duration::from_secs(60 * EVENT_POST_TIME_MINUTES),
    )
}

/// Build the gzipped capture for an event from the datafeeds in the database.
pub async fn collate_event(
    pool: &PgPool,
    airports_db: &HashMap<IcaoId, Airport>,
    event: &EventConfig,
) -> Result<CollatedEvent, anyhow::Error> {
    if event.advertised_end_time <= event.advertised_start_time {
        bail!("advertised end time cannot be before advertised start time");
    }

    let custom_airports = event
        .airports
        .iter()
        .filter(|icao_id| !airports_db.contains_key(*icao_id))
        .filter_map(|icao_id| {
            let custom = event.custom_airports.get(icao_id)?;
            info!(id = ?icao_id, lat = custom.latitude, lon = custom.longitude, "using custom airport coordinates");
            Some(Airport {
                faa_id: None,
                icao_id: icao_id.clone(),
                point: geo::Point::new(custom.longitude, custom.latitude),
                resp_artcc_id: None,
                towered: false,
            })
        })
        .collect::<Vec<_>>();

    let airports = event
        .airports
        .iter()
        .map(|icao_id| {
            let airport = airports_db
                .get(icao_id)
                .or_else(|| custom_airports.iter().find(|a| &a.icao_id == icao_id));
            if airport.is_none() {
                warn!(id = ?icao_id, "airport not found in FAA data or custom airports, using route-based matching only");
            }
            (icao_id.clone(), airport)
        })
        .collect::<Vec<_>>();

    let (query_start, query_end) = collation_window(event);

    info!(
        start = %query_start,
        end = %query_end,
        "Querying datafeeds from database"
    );

    let mut stream = sqlx::query_as::<_, (DateTime<Utc>, String)>(
        "SELECT update_timestamp, pilots FROM datafeeds WHERE update_timestamp BETWEEN $1 AND $2 ORDER BY update_timestamp",
    )
    .bind(query_start)
    .bind(query_end)
    .fetch(pool);

    // Shared lookups for optimized format
    let mut pilots_static: HashMap<String, PilotStatic> = HashMap::new();
    let mut flight_plans: HashMap<String, FlightPlan> = HashMap::new();
    let mut all_frames = HashMap::new();
    let mut min_key: Option<String> = None;
    let mut max_key: Option<String> = None;
    let mut row_count: usize = 0;

    while let Some((update_timestamp, pilots_json)) = stream
        .try_next()
        .await
        .context("failed to fetch datafeed row")?
    {
        row_count += 1;
        if row_count % 100 == 0 {
            info!(rows_fetched = row_count, "Fetch progress");
        }

        let pilots: Vec<Pilot> = match serde_json::from_str(&pilots_json) {
            Ok(p) => p,
            Err(e) => {
                warn!(error = ?e, time = %update_timestamp, "Could not deserialize pilots from JSON");
                continue;
            }
        };

        let filtered_pilots =
            filter_pilots_by_distance_and_field(pilots, &airports, CAPTURE_RANGE_NM);
        let collection = pilots_to_optimized_feature_collection(
            filtered_pilots,
            &mut pilots_static,
            &mut flight_plans,
        );

        let key = update_timestamp.format("%Y%m%d%H%M%S").to_string();

        if min_key
            .as_ref()
            .is_none_or(|min| key.as_str() < min.as_str())
        {
            min_key = Some(key.clone());
        }

        if max_key
            .as_ref()
            .is_none_or(|max| key.as_str() > max.as_str())
        {
            max_key = Some(key.clone());
        }

        all_frames.insert(key, collection);
    }

    if row_count == 0 {
        bail!("no datafeeds found between {query_start} and {query_end}");
    }

    info!(total_rows = row_count, "Finished fetching all datafeed rows");
    info!(
        pilots = pilots_static.len(),
        flight_plans = flight_plans.len(),
        "Deduplicated static data"
    );

    let centroid = calculate_centroid(&airports);
    let mut counter = CountingWriter::default();
    serde_json::to_writer(&mut counter, &all_frames)?;
    let frames_len = counter.bytes;

    let capture = OptimizedEventCapture {
        config: event.clone(),
        first_timestamp_key: min_key,
        last_timestamp_key: max_key,
        pilots: pilots_static,
        flight_plans,
        frames: all_frames,
        captures_length_bytes: frames_len,
        viewport_center: centroid,
    };

    let json_bytes = serde_json::to_vec(&capture)?;

    // Gzip compress the JSON
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(&json_bytes)?;
    let gzipped = encoder.finish()?;

    info!(
        uncompressed = json_bytes.len(),
        compressed = gzipped.len(),
        ratio = format!("{:.1}x", json_bytes.len() as f64 / gzipped.len() as f64),
        "Compression stats"
    );

    Ok(CollatedEvent {
        slug: event_slug(event),
        gzipped,
    })
}

/// Public URL an event's capture is served from.
#[must_use]
pub fn event_url(slug: &str) -> String {
    format!("{PUBLIC_DATA_URL}/{slug}.json")
}

/// An entry in `events.json`, as read by the frontend.
#[derive(Serialize, Debug, Clone)]
pub struct EventsJsonEntry {
    pub event: EventConfig,
    pub url: String,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub unlisted: bool,
}

pub struct R2Config {
    pub endpoint_url: String,
    pub bucket: String,
}

impl R2Config {
    pub fn from_env() -> Result<Self, std::env::VarError> {
        let var = |name| {
            std::env::var(name).and_then(|v| {
                if v.is_empty() {
                    Err(std::env::VarError::NotPresent)
                } else {
                    Ok(v)
                }
            })
        };
        Ok(Self {
            endpoint_url: var("R2_ENDPOINT_URL")?,
            bucket: var("R2_BUCKET")?,
        })
    }

    pub async fn client(&self) -> aws_sdk_s3::Client {
        let sdk_config = aws_config::from_env()
            .endpoint_url(&self.endpoint_url)
            .region(aws_config::Region::new("auto"))
            .load()
            .await;
        aws_sdk_s3::Client::new(&sdk_config)
    }
}

pub async fn upload_to_r2(
    client: &aws_sdk_s3::Client,
    config: &R2Config,
    slug: &str,
    gzipped_data: Vec<u8>,
) -> Result<(), anyhow::Error> {
    let key = format!("{slug}.json");

    info!(bucket = %config.bucket, key = %key, size = gzipped_data.len(), "Uploading gzipped data to R2");

    client
        .put_object()
        .bucket(&config.bucket)
        .key(&key)
        .body(ByteStream::from(gzipped_data))
        .content_type("application/json")
        .content_encoding("gzip")
        .send()
        .await
        .context("failed to upload to R2")?;

    info!(bucket = %config.bucket, key = %key, "Upload complete");
    Ok(())
}

/// Insert or replace an entry in `events.json` on R2.
///
/// Any existing entry with the same URL, or with `previous_url` (an earlier
/// publish of the same event under a different name), is replaced.
pub async fn publish_events_json_entry(
    client: &aws_sdk_s3::Client,
    config: &R2Config,
    entry: &EventsJsonEntry,
    previous_url: Option<&str>,
) -> Result<(), anyhow::Error> {
    let existing = client
        .get_object()
        .bucket(&config.bucket)
        .key(EVENTS_JSON_KEY)
        .send()
        .await
        .map_err(|e| {
            if e.as_service_error().is_some_and(aws_sdk_s3::operation::get_object::GetObjectError::is_no_such_key) {
                anyhow::anyhow!("{EVENTS_JSON_KEY} not found in R2; upload public/events.json to the bucket first")
            } else {
                anyhow::Error::new(e).context("failed to download events.json from R2")
            }
        })?
        .body
        .collect()
        .await
        .context("failed to read events.json from R2")?
        .into_bytes();

    let entries: Vec<serde_json::Value> =
        serde_json::from_slice(&existing).context("events.json in R2 is not a JSON array")?;
    let updated = merge_events_json_entry(entries, entry, previous_url)?;

    client
        .put_object()
        .bucket(&config.bucket)
        .key(EVENTS_JSON_KEY)
        .body(ByteStream::from(serde_json::to_vec_pretty(&updated)?))
        .content_type("application/json")
        .cache_control("max-age=60")
        .send()
        .await
        .context("failed to upload events.json to R2")?;

    info!(url = %entry.url, events = updated.len(), "Updated events.json");
    Ok(())
}

fn merge_events_json_entry(
    mut entries: Vec<serde_json::Value>,
    entry: &EventsJsonEntry,
    previous_url: Option<&str>,
) -> Result<Vec<serde_json::Value>, anyhow::Error> {
    entries.retain(|existing| {
        let url = existing.get("url").and_then(serde_json::Value::as_str);
        url != Some(entry.url.as_str()) && (previous_url.is_none() || url != previous_url)
    });
    entries.push(serde_json::to_value(entry)?);

    // RFC 3339 UTC timestamps sort chronologically as strings
    entries.sort_by(|a, b| {
        let start = |v: &serde_json::Value| {
            v.pointer("/event/advertised_start_time")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned)
        };
        start(a).cmp(&start(b))
    });
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use serde_json::json;

    fn entry(name: &str, start_hour: u32, url: &str) -> EventsJsonEntry {
        EventsJsonEntry {
            event: EventConfig {
                name: name.to_owned(),
                artccs: vec!["ZSE".to_owned()],
                airports: vec!["KSEA".to_owned()],
                custom_airports: HashMap::new(),
                advertised_start_time: Utc.with_ymd_and_hms(2026, 1, 1, start_hour, 0, 0).unwrap(),
                advertised_end_time: Utc.with_ymd_and_hms(2026, 1, 1, start_hour + 1, 0, 0).unwrap(),
            },
            url: url.to_owned(),
            unlisted: false,
        }
    }

    #[test]
    fn merge_replaces_same_and_previous_url_and_sorts() {
        let existing = vec![
            serde_json::to_value(entry("Late", 20, "late")).unwrap(),
            serde_json::to_value(entry("Old name", 10, "old")).unwrap(),
            json!({"event": {"advertised_start_time": "2026-01-01T05:00:00Z"}, "url": "early", "extra": 1}),
        ];

        let merged = merge_events_json_entry(existing, &entry("New name", 10, "new"), Some("old")).unwrap();
        let urls: Vec<_> = merged.iter().map(|e| e["url"].as_str().unwrap()).collect();
        assert_eq!(urls, ["early", "new", "late"]);
        // Unknown fields on other entries are preserved
        assert_eq!(merged[0]["extra"], 1);
        // `unlisted` is only written when set
        assert!(merged[1].get("unlisted").is_none());
    }
}
