/**
 * `mountTour`: embeds a tour in a DOM element and exposes the original control's interface
 * (tours-3d.md §2.1): `do(command) → string` and the `Event3DX(text)` callback, with the host
 * handshake of §2.2 played automatically unless `autoLoad` is false.
 */
import { TourControl, type ControlOptions } from "./control";
import { currentPose, hotspotDestination, playHostStart, poseCommand, type Pose } from "./host";
import { CAPTION_FONT_STACK, LayerPainter } from "./hud/draw";
import { parseEvent } from "./script/parse";
import { FixedTicker } from "./sim/clock";
import { TourView, type ViewOptions } from "./render/view";
import type { TourScene } from "./types";

export interface TourEvent {
  /** The text as fired, e.g. `HOTSPOTCLICKED:HOTSPOTID=701504853`. */
  text: string;
  /** Upper-cased event name (`WORLDLOADED`, `USERLOCUPDATE`, `HOTSPOTCLICKED`, `USERLEVELCHANGE`, …). */
  name: string;
  pairs: [string, string][];
  /** `HOTSPOTCLICKED` only: the media the host opens (`destmedia`), when the converted data has it. */
  destMedia?: number | null;
}

export interface MountOptions {
  /** URL of the tours directory (containing `<tourId>/scene.json`). */
  baseUrl: string;
  tourId: string;
  onEvent?: (event: TourEvent) => void;
  /** Play the host handshake and start `load.3sc` (default true). */
  autoLoad?: boolean;
  /**
   * After loading, put the camera here instead of at the default Tour Stop (returning to a tour
   * from a hotspot's media page). Only with `autoLoad`.
   */
  initialPose?: Pose | null;
  /** Send the default stop's camera command after `WORLDLOADED`, as Encarta does (default true). */
  defaultStop?: boolean;
  control?: ControlOptions;
  view?: ViewOptions;
  /**
   * Fixed control size in pixels, e.g. `[616, 420]` (Encarta's, measured in the VM). The 2D layer,
   * the loading screen and the field of view are laid out for this size, and the control is
   * scaled to fit the container, letterboxed. Default: the container's own size.
   */
  controlSize?: [number, number] | null;
}

export interface TourHandle {
  /** `Do(command)`: run one command string through the interpreter. */
  do(command: string): string;
  readonly control: TourControl;
  readonly scene: TourScene;
  /** Resolves after `WORLDLOADED`. */
  readonly loaded: Promise<void>;
  stats(): { fps: number; drawCalls: number; triangles: number; pvsCell: number | null };
  /** The camera pose in disc coordinates, with the active walk map. */
  pose(): Pose;
  /** Moves to a pose (as the stop commands do); returns the replies. */
  goTo(pose: Pose): string;
  /** Sends a Tour Stop's camera command; null when the stop has none. */
  goToStop(stopId: number): string | null;
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
    const name = ev.name.toUpperCase();
    if (name === "WORLDLOADED") resolveLoaded();
    const event: TourEvent = { text, name, pairs: ev.pairs };
    if (name === "HOTSPOTCLICKED") {
      const id = Number(ev.pairs.find(([k]) => k.toUpperCase() === "HOTSPOTID")?.[1]);
      event.destMedia = Number.isFinite(id) ? hotspotDestination(scene, id) : null;
    }
    options.onEvent?.(event);
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
  // cssW × cssH is the control's own pixel size; `scale` maps it to CSS pixels of the frame.
  let cssW = 0, cssH = 0, scale = 1, offX = 0, offY = 0;
  const dpr = view.options.pixelRatio;
  let hudScale = dpr;
  const fixed = options.controlSize ?? null;
  const resize = () => {
    const r = root.getBoundingClientRect();
    if (fixed) {
      [cssW, cssH] = fixed;
      scale = Math.max(0.1, Math.min(r.width / cssW, r.height / cssH));
      const fw = cssW * scale, fh = cssH * scale;
      offX = Math.round((r.width - fw) / 2);
      offY = Math.round((r.height - fh) / 2);
      for (const c of [glCanvas, hud]) c.style.cssText = `position:absolute;left:${offX}px;top:${offY}px;width:${fw}px;height:${fh}px;display:block`;
      view.renderer.setPixelRatio(dpr * scale);
    } else {
      cssW = Math.max(1, Math.round(r.width));
      cssH = Math.max(1, Math.round(r.height));
    }
    hudScale = dpr * scale;
    control.viewport = { w: cssW, h: cssH };
    view.resize(cssW, cssH);
    hud.width = Math.round(cssW * hudScale);
    hud.height = Math.round(cssH * hudScale);
  };
  resize();
  const ro = new ResizeObserver(resize);
  ro.observe(root);

  // ------------------------------------------------------------ input
  const local = (e: PointerEvent | MouseEvent) => {
    const r = root.getBoundingClientRect();
    return { x: (e.clientX - r.left - offX) / scale, y: (e.clientY - r.top - offY) / scale };
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
  const ticker = new FixedTicker(control.options.tickHz);
  let last = performance.now();
  let frames = 0, fpsT = last, fps = 0;
  let raf = 0;
  let cursorKey = "";
  const g = hud.getContext("2d");
  const frame = (now: number) => {
    raf = requestAnimationFrame(frame);
    const n = ticker.frame((now - last) / 1000);
    last = now;
    for (let i = 0; i < n; i++) control.tick();
    draw(now);
  };
  const draw = (now: number) => {
    view.render();
    if (g) {
      g.setTransform(hudScale, 0, 0, hudScale, 0, 0);
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
    if (options.initialPose || options.defaultStop !== false) playHostStart(control, options.initialPose ?? null);
  }
  void viewLoad;

  const handle: TourHandle = {
    do: (command) => control.do(command),
    control,
    scene,
    loaded,
    stats: () => ({ fps, ...view.stats() }),
    pose: () => currentPose(control),
    goTo: (pose) => control.do(poseCommand(scene, pose)),
    goToStop: (stopId) => {
      const cmd = scene.stops?.find((s) => s.id === stopId)?.command;
      return cmd ? control.do(cmd) : null;
    },
    advance: (ticks) => {
      for (let i = 0; i < ticks; i++) control.tick();
      draw(performance.now());
    },
    destroy: () => {
      cancelAnimationFrame(raf);
      ro.disconnect();
      view.dispose();
      root.remove();
      const live = (devHandle.__encartaTours ?? []).filter((x) => x !== handle);
      devHandle.__encartaTours = live;
      devHandle.__encartaTour = live.at(-1) ?? null;
    },
  };
  // Development builds expose the last mounted tour for driving it from the console or a test
  // browser (a hidden tab gets no animation frames; `advance()` steps it by hand).
  const devHandle = globalThis as { __encartaTour?: TourHandle | null; __encartaTours?: TourHandle[] };
  if (import.meta.env?.DEV) {
    devHandle.__encartaTours = [...(devHandle.__encartaTours ?? []), handle];
    devHandle.__encartaTour = handle;
  }
  return handle;
}
