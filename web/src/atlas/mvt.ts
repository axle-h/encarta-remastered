/**
 * A small Mapbox Vector Tile (MVT 2.1) decoder and encoder, for re-tiling the converter's vector
 * tiles into the flat (Miller) view (miller.ts). Geometry is kept as parts of integer tile
 * coordinates: point lists, line strings, or polygon rings (exterior rings positive area in tile
 * coordinates with y down, holes negative, as the spec and the converter write them).
 */

export const POINT = 1;
export const LINESTRING = 2;
export const POLYGON = 3;

export type Pt = [number, number];
export type PropValue = string | number | boolean;

export interface Feature {
  id?: number;
  type: number;
  props: Record<string, PropValue>;
  /** Points: one part with every point. Lines: one part per line. Polygons: one part per ring. */
  parts: Pt[][];
}

export interface Layer {
  name: string;
  extent: number;
  features: Feature[];
}

// ---------------------------------------------------------------------------------------------
// Protobuf primitives

export class Reader {
  pos = 0;
  constructor(readonly b: Uint8Array) {}
  varint(): number {
    let v = 0;
    let mul = 1;
    for (;;) {
      const c = this.b[this.pos++];
      if (c === undefined) throw new Error("mvt: truncated varint");
      v += (c & 0x7f) * mul;
      if (c < 0x80) return v;
      mul *= 128;
    }
  }
  bytes(): Uint8Array {
    const n = this.varint();
    const s = this.b.subarray(this.pos, this.pos + n);
    this.pos += n;
    return s;
  }
  skip(wire: number): void {
    if (wire === 0) this.varint();
    else if (wire === 1) this.pos += 8;
    else if (wire === 2) this.pos += this.varint();
    else if (wire === 5) this.pos += 4;
    else throw new Error(`mvt: wire type ${wire}`);
  }
  get done(): boolean {
    return this.pos >= this.b.length;
  }
}

const zigzag = (n: number) => (n % 2 === 1 ? -(n + 1) / 2 : n / 2);
const unzig = (n: number) => (n < 0 ? -2 * n - 1 : 2 * n);
const utf8 = new TextDecoder();
const utf8e = new TextEncoder();

function packed(b: Uint8Array): number[] {
  const r = new Reader(b);
  const out: number[] = [];
  while (!r.done) out.push(r.varint());
  return out;
}

function decodeValue(b: Uint8Array): PropValue {
  const r = new Reader(b);
  let v: PropValue = "";
  const dv = new DataView(b.buffer, b.byteOffset, b.byteLength);
  while (!r.done) {
    const key = r.varint();
    const field = Math.floor(key / 8);
    const wire = key & 7;
    if (field === 1) v = utf8.decode(r.bytes());
    else if (field === 2) {
      v = dv.getFloat32(r.pos, true);
      r.pos += 4;
    } else if (field === 3) {
      v = dv.getFloat64(r.pos, true);
      r.pos += 8;
    } else if (field === 4 || field === 5) v = r.varint();
    else if (field === 6) v = zigzag(r.varint());
    else if (field === 7) v = r.varint() !== 0;
    else r.skip(wire);
  }
  return v;
}

function decodeGeometry(cmds: number[], type: number): Pt[][] {
  const parts: Pt[][] = [];
  let x = 0;
  let y = 0;
  let cur: Pt[] | null = null;
  let i = 0;
  while (i < cmds.length) {
    const c = cmds[i++]!;
    const id = c & 7;
    const count = c >>> 3;
    if (id === 7) {
      cur = null;
      continue;
    }
    for (let k = 0; k < count; k++) {
      x += zigzag(cmds[i++]!);
      y += zigzag(cmds[i++]!);
      if (id === 1 && (type !== POINT || !cur)) {
        cur = [];
        parts.push(cur);
      }
      cur!.push([x, y]);
    }
  }
  return parts;
}

