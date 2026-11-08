import type { EventsMetadata, EventsMetadataEntry, EventPart } from "../types/vatsim-capture";

export function getSlugFromEventUrl(eventUrl: string): string {
  const filename = eventUrl.split("/").pop() ?? "";
  return filename.replace(/\.json$/, "");
}

/** URLs of every capture file of an event: one per part, or just the one. */
function eventUrls(entry: EventsMetadataEntry): string[] {
  return entry.parts?.length ? entry.parts.map((p) => p.url) : [entry.url];
}

export function findEventByUrl(
  metadata: EventsMetadata,
  url: string,
): EventsMetadataEntry | undefined {
  return metadata.find((e) => eventUrls(e).includes(url));
}

export function findEventBySlug(
  metadata: EventsMetadata,
  slug: string,
): { entry: EventsMetadataEntry; url: string } | undefined {
  for (const entry of metadata) {
    const url = eventUrls(entry).find((u) => getSlugFromEventUrl(u) === slug);
    if (url) return { entry, url };
  }
  return undefined;
}

function formatUtcTime(d: Date): string {
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${pad(d.getUTCHours())}:${pad(d.getUTCMinutes())}`;
}

/** e.g. "Nov 14 06:00–12:00Z" */
export function formatPartWindow(part: EventPart): string {
  const start = new Date(part.start);
  const day = start.toLocaleDateString("en-US", {
    month: "short",
    day: "numeric",
    timeZone: "UTC",
  });
  return `${day} ${formatUtcTime(start)}–${formatUtcTime(new Date(part.end))}Z`;
}
