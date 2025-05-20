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

export type { FlightPlan };
