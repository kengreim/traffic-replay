import { useMemo, useState } from "react";
import { StyledCheckbox } from "./ui-core/Checkbox.tsx";
import { Github, PanelLeftClose, PanelLeftOpen, PlusIcon, X } from "lucide-react";
import { useStore } from "../store";
import type { FormEvent } from "react";

interface Route {
  arrival: string;
  departure: string;
}

export function Sidebar() {
  const [collapsed, setCollapsed] = useState(false);

  const {
    setEventDrawerOpen,
    currentEventUrl,
    eventsMetadata,
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
    historyTrails,
    setHistoryTrails,
    showDisconnected,
    setShowDisconnected,
    routeFilters,
    newArrivalAirport,
    setNewArrivalAirport,
    newDepartureAirport,
    setNewDepartureAirport,
    addRouteFilter,
    removeRouteFilter,
  } = useStore();

  const currentEvent = useMemo(() => {
    if (!currentEventUrl) return null;
    return eventsMetadata.find((e) => e.url === currentEventUrl)?.event ?? null;
  }, [currentEventUrl, eventsMetadata]);

  const handleAddRouteFilter = (e: FormEvent) => {
    e.preventDefault();

    if (newArrivalAirport && newDepartureAirport) {
      const filter: Route = {
        arrival: newArrivalAirport.toUpperCase(),
        departure: newDepartureAirport.toUpperCase(),
      };
      addRouteFilter(filter);
    }
  };

  const handleRemoveRouteFilter = (route: Route) => {
    removeRouteFilter(route);
  };

  return (
    <div
      className="relative z-10 shrink-0 overflow-hidden bg-slate-900 text-white shadow-md transition-[width] duration-300 ease-in-out"
      style={{ width: collapsed ? "44px" : "310px" }}
      onTransitionEnd={() => window.dispatchEvent(new Event("resize"))}
    >
      {/* Collapse toggle — always visible, absolutely positioned */}
      <button
        onClick={() => setCollapsed(!collapsed)}
        className="absolute top-4 z-10 cursor-pointer rounded p-1 transition-[right] duration-300 ease-in-out hover:bg-slate-700"
        style={{ right: collapsed ? "8px" : "16px" }}
        title={collapsed ? "Expand sidebar" : "Collapse sidebar"}
      >
        {collapsed ? <PanelLeftOpen size={20} /> : <PanelLeftClose size={20} />}
      </button>
      {/* Sidebar content — slides in/out */}
      <div
        className="flex min-w-[262px] flex-col space-y-5 overflow-y-auto overscroll-contain p-6 transition-[transform,opacity] duration-300 ease-in-out"
        style={{
          transform: collapsed ? "translateX(-100%)" : "translateX(0)",
          opacity: collapsed ? 0 : 1,
        }}
      >
        <div className="flex items-center justify-between">
          <h1 className="text-2xl font-bold">Traffic Replay</h1>
        </div>
        <div className="flex flex-col space-y-2">
          <div className="mb-2">
            <h2 className="text-xl">Event</h2>
          </div>
          {currentEvent && (
            <div className="flex flex-col space-y-1 rounded border border-slate-600 p-2 text-sm">
              <p className="font-semibold">{currentEvent.name}</p>
              <p className="text-slate-300">
                <span className="text-slate-400">ARTCCs: </span>
                {currentEvent.artccs.join(", ")}
              </p>
              <p className="text-slate-300">
                <span className="text-slate-400">Airports: </span>
                {currentEvent.airports.join(", ")}
              </p>
              <p className="text-slate-300">
                <span className="text-slate-400">Start: </span>
                {new Date(currentEvent.advertised_start_time).toLocaleDateString("en-US", {
                  month: "short",
                  day: "numeric",
                  year: "numeric",
                  hour: "2-digit",
                  minute: "2-digit",
                  hourCycle: "h23",
                  timeZone: "UTC",
                  timeZoneName: "short",
                })}
              </p>
            </div>
          )}
          <button
            className="cursor-pointer rounded bg-sky-600 px-4 py-2 text-sm font-medium transition-colors hover:bg-sky-500"
            onClick={() => setEventDrawerOpen(true)}
          >
            {currentEvent ? "Change Event" : "Select Event"}
          </button>
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
              <h2 className="text-xl">History Trails</h2>
            </div>
            <div className="flex flex-col space-y-2 rounded border border-slate-600 p-2">
              <StyledCheckbox
                label="Show history trails"
                checked={historyTrails}
                onCheckedChange={(checked) => setHistoryTrails(checked)}
              />
              {historyTrails && (
                <StyledCheckbox
                  label="Include disconnected"
                  checked={showDisconnected}
                  onCheckedChange={(checked) => setShowDisconnected(checked)}
                />
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

        <div className="mt-auto flex flex-col items-center space-y-1 pt-4">
          <a
            href="https://github.com/kengreim/traffic-replay"
            target="_blank"
            rel="noopener noreferrer"
            className="transition-colors hover:text-slate-300"
            title="GitHub"
          >
            <Github size={20} />
          </a>
          <a
            href="https://github.com/kengreim/traffic-replay/issues/new"
            target="_blank"
            rel="noopener noreferrer"
            className="text-xs text-slate-400 transition-colors hover:text-slate-300"
          >
            Found a bug? Report it!
          </a>
        </div>
      </div>
    </div>
  );
}
