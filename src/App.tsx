import { useState, useEffect, useMemo, type FormEvent, useRef } from "react";
import DeckGL from "@deck.gl/react";
import { GeoJsonLayer, IconLayer, ScatterplotLayer, TextLayer } from "@deck.gl/layers";
import { Map } from "react-map-gl/mapbox";
import artccs from "./artccs.json";
import "mapbox-gl/dist/mapbox-gl.css";
import type { FeatureCollection, Point } from "geojson";
import { Slider } from "radix-ui";
import type { Feature } from "geojson";
import type { FlightPlan } from "./types/vatsim-capture.ts";
import { getAircraftIcon } from "./utils/icons.ts";
import { Play, PlusIcon, StepBack, StepForward, X, Pause } from "lucide-react";
import { StyledCheckbox } from "./components/ui-core/Checkbox.tsx";
import type { CheckedState } from "./components/ui-core/Checkbox.tsx";
import { type MapViewState, FlyToInterpolator } from "@deck.gl/core";

// Replace with your Mapbox access token
const MAPBOX_ACCESS_TOKEN =
  "pk.eyJ1Ijoia2VuZ3JlaW0iLCJhIjoiY2x3aDBucGZ5MGI3bjJxc2EyNzNuODAyMyJ9.20EFStYOA8-EvOu4tsCkGg";

const EVENTS_METADATA_URL = "https://data.vatsim-replay.com/events.json";

const DEFAULT_ZOOM = 7;
const DEFAULT_VIEWPORT = {
  longitude: -98.583333,
  latitude: 39.833333,
  pitch: 0,
  bearing: 0,
  zoom: 4,
};

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

