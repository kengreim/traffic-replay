#![warn(clippy::all, clippy::pedantic, clippy::nursery)]

use anyhow::{Error, anyhow, bail};
use chrono::{DateTime, Datelike, Utc};
use figment::Figment;
use figment::providers::{Format, Toml};
use geo::{BoundingRect, Distance, Haversine, MultiPolygon, Point, Polygon, Rect, unary_union};
use geojson::feature::Id;
use geojson::{Feature, FeatureCollection, GeoJson, JsonObject, Value};
use serde::{Deserialize, Serialize};
use slug::slugify;
use std::cmp::min;
use std::collections::HashMap;
use std::fs;
use std::fs::File;
use std::io::Write;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::time::sleep;
use tracing::{Level, debug, error, info, instrument, span, warn};
use tracing_subscriber::fmt::format::FmtSpan;
use vatsim_utils::live_api::Vatsim;
use vatsim_utils::models::{Pilot, V3ResponseData};
use walkdir::WalkDir;

const NM_TO_METERS: f64 = 1852.0;
const EVENT_PRE_TIME_MINUTES: u64 = 5;
const EVENT_POST_TIME_MINUTES: u64 = 5;
const CAPTURE_RANGE_NM: u16 = 600;

#[derive(Deserialize, Serialize)]
pub struct PilotData {
    pub cid: u64,
    pub name: String,
    pub callsign: String,
    pub latitude: f64,
    pub longitude: f64,
    pub altitude: i64,
    pub groundspeed: i64,
    pub transponder: String,
    pub heading: i64,
    pub flight_plan: Option<FlightPlan>,
    pub logon_time: String,
    pub last_updated: String,
}

#[derive(Deserialize, Serialize)]
pub struct FlightPlan {
    pub flight_rules: String,
    pub aircraft: String,
    pub aircraft_faa: String,
    pub aircraft_short: String,
    pub departure: String,
    pub arrival: String,
    pub alternate: String,
    pub altitude: String,
    pub route: String,
    pub revision_id: i64,
}

impl From<Pilot> for PilotData {
    fn from(value: Pilot) -> Self {
        Self {
            cid: value.cid,
            name: value.name,
            callsign: value.callsign,
            latitude: value.latitude,
            longitude: value.longitude,
            altitude: value.altitude,
            groundspeed: value.groundspeed,
            transponder: value.transponder,
            heading: value.heading,
            flight_plan: value.flight_plan.map(Into::into),
            logon_time: value.logon_time,
            last_updated: value.last_updated,
        }
    }
}

impl From<vatsim_utils::models::FlightPlan> for FlightPlan {
    fn from(value: vatsim_utils::models::FlightPlan) -> Self {
        Self {
            flight_rules: value.flight_rules,
            aircraft: value.aircraft,
            aircraft_faa: value.aircraft_faa,
            aircraft_short: value.aircraft_short,
            departure: value.departure,
            arrival: value.arrival,
            alternate: value.alternate,
            altitude: value.altitude,
            route: value.route,
            revision_id: value.revision_id,
        }
    }
}

#[derive(Deserialize, Serialize, Debug, Clone)]
struct EventConfig {
    pub name: String,
    pub artccs: Vec<String>,
    pub airports: Vec<String>,
    pub advertised_start_time: DateTime<Utc>,
    pub advertised_end_time: DateTime<Utc>,
}

#[derive(Serialize, Debug, Clone)]
struct EventCapture {
    pub config: EventConfig,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub first_timestamp_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_timestamp_key: Option<String>,
    pub captures: HashMap<String, FeatureCollection>,
    pub captures_length_bytes: usize,
    pub viewport_center: Point<f64>,
}

#[derive(Debug, serde::Deserialize)]
struct AirportRecord {
    #[serde(rename = "ARPT_ID")]
    faa_id: String,
    #[serde(rename = "ICAO_ID")]
    icao_id: String,
    #[serde(rename = "LAT_DECIMAL")]
    latitude: f64,
    #[serde(rename = "LONG_DECIMAL")]
    longitude: f64,
}

