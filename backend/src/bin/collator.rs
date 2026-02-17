#![warn(clippy::all, clippy::pedantic, clippy::nursery)]

use anyhow::{Context, bail};
use aws_sdk_s3::primitives::ByteStream;
use chrono::{DateTime, Utc};
use figment::Figment;
use figment::providers::{Format, Toml};
use flate2::Compression;
use flate2::write::GzEncoder;
use futures::TryStreamExt;
use sqlx::PgPool;
use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::time::Duration;
use tracing::{error, info, warn};
use tracing_subscriber::fmt::format::FmtSpan;
use vatsim_utils::models::Pilot;

use traffic_replay::{
    Airport, CAPTURE_RANGE_NM, EVENT_POST_TIME_MINUTES, EVENT_PRE_TIME_MINUTES, FlightPlan,
    OptimizedEventCapture, EventConfig, PilotStatic, calculate_centroid, event_slug,
    filter_pilots_by_distance_and_field, load_airports, pilots_to_optimized_feature_collection,
};

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

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let subscriber = tracing_subscriber::fmt()
        .compact()
        .json()
        .with_max_level(tracing::Level::DEBUG)
        .with_env_filter("collator=debug,traffic_replay=debug")
        .with_span_events(FmtSpan::CLOSE)
        .with_file(true)
        .with_line_number(true)
        .finish();

    tracing::subscriber::set_global_default(subscriber)?;

    dotenvy::dotenv().ok();
    let database_url =
        std::env::var("DATABASE_URL").context("DATABASE_URL must be set in environment or .env")?;

    let pool = PgPool::connect(&database_url)
        .await
        .context("failed to connect to Postgres")?;

    info!("Connected to Postgres");

    let all_airports = match load_airports() {
        Ok(airports) => airports,
        Err(e) => {
            error!(error = ?e, "failed to load airports CSV");
            return Err(e);
        }
    };

    let event_config: EventConfig = match Figment::new().merge(Toml::file("config.toml")).extract()
    {
        Ok(config) => config,
        Err(e) => {
            error!(error = ?e, "failed to load config file");
            return Err(e.into());
        }
    };

    if event_config.advertised_end_time <= event_config.advertised_start_time {
        bail!("advertised end time cannot be before advertised start time");
    }

    let mut custom_airport_storage: Vec<Airport> = vec![];
    let mut airports: Vec<(String, Option<&Airport>)> = vec![];
    for icao_id in &event_config.airports {
        if let Some(airport) = all_airports.get(icao_id) {
            airports.push((icao_id.clone(), Some(airport)));
        } else if let Some(custom) = event_config.custom_airports.get(icao_id) {
            info!(id = ?icao_id, lat = custom.latitude, lon = custom.longitude, "using custom airport coordinates");
            custom_airport_storage.push(Airport {
                faa_id: None,
                icao_id: icao_id.clone(),
                point: geo::Point::new(custom.longitude, custom.latitude),
            });
            airports.push((icao_id.clone(), None));
        } else {
            warn!(id = ?icao_id, "airport not found in FAA data or custom airports, using route-based matching only");
            airports.push((icao_id.clone(), None));
        }
    }
    // Patch in custom airport references now that storage is stable
    for (icao_id, airport_ref) in &mut airports {
        if airport_ref.is_none() {
            if let Some(custom) = custom_airport_storage.iter().find(|a| &a.icao_id == icao_id) {
                *airport_ref = Some(custom);
            }
        }
    }

    let query_start =
        event_config.advertised_start_time - Duration::from_secs(60 * EVENT_PRE_TIME_MINUTES);
    let query_end =
        event_config.advertised_end_time + Duration::from_secs(60 * EVENT_POST_TIME_MINUTES);

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
    .fetch(&pool);

    // Shared lookups for optimized format
    let mut pilots_static: HashMap<String, PilotStatic> = HashMap::new();
    let mut flight_plans: HashMap<String, FlightPlan> = HashMap::new();
    let mut all_frames = HashMap::new();
    let mut min_key: Option<String> = None;
    let mut max_key: Option<String> = None;
    let mut row_count: usize = 0;

    while let Some((update_timestamp, pilots_json)) = stream.try_next().await.context("failed to fetch datafeed row")? {
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
        config: event_config.clone(),
        first_timestamp_key: min_key,
        last_timestamp_key: max_key,
        pilots: pilots_static,
        flight_plans,
        frames: all_frames,
        captures_length_bytes: frames_len,
        viewport_center: centroid,
    };

    let slug = event_slug(&event_config);
    let output_dir = format!("./{slug}");
    fs::create_dir_all(&output_dir)?;

    let json_bytes = serde_json::to_vec(&capture)?;

    // Gzip compress the JSON
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(&json_bytes)?;
    let gzipped_bytes = encoder.finish()?;

    info!(
        uncompressed = json_bytes.len(),
        compressed = gzipped_bytes.len(),
        ratio = format!("{:.1}x", json_bytes.len() as f64 / gzipped_bytes.len() as f64),
        "Compression stats"
    );

    // Write gzipped file locally (with .gz extension for clarity)
    let output_file = format!("{output_dir}/{slug}.json.gz");
    let mut file = fs::File::create(&output_file)?;
    file.write_all(&gzipped_bytes)?;

    info!(file = %output_file, "Completed collation, output written locally");

    // Upload to R2 if configured
    if let Ok(r2_config) = R2Config::from_env() {
        upload_to_r2(&r2_config, &slug, gzipped_bytes).await?;
        fs::remove_dir_all(&output_dir)?;
        info!(dir = %output_dir, "Cleaned up local output directory");
    } else {
        info!("R2 not configured, skipping upload");
    }

    let events_json_entry = serde_json::json!({
        "event": event_config,
        "url": format!("https://data.vatsim-replay.com/{slug}.json"),
    });

    println!("\nevents.json entry:\n{}\n", serde_json::to_string_pretty(&events_json_entry)?);

    Ok(())
}

struct R2Config {
    endpoint_url: String,
    bucket: String,
}

impl R2Config {
    fn from_env() -> Result<Self, std::env::VarError> {
        Ok(Self {
            endpoint_url: std::env::var("R2_ENDPOINT_URL")?,
            bucket: std::env::var("R2_BUCKET")?,
        })
    }
}

async fn upload_to_r2(config: &R2Config, slug: &str, gzipped_data: Vec<u8>) -> Result<(), anyhow::Error> {
    let sdk_config = aws_config::from_env()
        .endpoint_url(&config.endpoint_url)
        .region(aws_config::Region::new("auto"))
        .load()
        .await;

    let client = aws_sdk_s3::Client::new(&sdk_config);
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
