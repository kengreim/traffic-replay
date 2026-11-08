//! Background loops: polling the event APIs and collating approved events.

use anyhow::Context;
use chrono::Utc;
use reqwest::Client;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;
use tracing::{error, info, warn};
use traffic_replay::DATAFEED_RETENTION_DAYS;
use traffic_replay::collate::{
    EventsJsonEntry, EventsJsonPart, collate_range, event_url, publish_events_json_entry,
    upload_to_r2,
};

use crate::db::{self, EventRow};
use crate::discovery::{self, AirportIndex};
use crate::{AppState, PollStatus};

const POLL_INTERVAL: Duration = Duration::from_mins(30);
const COLLATION_CHECK_INTERVAL: Duration = Duration::from_secs(60);

pub async fn discovery_loop(state: Arc<AppState>, client: Client) {
    let index = AirportIndex::new(&state.airports);
    info!("Starting event discovery loop");
    loop {
        let result = poll_once(&state, &client, &index).await;
        match &result {
            Ok(count) => info!(events = count, "Polled event APIs"),
            Err(e) => error!(error = ?e, "Failed to poll event APIs"),
        }
        *state.last_poll.lock().expect("last_poll lock poisoned") = Some(PollStatus {
            at: Utc::now(),
            result: result.map_err(|e| format!("{e:#}")),
        });

        tokio::select! {
            () = sleep(POLL_INTERVAL) => {}
            () = state.poll_now.notified() => {}
        }
    }
}

async fn poll_once(
    state: &AppState,
    client: &Client,
    index: &AirportIndex<'_>,
) -> Result<usize, anyhow::Error> {
    // Older events can't be collated, so there's no need to fetch them
    let from = (Utc::now() - chrono::Duration::days(DATAFEED_RETENTION_DAYS.into())).date_naive();
    let found = discovery::poll(
        client,
        &state.config.vatusa_url,
        &state.config.vatsim_url,
        from,
        index,
    )
    .await?;

    for event in &found {
        db::record_discovered(&state.pool, event)
            .await
            .with_context(|| format!("failed to record event '{}'", event.proposal.name))?;
    }
    Ok(found.len())
}

pub async fn collation_loop(state: Arc<AppState>) {
    match db::requeue_interrupted(&state.pool).await {
        Ok(interrupted) => {
            if interrupted.requeued > 0 {
                warn!(events = interrupted.requeued, "Requeued events interrupted mid-collation");
            }
            if interrupted.failed > 0 {
                warn!(events = interrupted.failed, "Failed events interrupted too many times");
            }
        }
        Err(e) => error!(error = ?e, "Failed to requeue interrupted events"),
    }

    info!("Starting collation loop");
    loop {
        loop {
            match db::claim_next_due(&state.pool).await {
                Ok(Some(row)) => collate(&state, &row).await,
                Ok(None) => break,
                Err(e) => {
                    error!(error = ?e, "Failed to claim next event for collation");
                    break;
                }
            }
        }

        tokio::select! {
            () = sleep(COLLATION_CHECK_INTERVAL) => {}
            () = state.collate_now.notified() => {}
        }
    }
}

async fn collate(state: &AppState, row: &EventRow) {
    info!(id = row.id, name = %row.name, "Collating event");
    let result = match collate_and_publish(state, row).await {
        Ok(url) => {
            info!(id = row.id, url = %url, "Published event");
            db::mark_published(&state.pool, row.id, &url).await
        }
        Err(e) => {
            error!(id = row.id, error = ?e, "Failed to collate event");
            db::mark_failed(&state.pool, row.id, &format!("{e:#}")).await
        }
    };
    if let Err(e) = result {
        error!(id = row.id, error = ?e, "Failed to record collation result");
    }
}

async fn collate_and_publish(state: &AppState, row: &EventRow) -> Result<String, anyhow::Error> {
    row.validate(Utc::now()).map_err(anyhow::Error::msg)?;
    let r2 = state
        .r2
        .as_ref()
        .context("R2 is not configured (set R2_ENDPOINT_URL and R2_BUCKET)")?;

    let config = row.config();
    let plans = row.parts();
    let client = r2.client().await;

    // One part at a time, so only one capture is held in memory
    let mut parts = vec![];
    for (i, plan) in plans.iter().enumerate() {
        info!(id = row.id, part = i + 1, of = plans.len(), slug = %plan.slug, "Collating part");
        let gzipped =
            collate_range(&state.pool, &state.airports, &config, plan.query_start, plan.query_end)
                .await
                .with_context(|| format!("part {} of {}", i + 1, plans.len()))?;
        upload_to_r2(&client, r2, &plan.slug, gzipped).await?;
        log_memory(row.id, i + 1);
        parts.push(EventsJsonPart {
            start: plan.start,
            end: plan.end,
            url: event_url(&plan.slug),
        });
    }

    let url = parts.first().context("event has no parts")?.url.clone();
    if parts.len() == 1 {
        parts.clear();
    }
    let entry = EventsJsonEntry {
        event: config,
        url,
        parts,
        unlisted: row.unlisted,
    };
    publish_events_json_entry(&client, r2, &entry, row.published_url.as_deref()).await?;
    Ok(entry.url)
}

/// Log the process's current and peak memory, to spot growth across parts.
fn log_memory(id: i64, part: usize) {
    let Ok(status) = std::fs::read_to_string("/proc/self/status") else {
        return;
    };
    // Lines look like "VmRSS:     123456 kB"
    let field_mb = |name: &str| {
        status
            .lines()
            .find_map(|line| line.strip_prefix(name))
            .and_then(|rest| rest.trim().strip_suffix("kB"))
            .and_then(|kb| kb.trim().parse::<u64>().ok())
            .map(|kb| kb / 1024)
    };
    info!(
        id,
        part,
        rss_mb = field_mb("VmRSS:"),
        peak_rss_mb = field_mb("VmHWM:"),
        "Memory after part"
    );
}
