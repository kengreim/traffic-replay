//! Queries against the `events` table.

use chrono::{DateTime, Utc};
use sqlx::types::Json;
use sqlx::{FromRow, PgPool, Postgres, Transaction};
use std::collections::HashMap;
use traffic_replay::collate::collation_window;
use traffic_replay::{CustomAirport, DATAFEED_RETENTION_DAYS, EVENT_POST_TIME_MINUTES, EventConfig};

use crate::discovery::{DiscoveredEvent, Proposal};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Status {
    Proposed,
    Approved,
    Collating,
    Published,
    Failed,
    Skipped,
}

impl Status {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Proposed => "proposed",
            Self::Approved => "approved",
            Self::Collating => "collating",
            Self::Published => "published",
            Self::Failed => "failed",
            Self::Skipped => "skipped",
        }
    }

    fn parse(s: &str) -> Self {
        match s {
            "approved" => Self::Approved,
            "collating" => Self::Collating,
            "published" => Self::Published,
            "failed" => Self::Failed,
            "skipped" => Self::Skipped,
            _ => Self::Proposed,
        }
    }
}

#[derive(FromRow, Debug, Clone)]
pub struct EventRow {
    pub id: i64,
    pub vatusa_id: Option<i64>,
    pub vatsim_id: Option<i64>,
    pub vatusa_source: Option<serde_json::Value>,
    pub vatsim_source: Option<serde_json::Value>,
    pub proposal: Option<Json<Proposal>>,
    pub name: String,
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
    pub airports: Vec<String>,
    pub artccs: Vec<String>,
    pub custom_airports: Json<HashMap<String, CustomAirport>>,
    pub airport_source: String,
    pub unlisted: bool,
    pub edited: bool,
    status: String,
    pub error: Option<String>,
    pub published_url: Option<String>,
    pub approved_at: Option<DateTime<Utc>>,
    pub collated_at: Option<DateTime<Utc>>,
    pub last_seen_at: Option<DateTime<Utc>>,
}

impl EventRow {
    pub fn status(&self) -> Status {
        Status::parse(&self.status)
    }

    fn is_untouched(&self) -> bool {
        self.status() == Status::Proposed && !self.edited
    }

    pub fn config(&self) -> EventConfig {
        EventConfig {
            name: self.name.clone(),
            artccs: self.artccs.clone(),
            airports: self.airports.clone(),
            custom_airports: self.custom_airports.0.clone(),
            advertised_start_time: self.start_time,
            advertised_end_time: self.end_time,
        }
    }

    /// When collation becomes due: the end of the event plus the post buffer.
    pub fn collation_due_at(&self) -> DateTime<Utc> {
        collation_window(&self.config()).1
    }

    pub fn has_ended(&self, now: DateTime<Utc>) -> bool {
        self.collation_due_at() <= now
    }

    /// When the oldest datafeed needed for collation will be pruned.
    pub fn data_expires_at(&self) -> DateTime<Utc> {
        collation_window(&self.config()).0 + chrono::Duration::days(DATAFEED_RETENTION_DAYS.into())
    }

    /// Reasons this event can't be approved or collated, if any.
    pub fn validate(&self, now: DateTime<Utc>) -> Result<(), String> {
        if self.name.trim().is_empty() {
            return Err("Name is required.".to_owned());
        }
        if self.end_time <= self.start_time {
            return Err("End time must be after start time.".to_owned());
        }
        if self.airports.is_empty() {
            return Err("At least one airport is required.".to_owned());
        }
        if self.data_expires_at() <= now {
            return Err(format!(
                "Datafeeds for this event were pruned after {DATAFEED_RETENTION_DAYS} days, so it can no longer be collated."
            ));
        }
        Ok(())
    }
}

pub async fn get_event(pool: &PgPool, id: i64) -> Result<Option<EventRow>, sqlx::Error> {
    sqlx::query_as("SELECT * FROM events WHERE id = $1")
        .bind(id)
        .fetch_optional(pool)
        .await
}

/// Events for the overview: recent and upcoming, plus anything still in progress.
pub async fn list_events(pool: &PgPool) -> Result<Vec<EventRow>, sqlx::Error> {
    sqlx::query_as(
        "SELECT * FROM events
         WHERE start_time >= NOW() - INTERVAL '30 days'
            OR status IN ('approved', 'collating', 'failed')
         ORDER BY start_time, id",
    )
    .fetch_all(pool)
    .await
}

