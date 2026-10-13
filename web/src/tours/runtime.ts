/**
 * `mountTour`: embeds a tour in a DOM element and exposes the original control's interface
 * (tours-3d.md §2.1): `do(command) → string` and the `Event3DX(text)` callback, with the host
 * handshake of §2.2 played automatically unless `autoLoad` is false.
 */
import { TourControl, type ControlOptions } from "./control";
import { CAPTION_FONT_STACK, LayerPainter } from "./hud/draw";
import { parseEvent } from "./script/parse";
import { TourView, type ViewOptions } from "./render/view";
import type { TourScene } from "./types";

export interface TourEvent {
  /** The text as fired, e.g. `HOTSPOTCLICKED:HOTSPOTID=701504853`. */
  text: string;
  /** Upper-cased event name (`WORLDLOADED`, `USERLOCUPDATE`, `HOTSPOTCLICKED`, `USERLEVELCHANGE`, …). */
  name: string;
  pairs: [string, string][];
}

export interface MountOptions {
  /** URL of the tours directory (containing `<tourId>/scene.json`). */
  baseUrl: string;
  tourId: string;
  onEvent?: (event: TourEvent) => void;
  /** Play the host handshake and start `load.3sc` (default true). */
  autoLoad?: boolean;
  control?: ControlOptions;
  view?: ViewOptions;
}

export interface TourHandle {
  /** `Do(command)`: run one command string through the interpreter. */
  do(command: string): string;
  readonly control: TourControl;
  readonly scene: TourScene;
  /** Resolves after `WORLDLOADED`. */
  readonly loaded: Promise<void>;
  stats(): { fps: number; drawCalls: number; triangles: number; pvsCell: number | null };
  /** Runs `ticks` simulation ticks and draws one frame, independent of requestAnimationFrame (tests, hidden pages). */
  advance(ticks: number): void;
  destroy(): void;
}

const NAV_KEYS = new Set(["ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight", "PageUp", "PageDown", "Home"]);

