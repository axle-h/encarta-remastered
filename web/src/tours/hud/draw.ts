/**
 * Draws the 2D layer onto a canvas overlay (tours-3d.md §8.4.2, §8.5.1): images with their
 * texture rectangle, captions rasterised into a 512×32 surface each and copied 1:1 with point
 * sampling, in element order.
 */
import type { Element2D, Layer2D } from "./layer";
import { CAPTION_HEIGHT, CAPTION_SURFACE } from "./caption";

/** Caption face: Tahoma as on XP, with metric-compatible fallbacks. */
export const CAPTION_FONT_STACK = `Tahoma, Verdana, "DejaVu Sans", Arial, sans-serif`;
/** Glyph cell height 13 px (§8.4.2): ascent+descent of Tahoma is 1.2078 em, so 13 px ≈ 10.76 px em. */
const CELL = 13;
const EM = CELL / 1.2078;
const ASCENT = EM * 1.0006;

interface Surface {
  canvas: HTMLCanvasElement | OffscreenCanvas;
  version: number;
  key: string;
}

export class LayerPainter {
  private images = new Map<string, HTMLImageElement | ImageBitmap | null>();
  private surfaces = new WeakMap<Element2D, Surface>();
  private iconPixels: { data: Uint8ClampedArray; w: number; h: number } | null = null;
  private iconSource: string | null = null;

  constructor(private readonly url: (path: string) => string) {}

  /** Starts loading an image; returns it when ready. */
  image(path: string | null): HTMLImageElement | ImageBitmap | null {
    if (!path) return null;
    if (this.images.has(path)) return this.images.get(path) ?? null;
    this.images.set(path, null);
    const img = new Image();
    img.decoding = "async";
    img.onload = () => this.images.set(path, img);
    img.src = this.url(path);
    return null;
  }

  /** Resolves when every image is loaded (or failed). */
  preload(paths: readonly (string | null)[]): Promise<void> {
    return Promise.all(
      paths.filter((p): p is string => !!p && !this.images.has(p)).map(
        (p) =>
          new Promise<void>((resolve) => {
            this.images.set(p, null);
            const img = new Image();
            img.onload = () => {
              this.images.set(p, img);
              resolve();
            };
            img.onerror = () => resolve();
            img.src = this.url(p);
          }),
      ),
    ).then(() => undefined);
  }

  draw(g: CanvasRenderingContext2D, layer: Layer2D, w: number, h: number): void {
    if (!layer.on) return;
    this.ensureIcons(layer);
    for (const el of layer.elements) {
      if (!layer.isDrawn(el, w, h)) continue;
      const [l, t, r, b] = el.rect;
      if (el.def.isLabel) {
        const s = this.surface(el, layer);
        const sw = Math.round(el.uv[2] * CAPTION_SURFACE[0]);
        const sh = Math.round(el.uv[3] * CAPTION_SURFACE[1]);
        g.imageSmoothingEnabled = false;
        g.drawImage(s.canvas as CanvasImageSource, 0, 0, sw, sh, l, t, r - l, b - t);
        continue;
      }
      if (!el.def.textured || !el.def.image) {
        g.fillStyle = el.def.diffuse;
        g.fillRect(l, t, r - l, b - t);
        continue;
      }
      const img = this.image(el.def.image);
      if (!img) continue;
      const iw = img.width, ih = img.height;
      const [u0, v0, u1, v1] = el.uv;
      g.imageSmoothingEnabled = true;
      // The engine's 2D surfaces hold image rows bottom-up relative to the file (the disc's GIFs
      // are stored upside down so that they show upright, like the icon strip of §8.4.1), so
      // texture row v counts from the file's last row and the quad shows the rows flipped.
      g.save();
      g.translate(0, t + b);
      g.scale(1, -1);
      g.drawImage(img, u0 * iw, (1 - v1) * ih, (u1 - u0) * iw, (v1 - v0) * ih, l, t, r - l, b - t);
      g.restore();
    }
  }

