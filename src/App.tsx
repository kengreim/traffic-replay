import { useEffect, useMemo, useRef } from "react";
import DeckGL from "@deck.gl/react";
import { GeoJsonLayer, IconLayer, PathLayer, ScatterplotLayer, TextLayer } from "@deck.gl/layers";
import { Map as MapboxMap } from "react-map-gl/mapbox";
import { Outlet } from "@tanstack/react-router";
import artccs from "./artccs.json";
import "mapbox-gl/dist/mapbox-gl.css";
import type { FeatureCollection, Feature, Point } from "geojson";
import type { EventsMetadata, PilotProperties, TrafficData } from "./types/vatsim-capture.ts";
import { getAircraftIcon } from "./utils/icons.ts";
import { Sidebar } from "./components/Sidebar.tsx";
import { PlaybackBar } from "./components/PlaybackBar.tsx";
import { EventDrawer } from "./components/EventDrawer.tsx";
import { EVENTS_METADATA_URL, MAPBOX_ACCESS_TOKEN } from "./consts.ts";
import { useStore } from "./store";

function App() {
  const {
    viewport,
    trafficData,
    timestamps,
    sliderIndex,
    routeFilters,
    setEventsMetadata,
    callsign,
    speed,
    altitude,
    departure,
    destination,
    hideSlowAircraft,
    rings,
    ringsDistance,
    isEventLoading,
    historyTrails,
    showDisconnected,
  } = useStore();

  const showRings = useMemo(() => {
    if (rings === "indeterminate") {
      return false;
    }
    return rings;
  }, [rings]);

  useEffect(() => {
    const fetchEventsMetadata = async () => {
      try {
        const response = await fetch(EVENTS_METADATA_URL);
        if (response.ok) {
          const metadata = (await response.json()) as EventsMetadata;
          setEventsMetadata(metadata);
        }
      } catch (error) {
        console.error("Failed to fetch events metadata:", error);
      }
    };

    fetchEventsMetadata();
  }, []);

  const currentData = useMemo(
    () => (trafficData as TrafficData)[timestamps[sliderIndex]],
    [sliderIndex, timestamps, trafficData],
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

  // History trail types and refs
  interface TrailEntry {
    path: [number, number][];
    flightPlan: { departure: string; arrival: string } | null;
  }

  const trailMapRef = useRef<Map<string, TrailEntry>>(new Map());
  const trailLogRef = useRef<string[][]>([]);
  const prevSliderIndexRef = useRef<number>(0);

  // Reset trails when a new event is loaded
  useEffect(() => {
    trailMapRef.current = new Map();
    trailLogRef.current = [];
    prevSliderIndexRef.current = 0;
  }, [trafficData]);

  const trailPaths = useMemo(() => {
    if (!historyTrails || timestamps.length === 0) {
      trailMapRef.current = new Map();
      trailLogRef.current = [];
      prevSliderIndexRef.current = 0;
      return [];
    }

    const trailMap = trailMapRef.current;
    const trailLog = trailLogRef.current;
    const prevIndex = prevSliderIndexRef.current;

    if (prevIndex < sliderIndex) {
      // Forward: append positions for each timestamp we moved past
      for (let i = prevIndex + 1; i <= sliderIndex; i++) {
        const tsData = (trafficData as TrafficData)[timestamps[i]];
        const keysAtStep: string[] = [];
        if (tsData) {
          for (const feature of tsData.features) {
            const data = feature.properties?.data;
            if (!data?.callsign) continue;
            const key = `${data.cid}-${data.callsign}`;
            let entry = trailMap.get(key);
            if (!entry) {
              entry = {
                path: [],
                flightPlan: data.flight_plan
                  ? { departure: data.flight_plan.departure, arrival: data.flight_plan.arrival }
                  : null,
              };
              trailMap.set(key, entry);
            }
            entry.path.push([data.longitude, data.latitude]);
            keysAtStep.push(key);
          }
        }
        trailLog[i] = keysAtStep;
      }
    } else if (prevIndex > sliderIndex) {
      // Backward: remove positions for each timestamp we reversed past
      for (let i = prevIndex; i > sliderIndex; i--) {
        const keysAtStep = trailLog[i];
        if (keysAtStep) {
          for (const key of keysAtStep) {
            const entry = trailMap.get(key);
            if (entry) {
              entry.path.pop();
              if (entry.path.length === 0) {
                trailMap.delete(key);
              }
            }
          }
        }
      }
      trailLog.length = sliderIndex + 1;
    }

    prevSliderIndexRef.current = sliderIndex;

    // Check if a trail's flight plan matches the active route filters
    const matchesRouteFilters = (fp: { departure: string; arrival: string } | null): boolean => {
      if (routeFilters.length === 0) return true;
      if (!fp) return false;
      for (const filter of routeFilters) {
        if (
          (filter.arrival === "*" || filter.arrival === fp.arrival) &&
          (filter.departure === "*" || filter.departure === fp.departure)
        ) {
          return true;
        }
      }
      return false;
    };

    // Build set of currently visible trail keys from currentData (route filters only, not ground speed)
    const visibleKeys = new Set<string>();
    if (currentData?.features) {
      for (const feature of currentData.features) {
        const data = feature.properties?.data;
        if (!data?.callsign) continue;
        if (!matchesRouteFilters(data.flight_plan ? { departure: data.flight_plan.departure, arrival: data.flight_plan.arrival } : null)) continue;
        visibleKeys.add(`${data.cid}-${data.callsign}`);
      }
    }

    const result: { key: string; path: [number, number][] }[] = [];
    for (const [key, entry] of trailMap) {
      if (entry.path.length < 2) continue;
      if (showDisconnected) {
        // Show all trails that match route filters
        if (matchesRouteFilters(entry.flightPlan)) {
          result.push({ key, path: entry.path });
        }
      } else {
        // Only show trails for currently visible aircraft
        if (visibleKeys.has(key)) {
          result.push({ key, path: entry.path });
        }
      }
    }
    return result;
  }, [sliderIndex, timestamps, trafficData, historyTrails, currentData, showDisconnected, routeFilters]);

  const layers = [
    new PathLayer({
      id: "trail-layer",
      data: trailPaths,
      getPath: (d: { key: string; path: [number, number][] }) => d.path,
      getColor: [255, 140, 0, 180],
      getWidth: 2,
      widthMinPixels: 1,
      widthMaxPixels: 3,
      billboard: false,
      visible: !!historyTrails,
    }),
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

  return (
    <div className="min-w-dvw font-manrope flex min-h-dvh">
      <Sidebar />
      <div style={{ flex: 1, position: "relative" }}>
        <DeckGL initialViewState={viewport} controller={true} layers={layers}>
          <MapboxMap
            mapboxAccessToken={MAPBOX_ACCESS_TOKEN}
            mapStyle="mapbox://styles/mapbox/light-v11"
            projection="mercator"
          />
        </DeckGL>
        <PlaybackBar />
        {isEventLoading && (
          <div className="absolute inset-0 z-30 flex items-center justify-center bg-black/50">
            <div className="rounded-lg bg-slate-800 px-6 py-4 text-white shadow-lg">
              Loading event...
            </div>
          </div>
        )}
        <EventDrawer />
      </div>
      <Outlet />
    </div>
  );
}

export default App;
