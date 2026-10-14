/**
 * Cylindrical panorama geometry, shared by the renderers and the hotspot overlay.
 *
 * The picture (`width` × `height` pixels) is wrapped round the inside of a vertical cylinder that
 * covers `part` degrees (360 on every disc panorama). The cylinder's radius in picture pixels is
 * `r = width / part (radians)`; the viewer stands on the axis at the height of the `horizon` row.
 * A view has a yaw (the picture angle at the view's centre, in degrees from the picture's left
 * edge), a focal length `f` in view pixels, and a vertical `shift` in view pixels (tilt is a shift
 * of the image plane, so verticals stay vertical and the horizon stays straight).
 *
 * Screen point (x, y), measured from the view centre with y down, sees the picture point
 *   angle  = yaw + atan(x / f)
 *   row    = horizon + (y + shift) · r / √(x² + f²)
 * and the inverse is `project` below. At the centre column one view pixel covers r / f picture
 * rows, so the picture fills a view of height H only when f ≥ H · r / height (`minFocal`).
 */

export interface PanoGeometry {
  width: number;
  height: number;
  /** Picture row at eye level. */
  horizon: number;
  /** Angular width of the picture in degrees (360 = full turn, wraps). */
  part: number;
}

export interface PanoView {
  /** Degrees from the picture's left edge to the view centre. */
  yaw: number;
  /** Focal length in view pixels. */
  f: number;
  /** Vertical image-plane shift in view pixels (positive looks down). */
  shift: number;
}

const RAD = Math.PI / 180;

export function radius(g: PanoGeometry): number {
  return g.width / (g.part * RAD);
}

export function isFullTurn(g: PanoGeometry): boolean {
  return g.part >= 359.5;
}

/** Wraps degrees into [0, 360). */
export function wrapDeg(a: number): number {
  const w = a % 360;
  return w < 0 ? w + 360 : w;
}

/** Wraps an angle difference in degrees into (-180, 180]. */
export function deltaDeg(a: number): number {
  const w = wrapDeg(a);
  return w > 180 ? w - 360 : w;
}

export function focalFromFov(fovDeg: number, viewWidth: number): number {
  return viewWidth / 2 / Math.tan((Math.min(Math.max(fovDeg, 1), 170) * RAD) / 2);
}

export function fovFromFocal(f: number, viewWidth: number): number {
  return (2 * Math.atan(viewWidth / 2 / f)) / RAD;
}

/** The shortest focal length (widest view) at which the picture still fills the view's height. */
export function minFocal(g: PanoGeometry, viewHeight: number): number {
  return (viewHeight * radius(g)) / g.height;
}

/** The shift range that keeps the picture's top and bottom outside the view, at focal `f`. */
export function shiftRange(g: PanoGeometry, f: number, viewHeight: number): [number, number] {
  const r = radius(g);
  const lo = viewHeight / 2 - (g.horizon * f) / r;
  const hi = ((g.height - g.horizon) * f) / r - viewHeight / 2;
  return lo <= hi ? [lo, hi] : [(lo + hi) / 2, (lo + hi) / 2];
}

/**
 * The original's tour field of view: the view centre is 1:1 with the picture, so the focal length
 * equals the cylinder radius and hfov = 2·atan(312 / (W / 2π)), 94° for W 1824 (vm-findings (b)).
 */
export function tourHfov(pictureWidth: number, viewWidth = 624): number {
  return (2 * Math.atan(viewWidth / 2 / (pictureWidth / (2 * Math.PI)))) / RAD;
}

/**
 * The site-map wedge's screen direction in degrees clockwise from map-right: view yaw − panangle
 * (vm-findings (e)). `yaw` is degrees from the picture's left edge to the view centre.
 */
export function wedgeAngle(yaw: number, panangle: number): number {
  return deltaDeg(yaw - panangle);
}

/** Half the horizontal field of view in degrees. */
export function halfFov(f: number, viewWidth: number): number {
  return Math.atan(viewWidth / 2 / f) / RAD;
}

