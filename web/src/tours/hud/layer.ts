/**
 * The 2D layer (tours-3d.md §8): elements of the loaded `.32d`, their script properties, film-strip
 * frames, caption sizing, anchoring with the cone test, hit testing, hover/press state and macros.
 * Pure state; drawing is in hud/draw.ts.
 */
import type { ElementDef, LayerFile, MacroEvent, Step, Vec3 } from "../types";
import { CAPTION_HEIGHT, CAPTION_SURFACE, captionWidth, DEFAULT_METRICS, type CaptionMetrics } from "./caption";

export type IconState = "normal" | "pressed" | "hover";

export interface Element2D {
  def: ElementDef;
  name: string;
  visible: boolean;
  text: string;
  anchor: Vec3;
  anchored: boolean;
  /** Normal icon index (pressed = +1, hover = +2), −1 = none. */
  icon: number;
  uv: [number, number, number, number];
  sky: boolean;
  macros: Partial<Record<MacroEvent, Step[]>>;
  macroText: Partial<Record<MacroEvent, string>>;
  /** Current quad, client pixels: left, top, right, bottom. */
  rect: [number, number, number, number];
  iconState: IconState;
  textColor: string;
  frameColor: string;
  /** Anchored and outside the view cone this frame: not drawn. */
  coneHidden: boolean;
  /**
   * The caption's rectangle lies strictly inside the viewport. Captions only count for hit tests
   * then. A cone-hidden caption keeps its last rectangle and this flag, so it stays clickable
   * there although it is not drawn (VM).
   */
  drawnUnclipped: boolean;
  /** Bumped whenever the caption surface must be rasterised again. */
  rasterVersion: number;
  other: Record<string, string>;
}

const HOVER_COLOUR = "#FFFFFF";

export interface Projection {
  /** Screen position of a world point, or null when it fails the cone test. */
  project(p: Vec3): { x: number; y: number } | null;
}

export class Layer2D {
  on = true;
  file: string | null = null;
  elements: Element2D[] = [];
  iconImage: string | null = null;
  iconCount = 0;
  metrics: CaptionMetrics = { ...DEFAULT_METRICS };
  settings: Record<string, string> = {};
  private pressed: Element2D | null = null;
  private hovered: Element2D | null = null;
  private buttonWasDown = false;

  constructor(private readonly layers: readonly LayerFile[]) {}

  /** `2D.LOAD`: replace all elements with the file's (§8.3). */
  load(name: string): boolean {
    const f = this.layers.find((l) => l.file.toLowerCase() === name.toLowerCase());
    if (!f) return false;
    this.file = f.file;
    this.on = f.on;
    this.pressed = null;
    this.hovered = null;
    this.elements = f.elements.map((def) => this.makeElement(def));
    return true;
  }

  private makeElement(def: ElementDef): Element2D {
    const el: Element2D = {
      def,
      name: def.name,
      visible: true,
      text: def.fileText,
      anchor: [0, 0, 0],
      anchored: false,
      icon: def.icons.normal,
      uv: [...def.uv],
      sky: false,
      macros: {},
      macroText: {},
      rect: [...def.rect],
      iconState: "normal",
      textColor: def.textColor,
      frameColor: def.frameColor,
      coneHidden: false,
      drawnUnclipped: false,
      rasterVersion: 0,
      other: {},
    };
    // The .32d macro strings (only `_dot.mousedown` is non-empty on the disc). scene.json keeps
    // them in the element state, which also holds macros the scripts set later; those scripts
    // overwrite the same entries with the same text before any input is possible.
    for (const [ev, entry] of Object.entries(def.state?.macros ?? {})) {
      if (!entry) continue;
      el.macros[ev as MacroEvent] = entry[1];
      el.macroText[ev as MacroEvent] = entry[0];
    }
    if (def.isLabel) this.relayout(el);
    return el;
  }

  /** `2D.NEW`. */
  clear(): void {
    this.elements = [];
    this.pressed = null;
    this.hovered = null;
  }

  /** Case-insensitive name, first match; `[n]` by index. */
  find(id: string): Element2D | null {
    const m = /^\[(\d+)\]$/.exec(id);
    if (m) return this.elements[Number(m[1])] ?? null;
    const up = id.toUpperCase();
    return this.elements.find((e) => e.name.toUpperCase() === up) ?? null;
  }

  /** Caption width rule; the quad keeps its top-left corner. */
  relayout(el: Element2D): void {
    if (!el.def.isLabel) return;
    const w = Math.min(CAPTION_SURFACE[0], captionWidth(el.text, el.def.colorKey, this.metrics));
    el.rect = [el.rect[0], el.rect[1], el.rect[0] + w, el.rect[1] + CAPTION_HEIGHT];
    el.uv = [0, 0, w / CAPTION_SURFACE[0], CAPTION_HEIGHT / CAPTION_SURFACE[1]];
    el.rasterVersion++;
  }

  setText(el: Element2D, text: string): void {
    el.text = text;
    this.relayout(el);
  }

  setIcon(el: Element2D, icon: number): void {
    el.icon = icon;
    el.rasterVersion++;
  }

