//! Web UI for reviewing, editing and approving events.

use askama::Template;
use axum::extract::{Form, Path, Query, Request, State};
use axum::http::{HeaderMap, Method, StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use axum::Router;
use base64::Engine;
use base64::prelude::BASE64_STANDARD;
use chrono::{DateTime, NaiveDateTime, Utc};
use serde::Deserialize;
use std::collections::HashMap;
use std::fmt::Write;
use std::sync::Arc;
use tracing::error;
use traffic_replay::{CustomAirport, US_ARTCCS};

use crate::AppState;
use crate::db::{self, EventEdit, EventRow, Status};

/// How long after a poll an upcoming event can go unseen before it's flagged.
const NOT_SEEN_WARNING_HOURS: i64 = 2;
const INPUT_TIME_FORMAT: &str = "%Y-%m-%dT%H:%M";

pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/", get(index))
        .route("/poll", post(poll_now))
        .route("/events/new", get(new_event))
        .route("/events", post(create_event))
        .route("/events/{id}", get(show_event).post(save_event))
        .route("/events/{id}/{action}", post(event_action))
        .layer(middleware::from_fn_with_state(state.clone(), require_auth))
        .with_state(state)
}

// --- Auth ---

async fn require_auth(State(state): State<Arc<AppState>>, req: Request, next: Next) -> Response {
    let authorized = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Basic "))
        .and_then(|v| BASE64_STANDARD.decode(v).ok())
        .and_then(|v| String::from_utf8(v).ok())
        .and_then(|v| {
            let (user, pass) = v.split_once(':')?;
            Some(
                constant_time_eq(user.as_bytes(), state.config.admin_username.as_bytes())
                    & constant_time_eq(pass.as_bytes(), state.config.admin_password.as_bytes()),
            )
        })
        .unwrap_or(false);

    if !authorized {
        return (
            StatusCode::UNAUTHORIZED,
            [(header::WWW_AUTHENTICATE, "Basic realm=\"traffic-replay scheduler\"")],
            "Unauthorized",
        )
            .into_response();
    }

    // Browsers resend basic auth credentials automatically, so reject form
    // posts that come from other sites
    if req.method() != Method::GET && !is_same_origin(req.headers()) {
        return (StatusCode::FORBIDDEN, "Cross-site request rejected").into_response();
    }

    next.run(req).await
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0, |acc, (x, y)| acc | (x ^ y)) == 0
}

fn is_same_origin(headers: &HeaderMap) -> bool {
    let get = |name| headers.get(name).and_then(|v| v.to_str().ok());
    if let Some(site) = get(header::HeaderName::from_static("sec-fetch-site")) {
        return site == "same-origin" || site == "none";
    }
    match (get(header::ORIGIN), get(header::HOST)) {
        (Some(origin), Some(host)) => {
            origin.split_once("://").map(|(_, rest)| rest) == Some(host)
        }
        // Non-browser clients send no Origin
        (None, _) => true,
        (Some(_), None) => false,
    }
}

// --- Errors ---

struct AppError(anyhow::Error);

impl<E: Into<anyhow::Error>> From<E> for AppError {
    fn from(e: E) -> Self {
        Self(e.into())
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        error!(error = ?self.0, "Request failed");
        (StatusCode::INTERNAL_SERVER_ERROR, format!("Something went wrong: {:#}", self.0))
            .into_response()
    }
}

type HandlerResult = Result<Response, AppError>;

fn not_found() -> Response {
    (StatusCode::NOT_FOUND, "Event not found").into_response()
}

fn render(template: &impl Template, status: StatusCode) -> HandlerResult {
    Ok((status, Html(template.render()?)).into_response())
}

// --- Formatting ---

fn format_time(t: DateTime<Utc>) -> String {
    t.format("%a %b %-d %Y, %H:%MZ").to_string()
}

fn format_window(start: DateTime<Utc>, end: DateTime<Utc>) -> String {
    let end_format = if end.date_naive() == start.date_naive() {
        "%H:%MZ"
    } else {
        "%a %b %-d, %H:%MZ"
    };
    format!(
        "{} – {} ({})",
        format_time(start),
        end.format(end_format),
        format_duration(end - start)
    )
}

