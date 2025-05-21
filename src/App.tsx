import { useState, useEffect, useMemo, type FormEvent } from "react";
import DeckGL from "@deck.gl/react";
import { GeoJsonLayer, IconLayer, ScatterplotLayer, TextLayer } from "@deck.gl/layers";
import { Map } from "react-map-gl/mapbox";
//import trafficData from "./consolidated3.json";
import artccs from "./artccs.json";
import "mapbox-gl/dist/mapbox-gl.css";
import type { FeatureCollection, Point } from "geojson";
import { Slider } from "radix-ui";
import type { Feature } from "geojson";
import type { FlightPlan } from "./types/vatsim-capture.ts";
import { getAircraftIcon } from "./utils/icons.ts";
import { PlusIcon, X } from "lucide-react";
import { StyledCheckbox } from "./components/ui-core/Checkbox.tsx";
import type { CheckedState } from "./components/ui-core/Checkbox.tsx";

// Replace with your Mapbox access token
const MAPBOX_ACCESS_TOKEN =
  "pk.eyJ1Ijoia2VuZ3JlaW0iLCJhIjoiY2x3aDBucGZ5MGI3bjJxc2EyNzNuODAyMyJ9.20EFStYOA8-EvOu4tsCkGg";

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

function App() {
  const [viewport] = useState({
    longitude: -122.4,
    latitude: 37.8,
    zoom: 11,
    pitch: 0,
    bearing: 0,
  });

  const [trafficData, setTrafficData] = useState<TrafficData>({});

  const [timestamps, setTimestamps] = useState<string[]>([]);
  const [sliderIndex, setSliderIndex] = useState(0);

  const [routeFilters, setRouteFilters] = useState<{ arrival: string; departure: string }[]>([]);

  const [newArrivalAirport, setNewArrivalAirport] = useState("");
  const [newDepartureAirport, setNewDepartureAirport] = useState("");

  // Label toggles
  const [callsign, setCallsign] = useState<CheckedState>(true);
  const [speed, setSpeed] = useState<CheckedState>(false);
  const [altitude, setAltitude] = useState<CheckedState>(false);
  const [destination, setDestination] = useState<CheckedState>(false);

  // Groundspeed filter
  const [hideSlowAircraft, setHideSlowAircraft] = useState<CheckedState>(false);

  // Aircraft rings
  const [rings, setRings] = useState<CheckedState>(false);
  const [ringsDistance, setRingsDistance] = useState(3);
  const showRings = useMemo(() => {
    if (rings === "indeterminate") {
      return false;
    }
    return rings;
  }, [rings]);

  const newRouteFilter = useMemo(
    () => ({
      arrival: newArrivalAirport.toUpperCase(),
      departure: newDepartureAirport.toUpperCase(),
    }),
    [newArrivalAirport, newDepartureAirport],
  );

  useEffect(() => {
    // get data
    const fetchData = async () => {
      const response = await fetch(
        "https://pub-22f92118e0b54022a686301adc9b496e.r2.dev/consolidated3.json",
      );
      if (response.ok) {
        const data = (await response.json()) as TrafficData;
        setTrafficData(data);

        // Extract timestamps from the data
        const timestamps = Object.keys(data).sort();
        setTimestamps(timestamps);
      }
    };

    fetchData().catch(console.error);
  }, []);

  const currentData = useMemo(
    () => (trafficData as TrafficData)[timestamps[sliderIndex]],
    [sliderIndex, timestamps],
  );

  const filteredData: FeatureCollection = useMemo(() => {
    if (!currentData) return currentData;

    const filteredFeatures = currentData.features.filter((feature) => {
      if (feature.properties?.data?.groundspeed) {
        if (
          hideSlowAircraft &&
          (feature.properties.data.groundspeed === 0 || feature.properties.data.groundspeed < 30)
        ) {
          return false;
        }
      } else {
        return false;
      }

      if (!feature.properties?.data?.flight_plan) {
        return false;
      }

      const flightPlan = feature.properties.data.flight_plan;

      if (routeFilters.length === 0) {
        return true;
      }

      for (const filter of routeFilters) {
        if (
          (filter.arrival === "*" || filter.arrival === flightPlan.arrival) &&
          (filter.departure === "*" || filter.departure === flightPlan.departure)
        ) {
          return true;
        }
      }

      return false;
    });

    return {
      type: "FeatureCollection",
      features: filteredFeatures,
    };
  }, [currentData, routeFilters, hideSlowAircraft]);

  const timestampString = useMemo(() => {
    if (timestamps !== undefined && sliderIndex !== undefined && timestamps.length > 0) {
      const s = timestamps[sliderIndex];
      return `${s.substring(8, 10)}:${s.substring(10, 12)}:${s.substring(12, 14)}`;
    } else {
      return "";
    }
  }, [timestamps, sliderIndex]);

  const handleAddRouteFilter = (e: FormEvent) => {
    e.preventDefault();

    if (newArrivalAirport && newDepartureAirport && !routeFilters.includes(newRouteFilter))
      setRouteFilters([...routeFilters, newRouteFilter]);
  };

  const handleRemoveRouteFilter = (route: { arrival: string; departure: string }) => {
    setRouteFilters(routeFilters.filter((r) => r != route));
  };

  //
  // const layers = [
  //   new GeoJsonLayer({
  //     id: "geojson-layer",
  //     data: filteredData,
  //     pickable: true,
  //     stroked: false,
  //     filled: true,
  //     extruded: true,
  //     pointType: "circle+text",
  //     lineWidthScale: 20,
  //     lineWidthMinPixels: 2,
  //     getFillColor: [255, 0, 0],
  //     getLineColor: [0, 0, 0],
  //     getPointRadius: 50,
  //     pointRadiusMinPixels: 2,
  //     getLineWidth: 1,
  //     getElevation: 30,
  //     getText: (f: Feature<Geometry, PilotProperties>) => f.properties.data.callsign,
  //     getTextSize: 12,
  //     getTextPixelOffset: [0, 15],
  //     getTextColor: [0, 0, 0],
  //   }),];

  const layers = [
    new IconLayer({
      id: "aircraft-layer",
      data: filteredData?.features ?? [],
      pickable: true,
      iconAtlas: "/atlas.png",
      iconMapping: "/iconMapping.json",
      getIcon: (d: Feature<Point, PilotProperties>) => {
        const aircraftType = d.properties.data.flight_plan?.aircraft_short?.toLowerCase();
        return getAircraftIcon(aircraftType).icon;
      },
      getPosition: (d: Feature<Point, PilotProperties>) => [
        d.properties.data.longitude,
        d.properties.data.latitude,
      ],
      getColor: [255, 0, 0],
      billboard: false,
      getAngle: (d: Feature<Point, PilotProperties>) =>
        Math.max(360 - d.properties.data.heading, 0),
      sizeMinPixels: 70,
      sizeMaxPixels: 150,
    }),
    new ScatterplotLayer({
      id: "aircraft-dot-layer",
      data: filteredData?.features ?? [],
      pickable: false,
      getPosition: (d: Feature<Point, PilotProperties>) => [
        d.properties.data.longitude,
        d.properties.data.latitude,
      ],
      billboard: false,
      stroked: true,
      filled: false,
      getLineColor: [100, 100, 100],
      getRadius: ringsDistance * 1852,
      radiusUnits: "meters",
      lineWidthMinPixels: 1,
      visible: showRings,
      updateTriggers: { getRadius: [ringsDistance] },
    }),
    new TextLayer({
      id: "heading-layer",
      data: filteredData?.features ?? [],
      pickable: false,
      getPosition: (d: Feature<Point, PilotProperties>) => [
        d.properties.data.longitude,
        d.properties.data.latitude,
      ],
      getText: (d: Feature<Point, PilotProperties>) => {
        const lines = [];
        if (callsign) {
          lines.push(d.properties.data.callsign);
        }
        if (speed) {
          lines.push(`${d.properties.data.groundspeed}kts`);
        }
        if (altitude) {
          lines.push(`${d.properties.data.altitude}ft`);
        }
        if (destination && d.properties.data.flight_plan) {
          lines.push(d.properties.data.flight_plan.arrival);
        }

        return lines.join("\n");
      },
      getSize: 12,
      getColor: [0, 0, 0],
      getAlignmentBaseline: "top",
      getPixelOffset: (d: Feature<Point, PilotProperties>) => {
        const aircraftType = d.properties.data.flight_plan?.aircraft_short?.toLowerCase();
        return [0, Math.round(getAircraftIcon(aircraftType).width)];
      },
      updateTriggers: { getText: [callsign, speed, altitude, destination] },
    }),
    new GeoJsonLayer({
      id: "boundaries",
      data: artccs as unknown as FeatureCollection,
      stroked: true,
      filled: false,
      getLineColor: [0, 0, 0],
      getLineWidth: 5,
      lineWidthMinPixels: 2,
    }),
  ];

  return (
    <div className="flex min-h-dvh min-w-dvw font-manrope">
      {/* Sidebar */}
      <div className="bg-slate-900 p-6 overflow-y-auto overscroll-contain z-10 shadow-md text-white flex flex-col space-y-5">
        <h1 className="text-2xl font-bold">Traffic Replay</h1>

        <div className="flex flex-col space-y-2">
          <div>
            <div className="mb-2">
              <h2 className="text-xl">Label Displays</h2>
            </div>
            <div className="border rounded border-slate-600 p-2 flex flex-col space-y-2">
              <StyledCheckbox
                label="Callsign"
                checked={callsign}
                onCheckedChange={(checked) => setCallsign(checked)}
              />
              <StyledCheckbox
                label="Speed"
                checked={speed}
                onCheckedChange={(checked) => setSpeed(checked)}
              />
              <StyledCheckbox
                label="Altitude"
                checked={altitude}
                onCheckedChange={(checked) => setAltitude(checked)}
              />
              <StyledCheckbox
                label="Destination"
                checked={destination}
                onCheckedChange={(checked) => setDestination(checked)}
              />
            </div>
          </div>
        </div>

        <div className="flex flex-col space-y-2">
          <div>
            <div className="mb-2">
              <h2 className="text-xl">Aircraft Rings</h2>
            </div>
            <div className="border rounded border-slate-600 p-2 flex flex-col space-y-2">
              <StyledCheckbox
                label="Show rings"
                checked={rings}
                onCheckedChange={(checked) => setRings(checked)}
              />
              {rings && (
                <div className="flex space-x-3 items-center">
                  <input
                    type="number"
                    min={0}
                    max={50}
                    step={0.5}
                    value={ringsDistance}
                    onChange={(e) => setRingsDistance(parseFloat(e.target.value))}
                    placeholder="3"
                    className="p-1 font-mono w-18 uppercase border border-neutral-600 rounded-sm focus:bg-slate-700 focus:outline-1 focus:outline-white"
                  />
                  <p>Radius (nm)</p>
                </div>
              )}
            </div>
          </div>
        </div>

        <div className="flex flex-col space-y-2">
          <div>
            <div className="mb-2">
              <h2 className="text-xl">Ground Filters</h2>
            </div>
            <div className="border rounded border-slate-600 p-2 flex flex-col space-y-2">
              <StyledCheckbox
                label="Hide aircraft < 30kts"
                checked={hideSlowAircraft}
                onCheckedChange={(checked) => setHideSlowAircraft(checked)}
              />
            </div>
          </div>
        </div>

        <div className="flex flex-col space-y-2">
          <div>
            <div className="mb-2">
              <h2 className="text-xl">Route Filters</h2>
            </div>
            <div className="border rounded border-slate-600 p-2">
              <form
                className="flex space-x-4 items-end"
                onSubmit={handleAddRouteFilter}
                style={{ marginBottom: "10px" }}
              >
                <div className="flex flex-col space-y-2">
                  <label className="text-sm text-neutral-300">Departure</label>
                  <input
                    type="text"
                    value={newDepartureAirport}
                    onChange={(e) => setNewDepartureAirport(e.target.value)}
                    placeholder="ICAO"
                    maxLength={4}
                    className="p-1 font-mono w-18 uppercase border border-neutral-600 rounded-sm focus:bg-slate-700 focus:outline-1 focus:outline-white"
                  />
                </div>
                <div className="flex flex-col space-y-2">
                  <label className="text-sm text-neutral-300">Arrival</label>
                  <input
                    type="text"
                    value={newArrivalAirport}
                    onChange={(e) => setNewArrivalAirport(e.target.value)}
                    placeholder="ICAO"
                    maxLength={4}
                    className="p-1 font-mono w-18 uppercase border border-neutral-600 rounded-sm focus:bg-slate-700 focus:outline-1 focus:outline-white"
                  />
                </div>

                <button
                  type="submit"
                  className="p-1 rounded cursor-pointer bg-sky-600 hover:bg-sky-500 transition-colors w-8 h-8 flex items-center"
                >
                  <PlusIcon />
                </button>
              </form>
              <div className="italic text-sm text-neutral-300">
                Use * as a wildcard for any airport
              </div>
              <div className="flex flex-col space-y-2 mt-4">
                {routeFilters.map((route) => (
                  <div
                    key={`${route.departure}-${route.arrival}`}
                    className="bg-sky-600 items-center rounded py-1 px-2 font-mono w-40 flex"
                  >
                    <p className="grow flex space-x-1">
                      <span className="w-9">{route.departure}</span>
                      <span>-</span>
                      <span className="w-9">{route.arrival}</span>
                    </p>
                    <button
                      onClick={() => handleRemoveRouteFilter(route)}
                      className="cursor-pointer pr-1"
                    >
                      <X size={14} />
                    </button>
                  </div>
                ))}
              </div>
            </div>
          </div>
        </div>
      </div>

      {/* Map */}
      <div style={{ flex: 1, position: "relative" }}>
        <DeckGL initialViewState={viewport} controller={true} layers={layers}>
          <Map
            mapboxAccessToken={MAPBOX_ACCESS_TOKEN}
            mapStyle="mapbox://styles/mapbox/light-v11"
          />
        </DeckGL>

        <div
          style={{
            position: "absolute",
            bottom: "20px",
            left: "50%",
            transform: "translateX(-50%)",
            width: "80%",
            padding: "20px",
            backgroundColor: "rgba(255, 255, 255, 0.9)",
            borderRadius: "8px",
            boxShadow: "0 2px 4px rgba(0,0,0,0.2)",
          }}
        >
          <form className="px-4">
            <Slider.Root
              className="relative flex h-5 w-full touch-none select-none items-center pt-7"
              defaultValue={[0]}
              min={0}
              max={timestamps.length - 1}
              step={1}
              value={[sliderIndex]}
              onValueChange={(v) => setSliderIndex(v[0])}
            >
              <Slider.Track className="relative h-[3px] grow rounded-full bg-neutral-300">
                <Slider.Range className="absolute h-full rounded-full bg-slate-700" />
              </Slider.Track>
              <Slider.Thumb
                className="block size-5 rounded-[10px] bg-white shadow-[0_2px_10px] shadow-black hover:bg-sky-600 transition-colors focus:shadow-[0_0_0_5px] focus:shadow-black focus:outline-none focus:bg-sky-600"
                aria-label="Volume"
              >
                <div className="relative -top-8 -left-6 font-mono">{timestampString}</div>
              </Slider.Thumb>
            </Slider.Root>
          </form>
        </div>
      </div>
    </div>
  );
}

export default App;
