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
