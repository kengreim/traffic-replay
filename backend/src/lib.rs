#![warn(clippy::all, clippy::pedantic, clippy::nursery)]

use anyhow::{Context, anyhow, bail};
use chrono::{DateTime, Datelike, Utc};
use geo::{BoundingRect, Distance, Haversine, MultiPolygon, Point, Polygon, Rect, unary_union};
use geojson::feature::Id;
use geojson::{Feature, FeatureCollection, GeoJson, JsonObject, Value};
use serde::{Deserialize, Serialize};
use slug::slugify;
use std::collections::HashMap;
use std::fs;
use std::fs::File;
use std::path::PathBuf;
use tracing::{debug, instrument, warn};
use vatsim_utils::models::Pilot;

pub const NM_TO_METERS: f64 = 1852.0;
pub const EVENT_PRE_TIME_MINUTES: u64 = 5;
pub const EVENT_POST_TIME_MINUTES: u64 = 5;
pub const CAPTURE_RANGE_NM: u16 = 600;

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

/// Static pilot data that doesn't change during a flight (used in optimized format)
#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct PilotStatic {
    pub name: String,
    pub callsign: String,
}

/// Dynamic pilot data per frame with flight plan reference (used in optimized format)
#[derive(Deserialize, Serialize)]
pub struct PilotDynamic {
    pub cid: u64,
    pub latitude: f64,
    pub longitude: f64,
    pub altitude: i64,
    pub groundspeed: i64,
    pub transponder: String,
    pub heading: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fp: Option<String>,
    pub logon_time: String,
    pub last_updated: String,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
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
pub struct CustomAirport {
    pub latitude: f64,
    pub longitude: f64,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct EventConfig {
    pub name: String,
    pub artccs: Vec<String>,
    pub airports: Vec<String>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub custom_airports: HashMap<String, CustomAirport>,
    pub advertised_start_time: DateTime<Utc>,
    pub advertised_end_time: DateTime<Utc>,
}

#[derive(Serialize, Debug, Clone)]
pub struct EventCapture {
    pub config: EventConfig,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub first_timestamp_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_timestamp_key: Option<String>,
    pub captures: HashMap<String, FeatureCollection>,
    pub captures_length_bytes: usize,
    pub viewport_center: Point<f64>,
}

/// Optimized event capture format with deduplicated flight plans and static pilot data
#[derive(Serialize, Debug)]
pub struct OptimizedEventCapture {
    pub config: EventConfig,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub first_timestamp_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_timestamp_key: Option<String>,
    /// Static pilot data (name, callsign) keyed by CID
    pub pilots: HashMap<String, PilotStatic>,
    /// Deduplicated flight plans keyed by "{callsign}_{cid}_{revision_id}"
    #[serde(rename = "flightPlans")]
    pub flight_plans: HashMap<String, FlightPlan>,
    /// GeoJSON FeatureCollections per timestamp with flight plan references
    pub frames: HashMap<String, FeatureCollection>,
    pub captures_length_bytes: usize,
    pub viewport_center: Point<f64>,
}

#[derive(Debug, serde::Deserialize)]
pub struct AirportRecord {
    #[serde(rename = "ARPT_ID")]
    pub faa_id: String,
    #[serde(rename = "ICAO_ID")]
    pub icao_id: String,
    #[serde(rename = "LAT_DECIMAL")]
    pub latitude: f64,
    #[serde(rename = "LONG_DECIMAL")]
    pub longitude: f64,
}

#[derive(Debug)]
pub struct Airport {
    #[allow(dead_code)]
    pub faa_id: Option<String>,
    pub icao_id: String,
    pub point: Point,
}

impl From<AirportRecord> for Airport {
    fn from(record: AirportRecord) -> Self {
        Self {
            faa_id: Some(record.faa_id),
            icao_id: record.icao_id,
            point: Point::new(record.longitude, record.latitude),
        }
    }
}

pub type IcaoId = String;

#[instrument]
pub fn load_airports() -> Result<HashMap<IcaoId, Airport>, anyhow::Error> {
    debug!("loading airports from file");
    let mut airports = HashMap::new();

    let csv_path = std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(|dir| dir.join("APT_BASE.csv")))
        .unwrap_or_else(|| PathBuf::from("./APT_BASE.csv"));
    let csv_file = match File::open(&csv_path) {
        Ok(file) => file,
        Err(err) => {
            warn!(
                error = ?err,
                path = %csv_path.display(),
                "failed to open airports CSV relative to executable, falling back to ./APT_BASE.csv"
            );
            File::open("./APT_BASE.csv")
                .with_context(|| "failed to open airports CSV at ./APT_BASE.csv")?
        }
    };
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

#[must_use]
pub fn filter_pilots_by_distance_and_field(
    mut pilots: Vec<Pilot>,
    config_airports: &[(String, Option<&Airport>)],
    distance_nm: u16,
) -> Vec<Pilot> {
    pilots.retain(|p| {
        let mut airports_with_data = config_airports
            .iter()
            .filter_map(|(_, airport_data)| *airport_data);

        p.flight_plan.as_ref().is_some_and(|fp| {
            config_airports
                .iter()
                .any(|apt| apt.0 == fp.departure || apt.0 == fp.arrival)
        }) || airports_with_data.any(|apt| {
            Haversine.distance(apt.point, Point::new(p.longitude, p.latitude))
                < (f64::from(distance_nm) * NM_TO_METERS)
        })
    });

    pilots
}

#[must_use]
pub fn event_slug(event: &EventConfig) -> String {
    format!(
        "{}-{:02}-{:02}-{}",
        event.advertised_start_time.year(),
        event.advertised_start_time.month(),
        event.advertised_start_time.day(),
        slugify(&event.name)
    )
}

#[must_use]
pub fn calculate_centroid(airports: &[(String, Option<&Airport>)]) -> Point<f64> {
    let airports_with_data_only = airports
        .iter()
        .filter_map(|(_, airport)| airport.to_owned())
        .collect::<Vec<_>>();
    if airports_with_data_only.is_empty() {
        warn!("no airports with coordinates; using CONUS centroid fallback");
        return Point::new(-98.583333, 39.833333);
    }
    let mut sum_x = 0.0;
    let mut sum_y = 0.0;
    let len = airports_with_data_only.len() as f64;
    for airport in airports_with_data_only {
        sum_x += airport.point.x();
        sum_y += airport.point.y();
    }

    Point::new(sum_x / len, sum_y / len)
}

/// Convert a list of pilots into a `GeoJSON` `FeatureCollection`.
#[must_use]
pub fn pilots_to_feature_collection(pilots: Vec<Pilot>) -> FeatureCollection {
    let features = pilots
        .into_iter()
        .map(|pilot| {
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
        })
        .collect::<Vec<_>>();

    FeatureCollection {
        bbox: None,
        features,
        foreign_members: None,
    }
}

/// Convert pilots to optimized format, updating shared lookups for static data and flight plans.
/// Returns a FeatureCollection with flight plan references instead of inline data.
#[must_use]
pub fn pilots_to_optimized_feature_collection(
    pilots: Vec<Pilot>,
    pilots_static: &mut HashMap<String, PilotStatic>,
    flight_plans: &mut HashMap<String, FlightPlan>,
) -> FeatureCollection {
    let features = pilots
        .into_iter()
        .map(|pilot| {
            let cid_str = pilot.cid.to_string();

            // Add static pilot data if not already present
            pilots_static.entry(cid_str.clone()).or_insert_with(|| PilotStatic {
                name: pilot.name.clone(),
                callsign: pilot.callsign.clone(),
            });

            // Build flight plan reference and add to lookup if present
            let fp_ref = pilot.flight_plan.as_ref().map(|fp| {
                let key = format!("{}_{}_{}", pilot.callsign, pilot.cid, fp.revision_id);
                flight_plans
                    .entry(key.clone())
                    .or_insert_with(|| FlightPlan::from(fp.clone()));
                key
            });

            let geometry = Some(Value::Point(vec![pilot.longitude, pilot.latitude]).into());
            let id = Some(Id::Number(pilot.cid.into()));

            let dynamic_data = PilotDynamic {
                cid: pilot.cid,
                latitude: pilot.latitude,
                longitude: pilot.longitude,
                altitude: pilot.altitude,
                groundspeed: pilot.groundspeed,
                transponder: pilot.transponder,
                heading: pilot.heading,
                fp: fp_ref,
                logon_time: pilot.logon_time,
                last_updated: pilot.last_updated,
            };

            let mut properties = JsonObject::new();
            properties.insert(
                "data".to_owned(),
                serde_json::to_value(dynamic_data).expect("could not serialize PilotDynamic"),
            );

            Feature {
                bbox: None,
                geometry,
                id,
                properties: Some(properties),
                foreign_members: None,
            }
        })
        .collect::<Vec<_>>();

    FeatureCollection {
        bbox: None,
        features,
        foreign_members: None,
    }
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
