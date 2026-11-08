const MAPBOX_ACCESS_TOKEN =
  "pk.eyJ1Ijoia2VuZ3JlaW0iLCJhIjoiY2x3aDBucGZ5MGI3bjJxc2EyNzNuODAyMyJ9.20EFStYOA8-EvOu4tsCkGg";

const EVENTS_METADATA_URL = "https://data.vatsim-replay.com/events.json";

const DEFAULT_ZOOM = 6;
const DEFAULT_VIEWPORT = {
  longitude: -98.583333,
  latitude: 39.833333,
  pitch: 0,
  bearing: 0,
  zoom: 4,
};

export { MAPBOX_ACCESS_TOKEN, EVENTS_METADATA_URL, DEFAULT_VIEWPORT, DEFAULT_ZOOM };
