import { useState, useEffect, useMemo, type FormEvent, useRef } from "react";
import DeckGL from "@deck.gl/react";
import { GeoJsonLayer, IconLayer, ScatterplotLayer, TextLayer } from "@deck.gl/layers";
import { Map } from "react-map-gl/mapbox";
import artccs from "./artccs.json";
import "mapbox-gl/dist/mapbox-gl.css";
import type { FeatureCollection, Feature, Point } from "geojson";
import type {
  EventCapture,
  EventsMetadata,
  PilotProperties,
  TrafficData,
} from "./types/vatsim-capture.ts";
import { getAircraftIcon } from "./utils/icons.ts";
import { Sidebar } from "lucide-react";
import type { CheckedState } from "./components/ui-core/Checkbox.tsx";
import { type MapViewState, FlyToInterpolator } from "@deck.gl/core";
import { PlaybackBar } from "./components/PlaybackBar.tsx";
import { DEFAULT_VIEWPORT, DEFAULT_ZOOM, EVENTS_METADATA_URL } from "./consts.ts";

function App() {
  const [viewport, setViewport] = useState<MapViewState>(DEFAULT_VIEWPORT);

  const [trafficData, setTrafficData] = useState<TrafficData>({});

  const [timestamps, setTimestamps] = useState<string[]>([]);
  const [sliderIndex, setSliderIndex] = useState(0);

  const [routeFilters, setRouteFilters] = useState<{ arrival: string; departure: string }[]>([]);

  const [newArrivalAirport, setNewArrivalAirport] = useState("");
  const [newDepartureAirport, setNewDepartureAirport] = useState("");

  const [eventsMetadata, setEventsMetadata] = useState<EventsMetadata>([]);
  const [selectedEventUrl, setSelectedEventUrl] = useState<string>("");

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

  const fetchEventData = async () => {
    if (!selectedEventUrl) return;

    const response = await fetch(selectedEventUrl);
    if (response.ok) {
      const event = (await response.json()) as EventCapture;
      setTrafficData(event.captures);

      // Extract timestamps from the data
      const timestamps = Object.keys(event.captures).sort();
      setTimestamps(timestamps);
      setSliderIndex(0); // Reset slider position
      setIsPlaying(false); // Stop playback

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
