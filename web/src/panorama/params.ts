/**
 * Viewer parameters for the two kinds of panorama, from the converted data (docs/web-client.md,
 * "Panoramas", has the table of where each one comes from and which are our own choices).
 *
 * - Single panoramas: the `vrslide` slide of the interactive's `.iax` (`/api/interactive`), whose
 *   `vrdata/scene` gives `fov`, `initangle`, `maxzoom`, `part`, `tilt`, `auto`, `maxspeed` and the
 *   display size; the picture and its horizon come from the image SWF (swf.ts).
 * - Panoramic tour scenes: the scene record's `default` (`meta.panorama`: `width`, `panangle`) and
 *   its `surround` picture; the field of view, view size and opening yaw are the VM measurements in
 *   measured.ts (`panangle` only orients the site-map wedge).
 */
import type { Interactive } from "../api/types";
import { tourHfov } from "./geometry";
import { SINGLE, TOUR, TOUR_AUTO_PAN } from "./measured";

export interface PanoParams {
  /** Horizontal field of view of the opening view in degrees, or "fit" (picture height fills the view). */
  fov: number | "fit";
  /** Opening yaw: degrees from the picture's left edge to the view centre (single panoramas: see openingYaw). */
  yaw: number;
  /** Largest magnification relative to the opening view, in percent. */
  maxZoom: number;
  /** Angular width of the picture in degrees. */
  part: number;
  /** Vertical look (image-plane shift) allowed. */
  tilt: boolean;
  /** Auto-pan speed in degrees per second (single panoramas: from `auto` px per Flash timer tick). */
  autoPan: number;
  /** Auto-pan runs when the panorama opens. */
  autoStart: boolean;
  /** A click on the view pauses auto-pan for this long (ms); absent = auto-pan stays off. */
  autoResumeMs?: number;
  /** Auto-pan moves in this many jumps a second; absent = smoothly. */
  autoStepsPerSecond?: number;
  /** Dragging turns at this many view px/s per px of pointer distance from the press point; absent = grab-and-drag. */
  dragRate?: number;
  /** Fastest keyboard pan in degrees per second. */
  maxSpeed: number;
  /** The view's own size in view pixels; the page scales it to fit. */
  viewWidth: number;
  viewHeight: number;
}

/** The FlashVR scene of a `vrslide` slide in an `.iax` definition (JSON as `/api/interactive` returns it). */
export interface VrSlide {
  image: string;
  scene: Record<string, string>;
  imageWidth: number;
  imageHeight: number;
}

function asArray<T>(v: T | T[] | undefined): T[] {
  return v === undefined ? [] : Array.isArray(v) ? v : [v];
}

function num(v: unknown, fallback: number): number {
  const n = Number(v);
  return typeof v === "string" && v.trim() !== "" && Number.isFinite(n) ? n : fallback;
}

/** The first `vrslide="1"` slide of an interactive definition, or null when it is not a panorama. */
export function vrSlide(definition: Record<string, unknown> | undefined): VrSlide | null {
  const slides = (definition?.slides as { slide?: unknown } | undefined)?.slide;
  for (const s of asArray(slides as Record<string, unknown> | Record<string, unknown>[] | undefined)) {
    const ss = s?.standardslide as Record<string, unknown> | undefined;
    if (!ss || ss["@vrslide"] !== "1") continue;
    const scene = ((ss.vrdata as Record<string, unknown> | undefined)?.scene ?? {}) as Record<string, unknown>;
    const attrs: Record<string, string> = {};
    for (const [k, v] of Object.entries(scene)) if (k.startsWith("@") && typeof v === "string") attrs[k.slice(1)] = v;
    const image = typeof ss.image === "string" ? ss.image : "";
    if (!image) continue;
    return { image, scene: attrs, imageWidth: num(ss.imagewidth, 544), imageHeight: num(ss.imageheight, 380) };
  }
  return null;
}

/** Defaults where neither record type says anything (ours; see docs/web-client.md). */
export const DEFAULTS = {
  /** The most common `fov` of the 167 single panoramas (150 of them). */
  fov: 80,
  maxZoom: 200,
  autoPan: 3,
  maxSpeed: 50,
};