async fn find_by(
    tx: &mut Transaction<'_, Postgres>,
    column: &str,
    id: i64,
) -> Result<Option<EventRow>, sqlx::Error> {
    sqlx::query_as(&format!("SELECT * FROM events WHERE {column} = $1 FOR UPDATE"))
        .bind(id)
        .fetch_optional(&mut **tx)
        .await
}

/// Record a polled event: refresh its sources and proposal, and while it is
/// untouched, its config too.
pub async fn record_discovered(pool: &PgPool, found: &DiscoveredEvent) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;
    let vatusa = found.vatusa.as_ref().map(|s| (s.event.id, &s.raw));
    let mut vatsim = found.vatsim.as_ref().map(|s| (s.event.id, &s.raw));

    // A VATSIM event can already have its own row, from before its VATUSA
    // counterpart was found or from an earlier, different match
    if let (Some((vatusa_id, _)), Some((vatsim_id, vatsim_raw))) = (vatusa, vatsim)
        && let Some(row) = find_by(&mut tx, "vatsim_id", vatsim_id).await?
        && row.vatusa_id != Some(vatusa_id)
    {
        if row.vatusa_id.is_none() && row.is_untouched() {
            sqlx::query("DELETE FROM events WHERE id = $1")
                .bind(row.id)
                .execute(&mut *tx)
                .await?;
        } else {
            sqlx::query("UPDATE events SET vatsim_source = $2, last_seen_at = NOW() WHERE id = $1")
                .bind(row.id)
                .bind(vatsim_raw)
                .execute(&mut *tx)
                .await?;
            vatsim = None;
        }
    }

    let existing = match (vatusa, found.vatsim.as_ref()) {
        (Some((id, _)), _) => find_by(&mut tx, "vatusa_id", id).await?,
        (None, Some(s)) => find_by(&mut tx, "vatsim_id", s.event.id).await?,
        (None, None) => None,
    };
    if found.vatusa.is_none() && existing.as_ref().is_some_and(|r| r.vatusa_id.is_some()) {
        // Linked to a VATUSA event that wasn't matched this poll; leave it be
        tx.commit().await?;
        return Ok(());
    }

    let p = &found.proposal;
    if let Some(row) = existing {
        sqlx::query(
            "UPDATE events SET
                vatusa_id = COALESCE($2, vatusa_id),
                vatsim_id = COALESCE($3, vatsim_id),
                vatusa_source = COALESCE($4, vatusa_source),
                vatsim_source = COALESCE($5, vatsim_source),
                proposal = $6,
                last_seen_at = NOW()
             WHERE id = $1",
        )
        .bind(row.id)
        .bind(vatusa.map(|v| v.0))
        .bind(vatsim.map(|v| v.0))
        .bind(vatusa.map(|v| v.1))
        .bind(vatsim.map(|v| v.1))
        .bind(Json(p))
        .execute(&mut *tx)
        .await?;

        if row.is_untouched() && row.proposal.as_ref().is_none_or(|old| &old.0 != p) {
            apply_proposal(&mut tx, row.id, p).await?;
        }
    } else {
        sqlx::query(
            "INSERT INTO events
                (vatusa_id, vatsim_id, vatusa_source, vatsim_source, proposal,
                 name, start_time, end_time, airports, artccs, airport_source, last_seen_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, NOW())",
        )
        .bind(vatusa.map(|v| v.0))
        .bind(vatsim.map(|v| v.0))
        .bind(vatusa.map(|v| v.1))
        .bind(vatsim.map(|v| v.1))
        .bind(Json(p))
        .bind(&p.name)
        .bind(p.start_time)
        .bind(p.end_time)
        .bind(&p.airports)
        .bind(&p.artccs)
        .bind(&p.airport_source)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await
}

async fn apply_proposal(
    tx: &mut Transaction<'_, Postgres>,
    id: i64,
    p: &Proposal,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE events SET name = $2, start_time = $3, end_time = $4, airports = $5,
            artccs = $6, airport_source = $7, updated_at = NOW()
         WHERE id = $1",
    )
    .bind(id)
    .bind(&p.name)
    .bind(p.start_time)
    .bind(p.end_time)
    .bind(&p.airports)
    .bind(&p.artccs)
    .bind(&p.airport_source)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// Fields editable from the UI.
pub struct EventEdit {
    pub name: String,
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
    pub airports: Vec<String>,
    pub artccs: Vec<String>,
    pub custom_airports: HashMap<String, CustomAirport>,
    pub unlisted: bool,
}

