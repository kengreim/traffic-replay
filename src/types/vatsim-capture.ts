import type { FeatureCollection } from "geojson";

interface FlightPlan {
  flight_rules: string;
  aircraft: string;
  aircraft_faa: string;
  aircraft_short: string;
  departure: string;
  arrival: string;
  alternate: string;
  altitude: string;
  route: string;
  revision_id: number;
}

interface EventCapture {
  config: EventConfig;
  first_timestamp_key: string;
  last_timestamp_key: string;
  captures: TrafficData;
  captures_length_bytes: number;
  viewport_center: { x: number; y: number };
}

interface EventConfig {
  name: string;
  artccs: string[];
  airports: string[];
  advertised_start_time: string;
  advertised_end_time: string;
}

type EventsMetadata = { event: EventConfig; url: string }[];

interface TrafficData {
  [key: string]: FeatureCollection;
}

interface PilotProperties {
  data: PilotData;
}

interface PilotData {
  name: string;
  callsign: string;
  latitude: number;
  longitude: number;
  altitude: number;
  groundspeed: number;
  transponder: string;
  heading: number;
  flight_plan?: FlightPlan;
  logon_time: string;
  last_updated: string;
}

export type {
  FlightPlan,
  EventCapture,
  EventConfig,
  EventsMetadata,
  TrafficData,
  PilotProperties,
  PilotData,
};
