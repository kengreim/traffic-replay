#![warn(clippy::all, clippy::pedantic, clippy::nursery)]

mod db;
mod discovery;
mod web;
mod worker;

use anyhow::Context;
use chrono::{DateTime, Utc};
use reqwest::Client;
use sqlx::PgPool;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::Notify;
use tracing::{info, warn};
use tracing_subscriber::fmt::format::FmtSpan;
use traffic_replay::collate::R2Config;
use traffic_replay::{Airport, IcaoId, load_airports};

const DEFAULT_VATUSA_API_URL: &str = "https://api.vatusa.net/v3";
const DEFAULT_VATSIM_EVENTS_URL: &str = "https://my.vatsim.net/api/v2/events/view/division/USA";
const DEFAULT_BIND_ADDR: &str = "0.0.0.0:8080";

pub struct Config {
    pub vatusa_url: String,
    pub vatsim_url: String,
    pub admin_username: String,
    pub admin_password: String,
    pub bind_addr: String,
}

impl Config {
    fn from_env() -> Result<Self, anyhow::Error> {
        let var_or = |name: &str, default: &str| {
            std::env::var(name)
                .ok()
                .filter(|v| !v.is_empty())
                .unwrap_or_else(|| default.to_owned())
        };
        let admin_password =
            std::env::var("ADMIN_PASSWORD").context("ADMIN_PASSWORD must be set")?;
        anyhow::ensure!(!admin_password.is_empty(), "ADMIN_PASSWORD must not be empty");
        Ok(Self {
            vatusa_url: var_or("VATUSA_API_URL", DEFAULT_VATUSA_API_URL),
            vatsim_url: var_or("VATSIM_EVENTS_URL", DEFAULT_VATSIM_EVENTS_URL),
            admin_username: var_or("ADMIN_USERNAME", "admin"),
            admin_password,
            bind_addr: var_or("SCHEDULER_BIND_ADDR", DEFAULT_BIND_ADDR),
        })
    }
}

pub struct PollStatus {
    pub at: DateTime<Utc>,
    /// Number of events found, or the error.
    pub result: Result<usize, String>,
}

pub struct AppState {
    pub pool: PgPool,
    pub airports: HashMap<IcaoId, Airport>,
    pub config: Config,
    pub r2: Option<R2Config>,
    pub poll_now: Notify,
    pub collate_now: Notify,
    pub last_poll: Mutex<Option<PollStatus>>,
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let subscriber = tracing_subscriber::fmt()
        .compact()
        .json()
        .with_max_level(tracing::Level::DEBUG)
        .with_env_filter("scheduler=debug,traffic_replay=debug")
        .with_span_events(FmtSpan::CLOSE)
        .with_file(true)
        .with_line_number(true)
        .finish();

    tracing::subscriber::set_global_default(subscriber)?;

    dotenvy::dotenv().ok();
    let database_url =
        std::env::var("DATABASE_URL").context("DATABASE_URL must be set in environment or .env")?;
    let config = Config::from_env()?;

    let pool = PgPool::connect(&database_url)
        .await
        .context("failed to connect to Postgres")?;

    info!("Connected to Postgres");

    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .context("failed to run database migrations")?;

    info!("Migrations applied");

    let airports = load_airports()?;
    let r2 = R2Config::from_env().ok();
    if r2.is_none() {
        warn!("R2 not configured; approved events will fail to publish");
    }

    let client = Client::builder()
        .user_agent("traffic-replay-scheduler")
        .timeout(Duration::from_secs(30))
        .build()
        .context("failed to build HTTP client")?;

    let state = Arc::new(AppState {
        pool,
        airports,
        config,
        r2,
        poll_now: Notify::new(),
        collate_now: Notify::new(),
        last_poll: Mutex::new(None),
    });

    tokio::spawn(worker::discovery_loop(state.clone(), client));
    tokio::spawn(worker::collation_loop(state.clone()));

    let listener = tokio::net::TcpListener::bind(&state.config.bind_addr)
        .await
        .with_context(|| format!("failed to bind {}", state.config.bind_addr))?;
    info!(addr = %state.config.bind_addr, "Serving web UI");
    axum::serve(listener, web::router(state)).await?;

    Ok(())
}
