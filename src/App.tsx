import {useState, useEffect, useMemo, type FormEvent} from "react";
import DeckGL from "@deck.gl/react";
import {GeoJsonLayer, IconLayer, ScatterplotLayer, TextLayer} from "@deck.gl/layers";
import {Map} from "react-map-gl/mapbox";
import trafficData from "./consolidated3.json";
import artccs from "./artccs.json";
import "mapbox-gl/dist/mapbox-gl.css";
import type {FeatureCollection, Point} from "geojson";
import {Slider} from "radix-ui";
import type {Feature} from "geojson";
import aircraft from "./data/aircraft.json";

// Replace with your Mapbox access token
const MAPBOX_ACCESS_TOKEN =
    "pk.eyJ1Ijoia2VuZ3JlaW0iLCJhIjoiY2x3aDBucGZ5MGI3bjJxc2EyNzNuODAyMyJ9.20EFStYOA8-EvOu4tsCkGg";

const typedAircraft = aircraft as string[];

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

function App() {
    const [viewport] = useState({
        longitude: -122.4,
        latitude: 37.8,
        zoom: 11,
        pitch: 0,
        bearing: 0,
    });

    const [timestamps, setTimestamps] = useState<string[]>([]);
    const [sliderIndex, setSliderIndex] = useState(0);

    const [routeFilters, setRouteFilters] = useState<{ arrival: string; departure: string }[]>([]);

    const [newArrivalAirport, setNewArrivalAirport] = useState("");
    const [newDepartureAirport, setNewDepartureAirport] = useState("");

    const newRouteFilter = useMemo(
        () => ({
            arrival: newArrivalAirport.toUpperCase(),
            departure: newDepartureAirport.toUpperCase(),
        }),
        [newArrivalAirport, newDepartureAirport],
    );

    useEffect(() => {
        // Extract timestamps from the data
        const timestamps = Object.keys(trafficData).sort();
        setTimestamps(timestamps);
    }, []);

    const currentData = useMemo(
        () => (trafficData as TrafficData)[timestamps[sliderIndex]],
        [sliderIndex, timestamps],
    );

    const filteredData: FeatureCollection = useMemo(() => {
        if (!currentData) return currentData;

        const filteredFeatures = currentData.features.filter((feature) => {
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
    }, [currentData, routeFilters]);

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
                if (aircraftType) {
                    return typedAircraft.includes(aircraftType) ? aircraftType : "c172";
                } else {
                    return "c172";
                }
            },
            getPosition: (d: Feature<Point, PilotProperties>) => [
                d.properties.data.longitude,
                d.properties.data.latitude,
            ],
            getColor: [255, 0, 0],
            billboard: false,
            getAngle: (d: Feature<Point, PilotProperties>) => d.properties.data.heading,
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
            getFillColor: [0, 0, 0],
            radiusScale: 25,
            getRadius: 5,
        }),
        new TextLayer({
            id: "heading-layer",
            data: filteredData?.features ?? [],
            pickable: false,
            getPosition: (d: Feature<Point, PilotProperties>) => [
                d.properties.data.longitude,
                d.properties.data.latitude,
            ],
            getText: (d: Feature<Point, PilotProperties>) => d.properties.data.heading.toString(),
            getSize: 12,
            getColor: [0, 0, 0],
            getAlignmentBaseline: "bottom",
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
        <div className="flex min-h-dvh min-w-dvw">
            {/* Sidebar */}
            <div className="w-[250px] bg-slate-900 p-6 overflow-y-auto overscroll-contain z-10 shadow-md text-white">
                <h2 style={{marginBottom: "20px"}}>Airport Filters</h2>

                {/* Arrival Airports */}
                <div style={{marginBottom: "20px"}}>
                    <h3>Arrival Airports</h3>
                    <form onSubmit={handleAddRouteFilter} style={{marginBottom: "10px"}}>
                        <input
                            type="text"
                            value={newDepartureAirport}
                            onChange={(e) => setNewDepartureAirport(e.target.value)}
                            placeholder="Enter ICAO code"
                            maxLength={4}
                            style={{
                                padding: "8px",
                                marginRight: "8px",
                                width: "100px",
                                textTransform: "uppercase",
                            }}
                        />
                        <input
                            type="text"
                            value={newArrivalAirport}
                            onChange={(e) => setNewArrivalAirport(e.target.value)}
                            placeholder="Enter ICAO code"
                            maxLength={4}
                            style={{
                                padding: "8px",
                                marginRight: "8px",
                                width: "100px",
                                textTransform: "uppercase",
                            }}
                        />
                        <button
                            type="submit"
                            style={{
                                padding: "8px 12px",
                                backgroundColor: "#4CAF50",
                                color: "white",
                                border: "none",
                                borderRadius: "4px",
                                cursor: "pointer",
                            }}
                        >
                            Add
                        </button>
                    </form>
                    <div style={{display: "flex", flexWrap: "wrap", gap: "8px"}}>
                        {routeFilters.map((route) => (
                            <div
                                key={`${route.departure}-${route.arrival}`}
                                style={{
                                    backgroundColor: "#e0e0e0",
                                    padding: "4px 8px",
                                    borderRadius: "4px",
                                    display: "flex",
                                    alignItems: "center",
                                    gap: "4px",
                                }}
                            >
                                {route.departure}-{route.arrival}
                                <button
                                    onClick={() => handleRemoveRouteFilter(route)}
                                    style={{
                                        border: "none",
                                        background: "none",
                                        cursor: "pointer",
                                        padding: "0 4px",
                                    }}
                                >
                                    ×
                                </button>
                            </div>
                        ))}
                    </div>
                </div>
            </div>

            {/* Map */}
            <div style={{flex: 1, position: "relative"}}>
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
                    <div style={{marginBottom: "10px"}}>Current Time: {timestamps[sliderIndex]}</div>
                    <form>
                        <Slider.Root
                            className="relative flex h-5 w-full touch-none select-none items-center"
                            defaultValue={[0]}
                            min={0}
                            max={timestamps.length - 1}
                            step={1}
                            value={[sliderIndex]}
                            onValueChange={(v) => setSliderIndex(v[0])}
                        >
                            <Slider.Track className="relative h-[3px] grow rounded-full bg-neutral-300">
                                <Slider.Range className="absolute h-full rounded-full bg-slate-700"/>
                            </Slider.Track>
                            <Slider.Thumb
                                className="block size-5 rounded-[10px] bg-white shadow-[0_2px_10px] shadow-black hover:bg-sky-600 transition-colors focus:shadow-[0_0_0_5px] focus:shadow-black focus:outline-none"
                                aria-label="Volume"
                            />
                        </Slider.Root>
                    </form>
                    <button
                        className="ml-2"
                        onClick={() => setSliderIndex((prevState) => Math.max(prevState - 1, 0))}
                    >
                        Prev
                    </button>
                    <button
                        className="ml-2"
                        onClick={() =>
                            setSliderIndex((prevState) => Math.min(prevState + 1, timestamps.length - 1))
                        }
                    >
                        Next
                    </button>
                </div>
            </div>
        </div>
    );
}

export default App;
