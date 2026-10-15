/** Synthetic styles in the converter's conventions (docs/atlas-output.md "Styles"); no disc data. */
import type { LayerSpecification, StyleSpecification } from "maplibre-gl";

const cls = (ids: number[]) => (ids.length === 1 ? ["==", ["get", "class"], ids[0]] : ["in", ["get", "class"], ["literal", ids]]);

export function layer(id: string, type: string, sourceLayer: string, ids: number[], names: string[], extra: Record<string, unknown> = {}): LayerSpecification {
  return {
    id,
    type,
    source: "earth",
    "source-layer": sourceLayer,
    filter: cls(ids),
    metadata: { "encarta:classes": names },
    layout: {},
    paint: {},
    ...extra,
  } as unknown as LayerSpecification;
}

export function syntheticStyle(): StyleSpecification {
  return {
    version: 8,
    name: "Test Style",
    glyphs: "/atlas/glyphs/{fontstack}/{range}.pbf",
    sprite: "/atlas/sprites/sprite",
    projection: { type: "globe" },
    metadata: {
      "encarta:style_id": 12,
      "encarta:raster": "HYPSO",
      "encarta:look": "HYPSO",
      "encarta:minzoom": 0.59,
      "encarta:maxzoom": 8.97,
      "encarta:space_colour": "#000000",
      "encarta:view_p_over_m": 3135,
      "encarta:view_p_over_m_per_height_px": 6.05,
      "encarta:interactive": { "62": [[0.97, 4, 1, 0], [4, 16.59, 1, 1]], "64": [[0.97, 5, 2]], "0": [[0, 24, 1]], "80": [[0, 24, 1, 2]], "999": [[0, 24, 1]] },
      "encarta:min_rects": [{ bbox: [12.2, 45.3, 12.5, 45.5], maxzoom: 13.88, min_altitude_km: 5 }],
    },
    sources: {
      earth: { type: "vector", url: "pmtiles:///atlas/vector/earth.pmtiles" },
      "earth-detail": { type: "vector", url: "pmtiles:///atlas/vector/earth-detail.pmtiles" },
      raster: { type: "raster", tileSize: 256, url: "pmtiles:///atlas/raster/hypso/hypso.pmtiles" },
    },
    layers: [
      { id: "background", type: "background", paint: { "background-color": "#ebeae1" } },
      { id: "raster", type: "raster", source: "raster", paint: { "raster-resampling": "nearest" } },
      layer("fill-areas-0", "fill", "areas", [62, 64], ["Sovereign", "Admin 1"], { layout: { visibility: "none" }, paint: { "fill-pattern": "bm75" } }),
      layer("fill-areas-1", "fill", "areas", [390], ["CountryRegion - Yellow"], { paint: { "fill-color": "#e7cca7" } }),
      layer("fill-areas-2", "fill", "areas", [562], ["Z - (STAT) Positive 2 - MAX"], { paint: { "fill-color": "#bd3d1f" } }),
      layer("fill-areas-3", "fill", "areas", [561], ["Z - (STAT) Positive 1"], { paint: { "fill-color": "#fbe3c6" } }),
      layer("fill-areas-4", "fill", "areas", [500], ["Z - (STAT) Negative 1 - MIN"], { paint: { "fill-color": "#7354b2" } }),
      layer("hover-areas-5", "fill", "areas", [62, 390], ["Sovereign", "CountryRegion - Yellow"], {
        minzoom: 0.58,
        maxzoom: 5.24,
        paint: { "fill-color": "#bcbcbc", "fill-opacity": ["*", 0.6, ["case", ["boolean", ["feature-state", "hover"], false], 1, 0]] },
      }),
      layer("line-lines-6", "line", "lines", [70], ["Sovereign - boundary"], { minzoom: 3, paint: { "line-color": "#000000", "line-width": 1, "line-dasharray": [7, 5] } }),
      layer("line-lines-7", "line", "lines", [80, 81], ["River", "Railroad - minor"], { paint: { "line-color": "#4b64f7", "line-width": 2 } }),
      layer("line-lines-11", "line", "lines", [70], ["Sovereign - boundary"], {
        minzoom: 3,
        filter: ["all", ["==", ["get", "class"], 70], ["==", ["get", "water"], true]],
        paint: { "line-color": "#d4c8ce", "line-width": 3 },
      }),
      layer("symbol-points-8", "symbol", "points", [0], ["PPL - non-capital million+ city"], {
        layout: { "icon-image": "bm10", "text-field": ["coalesce", ["get", "name"], ""], "text-font": ["MS Reference Sans Serif Regular"], "text-size": 14 },
        paint: { "text-color": "#000000", "text-halo-color": "#ffffff" },
      }),
      layer("symbol-points-9", "symbol", "points", [7], ["Airport"], {
        layout: { visibility: "none", "icon-image": "bm40", "text-field": ["get", "name"], "text-size": 10 },
        paint: { "text-opacity": 0 },
      }),
      layer("symbol-labels-10", "symbol", "labels", [62], ["Sovereign"], {
        layout: { "text-field": ["get", "name"], "text-font": ["Arial Bold"], "text-size": 12 },
        paint: { "text-color": "#990000" },
      }),
    ],
  } as unknown as StyleSpecification;
}
