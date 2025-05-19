import React, {useState, useEffect} from 'react';
import DeckGL from '@deck.gl/react';
import {GeoJsonLayer} from '@deck.gl/layers';
import Map from 'react-map-gl';
import Slider from 'react-slider';
import trafficData from './consolidated3.json';
import 'mapbox-gl/dist/mapbox-gl.css';

// Replace with your Mapbox access token
const MAPBOX_ACCESS_TOKEN = 'pk.eyJ1Ijoia2VuZ3JlaW0iLCJhIjoiY2x3aDBucGZ5MGI3bjJxc2EyNzNuODAyMyJ9.20EFStYOA8-EvOu4tsCkGg';

function App() {
    const [viewport, setViewport] = useState({
        longitude: -122.4,
        latitude: 37.8,
        zoom: 11,
        pitch: 0,
        bearing: 0
    });

    const [timestamps, setTimestamps] = useState([]);
    const [currentTimestamp, setCurrentTimestamp] = useState(null);
    const [currentData, setCurrentData] = useState(null);

    useEffect(() => {
        // Extract timestamps from the data
        const timestamps = Object.keys(trafficData).sort();
        setTimestamps(timestamps);
        setCurrentTimestamp(timestamps[0]);
    }, []);

    useEffect(() => {
        if (currentTimestamp) {
            setCurrentData(trafficData[currentTimestamp]);
        }
    }, [currentTimestamp]);

    const layers = [
        new GeoJsonLayer({
            id: 'geojson-layer',
            data: currentData,
            pickable: true,
            stroked: false,
            filled: true,
            extruded: true,
            pointType: 'circle',
            lineWidthScale: 20,
            lineWidthMinPixels: 2,
            getFillColor: [255, 0, 0],
            getLineColor: [0, 0, 0],
            getRadius: 100,
            getLineWidth: 1,
            getElevation: 30
        })
    ];

    return (
        <div style={{position: 'relative', width: '100vw', height: '100vh'}}>
            <DeckGL
                initialViewState={viewport}
                controller={true}
                layers={layers}
            >
                <Map
                    mapboxAccessToken={MAPBOX_ACCESS_TOKEN}
                    mapStyle="mapbox://styles/mapbox/dark-v10"
                />
            </DeckGL>

            <div style={{
                position: 'absolute',
                bottom: '20px',
                left: '50%',
                transform: 'translateX(-50%)',
                width: '80%',
                padding: '20px',
                backgroundColor: 'rgba(255, 255, 255, 0.9)',
                borderRadius: '8px',
                boxShadow: '0 2px 4px rgba(0,0,0,0.2)'
            }}>
                <div style={{marginBottom: '10px'}}>
                    Current Time: {currentTimestamp ? new Date(currentTimestamp).toLocaleString() : 'Loading...'}
                </div>
                <Slider
                    min={0}
                    max={timestamps.length - 1}
                    value={timestamps.indexOf(currentTimestamp)}
                    onChange={(value) => setCurrentTimestamp(timestamps[value])}
                    renderThumb={(props, state) => <div {...props}>{state.valueNow}</div>}
                />
            </div>
        </div>
    );
}

export default App; 