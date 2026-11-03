//! Fetching events from the VATUSA and VATSIM APIs and proposing configs for them.

use anyhow::{Context, bail};
use chrono::{DateTime, NaiveDate, Utc};
use regex::Regex;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;
use tracing::debug;
use traffic_replay::{Airport, IcaoId, US_ARTCCS};

const VATUSA_PAGE_COUNT: u32 = 100;
/// Minimum title similarity (Jaccard over words) to treat two events as the same.
const TITLE_MATCH_THRESHOLD: f64 = 0.5;
/// Slack allowed when checking whether two events' windows overlap.
const MATCH_TIME_SLACK_HOURS: i64 = 2;

#[derive(Deserialize, Debug, Clone)]
pub struct VatusaEvent {
    pub id: i64,
    pub title: String,
    pub body: String,
    pub facility: String,
    pub start_time: i64,
    pub end_time: i64,
}

#[derive(Deserialize, Debug, Clone)]
pub struct VatsimEvent {
    pub id: i64,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
    #[serde(default)]
    pub airports: Vec<VatsimAirport>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct VatsimAirport {
    pub icao: String,
}

#[derive(Deserialize)]
struct VatsimEventsResponse {
    data: Vec<serde_json::Value>,
}

/// An event as returned by one API, with its raw payload kept for display.
pub struct Sourced<T> {
    pub event: T,
    pub raw: serde_json::Value,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct Proposal {
    pub name: String,
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
    pub airports: Vec<String>,
    pub artccs: Vec<String>,
    pub airport_source: String,
}

/// One event found by polling, from either or both APIs.
pub struct DiscoveredEvent {
    pub vatusa: Option<Sourced<VatusaEvent>>,
    pub vatsim: Option<Sourced<VatsimEvent>>,
    pub proposal: Proposal,
}

pub async fn fetch_vatusa_events(
    client: &Client,
    base_url: &str,
    from: NaiveDate,
) -> Result<Vec<Sourced<VatusaEvent>>, anyhow::Error> {
    let mut events = vec![];
    let mut page = 1;
    loop {
        let response = client
            .get(format!("{}/events", base_url.trim_end_matches('/')))
            .query(&[
                ("start_date_from", from.format("%Y-%m-%d").to_string()),
                ("count", VATUSA_PAGE_COUNT.to_string()),
                ("page", page.to_string()),
            ])
            .send()
            .await?
            .error_for_status()?;

        let total_pages = response
            .headers()
            .get("X-Total-Pages")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<u32>().ok())
            .unwrap_or(1);

        let raw: Vec<serde_json::Value> = response.json().await?;
        for value in raw {
            let event = serde_json::from_value(value.clone())
                .context("unexpected VATUSA event format")?;
            events.push(Sourced { event, raw: value });
        }

        if page >= total_pages {
            break;
        }
        page += 1;
    }
    debug!(count = events.len(), "Fetched VATUSA events");
    Ok(events)
}

pub async fn fetch_vatsim_events(
    client: &Client,
    url: &str,
) -> Result<Vec<Sourced<VatsimEvent>>, anyhow::Error> {
    let response: VatsimEventsResponse = client
        .get(url)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    let events = response
        .data
        .into_iter()
        .map(|value| {
            let event = serde_json::from_value(value.clone())
                .context("unexpected VATSIM event format")?;
            Ok(Sourced { event, raw: value })
        })
        .collect::<Result<Vec<_>, anyhow::Error>>()?;
    debug!(count = events.len(), "Fetched VATSIM events");
    Ok(events)
}

/// Lookups over the FAA airport data used to recognize airports in event text.
pub struct AirportIndex<'a> {
    by_icao: &'a HashMap<IcaoId, Airport>,
    by_faa: HashMap<&'a str, &'a Airport>,
}

impl<'a> AirportIndex<'a> {
    pub fn new(by_icao: &'a HashMap<IcaoId, Airport>) -> Self {
        let by_faa = by_icao
            .values()
            .filter_map(|a| a.faa_id.as_deref().map(|faa| (faa, a)))
            .collect();
        Self { by_icao, by_faa }
    }