export async function mountTour(container: HTMLElement, options: MountOptions): Promise<TourHandle> {
  const tourUrl = `${options.baseUrl.replace(/\/$/, "")}/${options.tourId}`;
  const url = (p: string) => `${tourUrl}/${p}`;
  const res = await fetch(url("scene.json"));
  if (!res.ok) throw new Error(`${url("scene.json")}: HTTP ${res.status}`);
  const scene = (await res.json()) as TourScene;

  const root = document.createElement("div");
  root.className = "encarta-tour";
  root.tabIndex = 0;
  root.style.cssText = "position:relative;width:100%;height:100%;overflow:hidden;outline:none;background:#000;user-select:none;touch-action:none";
  const glCanvas = document.createElement("canvas");
  glCanvas.style.cssText = "position:absolute;inset:0;width:100%;height:100%;display:block";
  const hud = document.createElement("canvas");
  hud.style.cssText = "position:absolute;inset:0;width:100%;height:100%;display:block";
  root.append(glCanvas, hud);
  container.append(root);

  const control = new TourControl(scene, options.control);
  const view = new TourView(glCanvas, control, tourUrl, options.view);
  const painter = new LayerPainter(url);
  let resolveLoaded!: () => void;
  const loaded = new Promise<void>((r) => (resolveLoaded = r));
  control.events.onEvent = (text) => {
    const ev = parseEvent(text);
    if (ev.name.toUpperCase() === "WORLDLOADED") resolveLoaded();
    options.onEvent?.({ text, name: ev.name.toUpperCase(), pairs: ev.pairs });
  };
  control.modelReady = () => view.ready;

  // Images of the 2D layers and the icon strip, so the loading screen shows at once.
  const layerImages = scene.layers.flatMap((l) => l.elements.map((e) => e.image));
  await painter.preload([...layerImages, scene.hud.iconStrip?.image ?? null]);
  void document.fonts?.load(`700 11px ${CAPTION_FONT_STACK}`);
  const viewLoad = view.load().catch((e: unknown) => {
    view.error = String(e);
    console.error("tour view failed to load", e);
  });

  // ------------------------------------------------------------ sizing
  let cssW = 0, cssH = 0;
  const dpr = view.options.pixelRatio;
  const resize = () => {
    const r = root.getBoundingClientRect();
    cssW = Math.max(1, Math.round(r.width));
    cssH = Math.max(1, Math.round(r.height));
    control.viewport = { w: cssW, h: cssH };
    view.resize(cssW, cssH);
    hud.width = Math.round(cssW * dpr);
    hud.height = Math.round(cssH * dpr);
  };
  resize();
  const ro = new ResizeObserver(resize);
  ro.observe(root);

  // ------------------------------------------------------------ input
  const local = (e: PointerEvent | MouseEvent) => {
    const r = root.getBoundingClientRect();
    return { x: e.clientX - r.left, y: e.clientY - r.top };
  };
  const onDown = (e: PointerEvent) => {
    if (e.button !== 0) return;
    root.focus();
    const p = local(e);
    try {
      root.setPointerCapture(e.pointerId);
    } catch {
      // Synthetic or already-released pointers cannot be captured; dragging still works inside.
    }
    control.pointer(p.x, p.y, true);
    e.preventDefault();
  };
  const onMove = (e: PointerEvent) => {
    const p = local(e);
    const inside = p.x >= 0 && p.y >= 0 && p.x < cssW && p.y < cssH;
    control.pointer(p.x, p.y, (e.buttons & 1) === 1 && control.mouse.down, inside);
  };
  const onUp = (e: PointerEvent) => {
    if (e.button !== 0) return;
    const p = local(e);
    control.pointer(p.x, p.y, false);
  };
  const onLeave = () => (control.mouse.inside = false);
  const onKeyDown = (e: KeyboardEvent) => {
    if (!NAV_KEYS.has(e.key)) return;
    control.keys.add(e.key);
    e.preventDefault();
  };
  const onKeyUp = (e: KeyboardEvent) => {
    control.keys.delete(e.key);
  };
  const onBlur = () => control.keys.clear();
  root.addEventListener("pointerdown", onDown);
  root.addEventListener("pointermove", onMove);
  root.addEventListener("pointerup", onUp);
  root.addEventListener("pointercancel", onUp);
  root.addEventListener("pointerleave", onLeave);
  root.addEventListener("keydown", onKeyDown);
  root.addEventListener("keyup", onKeyUp);
  root.addEventListener("blur", onBlur);
  root.addEventListener("contextmenu", (e) => e.preventDefault());

  // ------------------------------------------------------------ loop
  const tickDt = 1 / control.options.tickHz;
  let last = performance.now();
  let acc = 0;
  let frames = 0, fpsT = last, fps = 0;
  let raf = 0;
  let cursorKey = "";
  const g = hud.getContext("2d");
  const frame = (now: number) => {
    raf = requestAnimationFrame(frame);
    acc += Math.min(0.25, (now - last) / 1000);
    last = now;
    let n = 0;
    while (acc >= tickDt && n < 16) {
      control.tick();
      acc -= tickDt;
      n++;
    }
    if (n === 16) acc = 0;
    draw(now);
  };
  const draw = (now: number) => {
    view.render();
    if (g) {
      g.setTransform(dpr, 0, 0, dpr, 0, 0);
      g.clearRect(0, 0, cssW, cssH);
      control.layoutLayer();
      painter.draw(g, control.layer, cssW, cssH);
      if (control.displayFps) {
        g.font = "12px monospace";
        g.fillStyle = "#ffff00";
        g.fillText(`${control.options.device === "hardware" ? "HW" : "SW"} ${fps.toFixed(0)}`, 4, cssH - 6);
      }
      if (view.error) {
        g.font = "13px sans-serif";
        g.fillStyle = "#ff6060";
        g.fillText(view.error, 8, 20);
      }
    }
    frames++;
    if (now - fpsT >= 1000) {
      fps = (frames * 1000) / (now - fpsT);
      frames = 0;
      fpsT = now;
    }
    // Cursors (§6.9, §8.5.3).
    const slot = control.cursorSlot();
    const c = slot === null ? null : control.cursorSlots[slot];
    const key = c?.image ? `${c.image}|${c.hotspot?.join(",")}` : "default";
    if (key !== cursorKey) {
      cursorKey = key;
      root.style.cursor = c?.image ? `url("${url(c.image)}") ${c.hotspot?.[0] ?? 0} ${c.hotspot?.[1] ?? 0}, auto` : "default";
    }
  };
  raf = requestAnimationFrame(frame);

  // ------------------------------------------------------------ handshake (§2.2)
  if (options.autoLoad !== false) {
    control.fireEvent("FinishedInitializing");
    control.do("SCENE.ALARM.NEW(1,1,0.3,STARTUP,SCENE.EVENT(STARTUP))");
    control.do("SCENE.ALARM.ACTIVATE");
    control.do(`script.itsfileurl=world\\7015\\${options.tourId}.eit`);
    control.do("script.load=load.3sc");
  }
  void viewLoad;

  return {
    do: (command) => control.do(command),
    control,
    scene,
    loaded,
    stats: () => ({ fps, ...view.stats() }),
    advance: (ticks) => {
      for (let i = 0; i < ticks; i++) control.tick();
      draw(performance.now());
    },
    destroy: () => {
      cancelAnimationFrame(raf);
      ro.disconnect();
      view.dispose();
      root.remove();
    },
  };
}
