/**
 * Walk maps (tours-3d.md §6.2–§6.5): which cell of each kind the eye is over, on the original's
 * 4096 × 4096 grid with the 1–3-unit gap closing, plus ground heights and the walk-map cache.
 *
 * All coordinates are glTF space. The grid row uses the original z (−z'), so
 * `gz = trunc((bbox.max.z − z') / (bbox.max.z − bbox.min.z) · 4095)` (docs/tour-scene.md).
 */
import type { Cell, CellKind, Step, WalkMapFile } from "../types";

export const GRID = 4096;
const BUCKET_SHIFT = 6; // 64 grid units per bucket
const BUCKETS = GRID >> BUCKET_SHIFT;

type CellOf<K extends CellKind> = Extract<Cell, { kind: K }>;

interface Entry {
  cell: Cell;
  /** Quantised polygon vertices, x then z, per polygon. */
  polys: Int32Array[];
  minX: number;
  maxX: number;
  minZ: number;
  maxZ: number;
}

export interface Segment {
  x1: number;
  z1: number;
  x2: number;
  z2: number;
  top: number;
  climbable: boolean;
}

export class WalkMap {
  readonly name: string;
  readonly file: WalkMapFile;
  private readonly minX: number;
  private readonly minZ: number;
  private readonly spanX: number;
  private readonly spanZ: number;
  private readonly maxZ: number;
  /** Per kind: buckets of entries, file order kept within each bucket. */
  private readonly index = new Map<CellKind, Entry[][]>();
  /** Collision segments per collision cell index. */
  readonly segments = new Map<number, Segment[]>();
  /** Hotspot cell the eye is over (§6.5 "current"). */
  current: CellOf<"hotspot"> | null = null;
  private readonly deleted = new Set<CellKind>();

  constructor(file: WalkMapFile) {
    this.file = file;
    this.name = file.file;
    this.minX = file.bbox.min[0];
    this.minZ = file.bbox.min[2];
    this.maxZ = file.bbox.max[2];
    this.spanX = file.bbox.max[0] - this.minX;
    this.spanZ = this.maxZ - this.minZ;
    for (const cell of file.cells) {
      if (cell.kind === "collision") {
        this.segments.set(
          cell.index,
          cell.segments.map(([x1, z1, x2, z2, top, climbable]) => ({ x1, z1, x2, z2, top, climbable: climbable !== 0 })),
        );
      }
      if (!this.valid()) continue;
      const polys: Int32Array[] = [];
      let minX = GRID, maxX = -1, minZ = GRID, maxZ = -1;
      for (const p of cell.polygons) {
        const q = new Int32Array((p.length / 3) * 2);
        for (let i = 0, j = 0; i + 2 < p.length; i += 3, j += 2) {
          const gx = this.gx(p[i] ?? 0);
          const gz = this.gz(p[i + 2] ?? 0);
          q[j] = gx;
          q[j + 1] = gz;
          minX = Math.min(minX, gx); maxX = Math.max(maxX, gx);
          minZ = Math.min(minZ, gz); maxZ = Math.max(maxZ, gz);
        }
        polys.push(q);
      }
      if (polys.length === 0) continue;
      const entry: Entry = { cell, polys, minX, maxX, minZ, maxZ };
      let buckets = this.index.get(cell.kind);
      if (!buckets) {
        buckets = Array.from({ length: BUCKETS * BUCKETS }, () => []);
        this.index.set(cell.kind, buckets);
      }
      const bx0 = Math.max(0, minX) >> BUCKET_SHIFT, bx1 = Math.min(GRID - 1, maxX) >> BUCKET_SHIFT;
      const bz0 = Math.max(0, minZ) >> BUCKET_SHIFT, bz1 = Math.min(GRID - 1, maxZ) >> BUCKET_SHIFT;
      for (let bz = bz0; bz <= bz1; bz++) for (let bx = bx0; bx <= bx1; bx++) buckets[bz * BUCKETS + bx]?.push(entry);
    }
  }

  private valid(): boolean {
    return this.spanX > 0.001 && this.spanZ > 0.001;
  }

  /** Grid column (may fall outside 0…4095). */
  gx(x: number): number {
    return Math.trunc(((x - this.minX) / this.spanX) * (GRID - 1));
  }