function App() {
  const [viewport, setViewport] = useState<MapViewState>(DEFAULT_VIEWPORT);

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
  const [departure, setDeparture] = useState<CheckedState>(false);
  const [destination, setDestination] = useState<CheckedState>(false);

  // Groundspeed filter
  const [hideSlowAircraft, setHideSlowAircraft] = useState<CheckedState>(false);

  // State for playing
  const [isPlaying, setIsPlaying] = useState(false);
  const [playbackSpeed, setPlaybackSpeed] = useState(1);
  const playIntervalRef = useRef<number | null>(null);
  const pointerDownSuspendedPlay = useRef(false);

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
        "https://data.vatsim-replay.com/2025-05-24-cowboys-spaceships-and-star-spangled-banners.json",
      );
      if (response.ok) {
        const event = (await response.json()) as EventCapture;
        setTrafficData(event.captures);

        // Extract timestamps from the data
        const timestamps = Object.keys(event.captures).sort();
        setTimestamps(timestamps);

        setViewport({
          longitude: event.viewport_center.x,
          latitude: event.viewport_center.y,
          pitch: 0,
          bearing: 0,
          zoom: DEFAULT_ZOOM,
          transitionInterpolator: new FlyToInterpolator({ speed: 2 }),
          transitionDuration: "auto",
        });
      }
    };

    const loadDevData = async () => {
      // @ts-ignore
      const event = (await import(
        "./test-data/2025-05-24-cowboys-spaceships-and-star-spangled-banners.json"
      )) as EventCapture;
      await new Promise((resolve) => setTimeout(resolve, 2000));
      setTrafficData(event.captures);
      const timestamps = Object.keys(event.captures).sort();
      setTimestamps(timestamps);
      console.log(event.captures_length_bytes);
      console.log(event.viewport_center);

      setViewport({
        longitude: event.viewport_center.x,
        latitude: event.viewport_center.y,
        pitch: 0,
        bearing: 0,
        zoom: DEFAULT_ZOOM,
        transitionInterpolator: new FlyToInterpolator({ speed: 2 }),
        transitionDuration: "auto",
      });
    };

    if (import.meta.env.PROD) {
      fetchData().catch(console.error);
    } else {
      loadDevData().then(() => console.log("dev data loaded"));
    }
  }, []);

  const currentData = useMemo(
    () => (trafficData as TrafficData)[timestamps[sliderIndex]],
    [sliderIndex, timestamps],
  );

  const filteredData: FeatureCollection = useMemo(() => {
    if (!currentData) return currentData;

    const filteredFeatures = currentData.features.filter((feature) => {
      if (feature.properties?.data?.groundspeed !== undefined) {
        if (hideSlowAircraft && feature.properties.data.groundspeed < 30) {
          return false;
        }
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
      id: "ring-layer",
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
      id: "label-layer",
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
        if (departure && d.properties.data.flight_plan) {
          lines.push(d.properties.data.flight_plan.departure);
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
      updateTriggers: { getText: [callsign, speed, altitude, destination, departure] },
    }),
    new GeoJsonLayer({
      id: "boundaries-layer",
      data: artccs as unknown as FeatureCollection,
      stroked: true,
      filled: false,
      getLineColor: [0, 0, 0],
      getLineWidth: 5,
      lineWidthMinPixels: 2,
    }),
  ];

  const incrementTimeSlider = () => {
    setSliderIndex((current) => {
      const next = Math.min(current + 1, timestamps.length - 1);
      if (current === next) {
        setIsPlaying(false);
        return current;
      }
      return next;
    });
    return sliderIndex < timestamps.length - 1;
  };

  const decrementTimeSlider = () => {
    setSliderIndex((current) => {
      const next = Math.max(current - 1, 0);
      if (current === next) {
        return current;
      }
      return next;
    });
    return sliderIndex > 0;
  };

  const togglePlayback = () => {
    setIsPlaying(!isPlaying);
  };

  useEffect(() => {
    if (isPlaying) {
      playIntervalRef.current = window.setInterval(() => {
        const hasMore = incrementTimeSlider();
        if (!hasMore) {
          setIsPlaying(false);
        }
      }, 1000 / playbackSpeed);
    } else if (playIntervalRef.current !== null) {
      window.clearInterval(playIntervalRef.current);
      playIntervalRef.current = null;
    }

    return () => {
      if (playIntervalRef.current !== null) {
        window.clearInterval(playIntervalRef.current);
      }
    };
  }, [isPlaying, playbackSpeed]);

  return (
    <div className="min-w-dvw font-manrope flex min-h-dvh">
      {/* Sidebar */}
      <div className="z-10 flex flex-col space-y-5 overflow-y-auto overscroll-contain bg-slate-900 p-6 text-white shadow-md">
        <h1 className="text-2xl font-bold">Traffic Replay</h1>

        <div className="flex flex-col space-y-2">
          <div>
            <div className="mb-2">
              <h2 className="text-xl">Label Displays</h2>
            </div>
            <div className="flex flex-col space-y-2 rounded border border-slate-600 p-2">
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
                label="Departure Airport"
                checked={departure}
                onCheckedChange={(checked) => setDeparture(checked)}
              />
              <StyledCheckbox
                label="Arrival Airport"
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
            <div className="flex flex-col space-y-2 rounded border border-slate-600 p-2">
              <StyledCheckbox
                label="Show rings"
                checked={rings}
                onCheckedChange={(checked) => setRings(checked)}
              />
              {rings && (
                <div className="flex items-center space-x-3">
                  <input
                    type="number"
                    min={0}
                    max={50}
                    step={0.5}
                    value={ringsDistance}
                    onChange={(e) => setRingsDistance(parseFloat(e.target.value))}
                    placeholder="3"
                    className="w-18 rounded-sm border border-neutral-600 p-1 font-mono uppercase focus:bg-slate-700 focus:outline-1 focus:outline-white"
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
            <div className="flex flex-col space-y-2 rounded border border-slate-600 p-2">
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
            <div className="rounded border border-slate-600 p-2">
              <form
                className="flex items-end space-x-4"
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
                    className="w-18 rounded-sm border border-neutral-600 p-1 font-mono uppercase focus:bg-slate-700 focus:outline-1 focus:outline-white"
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
                    className="w-18 rounded-sm border border-neutral-600 p-1 font-mono uppercase focus:bg-slate-700 focus:outline-1 focus:outline-white"
                  />
                </div>

                <button
                  type="submit"
                  className="flex h-8 w-8 cursor-pointer items-center rounded bg-sky-600 p-1 transition-colors hover:bg-sky-500"
                >
                  <PlusIcon />
                </button>
              </form>
              <div className="text-sm italic text-neutral-300">
                Use * as a wildcard for any airport
              </div>
              <div className="mt-4 flex flex-col space-y-2">
                {routeFilters.map((route) => (
                  <div
                    key={`${route.departure}-${route.arrival}`}
                    className="flex w-40 items-center rounded bg-sky-600 px-2 py-1 font-mono"
                  >
                    <p className="flex grow space-x-1">
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

        <div className="absolute bottom-5 w-full p-5">
          {Object.keys(trafficData).length > 0 && (
            <div className="flex items-end rounded bg-neutral-50/90 px-5 py-2 shadow">
              <div className="flex flex-col items-center">
                <div className="flex">
                  <StepBack
                    className="cursor-pointer hover:scale-105"
                    onClick={() => {
                      setIsPlaying(false);
                      decrementTimeSlider();
                    }}
                  />
                  {isPlaying ? (
                    <Pause className="cursor-pointer hover:scale-105" onClick={togglePlayback} />
                  ) : (
                    <Play className="cursor-pointer hover:scale-105" onClick={togglePlayback} />
                  )}
                  <StepForward
                    className="cursor-pointer hover:scale-105"
                    onClick={() => {
                      setIsPlaying(false);
                      incrementTimeSlider();
                    }}
                  />
                </div>
                <div className="mt-2">
                  <select
                    id="playback-speed"
                    value={playbackSpeed}
                    onChange={(e) => {
                      setPlaybackSpeed(Number(e.target.value));
                      if (isPlaying) {
                        setIsPlaying(false);
                        setTimeout(() => setIsPlaying(true), 0);
                      }
                    }}
                    className="rounded border border-gray-300 px-2 py-1 text-sm"
                  >
                    <option value={1}>1x</option>
                    <option value={2}>2x</option>
                    <option value={4}>4x</option>
                    <option value={8}>8x</option>
                    <option value={16}>16x</option>
                  </select>
                </div>
              </div>
              <div className="mb-5 ml-6 grow">
                <form className="px-4">
                  <Slider.Root
                    className="relative flex h-5 w-full touch-none select-none items-center pt-7"
                    defaultValue={[0]}
                    min={0}
                    max={timestamps.length - 1}
                    step={1}
                    value={[sliderIndex]}
                    onValueChange={(v) => setSliderIndex(v[0])}
                    onPointerDown={() => {
                      if (isPlaying) {
                        setIsPlaying(false);
                        pointerDownSuspendedPlay.current = true;
                      }
                    }}
                    onPointerUp={() => {
                      if (pointerDownSuspendedPlay.current) {
                        setIsPlaying(true);
                        pointerDownSuspendedPlay.current = false;
                      }
                    }}
                  >
                    <Slider.Track className="relative h-[3px] grow rounded-full bg-neutral-300">
                      <Slider.Range className="absolute h-full rounded-full bg-slate-700" />
                    </Slider.Track>
                    <Slider.Thumb
                      className="block size-5 rounded-[10px] bg-white shadow-[0_2px_10px] shadow-black transition-colors hover:bg-sky-600 focus:bg-sky-600 focus:shadow-[0_0_0_5px] focus:shadow-black focus:outline-none"
                      aria-label="Volume"
                    >
                      <div className="relative -left-6 -top-8 font-mono">{timestampString}</div>
                    </Slider.Thumb>
                  </Slider.Root>
                </form>
              </div>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

export default App;
