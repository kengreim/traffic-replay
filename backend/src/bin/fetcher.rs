#![warn(clippy::all, clippy::pedantic, clippy::nursery)]

use anyhow::Context;
use chrono::DateTime;
use rand::seq::IndexedRandom;
use reqwest::Client;
use serde::Deserialize;
use sqlx::PgPool;
use std::cmp::min;
use std::time::{Duration, Instant};
use tokio::time::sleep;
use tracing::{debug, error, info, warn};
use tracing_subscriber::fmt::format::FmtSpan;
use vatsim_utils::models::Pilot;

#[derive(Deserialize)]
struct StatusData {
    data: StatusEndpoints,
}

#[derive(Deserialize)]
struct StatusEndpoints {
    v3: Vec<String>,
}

#[derive(Deserialize)]
struct GeneralData {
    update: String,
    update_timestamp: String,
}

#[derive(Deserialize)]
struct V3ResponseData {
    general: GeneralData,
    pilots: Vec<Pilot>,
}

async fn get_v3_urls(client: &Client) -> Result<Vec<String>, anyhow::Error> {
    let status: StatusData = client
        .get("https://status.vatsim.net/status.json")
        .send()
        .await?
        .json()
        .await?;
    anyhow::ensure!(!status.data.v3.is_empty(), "No V3 URLs returned from status endpoint");
    Ok(status.data.v3)
}

async fn fetch_v3_data(client: &Client, url: &str) -> Result<V3ResponseData, anyhow::Error> {
    let data: V3ResponseData = client
        .get(url)
        .header("Cache-Control", "no-cache")
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    Ok(data)
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let subscriber = tracing_subscriber::fmt()
        .compact()
        .json()
        .with_max_level(tracing::Level::DEBUG)
        .with_env_filter("fetcher=debug,traffic_replay=debug")
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

    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .context("failed to run database migrations")?;

    info!("Migrations applied");

    let client = Client::builder()
        .user_agent("traffic-replay-fetcher")
        .build()
        .context("failed to build HTTP client")?;

    let v3_urls = get_v3_urls(&client)
        .await
        .context("failed to fetch VATSIM status")?;

    info!(mirrors = v3_urls.len(), "VATSIM API initialized");

    let cleanup_pool = pool.clone();
    tokio::spawn(async move {
        cleanup_loop(&cleanup_pool).await;
    });

    fetch_loop(&client, &v3_urls, &pool).await;

    Ok(())
}

async fn fetch_loop(client: &Client, v3_urls: &[String], pool: &PgPool) {
    let mut last_datafeed_update = String::new();
    let mut rng = rand::rng();

    info!("Starting datafeed fetch loop");
    loop {
        let start = Instant::now();

        let url = v3_urls.choose(&mut rng).expect("v3_urls is non-empty");

        let latest_data = match fetch_v3_data(client, url).await {
            Ok(data) => data,
            Err(e) => {
                warn!(error = ?e, "Could not fetch VATSIM data");
                sleep(Duration::from_secs(1)).await;
                continue;
            }
        };

        if latest_data.general.update == last_datafeed_update {
            debug!(time = %latest_data.general.update, "Found duplicate");
            sleep(Duration::from_secs(1)).await;
            continue;
        }

        last_datafeed_update.clone_from(&latest_data.general.update);

        let update_timestamp = match DateTime::parse_from_rfc3339(&latest_data.general.update_timestamp) {
            Ok(ts) => ts.to_utc(),
            Err(e) => {
                warn!(error = ?e, timestamp = latest_data.general.update_timestamp, "Could not parse timestamp");
                continue;
            }
        };

        let pilots_json = match serde_json::to_string(&latest_data.pilots) {
            Ok(v) => v,
            Err(e) => {
                warn!(error = ?e, "Could not serialize pilots to JSON");
                continue;
            }
        };

        let result = sqlx::query(
            "INSERT INTO datafeeds (update_timestamp, pilots) VALUES ($1, $2) ON CONFLICT (update_timestamp) DO NOTHING",
        )
        .bind(update_timestamp)
        .bind(&pilots_json)
        .execute(pool)
        .await;

        match result {
            Ok(r) => {
                if r.rows_affected() > 0 {
                    info!(time = %update_timestamp, pilots = latest_data.pilots.len(), "Inserted new datafeed");
                } else {
                    debug!(time = %update_timestamp, "Datafeed already exists, skipped");
                }
            }
            Err(e) => {
                error!(error = ?e, "Failed to insert datafeed into database");
            }
        }

        let loop_time = start.elapsed();
        if loop_time > Duration::from_secs(4) {
            warn!(?loop_time, "Long loop");
        }
        let sleep_duration = Duration::from_secs(5) - min(Duration::from_secs(4), loop_time);
        debug!(?sleep_duration, "Sleeping");
        sleep(sleep_duration).await;
    }
}

async fn cleanup_loop(pool: &PgPool) {
    info!("Starting cleanup loop");
    loop {
        sleep(Duration::from_secs(3600)).await;

        let result = sqlx::query("DELETE FROM datafeeds WHERE update_timestamp < NOW() - INTERVAL '7 days'")
            .execute(pool)
            .await;

        match result {
            Ok(r) => {
                info!(rows_deleted = r.rows_affected(), "Cleanup: pruned old datafeeds");
            }
            Err(e) => {
                error!(error = ?e, "Cleanup: failed to delete old datafeeds");
            }
        }
    }
}
