import { create } from "zustand";
import type { MapViewState } from "@deck.gl/core";
import type { CheckedState } from "./components/ui-core/Checkbox";
import type { EventsMetadata, TrafficData } from "./types/vatsim-capture";
import { DEFAULT_VIEWPORT } from "./consts";

interface ViewportState {
  viewport: MapViewState;
  setViewport: (viewport: MapViewState) => void;
}

interface TrafficState {
  trafficData: TrafficData;
  timestamps: string[];
  sliderIndex: number;
  setTrafficData: (data: TrafficData) => void;
  setTimestamps: (timestamps: string[]) => void;
  setSliderIndex: (index: number) => void;
}

interface RouteFilterState {
  routeFilters: { arrival: string; departure: string }[];
  newArrivalAirport: string;
  newDepartureAirport: string;
  setNewArrivalAirport: (airport: string) => void;
  setNewDepartureAirport: (airport: string) => void;
  addRouteFilter: (filter: { arrival: string; departure: string }) => void;
  removeRouteFilter: (filter: { arrival: string; departure: string }) => void;
}

interface LabelState {
  callsign: CheckedState;
  speed: CheckedState;
  altitude: CheckedState;
  departure: CheckedState;
  destination: CheckedState;
  setCallsign: (state: CheckedState) => void;
  setSpeed: (state: CheckedState) => void;
  setAltitude: (state: CheckedState) => void;
  setDeparture: (state: CheckedState) => void;
  setDestination: (state: CheckedState) => void;
}

interface PlaybackState {
  isPlaying: boolean;
  playbackSpeed: number;
  togglePlayback: () => void;
  setPlaybackSpeed: (speed: number) => void;
}

interface EventState {
  eventsMetadata: EventsMetadata;
  selectedEventUrl: string;
  setEventsMetadata: (metadata: EventsMetadata) => void;
  setSelectedEventUrl: (url: string) => void;
}

interface FilterState {
  hideSlowAircraft: CheckedState;
  setHideSlowAircraft: (state: CheckedState) => void;
}

interface RingState {
  rings: CheckedState;
  ringsDistance: number;
  setRings: (state: CheckedState) => void;
  setRingsDistance: (distance: number) => void;
}

type StoreState = ViewportState &
  TrafficState &
  RouteFilterState &
  LabelState &
  PlaybackState &
  EventState &
  FilterState &
  RingState;

export const useStore = create<StoreState>()((set) => ({
  // Viewport state
  viewport: DEFAULT_VIEWPORT,
  setViewport: (viewport: MapViewState) => set({ viewport }),

  // Traffic state
  trafficData: {},
  timestamps: [],
  sliderIndex: 0,
  setTrafficData: (trafficData: TrafficData) => set({ trafficData }),
  setTimestamps: (timestamps: string[]) => set({ timestamps }),
  setSliderIndex: (sliderIndex: number) => set({ sliderIndex }),

  // Route filter state
  routeFilters: [],
  newArrivalAirport: "",
  newDepartureAirport: "",
  setNewArrivalAirport: (newArrivalAirport: string) => set({ newArrivalAirport }),
  setNewDepartureAirport: (newDepartureAirport: string) => set({ newDepartureAirport }),
  addRouteFilter: (filter: { arrival: string; departure: string }) =>
    set((state: StoreState) => ({
      routeFilters: [...state.routeFilters, filter],
      newArrivalAirport: "",
      newDepartureAirport: "",
    })),
  removeRouteFilter: (filter: { arrival: string; departure: string }) =>
    set((state: StoreState) => ({
      routeFilters: state.routeFilters.filter((r) => r !== filter),
    })),

  // Label state
  callsign: true,
  speed: false,
  altitude: false,
  departure: false,
  destination: false,
  setCallsign: (callsign: CheckedState) => set({ callsign }),
  setSpeed: (speed: CheckedState) => set({ speed }),
  setAltitude: (altitude: CheckedState) => set({ altitude }),
  setDeparture: (departure: CheckedState) => set({ departure }),
  setDestination: (destination: CheckedState) => set({ destination }),

  // Playback state
  isPlaying: false,
  playbackSpeed: 1,
  togglePlayback: () => set((state: StoreState) => ({ isPlaying: !state.isPlaying })),
  setPlaybackSpeed: (playbackSpeed: number) => set({ playbackSpeed }),

  // Event state
  eventsMetadata: [],
  selectedEventUrl: "",
  setEventsMetadata: (eventsMetadata: EventsMetadata) => set({ eventsMetadata }),
  setSelectedEventUrl: (selectedEventUrl: string) => set({ selectedEventUrl }),

  // Filter state
  hideSlowAircraft: false,
  setHideSlowAircraft: (hideSlowAircraft: CheckedState) => set({ hideSlowAircraft }),

  // Ring state
  rings: false,
  ringsDistance: 3,
  setRings: (rings: CheckedState) => set({ rings }),
  setRingsDistance: (ringsDistance: number) => set({ ringsDistance }),
}));
