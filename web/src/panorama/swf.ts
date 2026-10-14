/**
 * Reads the picture out of a single panorama's image SWF (`<slide><image>T627478A.swf`), from the
 * public SWF file format: the header, the tag list, `DefineBitsJPEG2`/`3` (the JPEG) and the bitmap
 * fill of the `DefineShape` that draws it. On the disc (167 `vrslide` interactives, all checked) each
 * image SWF is an uncompressed version-5 file holding one JPEG (1200–7645 × 183–992) and a shape
 * that fills it at 1/10 scale; the shape's origin row is the panorama's horizon (docs/web-client.md,
 * "Panoramas").
 */

export interface SwfPanorama {
  /** The JPEG stream, ready for a Blob of type image/jpeg. */
  jpeg: Uint8Array;
  /** Picture size in pixels, from the JPEG's SOF marker (0 when the marker is missing). */
  width: number;
  height: number;
  /**
   * The picture row on the shape's y = 0 line, i.e. the horizon (eye level) of the cylinder, or
   * null when the SWF has no bitmap-filled shape for the picture.
   */
  horizon: number | null;
}

interface Tag {
  code: number;
  body: Uint8Array;
}

class Bits {
  private pos: number;
  constructor(
    private readonly b: Uint8Array,
    byteOffset = 0,
  ) {
    this.pos = byteOffset * 8;
  }
  u(n: number): number {
    let v = 0;
    for (let i = 0; i < n; i++) {
      const byte = this.b[this.pos >> 3] ?? 0;
      v = v * 2 + ((byte >> (7 - (this.pos & 7))) & 1);
      this.pos++;
    }
    return v;
  }
  s(n: number): number {
    if (n === 0) return 0;
    const v = this.u(n);
    return v >= 2 ** (n - 1) ? v - 2 ** n : v;
  }
  align(): void {
    this.pos = (this.pos + 7) & ~7;
  }
  get byte(): number {
    return this.pos >> 3;
  }
}

function readRect(bits: Bits): [number, number, number, number] {
  const n = bits.u(5);
  const r: [number, number, number, number] = [bits.s(n), bits.s(n), bits.s(n), bits.s(n)];
  bits.align();
  return r;
}

/** A MATRIX record: scale (16.16), rotate/skew (16.16) and translation (twips). */
interface Matrix {
  sx: number;
  sy: number;
  r0: number;
  r1: number;
  tx: number;
  ty: number;
}

function readMatrix(bits: Bits): Matrix {
  const m: Matrix = { sx: 1, sy: 1, r0: 0, r1: 0, tx: 0, ty: 0 };
  if (bits.u(1)) {
    const n = bits.u(5);
    m.sx = bits.s(n) / 65536;
    m.sy = bits.s(n) / 65536;
  }
  if (bits.u(1)) {
    const n = bits.u(5);
    m.r0 = bits.s(n) / 65536;
    m.r1 = bits.s(n) / 65536;
  }
  const n = bits.u(5);
  m.tx = bits.s(n);
  m.ty = bits.s(n);
  bits.align();
  return m;
}

function u16(b: Uint8Array, p: number): number {
  return (b[p] ?? 0) | ((b[p + 1] ?? 0) << 8);
}

function u32(b: Uint8Array, p: number): number {
  return (u16(b, p) | (u16(b, p + 2) << 16)) >>> 0;
}

function readTags(body: Uint8Array, start: number): Tag[] {
  const tags: Tag[] = [];
  let p = start;
  while (p + 2 <= body.length) {
    const head = u16(body, p);
    p += 2;
    const code = head >> 6;
    let len = head & 63;
    if (len === 63) {
      len = u32(body, p);
      p += 4;
    }
    if (p + len > body.length) break;
    tags.push({ code, body: body.subarray(p, p + len) });
    p += len;
    if (code === 0) break;
  }
  return tags;
}

/** Inflates a `CWS` body (zlib) with the platform's DecompressionStream. */
async function inflate(data: Uint8Array): Promise<Uint8Array> {
  const ds = new DecompressionStream("deflate");
  const stream = new Blob([data]).stream().pipeThrough(ds);
  return new Uint8Array(await new Response(stream).arrayBuffer());
}