export function decodeTile(data: Uint8Array): Layer[] {
  const r = new Reader(data);
  const layers: Layer[] = [];
  while (!r.done) {
    const key = r.varint();
    if (key >>> 3 !== 3 || (key & 7) !== 2) {
      r.skip(key & 7);
      continue;
    }
    const lr = new Reader(r.bytes());
    const layer: Layer = { name: "", extent: 4096, features: [] };
    const keys: string[] = [];
    const values: PropValue[] = [];
    const raw: { id?: number; type: number; tags: number[]; geom: number[] }[] = [];
    while (!lr.done) {
      const k = lr.varint();
      const field = k >>> 3;
      if (field === 1) layer.name = utf8.decode(lr.bytes());
      else if (field === 3) keys.push(utf8.decode(lr.bytes()));
      else if (field === 4) values.push(decodeValue(lr.bytes()));
      else if (field === 5) layer.extent = lr.varint();
      else if (field === 2) {
        const fr = new Reader(lr.bytes());
        const f: { id?: number; type: number; tags: number[]; geom: number[] } = { type: 0, tags: [], geom: [] };
        while (!fr.done) {
          const fk = fr.varint();
          const ff = fk >>> 3;
          if (ff === 1) f.id = fr.varint();
          else if (ff === 2) f.tags = packed(fr.bytes());
          else if (ff === 3) f.type = fr.varint();
          else if (ff === 4) f.geom = packed(fr.bytes());
          else fr.skip(fk & 7);
        }
        raw.push(f);
      } else lr.skip(k & 7);
    }
    for (const f of raw) {
      const props: Record<string, PropValue> = {};
      for (let t = 0; t + 1 < f.tags.length; t += 2) {
        const name = keys[f.tags[t]!];
        const v = values[f.tags[t + 1]!];
        if (name !== undefined && v !== undefined) props[name] = v;
      }
      layer.features.push({ id: f.id, type: f.type, props, parts: decodeGeometry(f.geom, f.type) });
    }
    layers.push(layer);
  }
  return layers;
}

// ---------------------------------------------------------------------------------------------
// Encoding

class Writer {
  buf: number[] = [];
  varint(v: number): void {
    while (v >= 0x80) {
      this.buf.push((v % 128) | 0x80);
      v = Math.floor(v / 128);
    }
    this.buf.push(v);
  }
  key(field: number, wire: number): void {
    this.varint(field * 8 + wire);
  }
  bytes(field: number, b: ArrayLike<number>): void {
    this.key(field, 2);
    this.varint(b.length);
    for (let i = 0; i < b.length; i++) this.buf.push(b[i]!);
  }
  packed(field: number, vals: number[]): void {
    const w = new Writer();
    for (const v of vals) w.varint(v);
    this.bytes(field, w.buf);
  }
}

function encodeValue(v: PropValue): number[] {
  const w = new Writer();
  if (typeof v === "string") w.bytes(1, utf8e.encode(v));
  else if (typeof v === "boolean") {
    w.key(7, 0);
    w.varint(v ? 1 : 0);
  } else if (Number.isInteger(v) && v >= 0) {
    w.key(5, 0);
    w.varint(v);
  } else if (Number.isInteger(v)) {
    w.key(6, 0);
    w.varint(unzig(v));
  } else {
    w.key(3, 1);
    const b = new Uint8Array(8);
    new DataView(b.buffer).setFloat64(0, v, true);
    w.buf.push(...b);
  }
  return w.buf;
}

function encodeGeometry(f: Feature): number[] {
  const out: number[] = [];
  let x = 0;
  let y = 0;
  const move = (p: Pt) => {
    out.push(unzig(p[0] - x), unzig(p[1] - y));
    x = p[0];
    y = p[1];
  };
  if (f.type === POINT) {
    const pts = f.parts.flat();
    if (!pts.length) return out;
    out.push(1 | (pts.length << 3));
    for (const p of pts) move(p);
    return out;
  }
  for (const part of f.parts) {
    const n = f.type === POLYGON ? part.length : part.length;
    if (n < (f.type === POLYGON ? 3 : 2)) continue;
    out.push(1 | (1 << 3));
    move(part[0]!);
    out.push(2 | ((n - 1) << 3));
    for (let i = 1; i < n; i++) move(part[i]!);
    if (f.type === POLYGON) out.push(7 | (1 << 3));
  }
  return out;
}

