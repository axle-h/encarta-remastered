/**
 * The host's site-map marker (tours-3d.md §4.1–§4.2), for hosts that draw the map: the
 * `WORLDLOADED` payload with its defaults, and the marker position/heading for `camera.pos?`.
 */

export interface SiteMapConfig {
  xOffset: number;
  zOffset: number;
  xExtent: number;
  zExtent: number;
  mapHeight: number | null;
  fieldOfViewDeg: number;
  rotateFrame: number;
  rect: { left: number; top: number; right: number; bottom: number } | null;
}

/** Parses `WORLDLOADED` pairs (keys case-insensitive; missing keys keep the §4.1 defaults). */
export function parseWorldLoaded(pairs: readonly [string, string][], clientRect?: { left: number; top: number; right: number; bottom: number }): SiteMapConfig {
  const get = (k: string) => pairs.find(([key]) => key.toUpperCase() === k)?.[1];
  const num = (k: string, d: number) => {
    const v = get(k);
    const n = v === undefined ? NaN : Number.parseFloat(v);
    return Number.isFinite(n) ? n : d;
  };
  const left = get("SM_RECTLEFT"), top = get("SM_RECTTOP"), right = get("SM_RECTRIGHT");
  const bottom = get("SM_RECTBOTTOM") ?? get("SMRECTBOTTOM");
  const mapHeight = get("MAPHEIGHT") === undefined ? null : num("MAPHEIGHT", 0);
  let rect = clientRect ?? null;
  if (left !== undefined || top !== undefined || right !== undefined || bottom !== undefined) {
    const t = Number(top ?? 0);
    rect = {
      left: Number(left ?? clientRect?.left ?? 0),
      top: t === 0 && mapHeight !== null ? mapHeight : t,
      right: Number(right ?? clientRect?.right ?? 0),
      bottom: Number(bottom ?? clientRect?.bottom ?? 0),
    };
  }
  return {
    xOffset: num("XOFFSET", 0),
    zOffset: num("ZOFFSET", 0),
    xExtent: num("XEXTENT", 0),
    zExtent: num("ZEXTENT", 0),
    mapHeight,
    fieldOfViewDeg: Math.min(120, Math.max(30, num("FIELDOFVIEW", 120))),
    rotateFrame: num("SM_ROTATEFRAME", 0),
    rect,
  };
}

function rotate(x: number, z: number, rot: number): [number, number] {
  if (rot === 90) return [z, -x];
  if (rot === -90) return [-z, x];
  if (rot === 180 || rot === -180) return [-x, -z];
  return [x, z];
}

/**
 * Marker pixel position and wedge heading (degrees) for an original-space camera position
 * (`camera.pos?` gives x, y, z, yaw with z in the disc's convention). Null when not drawn.
 */
export function siteMapMarker(cfg: SiteMapConfig, x: number, z: number, yawDeg: number): { px: number; py: number; heading: number } | null {
  if (!cfg.rect || cfg.xExtent === 0 || cfg.zExtent === 0) return null;
  const rot = cfg.rotateFrame;
  const [rx, rz] = rotate(x, z, rot);
  const [ox, oz] = rotate(cfg.xOffset, cfg.zOffset, rot);
  const swap = rot === 90 || rot === -90;
  const ex = swap ? cfg.zExtent : cfg.xExtent;
  const ez = swap ? cfg.xExtent : cfg.zExtent;
  const { left, top, right, bottom } = cfg.rect;
  const w = right - left, h = bottom - top;
  const clamp = (v: number, a: number, b: number) => Math.min(Math.max(v, Math.min(a, b)), Math.max(a, b));
  const px = clamp(left + Math.round(w / 2 + ((rx - ox) * w) / ex), left, right);
  const py = clamp(top + Math.round(h / 2 - ((rz - oz) * h) / ez), top, bottom);
  const heading = (((Math.round(360 - yawDeg + 90 - rot) % 360) + 360) % 360);
  return { px, py, heading };
}

export type SiteMapMarker = { px: number; py: number; heading: number };

/**
 * §4.2 step 4: the host redraws the map only when the heading changes by more than 2° or the
 * position by more than 2 px (per axis). `prev` null always redraws.
 */
export function markerMoved(prev: SiteMapMarker | null, next: SiteMapMarker | null): boolean {
  if (!prev || !next) return prev !== next;
  const dh = Math.abs(((next.heading - prev.heading + 540) % 360) - 180);
  return dh > 2 || Math.abs(next.px - prev.px) > 2 || Math.abs(next.py - prev.py) > 2;
}

/**
 * The view wedge as canvas arc angles (radians, y down): centred on the heading (degrees,
 * counter-clockwise from +x with y up, §4.2 step 3), `fieldOfViewDeg` wide.
 */
export function wedgeAngles(headingDeg: number, fieldOfViewDeg: number): [number, number] {
  const h = (headingDeg * Math.PI) / 180;
  const half = (fieldOfViewDeg * Math.PI) / 360;
  return [-h - half, -h + half];
}

/** The site-map window's size in Encarta (VM: screen (1,129)–(173,323)); the map images are two halves of this size. */
export const SITE_MAP_WINDOW: [number, number] = [172, 194];

/**
 * Draws the site-map window: the image's left half (the plain map), its right half (the tinted
 * map) inside the view wedge, then the "you are here" marker. `image` may be null (blank map).
 */
export function drawSiteMap(
  g: CanvasRenderingContext2D,
  image: CanvasImageSource | null,
  imageSize: [number, number],
  cfg: SiteMapConfig | null,
  marker: SiteMapMarker | null,
): void {
  const [w, h] = SITE_MAP_WINDOW;
  const half = Math.floor(imageSize[0] / 2);
  g.save();
  g.imageSmoothingEnabled = false;
  g.fillStyle = "#fff";
  g.fillRect(0, 0, w, h);
  if (image && half > 0) g.drawImage(image, 0, 0, half, imageSize[1], 0, 0, half, imageSize[1]);
  if (cfg && marker) {
    const [a0, a1] = wedgeAngles(marker.heading, cfg.fieldOfViewDeg);
    g.save();
    g.beginPath();
    g.moveTo(marker.px, marker.py);
    g.arc(marker.px, marker.py, w + h, a0, a1);
    g.closePath();
    g.clip();
    if (image && half > 0) g.drawImage(image, half, 0, half, imageSize[1], 0, 0, half, imageSize[1]);
    else {
      g.fillStyle = "#f7e3d6";
      g.fillRect(0, 0, w, h);
    }
    g.restore();
    drawMarker(g, marker.px, marker.py);
  }
  g.restore();
}

/** Our "you are here" marker: a four-way arrow in the host's marker colour, outlined in white. */
function drawMarker(g: CanvasRenderingContext2D, x: number, y: number): void {
  const cx = x + 0.5, cy = y + 0.5;
  g.beginPath();
  for (const [dx, dy] of [[0, -1], [1, 0], [0, 1], [-1, 0]] as const) {
    // arrow head 6 px from the centre, 5 px wide
    g.moveTo(cx + dx * 6.5, cy + dy * 6.5);
    g.lineTo(cx + dx * 3.5 - dy * 2.5, cy + dy * 3.5 + dx * 2.5);
    g.lineTo(cx + dx * 3.5 + dy * 2.5, cy + dy * 3.5 - dx * 2.5);
    g.closePath();
  }
  g.rect(cx - 2.5, cy - 2.5, 5, 5);
  g.lineJoin = "round";
  g.lineWidth = 2;
  g.strokeStyle = "#fff";
  g.stroke();
  g.fillStyle = "#103463";
  g.fill();
}