/** The yaw range for a partial panorama (the view's edges stay on the picture); full turns are unbounded. */
export function yawRange(g: PanoGeometry, f: number, viewWidth: number): [number, number] | null {
  if (isFullTurn(g)) return null;
  const h = halfFov(f, viewWidth);
  return h * 2 >= g.part ? [g.part / 2, g.part / 2] : [h, g.part - h];
}

export interface Limits {
  minF: number;
  maxF: number;
}

/** Applies the limits: focal length, then shift, then yaw. */
export function clampView(g: PanoGeometry, v: PanoView, lim: Limits, viewWidth: number, viewHeight: number): PanoView {
  const f = Math.min(Math.max(v.f, lim.minF), Math.max(lim.minF, lim.maxF));
  const [lo, hi] = shiftRange(g, f, viewHeight);
  const shift = Math.min(Math.max(v.shift, lo), hi);
  const yr = yawRange(g, f, viewWidth);
  const yaw = yr ? Math.min(Math.max(v.yaw, yr[0]), yr[1]) : wrapDeg(v.yaw);
  return { yaw, f, shift };
}

/** Picture pixel → view point (from the view centre, y down), or null when behind the viewer. */
export function project(g: PanoGeometry, v: PanoView, px: number, py: number): { x: number; y: number; angle: number } | null {
  const angle = isFullTurn(g) ? deltaDeg((px / g.width) * g.part - v.yaw) : (px / g.width) * g.part - v.yaw;
  if (Math.abs(angle) >= 89) return null;
  const x = v.f * Math.tan(angle * RAD);
  const y = ((py - g.horizon) * Math.hypot(x, v.f)) / radius(g) - v.shift;
  return { x, y, angle };
}

/** View point (from the view centre, y down) → picture pixel; x may need wrapping for full turns. */
export function unproject(g: PanoGeometry, v: PanoView, x: number, y: number): { px: number; py: number } {
  const angle = v.yaw + Math.atan2(x, v.f) / RAD;
  const px = (angle / g.part) * g.width;
  const py = g.horizon + ((y + v.shift) * radius(g)) / Math.hypot(x, v.f);
  return { px: isFullTurn(g) ? ((px % g.width) + g.width) % g.width : px, py };
}

/** Picture x → yaw that centres it. */
export function yawOfX(g: PanoGeometry, px: number): number {
  return (px / g.width) * g.part;
}

/**
 * The view-space bounding box of a picture rectangle (corners and edge midpoints projected; the
 * top and bottom edges bow outwards away from the centre column), or null when it is off-screen
 * or behind the viewer.
 */
export function projectRect(
  g: PanoGeometry,
  v: PanoView,
  r: { x: number; y: number; w: number; h: number },
  viewWidth: number,
  viewHeight: number,
): { x: number; y: number; w: number; h: number } | null {
  const xs = [r.x, r.x + r.w / 2, r.x + r.w];
  const ys = [r.y, r.y + r.h];
  let x0 = Infinity;
  let y0 = Infinity;
  let x1 = -Infinity;
  let y1 = -Infinity;
  for (const px of xs) {
    for (const py of ys) {
      const p = project(g, v, px, py);
      if (!p) return null;
      x0 = Math.min(x0, p.x);
      x1 = Math.max(x1, p.x);
      y0 = Math.min(y0, p.y);
      y1 = Math.max(y1, p.y);
    }
  }
  // A rectangle that straddles the seam projects its corners on both sides: treat as off-screen
  // unless all of it is in front (handled by `project` wrapping each corner separately).
  if (x1 - x0 > viewWidth * 2) return null;
  const hw = viewWidth / 2;
  const hh = viewHeight / 2;
  if (x1 < -hw || x0 > hw || y1 < -hh || y0 > hh) return null;
  return { x: x0, y: y0, w: x1 - x0, h: y1 - y0 };
}