export function encodeTile(layers: readonly Layer[]): Uint8Array {
  const w = new Writer();
  for (const layer of layers) {
    if (!layer.features.length) continue;
    const lw = new Writer();
    lw.key(15, 0);
    lw.varint(2);
    lw.bytes(1, utf8e.encode(layer.name));
    const keys = new Map<string, number>();
    const values = new Map<string, number>();
    const valueList: PropValue[] = [];
    for (const f of layer.features) {
      const geom = encodeGeometry(f);
      if (!geom.length) continue;
      const tags: number[] = [];
      for (const [k, v] of Object.entries(f.props)) {
        let ki = keys.get(k);
        if (ki === undefined) keys.set(k, (ki = keys.size));
        const vk = `${typeof v}:${String(v)}`;
        let vi = values.get(vk);
        if (vi === undefined) {
          values.set(vk, (vi = valueList.length));
          valueList.push(v);
        }
        tags.push(ki, vi);
      }
      const fw = new Writer();
      if (f.id !== undefined) {
        fw.key(1, 0);
        fw.varint(f.id);
      }
      if (tags.length) fw.packed(2, tags);
      fw.key(3, 0);
      fw.varint(f.type);
      fw.packed(4, geom);
      lw.bytes(2, fw.buf);
    }
    for (const k of keys.keys()) lw.bytes(3, utf8e.encode(k));
    for (const v of valueList) lw.bytes(4, encodeValue(v));
    lw.key(5, 0);
    lw.varint(layer.extent);
    w.bytes(3, lw.buf);
  }
  return new Uint8Array(w.buf);
}

// ---------------------------------------------------------------------------------------------
// Clipping (tile coordinates, as floats)

export type FPt = [number, number];

/** Sutherland–Hodgman against an axis-aligned box. */
export function clipRing(ring: readonly FPt[], x0: number, y0: number, x1: number, y1: number): FPt[] {
  let pts: FPt[] = ring.slice();
  const edges: [(p: FPt) => boolean, (a: FPt, b: FPt) => FPt][] = [
    [(p) => p[0] >= x0, (a, b) => [x0, a[1] + ((b[1] - a[1]) * (x0 - a[0])) / (b[0] - a[0])]],
    [(p) => p[0] <= x1, (a, b) => [x1, a[1] + ((b[1] - a[1]) * (x1 - a[0])) / (b[0] - a[0])]],
    [(p) => p[1] >= y0, (a, b) => [a[0] + ((b[0] - a[0]) * (y0 - a[1])) / (b[1] - a[1]), y0]],
    [(p) => p[1] <= y1, (a, b) => [a[0] + ((b[0] - a[0]) * (y1 - a[1])) / (b[1] - a[1]), y1]],
  ];
  for (const [inside, cut] of edges) {
    if (!pts.length) break;
    const out: FPt[] = [];
    for (let i = 0; i < pts.length; i++) {
      const a = pts[(i + pts.length - 1) % pts.length]!;
      const b = pts[i]!;
      const ia = inside(a);
      const ib = inside(b);
      if (ib) {
        if (!ia) out.push(cut(a, b));
        out.push(b);
      } else if (ia) out.push(cut(a, b));
    }
    pts = out;
  }
  return pts;
}

/** Splits a line at the box edges and keeps the parts inside (Liang–Barsky per segment). */
export function clipLine(line: readonly FPt[], x0: number, y0: number, x1: number, y1: number): FPt[][] {
  const out: FPt[][] = [];
  let cur: FPt[] = [];
  for (let i = 0; i + 1 < line.length; i++) {
    const a = line[i]!;
    const b = line[i + 1]!;
    const dx = b[0] - a[0];
    const dy = b[1] - a[1];
    let t0 = 0;
    let t1 = 1;
    let ok = true;
    for (const [p, q] of [
      [-dx, a[0] - x0],
      [dx, x1 - a[0]],
      [-dy, a[1] - y0],
      [dy, y1 - a[1]],
    ] as const) {
      if (p === 0) {
        if (q < 0) ok = false;
      } else {
        const t = q / p;
        if (p < 0) t0 = Math.max(t0, t);
        else t1 = Math.min(t1, t);
      }
    }
    if (!ok || t0 > t1) {
      if (cur.length > 1) out.push(cur);
      cur = [];
      continue;
    }
    const pa: FPt = [a[0] + t0 * dx, a[1] + t0 * dy];
    const pb: FPt = [a[0] + t1 * dx, a[1] + t1 * dy];
    if (!cur.length || t0 > 0) {
      if (cur.length > 1) out.push(cur);
      cur = [pa];
    }
    cur.push(pb);
    if (t1 < 1) {
      out.push(cur);
      cur = [];
    }
  }
  if (cur.length > 1) out.push(cur);
  return out;
}

/** Twice the signed area (positive = exterior in MVT's y-down coordinates). */
export function area2(ring: readonly Pt[] | readonly FPt[]): number {
  let s = 0;
  for (let i = 0; i < ring.length; i++) {
    const a = ring[i]!;
    const b = ring[(i + 1) % ring.length]!;
    s += a[0] * b[1] - b[0] * a[1];
  }
  return s;
}
