import {useState, useEffect, useMemo} from 'react';
import DeckGL from '@deck.gl/react';
import {GeoJsonLayer} from '@deck.gl/layers';
import {Map} from 'react-map-gl/mapbox';
import trafficData from './consolidated3.json';
import 'mapbox-gl/dist/mapbox-gl.css';
import type {FeatureCollection} from "geojson";
import "rc-slider/assets/index.css";
import {Slider} from "radix-ui";

// Replace with your Mapbox access token
const MAPBOX_ACCESS_TOKEN = 'pk.eyJ1Ijoia2VuZ3JlaW0iLCJhIjoiY2x3aDBucGZ5MGI3bjJxc2EyNzNuODAyMyJ9.20EFStYOA8-EvOu4tsCkGg';

interface TrafficData {
  [key: string]: FeatureCollection
}

function App() {
  const [viewport] = useState({
    longitude: -122.4,
    latitude: 37.8,
    zoom: 11,
    pitch: 0,
    bearing: 0
  });

  const [timestamps, setTimestamps] = useState<string[]>([]);
  const [sliderIndex, setSliderIndex] = useState(0);

  useEffect(() => {
    // Extract timestamps from the data
    const timestamps = Object.keys(trafficData).sort();
    setTimestamps(timestamps);
  }, []);

  const currentData = useMemo(() => (trafficData as TrafficData)[timestamps[sliderIndex]], [sliderIndex]);

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
          mapStyle="mapbox://styles/mapbox/dark-v10"/>
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
          Current Time: {timestamps[sliderIndex]}
        </div>
        <form>
          <Slider.Root
            className="relative flex h-5 w-full touch-none select-none items-center"
            defaultValue={[0]}
            min={0}
            max={timestamps.length - 1}
            step={1}
            onValueChange={v => setSliderIndex(v[0])}
          >
            <Slider.Track className="relative h-[3px] grow rounded-full bg-black">
              <Slider.Range className="absolute h-full rounded-full bg-white"/>
            </Slider.Track>
            <Slider.Thumb
              className="block size-5 rounded-[10px] bg-white shadow-[0_2px_10px] shadow-black hover:violet-800 focus:shadow-[0_0_0_5px] focus:shadow-black focus:outline-none"
              aria-label="Volume"
            />
          </Slider.Root>
        </form>
      </div>
    </div>
  );
}

export default App;