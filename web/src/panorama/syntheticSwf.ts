/** Synthetic image SWFs for the tests (our own bytes, shaped like the disc's panorama SWFs). */

class BitWriter {
  bits: number[] = [];
  u(v: number, n: number) {
    for (let i = n - 1; i >= 0; i--) this.bits.push(Math.floor(v / 2 ** i) % 2);
  }
  s(v: number, n: number) {
    this.u(v < 0 ? 2 ** n + v : v, n);
  }
  bytes(): number[] {
    while (this.bits.length % 8) this.bits.push(0);
    const out: number[] = [];
    for (let i = 0; i < this.bits.length; i += 8) out.push(parseInt(this.bits.slice(i, i + 8).join(""), 2));
    return out;
  }
}

function rect(x0: number, x1: number, y0: number, y1: number): number[] {
  const w = new BitWriter();
  w.u(16, 5);
  for (const v of [x0, x1, y0, y1]) w.s(v, 16);
  return w.bytes();
}

function tag(code: number, body: number[]): number[] {
  const out = [];
  if (body.length < 63) out.push(((code << 6) | body.length) & 0xff, (code << 6) >> 8);
  else out.push(((code << 6) | 63) & 0xff, ((code << 6) | 63) >> 8, body.length & 0xff, (body.length >> 8) & 0xff, (body.length >> 16) & 0xff, body.length >>> 24);
  return out.concat(body);
}

/** A tiny "JPEG": SOI, an APP0 segment, SOF0 with the size, EOI (only parsed, never decoded). */
export function fakeJpeg(width: number, height: number): number[] {
  return [0xff, 0xd8, 0xff, 0xe0, 0x00, 0x04, 0x4a, 0x46, 0xff, 0xc0, 0x00, 0x0b, 0x08, height >> 8, height & 0xff, width >> 8, width & 0xff, 0x01, 0x01, 0x11, 0x00, 0xff, 0xd9];
}

export function imageSwf(width: number, height: number, horizon: number, erroneousHeader = false): Uint8Array {
  const jpeg = (erroneousHeader ? [0xff, 0xd9, 0xff, 0xd8] : []).concat(fakeJpeg(width, height));
  // Bitmap fill matrix: scale 2 twips per pixel (1/10 on stage), translated so `horizon` is at y = 0.
  const m = new BitWriter();
  m.u(1, 1);
  m.u(20, 5);
  m.s(2 * 65536, 20);
  m.s(2 * 65536, 20);
  m.u(0, 1);
  m.u(16, 5);
  m.s(0, 16);
  m.s(-2 * horizon, 16);
  const shape = [2, 0, ...rect(0, width * 2, -2 * horizon, 2 * (height - horizon)), 1, 0x41, 1, 0, ...m.bytes(), 0, 0x10, 0];
  const body = [...rect(0, 7000, 0, 4480), 0, 24, 1, 0, ...tag(9, [255, 255, 255]), ...tag(21, [1, 0, ...jpeg]), ...tag(32, shape), ...tag(1, []), ...tag(0, [])];
  const len = body.length + 8;
  return new Uint8Array([0x46, 0x57, 0x53, 5, len & 0xff, (len >> 8) & 0xff, (len >> 16) & 0xff, len >>> 24, ...body]);
}

