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

interface EventConfig {
  name: string;
  artccs: string[];
  airports: string[];
  advertised_start_time: string;
  advertised_end_time: string;
}

// Original format (legacy)
interface EventCapture {
  config: EventConfig;
  first_timestamp_key: string;
  last_timestamp_key: string;
  captures: TrafficData;
  captures_length_bytes: number;
  viewport_center: { x: number; y: number };
}

// Optimized format with deduplicated flight plans
interface OptimizedEventCapture {
  config: EventConfig;
  first_timestamp_key: string;
  last_timestamp_key: string;
  pilots: { [cid: string]: PilotStatic };
  flightPlans: { [key: string]: FlightPlan };
  frames: TrafficData;
  captures_length_bytes: number;
  viewport_center: { x: number; y: number };
}

interface PilotStatic {
  name: string;
  callsign: string;
}

// Dynamic pilot data in optimized format (has fp reference instead of flight_plan)
interface PilotDynamic {
  cid: number;
  latitude: number;
  longitude: number;
  altitude: number;
  groundspeed: number;
  transponder: string;
  heading: number;
  fp?: string;
  logon_time: string;
  last_updated: string;
}

type EventsMetadata = { event: EventConfig; url: string; unlisted?: boolean }[];

interface TrafficData {
  [key: string]: FeatureCollection;
}

interface PilotProperties {
  data: PilotData;
}

interface PilotData {
  cid: number;
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

// Type guard to check if response is optimized format
function isOptimizedFormat(data: EventCapture | OptimizedEventCapture): data is OptimizedEventCapture {
  return "frames" in data && "flightPlans" in data && "pilots" in data;
}

export type {
  FlightPlan,
  EventCapture,
  OptimizedEventCapture,
  EventConfig,
  EventsMetadata,
  TrafficData,
  PilotProperties,
  PilotData,
  PilotStatic,
  PilotDynamic,
};

export { isOptimizedFormat };
