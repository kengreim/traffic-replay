import { useState, useMemo } from "react";
import type { EventsMetadata } from "../types/vatsim-capture";
import { Modal } from "./ui-core/Modal";
import {
  useReactTable,
  getCoreRowModel,
  getSortedRowModel,
  getFilteredRowModel,
  flexRender,
  createColumnHelper,
  type ColumnDef,
  type HeaderGroup,
  type Row,
  type Cell,
  type CellContext,
  type Header,
} from "@tanstack/react-table";

interface EventSelectionTableProps {
  isOpen: boolean;
  onClose: () => void;
  events: EventsMetadata;
  onSelectEvent: (url: string) => void;
}

const columnHelper = createColumnHelper<EventsMetadata[0]>();

export function EventSelectionTable({
  isOpen,
  onClose,
  events,
  onSelectEvent,
}: EventSelectionTableProps) {
  const [globalFilter, setGlobalFilter] = useState("");
  const [sorting, setSorting] = useState<{ id: string; desc: boolean }[]>([]);

  const columns = useMemo<ColumnDef<EventsMetadata[0], any>[]>(
    () => [
      columnHelper.accessor("event.name", {
        header: "Event Name",
        cell: (info: CellContext<EventsMetadata[0], string>) => info.getValue(),
      }),
      columnHelper.accessor("event.artccs", {
        header: "ARTCCs",
        cell: (info: CellContext<EventsMetadata[0], string[]>) => info.getValue().join(", "),
      }),
      columnHelper.accessor("event.airports", {
        header: "Airports",
        cell: (info: CellContext<EventsMetadata[0], string[]>) => info.getValue().join(", "),
      }),
      columnHelper.accessor("event.advertised_start_time", {
        header: "Start Time",
        cell: (info: CellContext<EventsMetadata[0], string>) =>
          new Date(info.getValue()).toLocaleString("en-US", {
            weekday: "short",
            month: "short",
            day: "numeric",
            hour: "numeric",
            minute: "numeric",
            timeZoneName: "short",
          }),
      }),
    ],
    []
  );

  const table = useReactTable({
    data: events,
    columns,
    state: {
      sorting,
      globalFilter,
    },
    onSortingChange: setSorting,
    onGlobalFilterChange: setGlobalFilter,
    getCoreRowModel: getCoreRowModel(),
    getSortedRowModel: getSortedRowModel(),
    getFilteredRowModel: getFilteredRowModel(),
  });

  return (
    <Modal isOpen={isOpen} onClose={onClose} title="Select Event">
      <div className="mb-4">
        <input
          type="text"
          placeholder="Search events..."
          value={globalFilter}
          onChange={(e) => setGlobalFilter(e.target.value)}
          className="w-full rounded bg-slate-800 px-3 py-2 text-white placeholder-slate-400 focus:outline-none focus:ring-2 focus:ring-sky-500"
        />
      </div>

      <div className="max-h-[60vh] overflow-auto">
        <table className="w-full border-collapse text-sm">
          <thead className="sticky top-0 bg-slate-800">
            {table.getHeaderGroups().map((headerGroup: HeaderGroup<EventsMetadata[0]>) => (
              <tr key={headerGroup.id}>
                {headerGroup.headers.map((header: Header<EventsMetadata[0], unknown>) => (
                  <th
                    key={header.id}
                    className="cursor-pointer px-4 py-2 text-left font-semibold text-white hover:bg-slate-700"
                    onClick={header.column.getToggleSortingHandler()}
                  >
                    {flexRender(
                      header.column.columnDef.header,
                      header.getContext()
                    )}
                    {{
                      asc: " ↑",
                      desc: " ↓",
                    }[header.column.getIsSorted() as string] ?? null}
                  </th>
                ))}
              </tr>
            ))}
          </thead>
          <tbody>
            {table.getRowModel().rows.map((row: Row<EventsMetadata[0]>) => (
              <tr
                key={row.id}
                className="cursor-pointer border-b border-slate-700 hover:bg-slate-800"
                onClick={() => {
                  onSelectEvent(row.original.url);
                  onClose();
                }}
              >
                {row.getVisibleCells().map((cell: Cell<EventsMetadata[0], unknown>) => (
                  <td key={cell.id} className="px-4 py-2 text-white">
                    {flexRender(cell.column.columnDef.cell, cell.getContext())}
                  </td>
                ))}
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </Modal>
  );
} 