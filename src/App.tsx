import { useEffect, useMemo } from "react";
import DeckGL from "@deck.gl/react";
import { GeoJsonLayer, IconLayer, ScatterplotLayer, TextLayer } from "@deck.gl/layers";
import { Map } from "react-map-gl/mapbox";
import artccs from "./artccs.json";
import "mapbox-gl/dist/mapbox-gl.css";
import type { FeatureCollection, Feature, Point } from "geojson";
import type { EventsMetadata, PilotProperties, TrafficData } from "./types/vatsim-capture.ts";
import { getAircraftIcon } from "./utils/icons.ts";
import { Sidebar } from "./components/Sidebar.tsx";
import { PlaybackBar } from "./components/PlaybackBar.tsx";
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
  } = useStore();

  const showRings = useMemo(() => {
    if (rings === "indeterminate") {
      return false;
    }
    return rings;
  }, [rings]);

  useEffect(() => {
    // Fetch events metadata
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

    fetchEventsMetadata().then(() => console.log("Event metadata fetched"));
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

  return (
    <div className="min-w-dvw font-manrope flex min-h-dvh">
      <Sidebar />
      <div style={{ flex: 1, position: "relative" }}>
        <DeckGL initialViewState={viewport} controller={true} layers={layers}>
          <Map
            mapboxAccessToken={MAPBOX_ACCESS_TOKEN}
            mapStyle="mapbox://styles/mapbox/light-v11"
            projection="mercator"
          />
        </DeckGL>
        <PlaybackBar />
      </div>
    </div>
  );
}

export default App;