    fn get(&self, icao_id: &str) -> Option<&'a Airport> {
        self.by_icao.get(icao_id)
    }
}

impl VatusaEvent {
    const fn start(&self) -> Option<DateTime<Utc>> {
        DateTime::from_timestamp(self.start_time, 0)
    }

    const fn end(&self) -> Option<DateTime<Utc>> {
        DateTime::from_timestamp(self.end_time, 0)
    }

    /// Host ARTCC, or `None` for division-hosted (`ZHQ`) events.
    fn host_artcc(&self) -> Option<&str> {
        US_ARTCCS
            .contains(&self.facility.as_str())
            .then_some(self.facility.as_str())
    }
}

/// Pair each VATUSA event with the VATSIM event it most likely duplicates, and
/// build a proposal for every resulting event.
pub fn discover(
    vatusa: Vec<Sourced<VatusaEvent>>,
    vatsim: Vec<Sourced<VatsimEvent>>,
    airports: &AirportIndex<'_>,
) -> Vec<DiscoveredEvent> {
    let mut candidates = vec![];
    for (i, a) in vatusa.iter().enumerate() {
        for (j, b) in vatsim.iter().enumerate() {
            if !windows_overlap(&a.event, &b.event) {
                continue;
            }
            let score = title_similarity(&a.event.title, &b.event.name);
            if score >= TITLE_MATCH_THRESHOLD {
                candidates.push((score, i, j));
            }
        }
    }
    candidates.sort_by(|a, b| b.0.total_cmp(&a.0));

    let mut vatusa_match: HashMap<usize, usize> = HashMap::new();
    let mut matched_vatsim = HashSet::new();
    for (_, i, j) in candidates {
        if !vatusa_match.contains_key(&i) && !matched_vatsim.contains(&j) {
            vatusa_match.insert(i, j);
            matched_vatsim.insert(j);
        }
    }

    let mut vatsim: Vec<Option<Sourced<VatsimEvent>>> = vatsim.into_iter().map(Some).collect();
    let mut discovered = vec![];
    for (i, a) in vatusa.into_iter().enumerate() {
        let b = vatusa_match.get(&i).and_then(|&j| vatsim[j].take());
        let Some(proposal) = propose(Some(&a.event), b.as_ref().map(|b| &b.event), airports)
        else {
            continue;
        };
        discovered.push(DiscoveredEvent {
            vatusa: Some(a),
            vatsim: b,
            proposal,
        });
    }
    for b in vatsim.into_iter().flatten() {
        if let Some(proposal) = propose(None, Some(&b.event), airports) {
            discovered.push(DiscoveredEvent {
                vatusa: None,
                vatsim: Some(b),
                proposal,
            });
        }
    }
    discovered
}

fn windows_overlap(a: &VatusaEvent, b: &VatsimEvent) -> bool {
    let (Some(start), Some(end)) = (a.start(), a.end()) else {
        return false;
    };
    // VATUSA end times are sometimes missing or before the start
    let end = end.max(start);
    let slack = chrono::Duration::hours(MATCH_TIME_SLACK_HOURS);
    start - slack <= b.end_time && b.start_time <= end + slack
}

fn title_words(title: &str) -> HashSet<String> {
    title
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_owned)
        .collect()
}

fn title_similarity(a: &str, b: &str) -> f64 {
    let a = title_words(a);
    let b = title_words(b);
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let shared = a.intersection(&b).count();
    #[allow(clippy::cast_precision_loss)]
    let score = shared as f64 / a.union(&b).count() as f64;
    score
}

