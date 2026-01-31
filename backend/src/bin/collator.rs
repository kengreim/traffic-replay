#![warn(clippy::all, clippy::pedantic, clippy::nursery)]

use anyhow::{Context, bail};
use aws_sdk_s3::primitives::ByteStream;
use chrono::{DateTime, Utc};
use figment::Figment;
use figment::providers::{Format, Toml};
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
    Airport, CAPTURE_RANGE_NM, EVENT_POST_TIME_MINUTES, EVENT_PRE_TIME_MINUTES, EventCapture,
    EventConfig, calculate_centroid, event_slug, filter_pilots_by_distance_and_field,
    load_airports, pilots_to_feature_collection,
};

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

    let mut airports: Vec<(String, Option<&Airport>)> = vec![];
    for icao_id in &event_config.airports {
        if let Some(airport) = all_airports.get(icao_id) {
            airports.push((icao_id.clone(), Some(airport)));
        } else {
            warn!(id = ?icao_id, "invalid airport ICAO id, not found in FAA data");
            airports.push((icao_id.clone(), None));
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

    let mut all_snapshots = HashMap::new();
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
        let collection = pilots_to_feature_collection(filtered_pilots);

        let key = update_timestamp.to_rfc3339();

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

        all_snapshots.insert(key, collection);
    }

    info!(total_rows = row_count, "Finished fetching all datafeed rows");

    let centroid = calculate_centroid(&airports);
    let captures_string = serde_json::to_string(&all_snapshots)?;
    let captures_len = captures_string.len();

    let capture = EventCapture {
        config: event_config.clone(),
        first_timestamp_key: min_key,
        last_timestamp_key: max_key,
        captures: all_snapshots,
        captures_length_bytes: captures_len,
        viewport_center: centroid,
    };

    let slug = event_slug(&event_config);
    let output_dir = format!("./{slug}");
    fs::create_dir_all(&output_dir)?;

    let json_bytes = serde_json::to_vec(&capture)?;
    let output_file = format!("{output_dir}/{slug}.json");
    let mut file = fs::File::create(&output_file)?;
    file.write_all(&json_bytes)?;

    info!(file = %output_file, "Completed collation, output written locally");

    // Upload to R2 if configured
    if let Ok(r2_config) = R2Config::from_env() {
        upload_to_r2(&r2_config, &slug, json_bytes).await?;
        fs::remove_dir_all(&output_dir)?;
        info!(dir = %output_dir, "Cleaned up local output directory");
    } else {
        info!("R2 not configured, skipping upload");
    }

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

async fn upload_to_r2(config: &R2Config, slug: &str, data: Vec<u8>) -> Result<(), anyhow::Error> {
    let sdk_config = aws_config::from_env()
        .endpoint_url(&config.endpoint_url)
        .region(aws_config::Region::new("auto"))
        .load()
        .await;

    let client = aws_sdk_s3::Client::new(&sdk_config);
    let key = format!("{slug}.json");

    info!(bucket = %config.bucket, key = %key, "Uploading to R2");

    client
        .put_object()
        .bucket(&config.bucket)
        .key(&key)
        .body(ByteStream::from(data))
        .content_type("application/json")
        .send()
        .await
        .context("failed to upload to R2")?;

    info!(bucket = %config.bucket, key = %key, "Upload complete");
    Ok(())
}
