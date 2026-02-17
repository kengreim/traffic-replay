import { useEffect } from "react";
import { useParams } from "@tanstack/react-router";
import { useStore } from "../store";
import type {
  EventCapture,
  OptimizedEventCapture,
  TrafficData,
  PilotData,
} from "../types/vatsim-capture";
import { isOptimizedFormat } from "../types/vatsim-capture";
import type { Feature, FeatureCollection, Point } from "geojson";

function getSlugFromEventUrl(eventUrl: string): string {
  const filename = eventUrl.split("/").pop() ?? "";
  return filename.replace(/\.json$/, "");
}

/**
 * Normalize optimized format to legacy format for internal use.
 * Expands flight plan references and adds static pilot data to each feature.
 */
function normalizeOptimizedFormat(data: OptimizedEventCapture): TrafficData {
  const captures: TrafficData = {};

  for (const [timestamp, fc] of Object.entries(data.frames)) {
    const normalizedFeatures = fc.features.map((feature) => {
      const props = feature.properties as { data: Record<string, unknown> } | null;
      if (!props?.data) return feature;

      const dynamicData = props.data;
      const cid = String(dynamicData.cid);
      const fpKey = dynamicData.fp as string | undefined;

      // Build full PilotData by combining dynamic data with static lookups
      const pilotData: PilotData = {
        cid: dynamicData.cid as number,
        name: data.pilots[cid]?.name ?? "",
        callsign: data.pilots[cid]?.callsign ?? "",
        latitude: dynamicData.latitude as number,
        longitude: dynamicData.longitude as number,
        altitude: dynamicData.altitude as number,
        groundspeed: dynamicData.groundspeed as number,
        transponder: dynamicData.transponder as string,
        heading: dynamicData.heading as number,
        flight_plan: fpKey ? data.flightPlans[fpKey] : undefined,
        logon_time: dynamicData.logon_time as string,
        last_updated: dynamicData.last_updated as string,
      };

      return {
        ...feature,
        properties: { data: pilotData },
      } as Feature<Point>;
    });

    captures[timestamp] = {
      type: "FeatureCollection",
      features: normalizedFeatures,
    } as FeatureCollection;
  }

  return captures;
}

export function EventAutoLoader() {
  const { slug } = useParams({ from: "/$slug" });
  const eventsMetadata = useStore((s) => s.eventsMetadata);
  const setCurrentEventUrl = useStore((s) => s.setCurrentEventUrl);
  const setEventLoading = useStore((s) => s.setEventLoading);
  const setTrafficData = useStore((s) => s.setTrafficData);
  const setTimestamps = useStore((s) => s.setTimestamps);
  const setSliderIndex = useStore((s) => s.setSliderIndex);
  const stopPlayback = useStore((s) => s.stopPlayback);
  const clearRouteFilters = useStore((s) => s.clearRouteFilters);

  useEffect(() => {
    if (!slug || eventsMetadata.length === 0) return;

    const matched = eventsMetadata.find((e) => getSlugFromEventUrl(e.url) === slug);
    if (!matched) return;

    const fetchEvent = async () => {
      setEventLoading(true);
      try {
        const response = await fetch(matched.url);
        if (response.ok) {
          const event = (await response.json()) as EventCapture | OptimizedEventCapture;
          setCurrentEventUrl(matched.url);

          // Handle both legacy and optimized formats
          let captures: TrafficData;
          if (isOptimizedFormat(event)) {
            captures = normalizeOptimizedFormat(event);
          } else {
            captures = event.captures;
          }

          setTrafficData(captures);
          const ts = Object.keys(captures).sort();
          setTimestamps(ts);
          setSliderIndex(0);
          stopPlayback();
          clearRouteFilters();
        }
      } catch (error) {
        console.error("Failed to fetch event data:", error);
      } finally {
        setEventLoading(false);
      }
    };

    fetchEvent();
  }, [slug, eventsMetadata]);

  return null;
}
