/**
 * The Geography Quiz map: Encarta's own "Atlas Quiz" style (Earth By Day satellite picture with
 * borders and no place names; out/atlas/styles/atlas-quiz.json) on MapLibre. The place being asked
 * is shown as in the VM: a country or continent filled yellow, a city as a target marker, with the
 * view fitted to it. The player can still pan and zoom.
 *
 * The full `<Atlas />` component shows names on hover and has no hook for highlighting one entity,
 * which would give the answers away, so the quiz runs its own small map over the same data
 * (docs/games.md "Geography Quiz").
 */
import { useEffect, useRef, useState } from "react";
import maplibregl, { type Map as MlMap } from "maplibre-gl";
import { Protocol } from "pmtiles";
import "maplibre-gl/dist/maplibre-gl.css";
import type { GeoPlace } from "../api";
import { rewriteStyle, viewFor } from "./view";

const FILL_LAYER = "gq-target-fill";
const LINE_LAYER = "gq-target-line";
/** The highlight yellow of the VM's quiz map. */
export const HIGHLIGHT = "#fbf06c";
let protocolReady = false;

export function QuizMap({ place, styleUrl = "/out/atlas/styles/atlas-quiz.json", onError }: { place: GeoPlace | null; styleUrl?: string; onError?(message: string): void }) {
  const box = useRef<HTMLDivElement>(null);
  const mapRef = useRef<MlMap | null>(null);
  const marker = useRef<maplibregl.Marker | null>(null);
  const [ready, setReady] = useState(false);
  const moved = useRef(false);

  useEffect(() => {
    if (!box.current) return;
    if (!protocolReady) {
      maplibregl.addProtocol("pmtiles", new Protocol({ metadata: true }).tile);
      protocolReady = true;
    }
    let cancelled = false;
    setReady(false);
    fetch(styleUrl)
      .then((r) => (r.ok ? r.json() : Promise.reject(new Error(`map style: HTTP ${r.status}`))))
      .then((style: maplibregl.StyleSpecification) => {
        if (cancelled || !box.current) return;
        const map = new maplibregl.Map({
          container: box.current,
          style: rewriteStyle(style),
          center: [10, 20],
          zoom: 1.3,
          attributionControl: false,
          dragRotate: false,
          pitchWithRotate: false,
        });
        mapRef.current = map;
        map.addControl(new maplibregl.NavigationControl({ showCompass: false }), "top-right");
        map.addControl(new maplibregl.ScaleControl({ unit: "imperial", maxWidth: 140 }), "bottom-right");
        map.on("style.load", () => {
          if (!map.getSource("earth")) return;
          const filter: maplibregl.FilterSpecification = ["==", ["get", "entity"], -1];
          // Under the borders: before the first line layer.
          const before = map.getStyle().layers?.find((l) => l.type === "line")?.id;
          map.addLayer({ id: FILL_LAYER, type: "fill", source: "earth", "source-layer": "areas", filter, paint: { "fill-color": HIGHLIGHT, "fill-antialias": false } }, before);
          map.addLayer({ id: LINE_LAYER, type: "line", source: "earth", "source-layer": "areas", filter, paint: { "line-color": HIGHLIGHT, "line-width": 0.6 } }, before);
          setReady(true);
        });
        map.on("error", (e) => {
          const msg = (e as unknown as { error?: Error }).error?.message;
          if (msg && /style/i.test(msg)) onError?.(msg);
        });
      })
      .catch((e: Error) => onError?.(e.message));
    return () => {
      cancelled = true;
      marker.current?.remove();
      mapRef.current?.remove();
      mapRef.current = null;
      setReady(false);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [styleUrl]);

  useEffect(() => {
    const map = mapRef.current;
    if (!map || !ready || !map.getLayer(FILL_LAYER)) return;
    marker.current?.remove();
    marker.current = null;
    const area = !!place && place.kind !== 1;
    const filter: maplibregl.FilterSpecification = ["==", ["get", "entity"], area ? place!.entity : -1];
    map.setFilter(FILL_LAYER, filter);
    map.setFilter(LINE_LAYER, filter);
    // Continent areas are coarse outlines around the land, so they are tinted rather than filled.
    map.setPaintProperty(FILL_LAYER, "fill-opacity", place?.kind === 2 ? 0.42 : 1);
    if (!place) return;
    if (!area) {
      const el = document.createElement("div");
      el.className = "gq-target";
      marker.current = new maplibregl.Marker({ element: el }).setLngLat([place.lon, place.lat]).addTo(map);
    }
    const v = viewFor(place);
    // The first place is shown at once; later ones fly there.
    const duration = moved.current ? 900 : 0;
    moved.current = true;
    if (v.bounds) map.fitBounds(v.bounds, { padding: 48, maxZoom: place.kind === 2 ? 2.6 : 5.5, duration });
    else map.flyTo({ center: v.center!, zoom: v.zoom!, duration });
  }, [place, ready]);

  return <div ref={box} className="gq-map" aria-label="Quiz map" />;
}