fn propose(
    vatusa: Option<&VatusaEvent>,
    vatsim: Option<&VatsimEvent>,
    airports: &AirportIndex<'_>,
) -> Option<Proposal> {
    let host = vatusa.and_then(VatusaEvent::host_artcc);
    let mut artccs: Vec<String> = host.map(str::to_owned).into_iter().collect();

    let vatusa_window = vatusa
        .and_then(|e| Some((e.start()?, e.end()?)))
        .filter(|(start, end)| end > start);
    let vatsim_window = vatsim.map(|e| (e.start_time, e.end_time));
    let (start_time, end_time) = vatusa_window
        .or(vatsim_window)
        .or_else(|| vatusa.and_then(VatusaEvent::start).map(|start| (start, start)))?;

    let name = vatusa
        .map(|e| e.title.trim().to_owned())
        .or_else(|| vatsim.map(|e| e.name.trim().to_owned()))?;

    let mut proposed_airports = vec![];
    let mut airport_source = "none";
    if let Some(vatsim) = vatsim {
        for code in vatsim.airports.iter().map(|a| a.icao.trim().to_uppercase()) {
            if US_ARTCCS.contains(&code.as_str()) {
                push_unique(&mut artccs, code);
            } else if !(code.len() == 3 && code.starts_with('Z')) {
                // Anything else three letters starting with Z is a foreign FIR
                push_unique(&mut proposed_airports, code);
            }
        }
        if !proposed_airports.is_empty() {
            airport_source = "vatsim";
        }
    }
    if proposed_airports.is_empty() {
        let text = match (vatusa, vatsim) {
            (Some(e), _) => format!("{}\n{}", e.title, e.body),
            (None, Some(e)) => format!("{}\n{}", e.name, e.description),
            (None, None) => String::new(),
        };
        proposed_airports = extract_airports(&text, host, airports);
        if !proposed_airports.is_empty() {
            airport_source = "text";
        }
    }

    for code in &proposed_airports {
        if let Some(artcc) = airports.get(code).and_then(|a| a.resp_artcc_id.as_deref())
            && US_ARTCCS.contains(&artcc)
        {
            push_unique(&mut artccs, artcc.to_owned());
        }
    }

    Some(Proposal {
        name,
        start_time,
        end_time,
        airports: proposed_airports,
        artccs,
        airport_source: airport_source.to_owned(),
    })
}

fn push_unique(list: &mut Vec<String>, value: String) {
    if !list.contains(&value) {
        list.push(value);
    }
}

static HTML_TAG: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"<[^>]*>").expect("valid regex"));
static AIRPORT_CODE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\b[A-Z]{3,4}\b").expect("valid regex"));

/// Find airports mentioned in free text, as ICAO IDs in order of first mention.
///
/// 4-letter codes must be known airports that are towered or in the host ARTCC.
/// 3-letter FAA codes are only accepted for towered airports in the host
/// ARTCC, so words like "FNO" or "THE" aren't mistaken for airports.
fn extract_airports(text: &str, host: Option<&str>, airports: &AirportIndex<'_>) -> Vec<String> {
    let text = HTML_TAG.replace_all(text, " ");
    let mut found = vec![];
    for code in AIRPORT_CODE.find_iter(&text).map(|m| m.as_str()) {
        let in_host = |a: &Airport| host.is_some() && a.resp_artcc_id.as_deref() == host;
        let airport = if code.len() == 4 {
            airports.get(code).filter(|a| a.towered || in_host(a))
        } else if US_ARTCCS.contains(&code) {
            None
        } else {
            airports
                .by_faa
                .get(code)
                .copied()
                .filter(|a| a.towered && in_host(a))
        };
        if let Some(airport) = airport {
            push_unique(&mut found, airport.icao_id.clone());
        }
    }
    found
}