fn format_duration(d: chrono::Duration) -> String {
    let minutes = d.num_minutes();
    if minutes < 0 {
        return "negative".to_owned();
    }
    let (days, hours, mins) = (minutes / 1440, minutes / 60 % 24, minutes % 60);
    match (days, hours) {
        (0, 0) => format!("{mins}m"),
        (0, _) if mins == 0 => format!("{hours}h"),
        (0, _) => format!("{hours}h {mins}m"),
        _ => format!("{days}d {hours}h"),
    }
}

fn notice_text(code: &str) -> Option<&'static str> {
    Some(match code {
        "created" => "Event created.",
        "saved" => "Changes saved.",
        "approved" => "Approved. It will be collated once the event ends.",
        "approved-ended" => "Approved. The event has ended, so collation is starting.",
        "unapproved" => "Approval removed.",
        "skipped" => "Event skipped.",
        "unskipped" => "Event moved back to review.",
        "queued" => "Queued for collation.",
        "reset" => "Reset to the latest proposal.",
        "polling" => "Polling the event APIs. Refresh in a few seconds.",
        "status-changed" => "The event's status changed in the meantime; nothing was done.",
        _ => return None,
    })
}

#[derive(Deserialize)]
struct NoticeQuery {
    msg: Option<String>,
}

impl NoticeQuery {
    fn notice(&self) -> Option<String> {
        self.msg.as_deref().and_then(notice_text).map(str::to_owned)
    }
}

// --- Index ---

struct EventSummary {
    id: i64,
    name: String,
    status: &'static str,
    window: String,
    airports: String,
    sources: String,
    note: String,
}

struct Section {
    title: &'static str,
    collapsed: bool,
    events: Vec<EventSummary>,
}

#[derive(Template)]
#[template(path = "index.html")]
struct IndexTemplate {
    sections: Vec<Section>,
    last_poll: String,
    poll_error: Option<String>,
    r2_configured: bool,
    notice: Option<String>,
}

fn sources_label(row: &EventRow) -> String {
    match (row.vatusa_id.is_some(), row.vatsim_id.is_some()) {
        (true, true) => "VATUSA + VATSIM",
        (true, false) => "VATUSA",
        (false, true) => "VATSIM",
        (false, false) => "Manual",
    }
    .to_owned()
}

fn summary_note(row: &EventRow, now: DateTime<Utc>) -> String {
    match row.status() {
        Status::Approved if !row.has_ended(now) => {
            format!("collates in {}", format_duration(row.collation_due_at() - now))
        }
        Status::Approved | Status::Collating => "collating".to_owned(),
        Status::Failed => row.error.clone().unwrap_or_default(),
        Status::Published => row
            .collated_at
            .map(|t| format!("published {}", format_time(t)))
            .unwrap_or_default(),
        Status::Proposed if row.has_ended(now) => {
            let left = row.data_expires_at() - now;
            if left > chrono::Duration::zero() {
                format!("ended; data expires in {}", format_duration(left))
            } else {
                "data expired".to_owned()
            }
        }
        Status::Proposed if row.airports.is_empty() => "no airports found".to_owned(),
        Status::Proposed | Status::Skipped => String::new(),
    }
}

async fn index(State(state): State<Arc<AppState>>, Query(q): Query<NoticeQuery>) -> HandlerResult {
    let now = Utc::now();
    let rows = db::list_events(&state.pool).await?;

    let mut sections = [
        ("Needs review", false),
        ("Approved", false),
        ("Failed", false),
        ("Published", false),
        ("Skipped or expired", true),
    ]
    .map(|(title, collapsed)| Section {
        title,
        collapsed,
        events: vec![],
    });

    for row in &rows {
        let section = match row.status() {
            Status::Proposed if row.data_expires_at() > now => 0,
            Status::Approved | Status::Collating => 1,
            Status::Failed => 2,
            Status::Published => 3,
            Status::Proposed | Status::Skipped => 4,
        };
        sections[section].events.push(EventSummary {
            id: row.id,
            name: row.name.clone(),
            status: row.status().as_str(),
            window: format_window(row.start_time, row.end_time),
            airports: row.airports.join(" "),
            sources: sources_label(row),
            note: summary_note(row, now),
        });
    }
    // Most recent first for published events
    sections[3].events.reverse();

    let (last_poll, poll_error) = state
        .last_poll
        .lock()
        .expect("last_poll lock poisoned")
        .as_ref()
        .map_or_else(
            || ("not yet".to_owned(), None),
            |poll| (format_time(poll.at), poll.result.as_ref().err().cloned()),
        );

    render(
        &IndexTemplate {
            sections: sections.into_iter().collect(),
            last_poll,
            poll_error,
            r2_configured: state.r2.is_some(),
            notice: q.notice(),
        },
        StatusCode::OK,
    )
}

