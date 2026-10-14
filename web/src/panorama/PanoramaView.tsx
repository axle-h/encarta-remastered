/**
 * The native panorama player: a cylindrical view of one picture (geometry.ts, renderer.ts) with
 * drag, keyboard and wheel control, zoom limits, auto-pan, and hotspots and labels placed on the
 * cylinder. Used by the panoramic tours (pages/PanoramaTour.tsx) and the single panoramas
 * (SinglePanorama.tsx).
 */
import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState, type KeyboardEvent as ReactKeyboardEvent, type MutableRefObject, type PointerEvent as ReactPointerEvent } from "react";
import {
  clampView,
  focalFromFov,
  isFullTurn,
  minFocal,
  project,
  projectRect,
  wrapDeg,
  yawRange,
  type Limits,
  type PanoGeometry,
  type PanoView,
} from "./geometry";
import type { PanoParams } from "./params";
import { createRenderer, type Picture, type Renderer } from "./renderer";
import "./panorama.css";

export interface PanoRect {
  x: number;
  y: number;
  w: number;
  h: number;
}

export interface PanoHotspot {
  id: number;
  /** The clickable region in picture pixels (`shed`). */
  rect: PanoRect;
  /** The marker box in picture pixels (`quadrant`), when the record has one. */
  marker?: PanoRect | null;
  title: string;
  /** "scene" leads to another scene of the tour; "media" and "audio" (a picture with a sound) open a media page. */
  kind: "scene" | "media" | "audio";
}

export interface PanoLabel {
  id: number;
  x: number;
  y: number;
  text: string;
}

export interface PanoramaViewProps {
  /** The picture's URL. */
  src: string;
  /** Declared picture size and horizon; width/height 0 = use the decoded size. */
  width: number;
  height: number;
  /** Horizon row; null = the middle row. */
  horizon: number | null;
  params: PanoParams;
  /** Opening yaw that overrides `params.yaw` (e.g. from the URL). */
  initialYaw?: number | null;
  title: string;
  hotspots?: PanoHotspot[];
  labels?: PanoLabel[];
  onHotspot?: (h: PanoHotspot) => void;
  /** A line across the top of the view (the tours' instruction); the caller clears it in `onDrag`. */
  instruction?: string | null;
  /** Called when a drag on the view moves it. */
  onDrag?: () => void;
  /** Called when the view settles after the user moved it (debounced), with the yaw in degrees. */
  onSettle?: (yaw: number) => void;
  /** Live view, for callers that read the yaw at the moment they navigate away. */
  viewRef?: MutableRefObject<PanoView | null>;
  /** Largest CSS scale of the view (1 = the view's own pixel size). */
  maxScale?: number;
  /** Controlled "labels and hot spots" and auto-pan settings (the tour's Tour Settings); uncontrolled when absent. */
  marksOn?: boolean;
  onMarksChange?: (on: boolean) => void;
  autoOn?: boolean;
  onAutoChange?: (on: boolean) => void;
  /** Test hook: force the Canvas 2D renderer. */
  renderer?: "webgl" | "canvas";
}

/** The view's limits and opening view (exported for tests). */
export function initialView(g: PanoGeometry, p: PanoParams, yaw: number | null | undefined): { view: PanoView; limits: Limits } {
  const fit = minFocal(g, p.viewHeight);
  // Never wider than 120° even when the picture is very tall (ours: keeps edge stretch moderate).
  const minF = Math.max(fit, focalFromFov(120, p.viewWidth));
  const f0 = Math.max(minF, p.fov === "fit" ? fit : focalFromFov(p.fov, p.viewWidth));
  const limits = { minF, maxF: f0 * Math.max(1, p.maxZoom / 100) };
  const view = clampView(g, { yaw: yaw ?? p.yaw, f: f0, shift: 0 }, limits, p.viewWidth, p.viewHeight);
  return { view, limits };
}

const KEY_PAN = new Map([
  ["ArrowLeft", -1],
  ["ArrowRight", 1],
]);
const KEY_TILT = new Map([
  ["ArrowUp", -1],
  ["ArrowDown", 1],
]);

/** The key as the player names it: arrows as they are, + and − (main row or keypad) as ZoomIn/ZoomOut. */
function keyName(e: { key: string; code: string }): string {
  if (e.key === "+" || e.key === "=" || e.code === "Equal" || e.code === "NumpadAdd") return "ZoomIn";
  if (e.key === "-" || e.key === "_" || e.code === "Minus" || e.code === "NumpadSubtract") return "ZoomOut";
  return e.key;
}

