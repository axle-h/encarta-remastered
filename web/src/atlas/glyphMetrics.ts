/**
 * Glyph advances from the style's glyph PBFs, so the label placement measures text exactly as
 * MapLibre shapes it (advance in 24-px units, letter spacing between glyphs, 1.2-em lines).
 */
import { Reader } from "./mvt";

/** MapLibre's glyph em and default line height. */
export const ONE_EM = 24;
export const LINE_HEIGHT = 1.2;

/** Advances by code point of one glyph range PBF (glyphs { stacks { glyphs { id = 1, advance = 7 } } }). */
export function parseGlyphAdvances(data: Uint8Array): Map<number, number> {
  const out = new Map<number, number>();
  const top = new Reader(data);
  while (!top.done) {
    const tag = top.varint();
    if (tag >> 3 !== 1 || (tag & 7) !== 2) {
      top.skip(tag & 7);
      continue;
    }
    const stack = new Reader(top.bytes());
    while (!stack.done) {
      const t = stack.varint();
      if (t >> 3 !== 3 || (t & 7) !== 2) {
        stack.skip(t & 7);
        continue;
      }
      const g = new Reader(stack.bytes());
      let id = -1;
      let advance = 0;
      while (!g.done) {
        const gt = g.varint();
        if ((gt & 7) === 0) {
          const v = g.varint();
          if (gt >> 3 === 1) id = v;
          else if (gt >> 3 === 7) advance = v;
        } else g.skip(gt & 7);
      }
      if (id >= 0) out.set(id, advance);
    }
  }
  return out;
}

export class GlyphMetrics {
  private ranges = new Map<string, Map<number, number> | "loading" | "missing">();
  private loads = new Map<string, Promise<void>>();
  constructor(
    /** The style's `glyphs` URL template (absolute), `{fontstack}` and `{range}`. */
    private template: string,
    private fetcher: (url: string) => Promise<Uint8Array | null> = defaultFetch,
  ) {}

  private key(font: string, range: number) {
    return `${font}\u0000${range}`;
  }

  /** Loads the ranges `text` needs in `font`; resolves when they are in (or failed). */
  async ensure(font: string, text: string): Promise<void> {
    const need = new Set<number>();
    for (const ch of text) need.add(Math.floor(ch.codePointAt(0)! / 256));
    await Promise.all(
      [...need].map((range) => {
        const k = this.key(font, range);
        let p = this.loads.get(k);
        if (!p) {
          this.ranges.set(k, "loading");
          const url = this.template.replace("{fontstack}", encodeURIComponent(font)).replace("{range}", `${range * 256}-${range * 256 + 255}`);
          p = this.fetcher(url)
            .catch(() => null)
            .then((data) => {
              let advances: Map<number, number> | null = null;
              try {
                advances = data ? parseGlyphAdvances(data) : null;
              } catch {
                advances = null; // not a glyph PBF (e.g. an HTML fallback page)
              }
              this.ranges.set(k, advances ?? "missing");
            });
          this.loads.set(k, p);
        }
        return p;
      }),
    );
  }

  /** Whether every range of `text` in `font` has been asked for and arrived (or failed). */
  ready(font: string, text: string): boolean {
    for (const ch of text) {
      const r = this.ranges.get(this.key(font, Math.floor(ch.codePointAt(0)! / 256)));
      if (r === undefined || r === "loading") return false;
    }
    return true;
  }

  /** Advance of one character in px at `size` (0.6 em when unknown). */
  advance(font: string, cp: number, size: number): number {
    const r = this.ranges.get(this.key(font, Math.floor(cp / 256)));
    const a = r instanceof Map ? r.get(cp) : undefined;
    return a === undefined ? 0.6 * size : (a * size) / ONE_EM;
  }

  /** Width of one line in px as MapLibre shapes it: advances plus the spacing between glyphs. */
  width(font: string, text: string, size: number, letterSpacingEm = 0): number {
    let w = 0;
    let n = 0;
    for (const ch of text) {
      w += this.advance(font, ch.codePointAt(0)!, size);
      n++;
    }
    return w + Math.max(0, n - 1) * letterSpacingEm * size;
  }
}

async function defaultFetch(url: string): Promise<Uint8Array | null> {
  const r = await fetch(url);
  if (!r.ok || (r.headers.get("content-type") ?? "").includes("text/html")) return null;
  return new Uint8Array(await r.arrayBuffer());
}
