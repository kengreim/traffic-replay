import { StyledCheckbox } from "./ui-core/Checkbox.tsx";
import { PlusIcon, X } from "lucide-react";
import { useStore } from "../store";
import type { FormEvent } from "react";
import type { EventCapture } from "../types/vatsim-capture";

interface Route {
  arrival: string;
  departure: string;
}

export function Sidebar() {
  const {
    selectedEventUrl,
    setSelectedEventUrl,
    eventsMetadata,
    setTrafficData,
    setTimestamps,
    setSliderIndex,
    callsign,
    setCallsign,
    speed,
    setSpeed,
    altitude,
    setAltitude,
    departure,
    setDeparture,
    destination,
    setDestination,
    rings,
    setRings,
    ringsDistance,
    setRingsDistance,
    hideSlowAircraft,
    setHideSlowAircraft,
    routeFilters,
    newArrivalAirport,
    setNewArrivalAirport,
    newDepartureAirport,
    setNewDepartureAirport,
    addRouteFilter,
    removeRouteFilter,
    togglePlayback,
  } = useStore();

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
      togglePlayback(); // Stop playback
    }
  };

  const handleAddRouteFilter = (e: FormEvent) => {
    e.preventDefault();

    if (newArrivalAirport && newDepartureAirport) {
      const filter: Route = {
        arrival: newArrivalAirport.toUpperCase(),
        departure: newDepartureAirport.toUpperCase(),
      };
      if (!routeFilters.includes(filter)) {
        addRouteFilter(filter);
      }
    }
  };

  const handleRemoveRouteFilter = (route: Route) => {
    removeRouteFilter(route);
  };

  return (
    <div className="z-10 flex flex-col space-y-5 overflow-y-auto overscroll-contain bg-slate-900 p-6 text-white shadow-md">
      <h1 className="text-2xl font-bold">Traffic Replay</h1>
      <div className="flex flex-col space-y-2">
        <div className="mb-2">
          <h2 className="text-xl">Event</h2>
        </div>
        <div className="flex items-center space-x-2">
          <select
            id="event-select"
            className="w-48 rounded bg-gray-700 px-2 py-1 text-sm text-white"
            value={selectedEventUrl}
            onChange={(e) => setSelectedEventUrl(e.target.value)}
          >
            <option value="" disabled selected hidden>
              Please Choose...
            </option>
            {eventsMetadata.map((event, index) => (
              <option key={`${event.url}-${index}`} value={event.url}>
                {event.event.name}
              </option>
            ))}
          </select>
          <button
            className="flex h-8 w-8 cursor-pointer items-center justify-center rounded bg-sky-600 transition-colors hover:bg-sky-500"
            onClick={fetchEventData}
          >
            Go
          </button>
        </div>
      </div>

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
  );
}
