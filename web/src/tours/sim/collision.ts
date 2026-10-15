/**
 * Wall collision in the XZ plane (tours-3d.md §6.7): a circle of radius R moving against the wall
 * segments of the collision cells the move passes over, with one slide along the first wall hit, a
 * second sweep for corners, and a final push-out from walls and wall endpoints. Leaving the
 * collision-cell coverage does not stop a move (VM: an open edge 0.72 from any wall is crossed at
 * full speed); `gather` still reports the gap.
 */
import type { Segment, WalkMap } from "./walkmap";

export interface Point2 {
  x: number;
  z: number;
}

const EPS = 1e-6;

interface Hit {
  t: number;
  seg: Segment;
}

/** Earliest crossing of the move P→P+v into the R-inflated band of a segment (from outside). */
export function sweep(px: number, pz: number, vx: number, vz: number, segs: readonly Segment[], R: number, skip?: Segment): Hit | null {
  let best: Hit | null = null;
  for (const s of segs) {
    if (s === skip) continue;
    const ax = s.x2 - s.x1, az = s.z2 - s.z1;
    const len = Math.hypot(ax, az);
    if (len < EPS) continue;
    const wx = ax / len, wz = az / len;
    const nx = -wz, nz = wx;
    const sp = (px - s.x1) * nx + (pz - s.z1) * nz;
    const sq = (px + vx - s.x1) * nx + (pz + vz - s.z1) * nz;
    let t = -1;
    if (sp >= R && sq < R) t = (sp - R) / (sp - sq);
    else if (sp <= -R && sq > -R) t = (-R - sp) / (sq - sp);
    if (t < 0 || t > 1) continue;
    const hx = px + vx * t, hz = pz + vz * t;
    const u = (hx - s.x1) * wx + (hz - s.z1) * wz;
    if (u < 0 || u > len) continue;
    if (!best || t < best.t) best = { t, seg: s };
  }
  return best;
}

/** Distance from a point to a segment, and the closest point. */
export function closestOnSegment(px: number, pz: number, s: Segment): { x: number; z: number; d: number } {
  const ax = s.x2 - s.x1, az = s.z2 - s.z1;
  const l2 = ax * ax + az * az;
  let u = l2 > 0 ? ((px - s.x1) * ax + (pz - s.z1) * az) / l2 : 0;
  u = Math.max(0, Math.min(1, u));
  const x = s.x1 + ax * u, z = s.z1 + az * u;
  return { x, z, d: Math.hypot(px - x, pz - z) };
}

/** Pushes the point out of every segment band and endpoint disc closer than R. */
export function depenetrate(p: Point2, segs: readonly Segment[], R: number, from: Point2): Point2 {
  let { x, z } = p;
  for (let pass = 0; pass < 2; pass++) {
    for (const s of segs) {
      const c = closestOnSegment(x, z, s);
      if (c.d >= R) continue;
      if (c.d > EPS) {
        x = c.x + ((x - c.x) / c.d) * R;
        z = c.z + ((z - c.z) / c.d) * R;
      } else {
        // Exactly on the wall: push towards the side the move came from.
        const ax = s.x2 - s.x1, az = s.z2 - s.z1;
        const len = Math.hypot(ax, az) || 1;
        let nx = -az / len, nz = ax / len;
        if ((from.x - c.x) * nx + (from.z - c.z) * nz < 0) { nx = -nx; nz = -nz; }
        x = c.x + nx * R;
        z = c.z + nz * R;
      }
    }
  }
  return { x, z };
}

/** Collision segments the move meets, and whether the extended move leaves the collision coverage. */
export function gather(map: WalkMap, px: number, pz: number, dx: number, dz: number, R: number): { segs: Segment[]; gap: boolean } {
  const len = Math.hypot(dx, dz);
  const ux = dx / len, uz = dz / len;
  const reach = len + R;
  const unit = Math.max(map.gridUnit(), reach / 64);
  const n = Math.max(1, Math.ceil(reach / unit));
  const cells = new Set<number>();
  let gap = false;
  for (let i = 0; i <= n; i++) {
    const f = (reach * i) / n;
    const found = map.collisionCellsAt(px + ux * f, pz + uz * f);
    if (found.length === 0) gap = true;
    for (const c of found) cells.add(c.index);
  }
  const segs: Segment[] = [];
  for (const idx of cells) {
    for (const s of map.segments.get(idx) ?? []) {
      if (closestOnSegment(px, pz, s).d < len + R) segs.push(s);
    }
  }
  return { segs, gap };
}

/**
 * Resolves one horizontal move. Returns the new eye XZ (the target point moves with the eye, so
 * the view direction is unchanged).
 */
export function collide(map: WalkMap, from: Point2, dx: number, dz: number, R: number): Point2 {
  const len = Math.hypot(dx, dz);
  if (len < 1e-12) return { ...from };
  const { segs } = gather(map, from.x, from.z, dx, dz, R);
  if (segs.length === 0) return { x: from.x + dx, z: from.z + dz };
  return slideMove(from, dx, dz, segs, R);
}

/** Sweep, slide along the first wall, sweep the slide once more (a corner), then push out. */
export function slideMove(from: Point2, dx: number, dz: number, segs: readonly Segment[], R: number): Point2 {
  let x = from.x, z = from.z;
  const hit = sweep(x, z, dx, dz, segs, R);
  if (!hit) {
    x += dx;
    z += dz;
  } else {
    const t = Math.max(0, hit.t - EPS);
    x += dx * t;
    z += dz * t;
    const rx = dx * (1 - hit.t), rz = dz * (1 - hit.t);
    const s = hit.seg;
    const wl = Math.hypot(s.x2 - s.x1, s.z2 - s.z1);
    const wx = (s.x2 - s.x1) / wl, wz = (s.z2 - s.z1) / wl;
    const along = rx * wx + rz * wz;
    const sx = wx * along, sz = wz * along;
    const hit2 = sweep(x, z, sx, sz, segs, R, s);
    const t2 = hit2 ? Math.max(0, hit2.t - EPS) : 1;
    x += sx * t2;
    z += sz * t2;
  }
  return depenetrate({ x, z }, segs, R, from);
}