pub async fn create_event(pool: &PgPool, edit: &EventEdit) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar(
        "INSERT INTO events
            (name, start_time, end_time, airports, artccs, custom_airports, unlisted,
             airport_source, edited)
         VALUES ($1, $2, $3, $4, $5, $6, $7, 'manual', TRUE)
         RETURNING id",
    )
    .bind(&edit.name)
    .bind(edit.start_time)
    .bind(edit.end_time)
    .bind(&edit.airports)
    .bind(&edit.artccs)
    .bind(Json(&edit.custom_airports))
    .bind(edit.unlisted)
    .fetch_one(pool)
    .await
}

/// Save edits, unless the event is mid-collation. Returns whether it saved.
pub async fn update_event(pool: &PgPool, id: i64, edit: &EventEdit) -> Result<bool, sqlx::Error> {
    let result = sqlx::query(
        "UPDATE events SET name = $2, start_time = $3, end_time = $4, artccs = $6,
            custom_airports = $7, unlisted = $8, edited = TRUE, updated_at = NOW(),
            airport_source = CASE WHEN airports = $5 THEN airport_source ELSE 'manual' END,
            airports = $5
         WHERE id = $1 AND status <> 'collating'",
    )
    .bind(id)
    .bind(&edit.name)
    .bind(edit.start_time)
    .bind(edit.end_time)
    .bind(&edit.airports)
    .bind(&edit.artccs)
    .bind(Json(&edit.custom_airports))
    .bind(edit.unlisted)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

/// Discard edits and go back to the latest proposal. Only for proposed events.
pub async fn reset_to_proposal(pool: &PgPool, row: &EventRow) -> Result<bool, sqlx::Error> {
    let Some(proposal) = &row.proposal else {
        return Ok(false);
    };
    let mut tx = pool.begin().await?;
    let result = sqlx::query(
        "UPDATE events SET edited = FALSE, custom_airports = '{}'
         WHERE id = $1 AND status = 'proposed'",
    )
    .bind(row.id)
    .execute(&mut *tx)
    .await?;
    if result.rows_affected() == 0 {
        return Ok(false);
    }
    apply_proposal(&mut tx, row.id, &proposal.0).await?;
    tx.commit().await?;
    Ok(true)
}

/// Move an event from one of `from` to `to`. Returns false if its status had
/// already changed.
///
/// Approving counts as reviewing the config, so later polls stop overwriting it
/// even if the event is unapproved again.
pub async fn transition(
    pool: &PgPool,
    id: i64,
    from: &[Status],
    to: Status,
) -> Result<bool, sqlx::Error> {
    let from: Vec<&str> = from.iter().map(|s| s.as_str()).collect();
    let result = sqlx::query(
        "UPDATE events SET status = $3, updated_at = NOW(),
            approved_at = CASE WHEN $3 = 'approved' THEN NOW() ELSE approved_at END,
            edited = edited OR $3 = 'approved'
         WHERE id = $1 AND status = ANY($2)",
    )
    .bind(id)
    .bind(&from)
    .bind(to.as_str())
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

/// Claim the next approved event that has ended, marking it as collating.
pub async fn claim_next_due(pool: &PgPool) -> Result<Option<EventRow>, sqlx::Error> {
    sqlx::query_as(
        "UPDATE events SET status = 'collating', error = NULL, updated_at = NOW()
         WHERE id = (
            SELECT id FROM events
            WHERE status = 'approved' AND end_time + make_interval(mins => $1) <= NOW()
            ORDER BY end_time
            LIMIT 1
            FOR UPDATE SKIP LOCKED
         )
         RETURNING *",
    )
    .bind(i32::try_from(EVENT_POST_TIME_MINUTES).unwrap_or(i32::MAX))
    .fetch_optional(pool)
    .await
}

pub async fn mark_published(pool: &PgPool, id: i64, url: &str) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE events SET status = 'published', published_url = $2, error = NULL,
            collated_at = NOW(), updated_at = NOW()
         WHERE id = $1",
    )
    .bind(id)
    .bind(url)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn mark_failed(pool: &PgPool, id: i64, error: &str) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE events SET status = 'failed', error = $2, updated_at = NOW() WHERE id = $1")
        .bind(id)
        .bind(error)
        .execute(pool)
        .await?;
    Ok(())
}

/// Requeue events left mid-collation by a restart.
pub async fn requeue_interrupted(pool: &PgPool) -> Result<u64, sqlx::Error> {
    let result = sqlx::query("UPDATE events SET status = 'approved' WHERE status = 'collating'")
        .execute(pool)
        .await?;
    Ok(result.rows_affected())
}