  /** `NEXTFRAME(n)`: advance a vertical film strip of n frames (§8.4.3). */
  nextFrame(el: Element2D, n: number): void {
    if (n <= 0) return;
    const f = 1 / n;
    const [u0, vTop, u1, vBottom] = el.uv;
    let k = Math.trunc(vTop / f) + 1;
    if (Math.abs(vBottom - vTop - f) >= 0.001) k = 0;
    if (k >= n) k = 0;
    el.uv = [u0, k * f, u1, k === n - 1 ? 1 : (k + 1) * f];
  }

  /** The current icon index for drawing (−1 = none). */
  currentIcon(el: Element2D): number {
    if (el.icon < 0) return -1;
    return el.icon + (el.iconState === "pressed" ? 1 : el.iconState === "hover" ? 2 : 0);
  }

  /**
   * Per frame (§8.5.1–§8.5.2): anchored elements follow their anchor (top-left corner at the
   * projected point) or are hidden by the cone test; clipping decides which captions count.
   */
  layout(proj: Projection, viewW: number, viewH: number): void {
    for (const el of this.elements) {
      el.coneHidden = false;
      if (el.visible && el.anchored) {
        const p = proj.project(el.anchor);
        if (!p) el.coneHidden = true;
        else {
          const w = el.rect[2] - el.rect[0], h = el.rect[3] - el.rect[1];
          el.rect = [p.x, p.y, p.x + w, p.y + h];
        }
      }
      // Out of the cone: the stale rectangle and its flag stay (VM: still clickable there).
      if (!el.coneHidden) el.drawnUnclipped = !el.sky && !rejected(el.rect, viewW, viewH) && !straddles(el.rect, viewW, viewH);
    }
  }

  /** Whether an element is drawn this frame (images that straddle an edge are clipped, captions dropped). */
  isDrawn(el: Element2D, viewW: number, viewH: number): boolean {
    if (!this.on || !el.visible || el.coneHidden || el.sky) return false;
    if (rejected(el.rect, viewW, viewH)) return false;
    if (el.def.isLabel && straddles(el.rect, viewW, viewH)) return false;
    return true;
  }

  /** Hit test from last to first (§8.5.3). */
  hitTest(x: number, y: number): Element2D | null {
    if (!this.on) return null;
    for (let i = this.elements.length - 1; i >= 0; i--) {
      const el = this.elements[i];
      if (!el || el.def.primitive !== 6 || !el.visible || el.sky) continue;
      if (el.def.isLabel && !el.drawnUnclipped) continue; // layer on and visible are checked above
      const [l, t, r, b] = el.rect;
      if (x >= l && x <= r && y >= t && y <= b) return el;
    }
    return null;
  }

  /**
   * One input update: MOUSEDOWN, MOUSEUP, MOUSEIN, MOUSEOUT in that order, with the hotspot side
   * effects. `run` executes a macro. Returns the element under the pointer.
   */
  pointer(x: number, y: number, down: boolean, run: (steps: Step[]) => void): Element2D | null {
    const hit = this.hitTest(x, y);
    const queue: [Element2D, MacroEvent][] = [];
    if (down && !this.buttonWasDown && hit && hit !== this.pressed) {
      this.pressed = hit;
      this.effect(hit, "pressed");
      queue.push([hit, "mouseDown"]);
    }
    if (!down && this.pressed) {
      const p = this.pressed;
      this.pressed = null;
      if (hit === p) {
        this.effect(p, "hover");
        queue.push([p, "mouseUp"]);
      }
    }
    if (hit !== this.hovered) {
      const prev = this.hovered;
      this.hovered = hit;
      if (hit) {
        this.effect(hit, "hover");
        queue.push([hit, "mouseIn"]);
      }
      if (prev) {
        this.effect(prev, "normal");
        queue.push([prev, "mouseOut"]);
      }
    }
    this.buttonWasDown = down;
    for (const [el, ev] of queue) {
      const steps = el.macros[ev];
      if (steps && steps.length) run(steps);
    }
    return hit;
  }

  /** The element the button went down on, while held. */
  get pressedElement(): Element2D | null {
    return this.pressed;
  }

  get hoveredElement(): Element2D | null {
    return this.hovered;
  }

  private effect(el: Element2D, state: IconState): void {
    if (el.def.kind !== "hotspot" || !el.def.hasText) return;
    el.iconState = state;
    el.textColor = state === "normal" ? el.def.textColor : HOVER_COLOUR;
    el.frameColor = state === "normal" ? el.def.frameColor : HOVER_COLOUR;
    el.rasterVersion++;
  }
}

/** Quad rejection (§8.5.1): right ≤ 0, bottom ≤ 0, left ≥ W−1 or top ≥ W−1 (sic, the width). */
export function rejected(r: readonly number[], w: number, _h: number): boolean {
  const [l = 0, t = 0, rr = 0, b = 0] = r;
  return rr <= 0 || b <= 0 || l >= w - 1 || t >= w - 1;
}

/**
 * A caption not strictly inside the viewport (VM: drawn only with left > 0, top > 0,
 * right < W − 1 and bottom < H − 1; a caption touching or crossing an edge is dropped whole).
 */
export function straddles(r: readonly number[], w: number, h: number): boolean {
  const [l = 0, t = 0, rr = 0, b = 0] = r;
  return l <= 0 || t <= 0 || rr >= w - 1 || b >= h - 1;
}