  /** Grid row from a glTF z (original z = −z'). */
  gz(z: number): number {
    return Math.trunc(((this.maxZ - z) / this.spanZ) * (GRID - 1));
  }

  /** `WALKMAP.CELL?`: grid coordinates of a point, or null when it is "nowhere". */
  cellOf(x: number, z: number): [number, number] | null {
    if (!this.valid()) return null;
    const gx = this.gx(x), gz = this.gz(z);
    if (gx < 0 || gx >= GRID || gz < 0 || gz >= GRID) return null;
    return [gx, gz];
  }

  private hitAt<K extends CellKind>(kind: K, gx: number, gz: number): CellOf<K> | null {
    if (gx < 0 || gx >= GRID) return null;
    const bucket = this.index.get(kind)?.[(gz >> BUCKET_SHIFT) * BUCKETS + (gx >> BUCKET_SHIFT)];
    if (!bucket) return null;
    for (const e of bucket) {
      if (gx < e.minX || gx > e.maxX || gz < e.minZ || gz > e.maxZ) continue;
      for (const q of e.polys) if (insideConvex(q, gx, gz)) return e.cell as CellOf<K>;
    }
    return null;
  }

  /**
   * The first cell of a kind (file order) whose polygons contain the point, on the quantised grid.
   * Gaps of 1–3 grid units between two cells of the same kind in a row belong to the left cell
   * (§6.3 gap closing).
   */
  find<K extends CellKind>(kind: K, x: number, z: number): CellOf<K> | null {
    if (this.deleted.has(kind)) return null;
    const g = this.cellOf(x, z);
    if (!g) return null;
    const [gx, gz] = g;
    const hit = this.hitAt(kind, gx, gz);
    if (hit) return hit;
    for (let a = 1; a <= 3; a++) {
      const left = this.hitAt(kind, gx - a, gz);
      if (!left) continue;
      for (let b = 1; a + b - 1 <= 3; b++) if (this.hitAt(kind, gx + b, gz)) return left;
      return null;
    }
    return null;
  }

  /** Ground height under the eye (§6.8): projection onto the cell plane along its normal, or null. */
  groundY(x: number, y: number, z: number): number | null {
    const cell = this.find("ground", x, z);
    if (!cell || !cell.plane) return null;
    const [a, b, c, d] = cell.plane;
    const dist = a * x + b * y + c * z + d;
    return y - dist * b;
  }

  /** `WALKMAP.DELETE.<kind>` (§6.10). `visibility.data` keeps the cells but drops the PVS. */
  deleteKind(what: string): boolean {
    const kinds: Record<string, CellKind> = { visibility: "visibility", ground: "ground", collision: "collision", hotspot: "hotspot" };
    if (what === "visibility.data") {
      this.deleted.add("visibility");
      return true;
    }
    const k = kinds[what];
    if (!k) return false;
    this.deleted.add(k);
    if (k === "hotspot") this.current = null;
    return true;
  }

  hasKind(kind: CellKind): boolean {
    return !this.deleted.has(kind);
  }

  /** Collision cells whose polygons contain the point (for the coverage test and segment gathering). */
  collisionCellsAt(x: number, z: number): CellOf<"collision">[] {
    if (this.deleted.has("collision")) return [];
    const g = this.cellOf(x, z);
    if (!g) return [];
    const bucket = this.index.get("collision")?.[(g[1] >> BUCKET_SHIFT) * BUCKETS + (g[0] >> BUCKET_SHIFT)];
    const out: CellOf<"collision">[] = [];
    for (const e of bucket ?? []) {
      if (g[0] < e.minX || g[0] > e.maxX || g[1] < e.minZ || g[1] > e.maxZ) continue;
      if (e.polys.some((q) => insideConvex(q, g[0], g[1]))) out.push(e.cell as CellOf<"collision">);
    }
    return out;
  }

  /** Grid unit in world units (the smaller axis). */
  gridUnit(): number {
    return Math.min(this.spanX, this.spanZ) / (GRID - 1);
  }
}

