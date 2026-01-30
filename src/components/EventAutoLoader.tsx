import { useEffect } from "react";
import { useParams } from "@tanstack/react-router";
import { useStore } from "../store";
import type { EventCapture } from "../types/vatsim-capture";

function getSlugFromEventUrl(eventUrl: string): string {
  const filename = eventUrl.split("/").pop() ?? "";
  return filename.replace(/\.json$/, "");
}

export function EventAutoLoader() {
  const { slug } = useParams({ from: "/$slug" });
  const eventsMetadata = useStore((s) => s.eventsMetadata);
  const setSelectedEventUrl = useStore((s) => s.setSelectedEventUrl);
  const setTrafficData = useStore((s) => s.setTrafficData);
  const setTimestamps = useStore((s) => s.setTimestamps);
  const setSliderIndex = useStore((s) => s.setSliderIndex);
  const stopPlayback = useStore((s) => s.stopPlayback);

  useEffect(() => {
    if (!slug || eventsMetadata.length === 0) return;

    const matched = eventsMetadata.find((e) => getSlugFromEventUrl(e.url) === slug);
    if (!matched) return;

    const fetchEvent = async () => {
      setSelectedEventUrl(matched.url);
      try {
        const response = await fetch(matched.url);
        if (response.ok) {
          const event = (await response.json()) as EventCapture;
          setTrafficData(event.captures);
          const ts = Object.keys(event.captures).sort();
          setTimestamps(ts);
          setSliderIndex(0);
          stopPlayback();
        }
      } catch (error) {
        console.error("Failed to fetch event data:", error);
      }
    };

    fetchEvent();
  }, [slug, eventsMetadata]);

  return null;
}