/** The SWF body after the 8-byte header (signature, version, length), uncompressed. */
export async function swfBody(file: Uint8Array): Promise<Uint8Array> {
  const sig = String.fromCharCode(file[0] ?? 0, file[1] ?? 0, file[2] ?? 0);
  if (sig === "FWS") return file.subarray(8);
  if (sig === "CWS") return inflate(file.subarray(8));
  throw new Error(`not a SWF file (${sig === "ZWS" ? "LZMA SWF is not supported" : "bad signature"})`);
}

/** Strips the pre-SWF8 "erroneous header" (FF D9 FF D8) some encoders put before the JPEG. */
function cleanJpeg(j: Uint8Array): Uint8Array {
  if (j[0] === 0xff && j[1] === 0xd9 && j[2] === 0xff && j[3] === 0xd8) return j.subarray(4);
  return j;
}

/** Width and height from the first SOF0–SOF3 marker of a JPEG stream. */
export function jpegSize(j: Uint8Array): { width: number; height: number } {
  let p = 2;
  while (p + 9 < j.length) {
    if (j[p] !== 0xff) {
      p++;
      continue;
    }
    const m = j[p + 1]!;
    if (m >= 0xc0 && m <= 0xc3) return { height: (j[p + 5]! << 8) | j[p + 6]!, width: (j[p + 7]! << 8) | j[p + 8]! };
    if (m === 0xd8 || m === 0x01 || (m >= 0xd0 && m <= 0xd7) || m === 0xff) {
      p += m === 0xff ? 1 : 2;
      continue;
    }
    p += 2 + ((j[p + 2]! << 8) | j[p + 3]!);
  }
  return { width: 0, height: 0 };
}

/**
 * The bitmap fill (`0x41` clipped or `0x40`–`0x43`) of a DefineShape/2/3/4 that uses `bitmapId`,
 * or null. Only the first fill style array is read, which is where the disc's shapes keep it.
 */
function bitmapFill(tag: Tag, bitmapId: number): Matrix | null {
  const b = tag.body;
  const bits = new Bits(b, 2);
  readRect(bits);
  if (tag.code === 83) {
    readRect(bits); // DefineShape4: edge bounds and a flags byte
    bits.u(8);
  }
  let p = bits.byte;
  let count = b[p++] ?? 0;
  if (count === 0xff && tag.code !== 2) {
    count = u16(b, p);
    p += 2;
  }
  const rgba = tag.code === 32 || tag.code === 83;
  for (let i = 0; i < count; i++) {
    const type = b[p++];
    if (type === 0x00) {
      p += rgba ? 4 : 3;
    } else if (type === 0x40 || type === 0x41 || type === 0x42 || type === 0x43) {
      const id = u16(b, p);
      const mb = new Bits(b, p + 2);
      const m = readMatrix(mb);
      if (id === bitmapId) return m;
      p = mb.byte;
    } else {
      return null; // gradients: not used by the panorama shapes; stop rather than misparse
    }
  }
  return null;
}

/** Extracts the panorama picture and its horizon from an image SWF. */
export async function readPanoramaSwf(file: Uint8Array): Promise<SwfPanorama> {
  const body = await swfBody(file);
  const rect = new Bits(body);
  readRect(rect);
  const tags = readTags(body, rect.byte + 4);
  let jpeg: Uint8Array | null = null;
  let bitmapId = -1;
  for (const t of tags) {
    if (t.code === 21) {
      bitmapId = u16(t.body, 0);
      jpeg = cleanJpeg(t.body.subarray(2));
      break;
    }
    if (t.code === 35 || t.code === 90) {
      bitmapId = u16(t.body, 0);
      const len = u32(t.body, 2);
      const start = t.code === 90 ? 8 : 6;
      jpeg = cleanJpeg(t.body.subarray(start, start + len));
      break;
    }
  }
  if (!jpeg) throw new Error("the SWF holds no JPEG picture");
  const { width, height } = jpegSize(jpeg);
  let horizon: number | null = null;
  for (const t of tags) {
    if (t.code !== 2 && t.code !== 22 && t.code !== 32 && t.code !== 83) continue;
    const m = bitmapFill(t, bitmapId);
    // The fill maps picture pixels to shape twips: y_twips = sy * row + ty, so the shape's
    // origin row is -ty / sy.
    if (m && m.sy > 0) {
      horizon = -m.ty / m.sy;
      break;
    }
  }
  return { jpeg, width, height, horizon };
}