/**
 * Opening yaw (degrees from the picture's left edge) for an `initangle`: FlashVR measures it from the
 * picture's middle, positive to the right, and keeps a partial picture's view inside it (VM, Flash pass:
 * five scenes, 44 → 224°, 62 → 242°, 180 → 0°, 252 → 72°, 305 → 125°).
 */
export function openingYaw(initangle: number, part: number, fov: number): number {
  if (part >= 360) return (((180 + initangle) % 360) + 360) % 360;
  const edge = part / 2 - fov / 2;
  return part / 2 + Math.max(-edge, Math.min(edge, initangle));
}

export function singleParams(vr: VrSlide): PanoParams {
  const s = vr.scene;
  const fovRaw = (s.fov ?? "").trim().toUpperCase();
  const fov = fovRaw === "AUTO" ? "fit" : num(s.fov, DEFAULTS.fov);
  const part = Math.min(360, Math.max(1, num(s.part, 360)));
  const viewWidth = num(s.displaywidth, vr.imageWidth);
  const auto = Math.max(0, num(s.auto, DEFAULTS.autoPan)) || DEFAULTS.autoPan;
  // Auto-pan: `auto` view pixels per timer tick; as an angle at the opening field of view.
  const focal = viewWidth / 2 / Math.tan(((fov === "fit" ? DEFAULTS.fov : fov) * Math.PI) / 360);
  return {
    fov,
    yaw: openingYaw(num(s.initangle, 0), part, fov === "fit" ? DEFAULTS.fov : fov),
    maxZoom: num(s.maxzoom, DEFAULTS.maxZoom),
    part,
    tilt: (s.tilt ?? "on").toLowerCase() !== "off",
    autoPan: (Math.atan((auto * SINGLE.autoTicksPerSecond) / focal) * 180) / Math.PI,
    autoStart: num(s.auto, DEFAULTS.autoPan) > 0,
    maxSpeed: Math.max(1, num(s.maxspeed, DEFAULTS.maxSpeed)),
    viewWidth,
    viewHeight: num(s.displayheight, vr.imageHeight),
  };
}

/** The image SWF's URL: the resolved `refs` entry when the server has one, else the IAF folder by convention. */
export function slideImageUrl(ia: Pick<Interactive, "refs" | "file_name" | "edition">, image: string): string {
  const ref = (ia.refs ?? []).find((r) => r.ref.toLowerCase() === image.toLowerCase() && r.url);
  if (ref?.url) return ref.url;
  const id = ia.file_name.replace(/\.iax$/i, "");
  return `/media/disc/${ia.edition === "kids" ? "kids" : "deluxe"}/dswMedia/IAF/a/${id}/${image}`;
}

/**
 * The view of the single panoramas: 120 of the 167 scenes show at 544 × 380 (`displaywidth`,
 * `displayheight`); tour scenes use the same size (ours: the scene records carry none).
 */
export const DISPLAY = { width: 544, height: 380 };

/**
 * A tour scene, as measured in the original (panorama/measured.ts): a 624 x 289 view that opens
 * with the picture's left edge at the view centre (`panangle` does not set the yaw; it only
 * orients the site-map wedge, see wedgeAngle in geometry.ts), a field of view from the picture
 * width (tourHfov), auto-pan on at one turn in 18.3 s in 7.5 jumps a second, and dragging that turns
 * at a speed set by the pointer's distance from the press point.
 */
export function tourParams(_meta: Record<string, unknown> | undefined, picture: { width: number; height: number }): PanoParams {
  return {
    fov: tourHfov(picture.width),
    yaw: TOUR.openYaw,
    maxZoom: DEFAULTS.maxZoom,
    part: 360,
    tilt: true,
    autoPan: TOUR_AUTO_PAN,
    autoStart: true,
    autoResumeMs: TOUR.autoResumeMs,
    autoStepsPerSecond: TOUR.autoStepsPerSecond,
    dragRate: TOUR.dragRate,
    maxSpeed: DEFAULTS.maxSpeed,
    viewWidth: TOUR.viewWidth,
    viewHeight: TOUR.viewHeight,
  };
}