async fn poll_now(State(state): State<Arc<AppState>>) -> Redirect {
    state.poll_now.notify_one();
    Redirect::to("/?msg=polling")
}

// --- Event form ---

#[derive(Deserialize, Default)]
struct EventForm {
    name: String,
    start_time: String,
    end_time: String,
    airports: String,
    artccs: String,
    custom_airports: String,
    unlisted: Option<String>,
}

impl EventForm {
    fn from_row(row: &EventRow) -> Self {
        let mut custom_airports = String::new();
        let mut custom: Vec<_> = row.custom_airports.0.iter().collect();
        custom.sort_by_key(|(id, _)| *id);
        for (id, c) in custom {
            let _ = writeln!(custom_airports, "{id} {} {}", c.latitude, c.longitude);
        }
        Self {
            name: row.name.clone(),
            start_time: row.start_time.format(INPUT_TIME_FORMAT).to_string(),
            end_time: row.end_time.format(INPUT_TIME_FORMAT).to_string(),
            airports: row.airports.join(" "),
            artccs: row.artccs.join(" "),
            custom_airports,
            unlisted: row.unlisted.then(|| "on".to_owned()),
        }
    }

    fn parse(&self) -> Result<EventEdit, String> {
        let name = self.name.trim().to_owned();
        if name.is_empty() {
            return Err("Name is required.".to_owned());
        }

        let artccs = parse_codes(&self.artccs);
        if let Some(unknown) = artccs.iter().find(|a| !US_ARTCCS.contains(&a.as_str())) {
            return Err(format!("Unknown ARTCC: {unknown}."));
        }

        let mut custom_airports = HashMap::new();
        for line in self.custom_airports.lines().filter(|l| !l.trim().is_empty()) {
            let parts: Vec<&str> = line
                .split(|c: char| c == ',' || c.is_whitespace())
                .filter(|p| !p.is_empty())
                .collect();
            let parsed = match parts.as_slice() {
                [id, lat, lon] => lat
                    .parse::<f64>()
                    .ok()
                    .zip(lon.parse::<f64>().ok())
                    .filter(|(lat, lon)| lat.abs() <= 90.0 && lon.abs() <= 180.0)
                    .map(|(latitude, longitude)| {
                        (id.to_uppercase(), CustomAirport { latitude, longitude })
                    }),
                _ => None,
            };
            let Some((id, coords)) = parsed else {
                return Err(format!(
                    "Couldn't read custom airport \"{}\"; use \"ID latitude longitude\".",
                    line.trim()
                ));
            };
            custom_airports.insert(id, coords);
        }

        Ok(EventEdit {
            name,
            start_time: parse_input_time(&self.start_time).ok_or("Start time is invalid.")?,
            end_time: parse_input_time(&self.end_time).ok_or("End time is invalid.")?,
            airports: parse_codes(&self.airports),
            artccs,
            custom_airports,
            unlisted: self.unlisted.is_some(),
        })
    }
}

fn parse_codes(input: &str) -> Vec<String> {
    let mut codes: Vec<String> = vec![];
    for code in input
        .split(|c: char| c == ',' || c.is_whitespace())
        .filter(|c| !c.is_empty())
        .map(str::to_uppercase)
    {
        if !codes.contains(&code) {
            codes.push(code);
        }
    }
    codes
}

