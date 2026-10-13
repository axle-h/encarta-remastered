/**
 * A small PNG reader for the converter's index tiles (8-bit grey, the raw CMT value per pixel;
 * docs/atlas-output.md "Rasters"). Decoding the bytes ourselves, instead of through a canvas, keeps
 * the values exact: no colour management, gamma or premultiplication can touch them.
 *
 * Supports 8-bit, non-interlaced images of every colour type; the first channel is returned (the
 * value for grey, the palette index for indexed images).
 */

export interface PngValues {
  width: number;
  height: number;
  /** One byte per pixel, row-major. */
  values: Uint8Array;
}

const SIGNATURE = [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a];
const CHANNELS: Record<number, number> = { 0: 1, 2: 3, 3: 1, 4: 2, 6: 4 };

export async function inflate(data: Uint8Array): Promise<Uint8Array> {
  const stream = new Blob([data]).stream().pipeThrough(new DecompressionStream("deflate"));
  return new Uint8Array(await new Response(stream).arrayBuffer());
}

export async function decodePngValues(bytes: Uint8Array): Promise<PngValues> {
  for (let i = 0; i < 8; i++) if (bytes[i] !== SIGNATURE[i]) throw new Error("not a PNG");
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  let width = 0;
  let height = 0;
  let channels = 1;
  const idat: Uint8Array[] = [];
  let pos = 8;
  while (pos + 8 <= bytes.length) {
    const len = view.getUint32(pos);
    const type = String.fromCharCode(bytes[pos + 4]!, bytes[pos + 5]!, bytes[pos + 6]!, bytes[pos + 7]!);
    const body = bytes.subarray(pos + 8, pos + 8 + len);
    if (type === "IHDR") {
      width = view.getUint32(pos + 8);
      height = view.getUint32(pos + 12);
      const depth = body[8];
      const colourType = body[9]!;
      const interlace = body[12];
      if (depth !== 8) throw new Error(`PNG bit depth ${depth} not supported`);
      if (interlace !== 0) throw new Error("interlaced PNG not supported");
      const c = CHANNELS[colourType];
      if (!c) throw new Error(`PNG colour type ${colourType} not supported`);
      channels = c;
    } else if (type === "IDAT") {
      idat.push(body);
    } else if (type === "IEND") {
      break;
    }
    pos += 12 + len;
  }
  if (!width || !height) throw new Error("PNG without IHDR");
  const total = idat.reduce((n, c) => n + c.length, 0);
  const z = new Uint8Array(total);
  let o = 0;
  for (const c of idat) {
    z.set(c, o);
    o += c.length;
  }
  const raw = await inflate(z);
  const stride = width * channels;
  if (raw.length < (stride + 1) * height) throw new Error("PNG data too short");
  const cur = new Uint8Array(stride);
  const prev = new Uint8Array(stride);
  const values = new Uint8Array(width * height);
  for (let y = 0; y < height; y++) {
    const off = y * (stride + 1);
    const filter = raw[off]!;
    for (let i = 0; i < stride; i++) {
      const x = raw[off + 1 + i]!;
      const a = i >= channels ? cur[i - channels]! : 0;
      const b = prev[i]!;
      const c = i >= channels ? prev[i - channels]! : 0;
      let v: number;
      switch (filter) {
        case 0: v = x; break;
        case 1: v = x + a; break;
        case 2: v = x + b; break;
        case 3: v = x + ((a + b) >> 1); break;
        case 4: {
          const p = a + b - c;
          const pa = Math.abs(p - a);
          const pb = Math.abs(p - b);
          const pc = Math.abs(p - c);
          v = x + (pa <= pb && pa <= pc ? a : pb <= pc ? b : c);
          break;
        }
        default:
          throw new Error(`PNG filter ${filter} not supported`);
      }
      cur[i] = v & 0xff;
    }
    for (let px = 0; px < width; px++) values[y * width + px] = cur[px * channels]!;
    prev.set(cur);
  }
  return { width, height, values };
}