/** Inclusive point-in-convex-polygon on integer coordinates (either winding; degenerate polygons count their edges). */
export function insideConvex(q: Int32Array, x: number, z: number): boolean {
  const n = q.length / 2;
  let pos = false, neg = false;
  for (let i = 0; i < n; i++) {
    const ax = q[2 * i] ?? 0, az = q[2 * i + 1] ?? 0;
    const j = (i + 1) % n;
    const bx = q[2 * j] ?? 0, bz = q[2 * j + 1] ?? 0;
    const cross = (bx - ax) * (z - az) - (bz - az) * (x - ax);
    if (cross > 0) pos = true;
    else if (cross < 0) neg = true;
    if (pos && neg) return false;
  }
  if (!pos && !neg) {
    // Fully degenerate: inside only when on one of the edges' bounding boxes.
    for (let i = 0; i < n; i++) {
      const ax = q[2 * i] ?? 0, az = q[2 * i + 1] ?? 0;
      const j = (i + 1) % n;
      const bx = q[2 * j] ?? 0, bz = q[2 * j + 1] ?? 0;
      if (x >= Math.min(ax, bx) && x <= Math.max(ax, bx) && z >= Math.min(az, bz) && z <= Math.max(az, bz)) return true;
    }
    return false;
  }
  return true;
}

/**
 * The walk-map cache (§6.4): maps keyed by file name (case-insensitive), exactly one active, plus
 * the hotspot enter/exit bookkeeping of §6.5.
 */
export class WalkMapSet {
  private readonly cache = new Map<string, WalkMap>();
  active: WalkMap | null = null;
  /** `WALKMAP.ON/OFF`; `TOGGLE` negates it, so it never switches off (§6.10). */
  on = 0;
  private lastSeen: WalkMap | null = null;

  /**
   * How a switch to an already-cached map treats its hotspot cells (the ordering is a VM item,
   * §6.5). `settle`: like a first load, the new map's current cell becomes the one under the eye
   * without running its enter macro, and the switch counts as seen. `spec`: the literal reading of
   * §6.5 (the next check runs the old cell's exit, clears both maps and enters the cell under the
   * eye), which makes the overlapping stair strips flip the map on every tick inside the middle
   * strip.
   */
  switchRule: "settle" | "spec" = "settle";

  constructor(private readonly available: (name: string) => WalkMapFile | null) {}

  get enabled(): boolean {
    return this.on !== 0 && this.active !== null;
  }

  /**
   * `WALKMAP.LOAD`: switch to a cached map, or load it, make it active and initialise its current
   * hotspot cell under the eye without running the enter macro.
   */
  load(name: string, eye: { x: number; z: number }): boolean {
    const key = name.toLowerCase();
    const cached = this.cache.get(key);
    if (cached) {
      if (this.switchRule === "settle" && cached !== this.active) {
        cached.current = cached.find("hotspot", eye.x, eye.z);
        if (this.lastSeen) this.lastSeen = cached;
      }
      this.active = cached;
      return true;
    }
    const file = this.available(name);
    if (!file) return false;
    const map = new WalkMap(file);
    this.cache.set(key, map);
    this.active = map;
    map.current = map.find("hotspot", eye.x, eye.z);
    return true;
  }

  maps(): WalkMap[] {
    return [...this.cache.values()];
  }

  clear(): void {
    this.cache.clear();
    this.active = null;
    this.lastSeen = null;
    this.on = 0;
  }

  /**
   * The hotspot check after every camera move (§6.5). When the active map changed since the last
   * check, the old map's current cell's exit macro runs and both maps forget their current cell;
   * the cell test then uses the map that was active when the check started.
   */
  hotspotCheck(x: number, z: number, run: (script: Step[]) => void): void {
    if (!this.enabled) return;
    const map = this.active;
    if (!map) return;
    if (map !== this.lastSeen) {
      const old = this.lastSeen;
      this.lastSeen = map;
      if (old) {
        const cur = old.current;
        old.current = null;
        map.current = null;
        if (cur) run(cur.exit.steps);
      }
    }
    const cell = map.find("hotspot", x, z);
    if (cell === map.current) return;
    const prev = map.current;
    map.current = cell;
    if (prev) run(prev.exit.steps);
    if (cell) run(cell.enter.steps);
  }
}