  private ensureIcons(layer: Layer2D): void {
    if (layer.iconImage === this.iconSource && this.iconPixels) return;
    const img = this.image(layer.iconImage);
    if (!img) return;
    const c = document.createElement("canvas");
    c.width = img.width;
    c.height = img.height;
    const g = c.getContext("2d", { willReadFrequently: true });
    if (!g) return;
    g.drawImage(img, 0, 0);
    this.iconPixels = { data: g.getImageData(0, 0, c.width, c.height).data, w: c.width, h: c.height };
    this.iconSource = layer.iconImage;
    for (const el of layer.elements) el.rasterVersion++;
  }

  private surface(el: Element2D, layer: Layer2D): Surface {
    const key = `${layer.iconImage}|${layer.iconCount}|${layer.metrics.verticalOffset}`;
    let s = this.surfaces.get(el);
    if (s && s.version === el.rasterVersion && s.key === key) return s;
    const canvas = s?.canvas ?? document.createElement("canvas");
    canvas.width = CAPTION_SURFACE[0];
    canvas.height = CAPTION_SURFACE[1];
    s = { canvas, version: el.rasterVersion, key };
    this.surfaces.set(el, s);
    this.raster(canvas.getContext("2d") as CanvasRenderingContext2D, el, layer);
    return s;
  }

  /** The caption raster of §8.4.2. */
  private raster(g: CanvasRenderingContext2D, el: Element2D, layer: Layer2D): void {
    const w = el.rect[2] - el.rect[0];
    const h = CAPTION_HEIGHT;
    const icon = layer.currentIcon(el);
    const strip = this.image(layer.iconImage);
    const count = layer.iconCount > 0 ? layer.iconCount : 20;
    const cellW = strip ? Math.floor(strip.width / count) : 21;
    const cellH = strip ? strip.height : 22;
    const hasIcon = icon >= 0 && !!strip && icon < count;
    const voff = layer.metrics.verticalOffset;
    g.clearRect(0, 0, CAPTION_SURFACE[0], CAPTION_SURFACE[1]);
    g.save();
    g.beginPath();
    g.rect(0, 0, w, h);
    g.clip();
    if (el.def.colorKey) {
      // Outlined caption: transparent background, white bold text with a black outline.
      if (hasIcon && strip) g.drawImage(strip, icon * cellW, 0, cellW, cellH, 0, 0, cellW, cellH);
      const x = hasIcon ? cellW : 6;
      g.font = `700 ${EM.toFixed(2)}px ${CAPTION_FONT_STACK}`;
      g.textBaseline = "alphabetic";
      g.lineJoin = "round";
      g.lineWidth = 2; // the 2-px pen on the glyph outlines: a 1-px black rim outside the white text (VM)
      g.strokeStyle = "#000000";
      g.strokeText(el.text, x, voff + ASCENT);
      g.fillStyle = "#FFFFFF";
      g.fillText(el.text, x, voff + ASCENT);
    } else {
      // Hotspot box: fill with the icon cell's top-left pixel, icon, text, 1-px frame.
      let fill = "#000000";
      if (hasIcon && this.iconPixels) {
        const o = (icon * cellW) * 4;
        const d = this.iconPixels.data;
        fill = `rgb(${d[o]},${d[o + 1]},${d[o + 2]})`;
      }
      g.fillStyle = fill;
      g.fillRect(0, 0, w, h);
      if (hasIcon && strip) g.drawImage(strip, icon * cellW, 0, cellW, cellH, 0, 0, cellW, cellH);
      const x = hasIcon ? cellW : 6;
      g.font = `400 ${EM.toFixed(2)}px ${CAPTION_FONT_STACK}`;
      g.textBaseline = "alphabetic";
      g.fillStyle = el.textColor;
      g.fillText(el.text, x, voff + ASCENT);
      g.strokeStyle = el.frameColor;
      g.lineWidth = 1;
      g.strokeRect(0.5, 0.5, w - 1, h - 1);
    }
    g.restore();
  }
}
