import { useMemo, useState } from "react";
import { X, ChevronUp, ChevronDown } from "lucide-react";
import { useNavigate } from "@tanstack/react-router";
import { useStore } from "../store";
import type { EventConfig } from "../types/vatsim-capture";

type SortField = "name" | "artccs" | "airports" | "start" | "end";
type SortDirection = "asc" | "desc";

function getSlugFromEventUrl(eventUrl: string): string {
  const filename = eventUrl.split("/").pop() ?? "";
  return filename.replace(/\.json$/, "");
}

function formatDateTime(iso: string): string {
  const d = new Date(iso);
  return d.toLocaleDateString("en-US", {
    month: "short",
    day: "numeric",
    year: "numeric",
    hour: "2-digit",
    minute: "2-digit",
    hourCycle: "h23",
    timeZone: "UTC",
    timeZoneName: "short",
  });
}

function getSortValue(event: EventConfig, field: SortField): string {
  switch (field) {
    case "name":
      return event.name.toLowerCase();
    case "artccs":
      return event.artccs.join(", ").toLowerCase();
    case "airports":
      return event.airports.join(", ").toLowerCase();
    case "start":
      return event.advertised_start_time;
    case "end":
      return event.advertised_end_time;
  }
}

function SortIcon({ field, sortField, sortDirection }: { field: SortField; sortField: SortField; sortDirection: SortDirection }) {
  if (field !== sortField) return null;
  return sortDirection === "asc" ? <ChevronUp size={14} className="inline" /> : <ChevronDown size={14} className="inline" />;
}

export function EventDrawer() {
  const { eventsMetadata, isEventDrawerOpen, setEventDrawerOpen } = useStore();
  const navigate = useNavigate();

  const [sortField, setSortField] = useState<SortField>("start");
  const [sortDirection, setSortDirection] = useState<SortDirection>("desc");
  const [filterText, setFilterText] = useState("");

  const handleSort = (field: SortField) => {
    if (field === sortField) {
      setSortDirection((d) => (d === "asc" ? "desc" : "asc"));
    } else {
      setSortField(field);
      setSortDirection("asc");
    }
  };

  const filteredAndSorted = useMemo(() => {
    const lower = filterText.toLowerCase();
    const filtered = eventsMetadata.filter((item) => {
      const e = item.event;
      return (
        e.name.toLowerCase().includes(lower) ||
        e.artccs.join(", ").toLowerCase().includes(lower) ||
        e.airports.join(", ").toLowerCase().includes(lower) ||
        e.advertised_start_time.toLowerCase().includes(lower) ||
        e.advertised_end_time.toLowerCase().includes(lower)
      );
    });

    return filtered.sort((a, b) => {
      const aVal = getSortValue(a.event, sortField);
      const bVal = getSortValue(b.event, sortField);
      const cmp = aVal.localeCompare(bVal);
      return sortDirection === "asc" ? cmp : -cmp;
    });
  }, [eventsMetadata, filterText, sortField, sortDirection]);

  if (!isEventDrawerOpen) return null;

  const thClass = "px-4 py-2 text-left text-sm font-semibold text-slate-300 cursor-pointer select-none hover:text-white";
  const tdClass = "px-4 py-2 text-sm text-slate-200";

  return (
    <div className="absolute inset-0 z-20 flex flex-col overflow-hidden bg-slate-800">
      <div className="flex items-center justify-between border-b border-slate-700 px-4 py-3">
        <h2 className="text-lg font-semibold text-white">Select Event</h2>
        <button
          onClick={() => setEventDrawerOpen(false)}
          className="cursor-pointer rounded p-1 text-slate-400 transition-colors hover:bg-slate-700 hover:text-white"
        >
          <X size={20} />
        </button>
      </div>
      <div className="px-4 py-2">
        <input
          type="text"
          placeholder="Filter events..."
          value={filterText}
          onChange={(e) => setFilterText(e.target.value)}
          className="w-full rounded border border-slate-600 bg-slate-900 px-3 py-1.5 text-sm text-white placeholder-slate-500 focus:border-sky-500 focus:outline-none"
        />
      </div>
      <div className="flex-1 overflow-y-auto">
        <table className="w-full">
          <thead className="sticky top-0 bg-slate-800">
            <tr className="border-b border-slate-700">
              <th className={thClass} onClick={() => handleSort("name")}>
                Name <SortIcon field="name" sortField={sortField} sortDirection={sortDirection} />
              </th>
              <th className={thClass} onClick={() => handleSort("artccs")}>
                ARTCCs <SortIcon field="artccs" sortField={sortField} sortDirection={sortDirection} />
              </th>
              <th className={thClass} onClick={() => handleSort("airports")}>
                Airports <SortIcon field="airports" sortField={sortField} sortDirection={sortDirection} />
              </th>
              <th className={thClass} onClick={() => handleSort("start")}>
                Start <SortIcon field="start" sortField={sortField} sortDirection={sortDirection} />
              </th>
              <th className={thClass} onClick={() => handleSort("end")}>
                End <SortIcon field="end" sortField={sortField} sortDirection={sortDirection} />
              </th>
            </tr>
          </thead>
          <tbody>
            {filteredAndSorted.map((item, i) => {
              const slug = getSlugFromEventUrl(item.url);
              return (
                <tr
                  key={`${item.url}-${i}`}
                  className="cursor-pointer border-b border-slate-700/50 transition-colors hover:bg-slate-700"
                  onClick={() => {
                    navigate({ to: "/$slug", params: { slug } });
                    setEventDrawerOpen(false);
                  }}
                >
                  <td className={tdClass}>{item.event.name}</td>
                  <td className={tdClass}>{item.event.artccs.join(", ")}</td>
                  <td className={tdClass}>{item.event.airports.join(", ")}</td>
                  <td className={tdClass}>{formatDateTime(item.event.advertised_start_time)}</td>
                  <td className={tdClass}>{formatDateTime(item.event.advertised_end_time)}</td>
                </tr>
              );
            })}
            {filteredAndSorted.length === 0 && (
              <tr>
                <td colSpan={5} className="px-4 py-8 text-center text-sm text-slate-400">
                  No events found.
                </td>
              </tr>
            )}
          </tbody>
        </table>
      </div>
    </div>
  );
}