/// Fetch both sources and build the list of discovered events.
pub async fn poll(
    client: &Client,
    vatusa_url: &str,
    vatsim_url: &str,
    from: NaiveDate,
    airports: &AirportIndex<'_>,
) -> Result<Vec<DiscoveredEvent>, anyhow::Error> {
    let (vatusa, vatsim) = tokio::join!(
        fetch_vatusa_events(client, vatusa_url, from),
        fetch_vatsim_events(client, vatsim_url),
    );
    let vatusa = vatusa.context("failed to fetch VATUSA events")?;
    let vatsim = vatsim.context("failed to fetch VATSIM events")?;
    if vatusa.is_empty() && vatsim.is_empty() {
        bail!("both event APIs returned no events");
    }
    Ok(discover(vatusa, vatsim, airports))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use geo::Point;

    fn airport(icao: &str, faa: &str, artcc: &str, towered: bool) -> (String, Airport) {
        (
            icao.to_owned(),
            Airport {
                faa_id: Some(faa.to_owned()),
                icao_id: icao.to_owned(),
                point: Point::new(0.0, 0.0),
                resp_artcc_id: Some(artcc.to_owned()),
                towered,
            },
        )
    }

    fn airports_db() -> HashMap<IcaoId, Airport> {
        HashMap::from([
            airport("KSEA", "SEA", "ZSE", true),
            airport("KPWT", "PWT", "ZSE", false),
            airport("KIAD", "IAD", "ZDC", true),
            airport("KEST", "EST", "ZMP", false),
            airport("KDEN", "DEN", "ZDV", true),
        ])
    }

    fn vatusa(title: &str, body: &str, facility: &str, start: i64, end: i64) -> VatusaEvent {
        VatusaEvent {
            id: 1,
            title: title.to_owned(),
            body: body.to_owned(),
            facility: facility.to_owned(),
            start_time: start,
            end_time: end,
        }
    }

    fn vatsim(name: &str, start: i64, end: i64, airports: &[&str]) -> VatsimEvent {
        VatsimEvent {
            id: 2,
            name: name.to_owned(),
            description: String::new(),
            start_time: Utc.timestamp_opt(start, 0).unwrap(),
            end_time: Utc.timestamp_opt(end, 0).unwrap(),
            airports: airports
                .iter()
                .map(|a| VatsimAirport { icao: (*a).to_owned() })
                .collect(),
        }
    }

    fn sourced<T>(event: T) -> Sourced<T> {
        Sourced {
            event,
            raw: serde_json::Value::Null,
        }
    }

    #[test]
    fn extracts_icao_and_host_faa_codes() {
        let db = airports_db();
        let index = AirportIndex::new(&db);
        let text = "<p>Join <b>SEA</b> and KPWT for our FNO!</p> Starts 1700 PST / EST. Also IAD, KDEN.";
        // SEA: towered in host; KPWT: untowered but in host; IAD: towered but
        // another ARTCC (3-letter rejected); EST: not towered; KDEN: towered ICAO
        assert_eq!(
            extract_airports(text, Some("ZSE"), &index),
            ["KSEA", "KPWT", "KDEN"]
        );
        // Division-hosted events only match ICAO codes
        assert_eq!(extract_airports(text, None, &index), ["KDEN"]);
    }

    #[test]
    fn matches_by_title_and_time_and_prefers_vatsim_airports() {
        let db = airports_db();
        let index = AirportIndex::new(&db);
        let found = discover(
            vec![
                sourced(vatusa("Never Dull at Dulles", "Come fly KIAD", "ZDC", 1000, 15000)),
                sourced(vatusa("Seattle Night Ops", "KSEA", "ZSE", 100_000, 110_000)),
            ],
            vec![
                sourced(vatsim("Never Dull at Dulles!", 1000, 15000, &["KIAD", "KDEN", "ZNY", "ZUL"])),
                sourced(vatsim("Never Dull at Dulles", 500_000, 510_000, &["KIAD"])),
            ],
            &index,
        );

        assert_eq!(found.len(), 3);
        let dulles = &found[0];
        assert!(dulles.vatsim.as_ref().is_some_and(|e| e.event.start_time.timestamp() == 1000));
        assert_eq!(dulles.proposal.airports, ["KIAD", "KDEN"]);
        assert_eq!(dulles.proposal.artccs, ["ZDC", "ZNY", "ZDV"]);
        assert_eq!(dulles.proposal.airport_source, "vatsim");

        let seattle = &found[1];
        assert!(seattle.vatsim.is_none());
        assert_eq!(seattle.proposal.airports, ["KSEA"]);
        assert_eq!(seattle.proposal.airport_source, "text");

        // Same title but weeks apart: kept as its own VATSIM-only event
        assert!(found[2].vatusa.is_none());
    }

    #[test]
    fn invalid_vatusa_window_falls_back_to_vatsim() {
        let db = airports_db();
        let index = AirportIndex::new(&db);
        let found = discover(
            vec![sourced(vatusa("The 13/31 Special", "", "ZFW", 10_000, 5_000))],
            vec![sourced(vatsim("The 13/31 Special", 10_000, 20_000, &[]))],
            &index,
        );
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].proposal.end_time.timestamp(), 20_000);
    }
}