export function PanoramaView(props: PanoramaViewProps) {
  const { src, hotspots = [], labels = [], title, maxScale = 1.6 } = props;
  // Callers may rebuild the params object on every render; only a change of value resets the view.
  const pKey = JSON.stringify(props.params);
  // eslint-disable-next-line react-hooks/exhaustive-deps
  const p = useMemo(() => props.params, [pKey]);
  const [pic, setPic] = useState<Picture | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [mode, setMode] = useState<"webgl" | "canvas" | "none">(props.renderer ?? "webgl");
  const [scale, setScale] = useState(1);
  const [marksState, setMarksState] = useState(true);
  const showMarks = props.marksOn ?? marksState;
  const setShowMarks = (v: boolean) => {
    setMarksState(v);
    props.onMarksChange?.(v);
  };
  const [autoState, setAutoState] = useState(p.autoStart);
  // A click on the view pauses auto-pan for `autoResumeMs`, then it resumes (vm-findings (c)).
  const [paused, setPaused] = useState(false);
  const pauseTimer = useRef<number | undefined>(undefined);
  const autoSetting = props.autoOn ?? autoState;
  const auto = autoSetting && !paused;
  const setAuto = (v: boolean | ((x: boolean) => boolean)) => {
    const nv = typeof v === "function" ? v(autoSetting) : v;
    window.clearTimeout(pauseTimer.current);
    setPaused(false);
    setAutoState(nv);
    props.onAutoChange?.(nv);
  };
  const [status, setStatus] = useState("");
  const [fovText, setFovText] = useState("");
  const wrap = useRef<HTMLDivElement>(null);
  const canvas = useRef<HTMLCanvasElement>(null);
  const overlay = useRef<HTMLDivElement>(null);
  const renderer = useRef<Renderer | null>(null);
  const viewRef = useRef<PanoView | null>(null);
  const limits = useRef<Limits>({ minF: 1, maxF: 1 });
  const keys = useRef(new Set<string>());
  /** The press point, the pointer's current offset from it in view pixels, and the view at the press. */
  const drag = useRef<{ x: number; y: number; dx: number; dy: number; view: PanoView; moved: boolean } | null>(null);
  const anim = useRef<{ yaw: number; f?: number } | null>(null);
  const raf = useRef(0);
  const last = useRef(0);
  const settleTimer = useRef<number | undefined>(undefined);
  const autoRef = useRef(auto);
  autoRef.current = auto;
  const autoDir = useRef(1);
  /** Time since the last auto-pan jump, in seconds. */
  const autoAcc = useRef(0);
  const callbacks = useRef(props);
  callbacks.current = props;

  // Load the picture.
  useEffect(() => {
    let cancelled = false;
    setPic(null);
    setError(null);
    const img = new Image();
    img.decoding = "async";
    img.src = src;
    img
      .decode()
      .then(() => !cancelled && setPic({ source: img, width: img.naturalWidth, height: img.naturalHeight }))
      .catch(() => !cancelled && setError("The panorama picture could not be loaded."));
    return () => {
      cancelled = true;
    };
  }, [src]);

  const geometry = useMemo<PanoGeometry | null>(() => {
    if (!pic) return null;
    const width = props.width || pic.width;
    const height = props.height || pic.height;
    return { width, height, horizon: props.horizon ?? height / 2, part: p.part };
  }, [pic, props.width, props.height, props.horizon, p.part]);

  // Fit the view's own size into the container.
  useLayoutEffect(() => {
    const el = wrap.current;
    if (!el) return;
    const fit = () => {
      const w = el.clientWidth || p.viewWidth;
      // In the media viewer the title bar, caption and strip share the window with the view.
      const inViewer = !!el.closest(".mv");
      const maxH = Math.max(160, inViewer ? window.innerHeight - 300 : window.innerHeight * 0.62);
      setScale(Math.max(0.3, Math.min(maxScale, w / p.viewWidth, maxH / p.viewHeight)));
    };
    fit();
    const ro = typeof ResizeObserver !== "undefined" ? new ResizeObserver(fit) : null;
    ro?.observe(el);
    return () => ro?.disconnect();
  }, [p.viewWidth, p.viewHeight, maxScale]);

  const cssW = Math.round(p.viewWidth * scale);
  const cssH = Math.round(p.viewHeight * scale);

  const placeOverlay = useCallback(
    (g: PanoGeometry, v: PanoView) => {
      const el = overlay.current;
      if (!el) return;
      const vw = p.viewWidth;
      const vh = p.viewHeight;
      const s = scale;
      for (const node of Array.from(el.children) as HTMLElement[]) {
        const kind = node.dataset.kind;
        const i = Number(node.dataset.index);
        if (kind === "label") {
          const l = labels[i];
          const q = l && project(g, v, l.x, l.y);
          if (!q || Math.abs(q.x) > vw / 2 + 80 || Math.abs(q.y) > vh / 2 + 20) {
            node.style.display = "none";
            continue;
          }
          node.style.display = "";
          node.style.transform = `translate(${(q.x + vw / 2) * s}px, ${(q.y + vh / 2) * s}px) translate(-50%, -50%)`;
        } else if (kind === "hot" || kind === "mark") {
          const h = hotspots[i];
          const r = h && (kind === "hot" ? h.rect : (h.marker ?? h.rect));
          const box = r && projectRect(g, v, r, vw, vh);
          if (!box) {
            node.style.display = "none";
            continue;
          }
          node.style.display = "";
          if (kind === "hot") {
            node.style.transform = `translate(${(box.x + vw / 2) * s}px, ${(box.y + vh / 2) * s}px)`;
            node.style.width = `${Math.max(8, box.w * s)}px`;
            node.style.height = `${Math.max(8, box.h * s)}px`;
          } else {
            // The titled box sits just above the quadrant area (vm-findings (d)), centred on it.
            node.style.transform = `translate(${(box.x + box.w / 2 + vw / 2) * s}px, ${(box.y + vh / 2) * s}px) translate(-50%, -100%)`;
          }
        }
      }
    },
    [hotspots, labels, p.viewWidth, p.viewHeight, scale],
  );

  const draw = useCallback(() => {
    const g = geometry;
    const v = viewRef.current;
    if (!g || !v) return;
    const dpr = Math.min(2, window.devicePixelRatio || 1);
    renderer.current?.draw(v, p.viewWidth, p.viewHeight, scale, dpr);
    placeOverlay(g, v);
  }, [geometry, p.viewWidth, p.viewHeight, scale, placeOverlay]);

  const notifySettle = useCallback(() => {
    window.clearTimeout(settleTimer.current);
    settleTimer.current = window.setTimeout(() => {
      const v = viewRef.current;
      if (v) callbacks.current.onSettle?.(Math.round(v.yaw * 10) / 10);
    }, 350);
  }, []);

  const setView = useCallback(
    (next: PanoView) => {
      const g = geometry;
      if (!g) return;
      viewRef.current = clampView(g, next, limits.current, p.viewWidth, p.viewHeight);
      if (props.viewRef) props.viewRef.current = viewRef.current;
    },
    [geometry, p.viewWidth, p.viewHeight, props.viewRef],
  );

  // The animation loop: keys held, auto-pan and button animations; idles when nothing moves.
  const tick = useCallback(
    (now: number) => {
      raf.current = 0;
      const g = geometry;
      const v = viewRef.current;
      if (!g || !v) return;
      const dt = Math.min(0.1, last.current ? (now - last.current) / 1000 : 1 / 60);
      last.current = now;
      let moving = false;
      let next = v;
      let pan = 0;
      let tilt = 0;
      for (const k of keys.current) {
        pan += KEY_PAN.get(k) ?? 0;
        tilt += KEY_TILT.get(k) ?? 0;
      }
      const zoomKey = (keys.current.has("ZoomIn") ? 1 : 0) - (keys.current.has("ZoomOut") ? 1 : 0);
      if (pan || tilt || zoomKey) {
        // Keyboard: `maxSpeed` degrees per second, scaled by the zoom so the picture moves at the same apparent speed.
        const zoom = v.f / limits.current.minF;
        next = {
          yaw: next.yaw + (pan * p.maxSpeed * dt) / zoom,
          shift: p.tilt ? next.shift + tilt * p.maxSpeed * dt * (v.f / 57.3) : next.shift,
          f: next.f * Math.exp(zoomKey * 1.2 * dt),
        };
        moving = true;
      } else if (anim.current) {
        const a = anim.current;
        const k = 1 - Math.exp(-dt * 10);
        const dy = ((a.yaw - next.yaw + 540) % 360) - 180;
        const df = a.f !== undefined ? a.f - next.f : 0;
        next = { ...next, yaw: next.yaw + dy * k, f: next.f + df * k };
        if (Math.abs(dy) < 0.05 && Math.abs(df) < 0.5) anim.current = null;
        moving = true;
      } else if (drag.current && p.dragRate) {
        // Rate drag: the view turns toward the pointer, faster the farther it is from the press point.
        const d = drag.current;
        next = {
          ...next,
          yaw: next.yaw + (Math.atan2(d.dx * p.dragRate * dt, next.f) * 180) / Math.PI,
          shift: p.tilt ? next.shift + d.dy * p.dragRate * dt : next.shift,
        };
        moving = true;
      } else if (autoRef.current && !drag.current) {
        let step = p.autoPan * dt;
        if (p.autoStepsPerSecond) {
          const period = 1 / p.autoStepsPerSecond;
          const n = Math.floor((autoAcc.current += dt) / period);
          autoAcc.current -= n * period;
          step = p.autoPan * period * n;
        }
        if (step) {
          let yaw = next.yaw + autoDir.current * step;
          const yr = yawRange(g, next.f, p.viewWidth);
          if (yr && (yaw <= yr[0] || yaw >= yr[1])) {
            autoDir.current = -autoDir.current;
            yaw = Math.min(Math.max(yaw, yr[0]), yr[1]);
          }
          next = { ...next, yaw };
        }
        moving = true;
      }
      if (moving) {
        if (next !== v) {
          setView(next);
          draw();
        }
        if (pan || tilt || zoomKey || anim.current) notifySettle();
        raf.current = requestAnimationFrame(tick);
      } else {
        last.current = 0;
      }
    },
    [geometry, p.maxSpeed, p.tilt, p.autoPan, p.autoStepsPerSecond, p.dragRate, p.viewWidth, setView, draw, notifySettle],
  );

  const kick = useCallback(() => {
    if (!raf.current) {
      last.current = 0;
      raf.current = requestAnimationFrame(tick);
    }
  }, [tick]);

  // Opening view, when the picture and geometry are known.
  useEffect(() => {
    if (!geometry) return;
    const { view, limits: lim } = initialView(geometry, p, props.initialYaw);
    limits.current = lim;
    viewRef.current = view;
    if (props.viewRef) props.viewRef.current = view;
    // initialYaw only applies when the scene opens
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [geometry, p]);

  // Renderer on the current canvas.
  useEffect(() => {
    const el = canvas.current;
    if (!el || !pic || !geometry || mode === "none") return;
    const r = createRenderer(el, pic, geometry, mode);
    if (!r) {
      setMode(mode === "webgl" ? "canvas" : "none");
      return;
    }
    renderer.current = r;
    draw();
    return () => {
      r.dispose();
      renderer.current = null;
    };
    // draw is redefined on scale changes; the resize effect below redraws
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [pic, geometry, mode]);

  // Canvas backing size and redraw on resize.
  useLayoutEffect(() => {
    const el = canvas.current;
    if (!el) return;
    const dpr = Math.min(2, window.devicePixelRatio || 1);
    el.width = Math.round(cssW * dpr);
    el.height = Math.round(cssH * dpr);
    draw();
  }, [cssW, cssH, draw, mode]);

  // A running loop keeps the `tick` it started with; restart it on the new one (e.g. once the picture's geometry is known).
  useEffect(() => {
    if (!auto && !raf.current) return;
    cancelAnimationFrame(raf.current);
    raf.current = 0;
    kick();
  }, [auto, kick]);

  useEffect(
    () => () => {
      cancelAnimationFrame(raf.current);
      raf.current = 0;
      window.clearTimeout(settleTimer.current);
      window.clearTimeout(pauseTimer.current);
    },
    [],
  );

  // Status line: field of view.
  useEffect(() => {
    const id = window.setInterval(() => {
      const v = viewRef.current;
      if (v) setFovText(`${Math.round((2 * Math.atan(p.viewWidth / 2 / v.f) * 180) / Math.PI)}°`);
    }, 500);
    return () => window.clearInterval(id);
  }, [p.viewWidth]);

  const stopAuto = () => {
    if (autoRef.current) setAuto(false);
  };

  // Wheel zoom needs a non-passive listener to keep the page from scrolling.
  useEffect(() => {
    const el = wrap.current?.querySelector<HTMLElement>(".panov-stage");
    if (!el) return;
    const onWheel = (e: WheelEvent) => {
      const v = viewRef.current;
      if (!v) return;
      e.preventDefault();
      stopAuto();
      anim.current = null;
      setView({ ...v, f: v.f * Math.exp(-e.deltaY * 0.0015) });
      draw();
      notifySettle();
    };
    el.addEventListener("wheel", onWheel, { passive: false });
    return () => el.removeEventListener("wheel", onWheel);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [setView, draw, notifySettle, pic]);

  const down = (e: ReactPointerEvent<HTMLDivElement>) => {
    if (e.button !== 0 || (e.target as HTMLElement).closest("button")) return;
    const v = viewRef.current;
    if (!v) return;
    if (p.autoResumeMs && autoRef.current) {
      setPaused(true);
      window.clearTimeout(pauseTimer.current);
      pauseTimer.current = window.setTimeout(() => setPaused(false), p.autoResumeMs);
    } else stopAuto();
    anim.current = null;
    drag.current = { x: e.clientX, y: e.clientY, dx: 0, dy: 0, view: v, moved: false };
    e.currentTarget.setPointerCapture(e.pointerId);
    e.currentTarget.focus({ preventScroll: true });
  };
  const move = (e: ReactPointerEvent<HTMLDivElement>) => {
    const d = drag.current;
    if (!d) return;
    const dx = (d.dx = (e.clientX - d.x) / scale);
    const dy = (d.dy = (e.clientY - d.y) / scale);
    if (!d.moved && Math.abs(dx) + Math.abs(dy) > 2) {
      d.moved = true;
      callbacks.current.onDrag?.();
    }
    if (p.dragRate) {
      kick();
      return;
    }
    // Grab-and-drag: the picture point under the pointer follows it (near the centre column).
    setView({ yaw: d.view.yaw - (Math.atan2(dx, d.view.f) * 180) / Math.PI, f: d.view.f, shift: p.tilt ? d.view.shift - dy : d.view.shift });
    draw();
  };
  const up = () => {
    if (drag.current?.moved) notifySettle();
    drag.current = null;
  };

  const onKeyDown = (e: ReactKeyboardEvent) => {
    const k = keyName(e);
    if (KEY_PAN.has(k) || KEY_TILT.has(k) || k === "ZoomIn" || k === "ZoomOut") {
      e.preventDefault();
      e.stopPropagation(); // the media viewer's ←/→ must not step to the next item
      stopAuto();
      anim.current = null;
      const v = viewRef.current;
      if (!e.repeat && !keys.current.has(k) && v) {
        // A tap moves one step at once (3°, 8 px or 6 %); holding the key keeps moving in `tick`.
        const zoom = v.f / limits.current.minF;
        const z = k === "ZoomIn" ? 1.06 : k === "ZoomOut" ? 1 / 1.06 : 1;
        setView({ yaw: v.yaw + ((KEY_PAN.get(k) ?? 0) * 3) / zoom, shift: v.shift + (p.tilt ? (KEY_TILT.get(k) ?? 0) * 8 : 0), f: v.f * z });
        draw();
        notifySettle();
      }
      keys.current.add(k);
      kick();
    } else if (k === "Home") {
      e.preventDefault();
      reset();
    } else if (k === "a" || k === "A") {
      setAuto((x) => !x);
    }
  };
  const onKeyUp = (e: ReactKeyboardEvent) => {
    keys.current.delete(keyName(e));
  };

  const animateTo = (yaw: number, f?: number) => {
    stopAuto();
    anim.current = f !== undefined ? { yaw, f } : { yaw };
    kick();
  };
  const turn = (deg: number) => {
    const v = viewRef.current;
    if (v) animateTo(isFullTurn(geometry!) ? wrapDeg(v.yaw + deg) : v.yaw + deg);
  };
  const zoomBy = (k: number) => {
    const v = viewRef.current;
    if (!v) return;
    const f = Math.min(Math.max(v.f * k, limits.current.minF), limits.current.maxF);
    animateTo(v.yaw, f);
  };
  const reset = () => {
    if (!geometry) return;
    const { view } = initialView(geometry, p, null);
    viewRef.current = { ...viewRef.current!, shift: view.shift };
    animateTo(view.yaw, view.f);
  };

  const marks = showMarks && (hotspots.length > 0 || labels.length > 0);
  const hint = hotspots.length ? "Drag to look around; click a marked area to go there." : "Drag or use the arrow keys to look around; + and − zoom.";
  return (
    <div className="panov" ref={wrap}>
      <div
        className="panov-stage"
        style={{ width: cssW, height: cssH }}
        tabIndex={0}
        role="application"
        aria-roledescription="panorama"
        aria-label={`${title}. Arrow keys look around, plus and minus zoom, Home returns to the start, A turns auto-pan on or off.`}
        onPointerDown={down}
        onPointerMove={move}
        onPointerUp={up}
        onPointerCancel={up}
        onKeyDown={onKeyDown}
        onKeyUp={onKeyUp}
        onBlur={() => keys.current.clear()}
      >
        <canvas key={mode} ref={canvas} className="panov-canvas" style={{ width: cssW, height: cssH }} aria-hidden="true" />
        {!pic && !error && (
          <div className="panov-wait">
            <div className="enc-spinner" />
          </div>
        )}
        {props.instruction && pic && <p className="panov-instruction">{props.instruction}</p>}
        {(error || mode === "none") && <p className="panov-error">{error ?? "This browser cannot draw the panorama."}</p>}
        <div className={`panov-overlay${marks ? "" : " hidden"}`} ref={overlay}>
          {labels.map((l, i) => (
            <span key={`l${l.id}`} className="panov-label" data-kind="label" data-index={i} style={{ display: "none" }}>
              {l.text}
            </span>
          ))}
          {hotspots.map((h, i) => (
            <button
              key={`h${h.id}`}
              type="button"
              className={`panov-hot ${h.kind}`}
              data-kind="hot"
              data-index={i}
              style={{ display: "none" }}
              title={h.title}
              aria-label={`${h.title}${h.kind === "scene" ? " (go there)" : ""}`}
              onMouseEnter={() => setStatus(h.title)}
              onMouseLeave={() => setStatus("")}
              onFocus={() => setStatus(h.title)}
              onBlur={() => setStatus("")}
              onClick={() => callbacks.current.onHotspot?.(h)}
            />
          ))}
          {hotspots.map((h, i) =>
            (
              <button
                key={`m${h.id}`}
                type="button"
                tabIndex={-1}
                aria-hidden="true"
                className={`panov-mark ${h.kind}`}
                data-kind="mark"
                data-index={i}
                style={{ display: "none" }}
                title={h.title}
                onMouseEnter={() => setStatus(h.title)}
                onMouseLeave={() => setStatus("")}
                onClick={() => callbacks.current.onHotspot?.(h)}
              >
                <span className="panov-mark-icon" aria-hidden="true">
                  {h.kind === "scene" ? "✥" : h.kind === "audio" ? <SpeakerIcon /> : "▣"}
                </span>
                {h.title}
              </button>
            ),
          )}
        </div>
      </div>
      <div className="panov-bar" style={{ maxWidth: Math.max(cssW, 320) }}>
        <button type="button" className="panov-btn" onClick={() => turn(-45)} aria-label="Look left" title="Look left">
          ◀
        </button>
        <button type="button" className="panov-btn" onClick={() => turn(45)} aria-label="Look right" title="Look right">
          ▶
        </button>
        <button type="button" className="panov-btn" onClick={() => zoomBy(1.25)} aria-label="Zoom in" title="Zoom in">
          +
        </button>
        <button type="button" className="panov-btn" onClick={() => zoomBy(0.8)} aria-label="Zoom out" title="Zoom out">
          −
        </button>
        <button type="button" className={`panov-btn${auto ? " on" : ""}`} onClick={() => setAuto((x) => !x)} aria-pressed={auto} title="Auto-pan">
          Auto
        </button>
        <button type="button" className="panov-btn" onClick={reset} title="Back to the starting view">
          Reset
        </button>
        {(hotspots.length > 0 || labels.length > 0) && (
          <label className="panov-check">
            <input type="checkbox" checked={showMarks} onChange={(e) => setShowMarks(e.target.checked)} /> Labels and hot spots
          </label>
        )}
        <span className="panov-status" aria-live="polite">
          {status || hint}
        </span>
        <span className="panov-fov" title="Field of view">
          {fovText}
        </span>
      </div>
    </div>
  );
}

/** The audio hotspots' speaker (vm-findings (d)). */
function SpeakerIcon() {
  return (
    <svg width="10" height="9" viewBox="0 0 10 9" fill="currentColor">
      <path d="M0 3h2l3-3v9L2 6H0z" />
      <path d="M6.5 2.5q1.5 2 0 4M8 1q3 3.5 0 7" fill="none" stroke="currentColor" strokeWidth="1" />
    </svg>
  );
}