#[derive(Debug)]
struct Airport {
    #[allow(dead_code)]
    faa_id: String,
    icao_id: String,
    point: Point,
}

impl From<AirportRecord> for Airport {
    fn from(record: AirportRecord) -> Self {
        Self {
            faa_id: record.faa_id,
            icao_id: record.icao_id,
            point: Point::new(record.longitude, record.latitude),
        }
    }
}

type IcaoId = String;

#[instrument]
fn load_airports() -> Result<HashMap<IcaoId, Airport>, anyhow::Error> {
    debug!("loading airports from file");
    let mut airports = HashMap::new();

    let csv_file = File::open("./APT_BASE.csv")?;
    let mut csv_reader = csv::Reader::from_reader(csv_file);
    for record in csv_reader.deserialize() {
        let record: AirportRecord = record?;
        if !record.icao_id.is_empty() {
            airports.insert(record.icao_id.clone(), Airport::from(record));
        }
    }

    debug!("completed loading data for {} airports", airports.len());
    Ok(airports)
}

fn filter_pilots_by_distance_and_field(
    mut pilots: Vec<Pilot>,
    icao_airports: &[&Airport],
    distance_nm: u16,
) -> Vec<Pilot> {
    pilots.retain(|p| {
        p.flight_plan.as_ref().is_some_and(|fp| {
            icao_airports
                .iter()
                .any(|apt| apt.icao_id == fp.departure || apt.icao_id == fp.arrival)
        }) || icao_airports.iter().any(|apt| {
            Haversine.distance(apt.point, Point::new(p.longitude, p.latitude))
                < (f64::from(distance_nm) * NM_TO_METERS)
        })
    });

    pilots
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let subscriber = tracing_subscriber::fmt()
        .compact()
        .json()
        .with_max_level(tracing::Level::DEBUG)
        .with_env_filter("traffic_replay=debug")
        .with_span_events(FmtSpan::CLOSE)
        .with_file(true)
        .with_line_number(true)
        .finish();

    tracing::subscriber::set_global_default(subscriber)?;

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

    let mut airports = vec![];
    for icao_id in &event_config.airports {
        if let Some(airport) = all_airports.get(icao_id) {
            airports.push(airport);
        } else {
            error!(id = ?icao_id, "invalid airport ICAO id, not found in FAA data");
            bail!("invalid airport ICAO id {icao_id}, not found in FAA data");
        }
    }

    let api = match Vatsim::new().await {
        Ok(api) => api,
        Err(e) => {
            error!(error = ?e, "failed to initialize VATSIM API");
            return Err(e.into());
        }
    };

    let event_config_clone = event_config.clone();

    let (tx, rx) = mpsc::channel(32);
    tokio::spawn(async move { datafeed_loop(api, tx, &event_config_clone).await });

    if let Err(e) = process_datafeeds(rx, &airports, &event_slug(&event_config)).await {
        error!(error = ?e, "failed to process data feeds");
    }

    if let Err(e) = combine_captures(&event_config, &airports) {
        error!(error = ?e, "failed to combine captures");
    }

    Ok(())
}

