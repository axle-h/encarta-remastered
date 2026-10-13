import { describe, expect, it } from "vitest";
import { decodePngValues } from "../png";

async function deflate(data: Uint8Array): Promise<Uint8Array> {
  const s = new Blob([data]).stream().pipeThrough(new CompressionStream("deflate"));
  return new Uint8Array(await new Response(s).arrayBuffer());
}

function chunk(type: string, body: Uint8Array): Uint8Array {
  const out = new Uint8Array(12 + body.length);
  const dv = new DataView(out.buffer);
  dv.setUint32(0, body.length);
  for (let i = 0; i < 4; i++) out[4 + i] = type.charCodeAt(i);
  out.set(body, 8);
  return out; // CRC left zero: the reader does not check it
}

/** A synthetic 8-bit PNG, each row with its own filter, filtered by hand. */
async function makePng(w: number, h: number, channels: 1 | 3, colourType: number, px: (x: number, y: number) => number[], filters: number[]): Promise<Uint8Array> {
  const stride = w * channels;
  const raw = new Uint8Array((stride + 1) * h);
  let prev = new Uint8Array(stride);
  for (let y = 0; y < h; y++) {
    const cur = new Uint8Array(stride);
    for (let x = 0; x < w; x++) cur.set(px(x, y), x * channels);
    const f = filters[y % filters.length]!;
    raw[y * (stride + 1)] = f;
    for (let i = 0; i < stride; i++) {
      const a = i >= channels ? cur[i - channels]! : 0;
      const b = prev[i]!;
      const c = i >= channels ? prev[i - channels]! : 0;
      const pa = Math.abs(b - c), pb = Math.abs(a - c), pc = Math.abs(a + b - 2 * c);
      const pred = f === 0 ? 0 : f === 1 ? a : f === 2 ? b : f === 3 ? (a + b) >> 1 : pa <= pb && pa <= pc ? a : pb <= pc ? b : c;
      raw[y * (stride + 1) + 1 + i] = (cur[i]! - pred) & 0xff;
    }
    prev = cur;
  }
  const ihdr = new Uint8Array(13);
  const dv = new DataView(ihdr.buffer);
  dv.setUint32(0, w);
  dv.setUint32(4, h);
  ihdr[8] = 8;
  ihdr[9] = colourType;
  const idat = await deflate(raw);
  const parts = [new Uint8Array([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]), chunk("IHDR", ihdr), chunk("IDAT", idat.subarray(0, 7)), chunk("IDAT", idat.subarray(7)), chunk("IEND", new Uint8Array())];
  const out = new Uint8Array(parts.reduce((n, p) => n + p.length, 0));
  let o = 0;
  for (const p of parts) {
    out.set(p, o);
    o += p.length;
  }
  return out;
}

describe("PNG value decoder", () => {
  it("decodes 8-bit grey with every filter type, across split IDAT chunks", async () => {
    const v = (x: number, y: number) => (x * 37 + y * 11 + ((x * y) % 7)) & 0xff;
    const png = await makePng(9, 10, 1, 0, (x, y) => [v(x, y)], [0, 1, 2, 3, 4]);
    const d = await decodePngValues(png);
    expect(d.width).toBe(9);
    expect(d.height).toBe(10);
    for (let y = 0; y < 10; y++) for (let x = 0; x < 9; x++) expect(d.values[y * 9 + x]).toBe(v(x, y));
  });
  it("returns the first channel of RGB images", async () => {
    const png = await makePng(4, 3, 3, 2, (x, y) => [x + 10 * y, 200, 100], [4, 1, 2]);
    const d = await decodePngValues(png);
    expect([...d.values]).toEqual([0, 1, 2, 3, 10, 11, 12, 13, 20, 21, 22, 23]);
  });
  it("rejects what it does not support", async () => {
    await expect(decodePngValues(new Uint8Array([1, 2, 3, 4, 5, 6, 7, 8]))).rejects.toThrow("not a PNG");
  });
});
