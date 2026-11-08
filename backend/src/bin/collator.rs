#![warn(clippy::all, clippy::pedantic, clippy::nursery)]

use anyhow::Context;
use figment::Figment;
use figment::providers::{Format, Toml};
use sqlx::PgPool;
use std::fs;
use std::io::Write;
use tracing::{error, info};
use tracing_subscriber::fmt::format::FmtSpan;

use traffic_replay::EventConfig;
use traffic_replay::collate::{
    EventsJsonEntry, R2Config, collate_event, event_url, upload_to_r2,
};
use traffic_replay::load_airports;

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

    let collated = collate_event(&pool, &all_airports, &event_config).await?;
    let slug = collated.slug;

    let output_dir = format!("./{slug}");
    fs::create_dir_all(&output_dir)?;

    // Write gzipped file locally (with .gz extension for clarity)
    let output_file = format!("{output_dir}/{slug}.json.gz");
    let mut file = fs::File::create(&output_file)?;
    file.write_all(&collated.gzipped)?;

    info!(file = %output_file, "Completed collation, output written locally");

    // Upload to R2 if configured
    if let Ok(r2_config) = R2Config::from_env() {
        let client = r2_config.client().await;
        upload_to_r2(&client, &r2_config, &slug, collated.gzipped).await?;
        fs::remove_dir_all(&output_dir)?;
        info!(dir = %output_dir, "Cleaned up local output directory");
    } else {
        info!("R2 not configured, skipping upload");
    }

    let events_json_entry = EventsJsonEntry {
        event: event_config,
        url: event_url(&slug),
        parts: vec![],
        unlisted: false,
    };

    println!("\nevents.json entry:\n{}\n", serde_json::to_string_pretty(&events_json_entry)?);

    Ok(())
}