async fn datafeed_loop(api: Vatsim, tx: Sender<V3ResponseData>, event_config: &EventConfig) {
    let loop_start_time =
        event_config.advertised_start_time - Duration::from_secs(60 * EVENT_PRE_TIME_MINUTES);
    let loop_end_time =
        event_config.advertised_end_time + Duration::from_secs(60 * EVENT_POST_TIME_MINUTES);
    let initial_sleep = if Utc::now() > loop_start_time {
        None
    } else {
        Some(loop_start_time - Utc::now())
    };

    let mut last_datafeed_update = String::new();

    if let Some(duration) = initial_sleep {
        match duration.to_std() {
            Ok(duration) => {
                info!(duration = ?duration, "Sleeping until captures start");
                tokio::time::sleep(duration).await;
            }
            Err(e) => {
                error!(error = ?e, "failed to convert TimeDelta to Duration required for intial sleep");
                return;
            }
        }
    }

    info!("Starting datafeed loop");
    loop {
        let start = Instant::now();

        // Get data and check that there was no error
        let latest_data_result = api.get_v3_data().await;
        if let Err(e) = latest_data_result {
            warn!(error = ?e, "Could not fetch VATSIM data");
            sleep(Duration::from_secs(1)).await;
            continue;
        }

        // Unwrap and check if duplicate from last fetch
        // Safe to unwrap because checked Err case above already
        let latest_data = latest_data_result.expect("Error getting VATSIM API data");

        if latest_data.general.update == last_datafeed_update {
            debug!(time = %latest_data.general.update, "Found duplicate");
            sleep(Duration::from_secs(1)).await;
            continue;
        }

        // Update timestamp of latest data and process datafeed
        last_datafeed_update.clone_from(&latest_data.general.update);

        let update_timestamp = if let Ok(update_timestamp) =
            DateTime::parse_from_rfc3339(&latest_data.general.update_timestamp)
        {
            update_timestamp.to_utc()
        } else {
            warn!(
                timestamp = latest_data.general.update_timestamp,
                "Could not parse timestamp"
            );
            continue;
        };

        if update_timestamp > loop_end_time {
            info!("Ending datafeed collection");
            return;
        }

        if let Err(e) = tx.send(latest_data).await {
            error!(error = ?e, "Error sending datafeed through mpsc. Ending datafeed loop");
            return;
        }
        info!(time = %update_timestamp, "Found new datafeed");

        // Sleep for 5 seconds minus the time this loop took, with some protections to make sure we
        // don't have a negative duration
        let loop_time = start.elapsed();
        if loop_time > Duration::from_secs(4) {
            warn!(?loop_time, "Long loop");
        }
        let sleep_duration = Duration::from_secs(5) - min(Duration::from_secs(4), loop_time);
        debug!(?sleep_duration, "Sleeping");
        sleep(sleep_duration).await;
    }
}

async fn process_datafeeds(
    mut rx: Receiver<V3ResponseData>,
    airports: &[&Airport],
    event_slug: &str,
) -> Result<(), Error> {
    info!("Starting datafeed processor");

    // Create Directory for captures
    let captures_dir_string = format!("./{event_slug}/captures");
    fs::create_dir_all(&captures_dir_string)?;

    while let Some(datafeed) = rx.recv().await {
        let span = span!(
            Level::DEBUG,
            "process datafeed",
            update = datafeed.general.update
        );
        let _enter = span.enter();

        let captured_pilots =
            filter_pilots_by_distance_and_field(datafeed.pilots, airports, CAPTURE_RANGE_NM);

        let filename = format!("{captures_dir_string}/{}.json", datafeed.general.update);

        let mut file = match File::create(&filename) {
            Ok(file) => file,
            Err(e) => {
                warn!(error = ?e, update = datafeed.general.update, "Could not create file for capture");
                continue;
            }
        };

        let json_string = match serde_json::to_string(&captured_pilots) {
            Ok(json_string) => json_string,
            Err(e) => {
                warn!(error = ?e, update = datafeed.general.update, "Could not serialize json for capture");
                continue;
            }
        };

        if let Err(e) = file.write_all(&json_string.into_bytes()) {
            warn!("Could not write to file: {}", e);
        }

        debug!(
            timestamp = datafeed.general.update,
            "Finished processing datafeed"
        );
    }

    Ok(())
}