fn parse_input_time(input: &str) -> Option<DateTime<Utc>> {
    let input = input.trim();
    NaiveDateTime::parse_from_str(input, INPUT_TIME_FORMAT)
        .or_else(|_| NaiveDateTime::parse_from_str(input, "%Y-%m-%dT%H:%M:%S"))
        .ok()
        .map(|t| t.and_utc())
}

// --- Event page ---

struct Action {
    path: &'static str,
    label: &'static str,
    primary: bool,
    confirm: Option<&'static str>,
}

struct ProposalView {
    name: String,
    window: String,
    airports: String,
    artccs: String,
    airport_source: String,
}

#[derive(Template)]
#[template(path = "event.html")]
struct EventTemplate {
    id: Option<i64>,
    title: String,
    status: &'static str,
    form: EventForm,
    editable: bool,
    notice: Option<String>,
    error: Option<String>,
    warnings: Vec<String>,
    actions: Vec<Action>,
    details: Vec<(&'static str, String)>,
    proposal: Option<ProposalView>,
    vatsim_link: Option<String>,
    vatusa_body: Option<String>,
    sources_json: Vec<(&'static str, String)>,
}

impl EventTemplate {
    fn new_event(form: EventForm, error: Option<String>) -> Self {
        Self {
            id: None,
            title: "New event".to_owned(),
            status: "new",
            form,
            editable: true,
            notice: None,
            error,
            warnings: vec![],
            actions: vec![],
            details: vec![],
            proposal: None,
            vatsim_link: None,
            vatusa_body: None,
            sources_json: vec![],
        }
    }

    #[allow(clippy::too_many_lines)]
    fn for_row(state: &AppState, row: &EventRow, form: EventForm) -> Self {
        let now = Utc::now();
        let status = row.status();

        let mut actions = vec![];
        let mut action = |path, label, primary, confirm| {
            actions.push(Action {
                path,
                label,
                primary,
                confirm,
            });
        };
        match status {
            Status::Proposed => {
                action("approve", "Approve", true, None);
                action("skip", "Skip", false, None);
                if row.edited && row.proposal.is_some() {
                    action(
                        "reset",
                        "Reset to proposal",
                        false,
                        Some("Discard your edits and use the latest proposal?"),
                    );
                }
            }
            Status::Approved => {
                action("unapprove", "Unapprove", false, None);
                action("skip", "Skip", false, None);
            }
            Status::Failed => {
                action("collate", "Retry collation", true, None);
                action("skip", "Skip", false, None);
            }
            Status::Published => action(
                "collate",
                "Re-collate",
                false,
                Some("Collate again and replace the published file?"),
            ),
            Status::Skipped => action("unskip", "Move back to review", false, None),
            Status::Collating => {}
        }

        let mut warnings = vec![];
        if row.end_time <= row.start_time {
            warnings.push("The end time is not after the start time.".to_owned());
        }
        if row.airports.is_empty() {
            warnings.push(
                "No airports were found in either API. Add them before approving.".to_owned(),
            );
        }
        for code in &row.airports {
            if !state.airports.contains_key(code) && !row.custom_airports.0.contains_key(code) {
                warnings.push(format!(
                    "{code} isn't in the FAA airport data, so only flight plans to or from it will be captured. Add custom coordinates to also capture traffic near it."
                ));
            }
        }
        if let Some(p) = &row.proposal
            && (row.edited || status != Status::Proposed)
            && (p.0.start_time != row.start_time || p.0.end_time != row.end_time)
        {
            warnings.push(format!(
                "The source times have changed to {}.",
                format_window(p.0.start_time, p.0.end_time)
            ));
        }
        if let Some(seen) = row.last_seen_at
            && row.start_time > now
            && now - seen > chrono::Duration::hours(NOT_SEEN_WARNING_HOURS)
        {
            warnings.push(format!(
                "Not listed by the APIs since {}; it may have been cancelled.",
                format_time(seen)
            ));
        }

        let mut details = vec![("Sources", sources_label(row))];
        details.push(("Airports from", row.airport_source.clone()));
        if matches!(status, Status::Proposed | Status::Approved | Status::Failed) {
            if row.has_ended(now) {
                let left = row.data_expires_at() - now;
                details.push((
                    "Data expires",
                    if left > chrono::Duration::zero() {
                        format!("in {} ({})", format_duration(left), format_time(row.data_expires_at()))
                    } else {
                        "expired".to_owned()
                    },
                ));
            } else {
                details.push(("Collation due", format_time(row.collation_due_at())));
            }
        }
        if let Some(t) = row.approved_at {
            details.push(("Approved", format_time(t)));
        }
        if let Some(t) = row.collated_at {
            details.push(("Last collated", format_time(t)));
        }
        if let Some(url) = &row.published_url {
            details.push(("Published URL", url.clone()));
        }

        let source_value = |v: &Option<serde_json::Value>, key: &str| {
            v.as_ref()
                .and_then(|v| v.get(key))
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned)
        };
        let sources_json = [("VATUSA", &row.vatusa_source), ("VATSIM", &row.vatsim_source)]
            .into_iter()
            .filter_map(|(label, v)| {
                Some((label, serde_json::to_string_pretty(v.as_ref()?).ok()?))
            })
            .collect();

        Self {
            id: Some(row.id),
            title: row.name.clone(),
            status: status.as_str(),
            form,
            editable: status != Status::Collating,
            notice: None,
            error: status.eq(&Status::Failed).then(|| row.error.clone()).flatten(),
            warnings,
            actions,
            details,
            proposal: row.proposal.as_ref().map(|p| ProposalView {
                name: p.0.name.clone(),
                window: format_window(p.0.start_time, p.0.end_time),
                airports: p.0.airports.join(" "),
                artccs: p.0.artccs.join(" "),
                airport_source: p.0.airport_source.clone(),
            }),
            vatsim_link: source_value(&row.vatsim_source, "link"),
            vatusa_body: source_value(&row.vatusa_source, "body"),
            sources_json,
        }
    }
}

async fn new_event() -> HandlerResult {
    render(&EventTemplate::new_event(EventForm::default(), None), StatusCode::OK)
}

async fn create_event(State(state): State<Arc<AppState>>, Form(form): Form<EventForm>) -> HandlerResult {
    let edit = match form.parse() {
        Ok(edit) => edit,
        Err(e) => {
            return render(&EventTemplate::new_event(form, Some(e)), StatusCode::UNPROCESSABLE_ENTITY);
        }
    };
    let id = db::create_event(&state.pool, &edit).await?;
    Ok(Redirect::to(&format!("/events/{id}?msg=created")).into_response())
}

async fn show_event(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
    Query(q): Query<NoticeQuery>,
) -> HandlerResult {
    let Some(row) = db::get_event(&state.pool, id).await? else {
        return Ok(not_found());
    };
    let mut page = EventTemplate::for_row(&state, &row, EventForm::from_row(&row));
    page.notice = q.notice();
    render(&page, StatusCode::OK)
}

async fn save_event(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
    Form(form): Form<EventForm>,
) -> HandlerResult {
    let Some(row) = db::get_event(&state.pool, id).await? else {
        return Ok(not_found());
    };
    let edit = match form.parse() {
        Ok(edit) => edit,
        Err(e) => {
            let mut page = EventTemplate::for_row(&state, &row, form);
            page.error = Some(e);
            return render(&page, StatusCode::UNPROCESSABLE_ENTITY);
        }
    };
    let msg = if db::update_event(&state.pool, id, &edit).await? {
        "saved"
    } else {
        "status-changed"
    };
    Ok(Redirect::to(&format!("/events/{id}?msg={msg}")).into_response())
}

async fn event_action(
    State(state): State<Arc<AppState>>,
    Path((id, action)): Path<(i64, String)>,
) -> HandlerResult {
    let Some(row) = db::get_event(&state.pool, id).await? else {
        return Ok(not_found());
    };
    let now = Utc::now();

    // Approving and collating need a complete config with data still available
    if matches!(action.as_str(), "approve" | "collate") {
        let invalid = row.validate(now).err().or_else(|| {
            (action == "collate" && !row.has_ended(now))
                .then(|| "The event hasn't ended yet.".to_owned())
        });
        if let Some(e) = invalid {
            let mut page = EventTemplate::for_row(&state, &row, EventForm::from_row(&row));
            page.error = Some(e);
            return render(&page, StatusCode::UNPROCESSABLE_ENTITY);
        }
    }

    let (changed, msg) = match action.as_str() {
        "approve" => {
            let changed = db::transition(&state.pool, id, &[Status::Proposed], Status::Approved).await?;
            (changed, if row.has_ended(now) { "approved-ended" } else { "approved" })
        }
        "unapprove" => (
            db::transition(&state.pool, id, &[Status::Approved], Status::Proposed).await?,
            "unapproved",
        ),
        "skip" => (
            db::transition(
                &state.pool,
                id,
                &[Status::Proposed, Status::Approved, Status::Failed],
                Status::Skipped,
            )
            .await?,
            "skipped",
        ),
        "unskip" => (
            db::transition(&state.pool, id, &[Status::Skipped], Status::Proposed).await?,
            "unskipped",
        ),
        "collate" => (
            db::transition(
                &state.pool,
                id,
                &[Status::Published, Status::Failed],
                Status::Approved,
            )
            .await?,
            "queued",
        ),
        "reset" => (db::reset_to_proposal(&state.pool, &row).await?, "reset"),
        _ => return Ok(not_found()),
    };

    if changed && matches!(action.as_str(), "approve" | "collate") {
        state.collate_now.notify_one();
    }
    let msg = if changed { msg } else { "status-changed" };
    Ok(Redirect::to(&format!("/events/{id}?msg={msg}")).into_response())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_event_form() {
        let form = EventForm {
            name: " Test FNO ".to_owned(),
            start_time: "2026-11-06T23:59".to_owned(),
            end_time: "2026-11-07T04:00:00".to_owned(),
            airports: "ksea, KPDX KSEA\nGE99".to_owned(),
            artccs: "zse".to_owned(),
            custom_airports: "ge99 34.5 -84.25\n\n".to_owned(),
            unlisted: Some("on".to_owned()),
        };
        let edit = form.parse().unwrap();
        assert_eq!(edit.name, "Test FNO");
        assert_eq!(edit.airports, ["KSEA", "KPDX", "GE99"]);
        assert_eq!(edit.artccs, ["ZSE"]);
        assert_eq!(edit.custom_airports["GE99"].longitude, -84.25);
        assert_eq!((edit.end_time - edit.start_time).num_minutes(), 241);
        assert!(edit.unlisted);
    }

    #[test]
    fn rejects_bad_form_values() {
        let base = || EventForm {
            name: "X".to_owned(),
            start_time: "2026-11-06T23:59".to_owned(),
            end_time: "2026-11-07T04:00".to_owned(),
            ..EventForm::default()
        };
        assert!(EventForm { artccs: "ZXX".to_owned(), ..base() }.parse().is_err());
        assert!(EventForm { custom_airports: "GE99 95 0".to_owned(), ..base() }.parse().is_err());
        assert!(EventForm { start_time: "tomorrow".to_owned(), ..base() }.parse().is_err());
        assert!(EventForm { name: "  ".to_owned(), ..base() }.parse().is_err());
    }

    #[test]
    fn checks_same_origin() {
        let headers = |pairs: &[(&'static str, &'static str)]| {
            let mut h = HeaderMap::new();
            for (k, v) in pairs {
                h.insert(*k, v.parse().unwrap());
            }
            h
        };
        assert!(is_same_origin(&headers(&[("sec-fetch-site", "same-origin")])));
        assert!(!is_same_origin(&headers(&[("sec-fetch-site", "cross-site")])));
        assert!(is_same_origin(&headers(&[("origin", "https://sched.example"), ("host", "sched.example")])));
        assert!(!is_same_origin(&headers(&[("origin", "https://evil.example"), ("host", "sched.example")])));
        assert!(is_same_origin(&headers(&[])));
    }

    #[test]
    fn formats_durations() {
        assert_eq!(format_duration(chrono::Duration::minutes(241)), "4h 1m");
        assert_eq!(format_duration(chrono::Duration::minutes(180)), "3h");
        assert_eq!(format_duration(chrono::Duration::minutes(3000)), "2d 2h");
    }
}
