/**
 * Client-side LOOKs: draw any LOOK of a raster from its `index` archive (raw CMT values) through
 * `raster/palettes.json`, as docs/atlas-output.md "Recolouring" foresees. The converter bakes only the
 * LOOKs its MapStyles use, so this is how the others (HYPSO `LBU_BORDERS`) are drawn, and how a user
 * can switch the terrain look of a style (hill shading on or off) without a second archive.
 *
 * Tiles are requested as `encarta-look://<RASTER>/<LOOK>/{z}/{x}/{y}`.
 */
import { PMTiles } from "pmtiles";
import type { AddProtocolAction, RasterSourceSpecification } from "maplibre-gl";
import { decodePngValues } from "./png";
import { rasterArchive, type Endpoints, type Palettes } from "./data";

export const LOOK_PROTOCOL = "encarta-look";

/** `#rrggbb` (or `#rrggbbaa`) per CMT value → RGBA packed for a little-endian `Uint32Array` view of ImageData. */
export function buildLut(colours: readonly string[], transparent: ReadonlySet<number> = new Set()): Uint32Array {
  const lut = new Uint32Array(256);
  for (let i = 0; i < 256; i++) {
    const c = colours[i];
    if (!c || transparent.has(i)) continue;
    const h = c.replace("#", "");
    const r = parseInt(h.slice(0, 2), 16);
    const g = parseInt(h.slice(2, 4), 16);
    const b = parseInt(h.slice(4, 6), 16);
    const a = h.length >= 8 ? parseInt(h.slice(6, 8), 16) : 255;
    lut[i] = ((a << 24) | (b << 16) | (g << 8) | r) >>> 0;
  }
  return lut;
}

export function applyLut(values: Uint8Array, lut: Uint32Array, out = new Uint32Array(values.length)): Uint32Array {
  for (let i = 0; i < values.length; i++) out[i] = lut[values[i]!]!;
  return out;
}

export function lookTileUrl(raster: string, look: string): string {
  return `${LOOK_PROTOCOL}://${raster.toUpperCase()}/${look.toUpperCase()}/{z}/{x}/{y}`;
}

export function parseLookUrl(url: string): { raster: string; look: string; z: number; x: number; y: number } | null {
  const m = new RegExp(`^${LOOK_PROTOCOL}://([^/]+)/([^/]+)/(\\d+)/(\\d+)/(\\d+)`).exec(url);
  if (!m) return null;
  return { raster: m[1]!, look: m[2]!, z: Number(m[3]), x: Number(m[4]), y: Number(m[5]) };
}

/** A MapLibre raster source drawing `look` of `raster` from its index tiles. */
export function lookSource(raster: string, look: string, palettes: Palettes): RasterSourceSpecification {
  const p = palettes[raster.toUpperCase()];
  return {
    type: "raster",
    tiles: [lookTileUrl(raster, look)],
    tileSize: 256,
    minzoom: 0,
    // Archive zooms are pyramid level + 1 (docs/atlas-output.md "Rasters"); MapLibre overzooms past it.
    maxzoom: (p?.max_level ?? 0) + 1,
  };
}

/** Is a LOOK baked into its own archive? (One archive per LOOK that a MapStyle uses.) */
export function bakedLooks(index: readonly { raster: string; look: string }[]): Set<string> {
  return new Set(index.map((e) => `${e.raster.toUpperCase()}/${e.look.toUpperCase()}`));
}

/** The protocol handler. One per page; palettes and endpoints are read when a tile is requested. */
export function createLookProtocol(get: () => { palettes: Palettes | null; ep: Endpoints }): AddProtocolAction {
  const archives = new Map<string, PMTiles>();
  const luts = new Map<string, Uint32Array>();
  return async (params, abort) => {
    const t = parseLookUrl(params.url);
    if (!t) throw new Error(`bad ${LOOK_PROTOCOL} URL ${params.url}`);
    const { palettes, ep } = get();
    const look = palettes?.[t.raster]?.looks[t.look];
    if (!look) throw new Error(`no palette for ${t.raster}/${t.look}`);
    let lut = luts.get(`${t.raster}/${t.look}`);
    if (!lut) {
      lut = buildLut(look.colours);
      luts.set(`${t.raster}/${t.look}`, lut);
    }
    let archive = archives.get(t.raster);
    if (!archive) {
      archive = new PMTiles(rasterArchive(t.raster, "index", ep));
      archives.set(t.raster, archive);
    }
    const tile = await archive.getZxy(t.z, t.x, t.y, abort.signal);
    if (!tile) return { data: await blankTile() };
    const png = await decodePngValues(new Uint8Array(tile.data));
    const rgba = applyLut(png.values, lut);
    const image = new ImageData(new Uint8ClampedArray(rgba.buffer), png.width, png.height);
    return { data: await createImageBitmap(image), cacheControl: tile.cacheControl, expires: tile.expires };
  };
}

function blankTile(): Promise<ImageBitmap> {
  return createImageBitmap(new ImageData(256, 256));
}