#[tracing::instrument]
fn combine_captures(config: &EventConfig, airports: &[&Airport]) -> Result<(), Error> {
    let mut all_snapshots = HashMap::new();
    let mut min_key = None;
    let mut max_key = None;
    let event_slug = event_slug(config);

    let captures_dir_string = format!("./{}/captures", &event_slug);
    for path in WalkDir::new(&captures_dir_string)
        .min_depth(1)
        .max_depth(1)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
        .map(walkdir::DirEntry::into_path)
    {
        let file = File::open(&path)?;
        let pilots: Vec<Pilot> = serde_json::from_reader(file)?;
        let update = path
            .file_stem()
            .ok_or_else(|| anyhow!("could not extract file stem for {path:?}"))?
            .to_str()
            .ok_or_else(|| anyhow!("could not convert OsStr to Str for {path:?}"))?;

        if min_key
            .as_ref()
            .is_none_or(|min: &String| update < min.as_str())
        {
            min_key = Some(update.to_owned());
        }

        if max_key
            .as_ref()
            .is_none_or(|max: &String| update > max.as_str())
        {
            max_key = Some(update.to_owned());
        }

        let features = pilots.into_iter().map(|pilot| {
            let geometry = Some(Value::Point(vec![pilot.longitude, pilot.latitude]).into());
            let id = Some(Id::Number(pilot.cid.into()));

            let mut properties = JsonObject::new();
            properties.insert(
                "data".to_owned(),
                serde_json::to_value(PilotData::from(pilot)).expect("could not serialize Pilot"),
            );

            Feature {
                bbox: None,
                geometry,
                id,
                properties: Some(properties),
                foreign_members: None,
            }
        });

        let collection = FeatureCollection {
            bbox: None,
            features: features.collect::<Vec<_>>(),
            foreign_members: None,
        };

        all_snapshots.insert(update.to_owned(), collection);
    }

    let centroid = calculate_centroid(airports);
    let captures_string = serde_json::to_string(&all_snapshots)?;
    let captures_len = captures_string.len();
    drop(captures_dir_string);

    let capture = EventCapture {
        config: config.clone(),
        first_timestamp_key: min_key,
        last_timestamp_key: max_key,
        captures: all_snapshots,
        captures_length_bytes: captures_len,
        viewport_center: centroid,
    };

    let output_file_string = format!("./{}/{}.json", &event_slug, &event_slug);
    let mut file = File::create(&output_file_string)?;
    file.write_all(&serde_json::to_string(&capture)?.into_bytes())?;

    info!("Completed combining all datafeed captures");
    Ok(())
}

fn event_slug(event: &EventConfig) -> String {
    format!(
        "{}-{:02}-{:02}-{}",
        event.advertised_start_time.year(),
        event.advertised_start_time.month(),
        event.advertised_start_time.day(),
        slugify(&event.name)
    )
}

fn calculate_centroid(airports: &[&Airport]) -> Point<f64> {
    let mut sum_x = 0.0;
    let mut sum_y = 0.0;
    let len = airports.len() as f64;
    for airport in airports {
        sum_x += airport.point.x();
        sum_y += airport.point.y();
    }

    Point::new(sum_x / len, sum_y / len)
}

#[allow(dead_code)]
fn load_artcc_polygons() -> Result<HashMap<String, Polygon>, geojson::Error> {
    let geojson_str = fs::read_to_string("./src/artccs.json").expect("Could not read artccs.json");
    let geojson = geojson_str
        .parse::<GeoJson>()
        .expect("Could not parse artccs.json");
    let collection = FeatureCollection::try_from(geojson)?;

    let mut boundaries = HashMap::new();
    for feature in collection.features {
        let id = feature
            .properties
            .clone()
            .expect("Missing properties")
            .get("id")
            .expect("Missing id")
            .as_str()
            .expect("Unable to parse id to &str")
            .to_owned();
        let poly = Polygon::<f64>::try_from(feature)?;
        boundaries.insert(id, poly);
    }

    Ok(boundaries)
}

#[allow(dead_code)]
fn combine_artccs(
    boundaries: &HashMap<String, Polygon<f64>>,
    artccs: &[&str],
) -> Result<MultiPolygon<f64>, anyhow::Error> {
    let mut polygons = vec![];
    for artcc in artccs {
        let Some(poly) = boundaries.get(&(*artcc).to_owned()) else {
            bail!("invalid ARTCC id: {artcc}")
        };
        polygons.push(poly.clone());
    }
    Ok(unary_union(&polygons))
}

#[allow(dead_code)]
fn artccs_bounding_rect(
    boundaries: &HashMap<String, Polygon<f64>>,
    artccs: &[&str],
) -> Result<Rect<f64>, anyhow::Error> {
    let combined_poly = combine_artccs(boundaries, artccs)?;
    combined_poly
        .bounding_rect()
        .ok_or_else(|| anyhow!("no bounding rectangle"))
}
