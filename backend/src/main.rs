use chrono::{DateTime, Utc};
use geo::{Contains, LineString, Polygon, point};
use geojson::feature::Id;
use geojson::{Feature, FeatureCollection, JsonObject, Value};
use serde::{Deserialize, Serialize};
use std::cmp::min;
use std::collections::HashMap;
use std::fs::File;
use std::io::Write;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tokio::sync::mpsc::Sender;
use tokio::time::sleep;
use tracing::dispatcher::SetGlobalDefaultError;
use tracing::{debug, error, info, warn};
use vatsim_utils::live_api::Vatsim;
use vatsim_utils::models::V3ResponseData;
use walkdir::WalkDir;

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

impl From<vatsim_utils::models::Pilot> for PilotData {
    fn from(value: vatsim_utils::models::Pilot) -> Self {
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
            flight_plan: value.flight_plan.map_or(None, |f| Some(f.into())),
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

#[tokio::main]
async fn main() -> Result<(), SetGlobalDefaultError> {
    let subscriber = tracing_subscriber::fmt()
        .compact()
        .json()
        .with_file(true)
        .with_line_number(true)
        .finish();

    tracing::subscriber::set_global_default(subscriber)?;

    let mut all_snapshots = HashMap::new();

    for path in WalkDir::new("./src/captures")
        .min_depth(1)
        .max_depth(1)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .map(|e| e.into_path())
    {
        if let Ok(file) = File::open(&path) {
            let pilots: Vec<vatsim_utils::models::Pilot> = serde_json::from_reader(file).unwrap();
            let update = path.file_stem().unwrap().to_str().unwrap();

            let features = pilots.into_iter().map(|pilot| {
                let geometry = Some(Value::Point(vec![pilot.longitude, pilot.latitude]).into());
                let id = Some(Id::Number(pilot.cid.into()));

                let mut properties = JsonObject::new();
                properties.insert(
                    "data".to_string(),
                    serde_json::to_value(PilotData::from(pilot))
                        .expect("could not serialize Pilot")
                        .into(),
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

            // let snapshot = pilots
            //     .into_iter()
            //     .map(|p| PilotSnapshot::from((update.to_string(), p)))
            //     .collect::<Vec<_>>();
            all_snapshots.insert(update.to_string(), collection);
        }
    }

    let mut file = File::create("./src/consolidated3.json").expect("Could not create file");
    if let Err(e) = file.write_all(
        &serde_json::to_string(&all_snapshots)
            .expect("could not serialize")
            .into_bytes(),
    ) {
        warn!("Could not write to file: {}", e);
    }

    // // Set up VATSIM Datafeed
    // let api = Vatsim::new()
    //     .await
    //     .expect("Could not initialize VATSIM API");
    //
    // let (tx, mut rx) = mpsc::channel(32);
    // let end_time = Utc::now() + Duration::from_secs(60 * 60 * 4);
    //
    // tokio::spawn(async move { datafeed_loop(api, tx, end_time).await });
    //
    // let bounding_coords = vec![
    //     (-127.3917173, 42.7792754),
    //     (-124.9738367, 30.9776091),
    //     (-111.6961128, 32.1756125),
    //     (-118.6424886, 43.1009829),
    //     (-127.3917173, 42.7792754),
    // ];
    //
    // let bounding_poly = Polygon::new(LineString::from(bounding_coords), vec![]);
    //
    // println!("Starting datafeed processor");
    // while let Some(mut datafeed) = rx.recv().await {
    //     let captured_pilots = datafeed
    //         .pilots
    //         .retain(|p| bounding_poly.contains(&point! { x: p.longitude, y: p.latitude }));
    //
    //     let filename = format!("./src/captures/{}.json", datafeed.general.update);
    //     let mut file = File::create(&filename).expect("Could not create file");
    //     if let Err(e) = file.write_all(
    //         &serde_json::to_string(&captured_pilots)
    //             .expect("could not serialize")
    //             .into_bytes(),
    //     ) {
    //         warn!("Could not write to file: {}", e);
    //     }
    //
    //     println!("{:?}", datafeed.general.update);
    // }

    Ok(())
}

// async fn datafeed_processor(mut rx: Receiver<V3ResponseData>) {
//     println!("Starting datafeed processor");
//     while let Some(datafeed) = rx.recv().await {
//         println!("{:?}", datafeed.general.update);
//     }
// }

async fn datafeed_loop(api: Vatsim, tx: Sender<V3ResponseData>, end_time: DateTime<Utc>) {
    let mut last_datafeed_update = String::new();

    loop {
        let start = Instant::now();

        // Get data and check that there was no error
        let latest_data_result = api.get_v3_data().await;
        if let Err(e) = latest_data_result {
            warn!(error = ?e, "Could not fetch VATSIM data");
            sleep(Duration::from_secs(1)).await;
            continue;
        };

        // Unwrap and check if duplicate from last fetch
        // Safe to unwrap because checked Err case above already
        let latest_data = latest_data_result.expect("Error getting VATSIM API data");

        if latest_data.general.update == last_datafeed_update {
            debug!(time = %latest_data.general.update, "Found duplicate");
            sleep(Duration::from_secs(1)).await;
            continue;
        }

        // Update timestamp of latest data and process datafeed
        last_datafeed_update = latest_data.general.update.clone();

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

        if update_timestamp > end_time {
            info!("Ending datafeed collection");
            return;
        }

        if let Err(e) = tx.send(latest_data).await {
            println!("Error sending datafeed: {}", e);
            error!(error = ?e, "Error sending datafeed through mpsc. Ending datafeed loop");
            return;
        }

        // Sleep for 5 seconds minus the time this loop took, with some protections to make sure we
        // don't have a negative duration
        let loop_time = Instant::now() - start;
        if loop_time > Duration::from_secs(4) {
            warn!(?loop_time, "Long loop");
        }
        let sleep_duration = Duration::from_secs(5) - min(Duration::from_secs(4), loop_time);
        debug!(?sleep_duration, "Sleeping");
        sleep(sleep_duration).await;
    }
}
