/**
 * The tour control: everything ACTX3DX3 keeps as state, driven by script steps and the tick
 * (tours-3d.md §2–§8). No DOM and no WebGL here, so the whole simulation runs in tests; the
 * renderer (render/view.ts) and the page glue (runtime.ts) observe it.
 */
import { Layer2D } from "./hud/layer";
import { Interpreter, type Handlers } from "./script/interp";
import {
  aimLook,
  CAMERA_DEFAULTS,
  cursorZone,
  followGround,
  lookDir,
  mouseVector,
  newCamera,
  normYaw,
  PITCH_CLAMP,
  setPitch,
  stepMove,
  stepPitch,
  timeFactor,
  type CameraState,
  type GroundMode,
  type StepInput,
} from "./sim/camera";
import { Filmstrip, FlickerLight, lcg, Ripple, type Rng } from "./sim/effects";
import { WalkMapSet } from "./sim/walkmap";
import type { CursorSlot, LightInfo, ObjectInfo, RGB, SeqStep, Step, StepOf, TourScene, Vec3 } from "./types";

export type FovAxis = "horizontal" | "vertical";

export interface ControlOptions {
  /**
   * Device the original would run on. Software is what Encarta uses on a machine without a D3D device
   * (the reference VM); hardware re-renders clouds only every ~5° (§7.3.4). Both follow the ground in
   * gravity mode (§6.8; VM, debugger).
   */
  device?: "software" | "hardware";
  /** Overrides the ground mode the device implies. */
  groundMode?: GroundMode;
  /**
   * Simulation ticks per second. Default 64: the VM's idle rate (`SW 65.8` with DISPLAYFPS, the
   * 15.6 ms XP timer, §6.9). Walking and turning are time-scaled (100·SPEED.FLY units/s,
   * 200·SPEED.TURN °/s) for any rate from 20 to 100 Hz; pitch keys step once per tick.
   */
  tickHz?: number;
  /** Which axis `CAMERA.FOV` spans. The VM measured horizontal (§6.13); kept as a switch for experiments. */
  fovAxis?: FovAxis;
  /**
   * Shortest time the loading screen stays up, in seconds of simulation time from `script.load`.
   * The original streams one texture per tick after `3D.LOAD`; in the VM the LOADING screen
   * showed for about 2 s (Colosseum 0.67–2.65 s; harness loads 1.6–2.3 s). Default 2.
   */
  minLoadSeconds?: number;
  /** Hotspot handling on a switch to a cached walk map (see WalkMapSet.switchRule). */
  walkmapSwitch?: "settle" | "spec";
  rng?: Rng;
}

export interface ObjectRt {
  info: ObjectInfo;
  visible: boolean;
  sprite: boolean;
  noIllumination: boolean;
  water: boolean;
  filmstrip: Filmstrip | null;
  cloud: { file: string; scale: number } | null;
  cloudOffset: number;
  other: Record<string, string>;
}

export interface LightRt {
  info: LightInfo;
  on: boolean;
  colour: RGB;
  flicker: FlickerLight | null;
  /** Flicker slot in the vertex attributes, or null when the light was baked as static. */
  slot: number | null;
}

export interface WaterRt {
  ripple: Ripple | null;
  version: number;
  /** Texture size, set by the renderer when it knows the image. */
  size: [number, number] | null;
}

export interface ControlEvents {
  onEvent?: (text: string) => void;
  /** Called with every walk-map switch (floor changes). */
  onWalkmap?: (name: string) => void;
}

/** One rendered point of view, derived from the camera and the viewport. */
export interface View {
  eye: Vec3;
  /** Unit forward, right and up vectors (glTF space). */
  fwd: Vec3;
  right: Vec3;
  up: Vec3;
  /** Projection scale in pixels and the viewport centre. */
  k: number;
  cx: number;
  cy: number;
  /** Vertical field of view in degrees (for three.js). */
  fovY: number;
}

const SUCCESS = "Success";
const FAILED = "Failed";
const fmt = (n: number) => n.toFixed(6);
const bool = (b: boolean) => (b ? "1" : "0");

export class TourControl implements Handlers {
  readonly scene: TourScene;
  readonly interp: Interpreter;
  readonly walk: WalkMapSet;
  readonly layer: Layer2D;
  camera: CameraState = newCamera();
  readonly options: Required<Omit<ControlOptions, "rng">>;
  readonly rng: Rng;
  events: ControlEvents = {};

  // 3D model state.
  modelLoaded = false;
  modelOn = true;
  objects: ObjectRt[] = [];
  private byName = new Map<string, ObjectRt | null>();
  private byNode = new Map<string, ObjectRt>();
  /** Texture record (glTF material index) → replacement image (APPLYTEXTURE/FILMSTRIP). */
  textureRecords = new Map<number, string>();
  textureVersion = 0;
  /** Texture record → water ripple state. */
  water = new Map<number, WaterRt>();
  calc64bit = 0.1;
  lodDistance = 0;

  // Lights.
  lights: LightRt[] = [];
  lightsOn = 1;
  lightsVersion = 0;

  // Scene switches.
  gravity: number = CAMERA_DEFAULTS.gravity;
  fog = { color: [75, 150, 230] as RGB, start: 0, end: 20, enabled: false };
  ambient: RGB | null = null;
  specular = false;
  highSpecular = false;
  displayFps = false;
  halfResDriving = false;
  showCollision = false;
  cursorsEnabled = false;
  cursorSlots: (CursorSlot | null)[] = [];

  // Loading (§3.7).
  loading = false;
  private pending: SeqStep[] = [];
  private pc = 0;
  private waitingForModel = false;
  private loadStartedAt = 0;
  /** Host actions waiting for the end of loading (run on the first tick after it). */
  private afterLoad: (() => void)[] = [];
  /** True while the original would still be streaming the model (3D view hidden). */
  modelReady: () => boolean = () => true;
  loaded = false;

  // Input.
  readonly keys = new Set<string>();
  mouse = { x: 0, y: 0, down: false, inside: false, nav: false };
  viewport = { w: 640, h: 480 };

  // Clock.
  simTime = 0;
  tickCount = 0;
  /** Image paths by upper-cased disc file name, from every converted step. */
  private images = new Map<string, string>();
  private home = { x: CAMERA_DEFAULTS.x, y: CAMERA_DEFAULTS.y, z: CAMERA_DEFAULTS.z, yawDeg: 0, pitchDeg: 0 };

  constructor(scene: TourScene, options: ControlOptions = {}) {
    this.scene = scene;
    const device = options.device ?? "software";
    this.options = {
      device,
      groundMode: options.groundMode ?? "gravity",
      tickHz: options.tickHz ?? 64,
      fovAxis: options.fovAxis ?? "horizontal",
      walkmapSwitch: options.walkmapSwitch ?? "settle",
      minLoadSeconds: options.minLoadSeconds ?? 2,
    };
    this.rng = options.rng ?? Math.random;
    this.interp = new Interpreter(this);
    this.walk = new WalkMapSet((name) => scene.walkmaps.maps.find((m) => m.file.toLowerCase() === name.toLowerCase()) ?? null);
    this.walk.switchRule = this.options.walkmapSwitch;
    this.layer = new Layer2D(scene.layers);
    this.collectImages();
  }

  private collectImages(): void {
    const visit = (steps: readonly Step[]) => {
      for (const s of steps) {
        if ((s.op === "applyTexture" || s.op === "filmstrip") && s.image) {
          this.images.set(s.file.toUpperCase(), s.image);
          if (s.resolved) this.images.set(s.resolved.toUpperCase(), s.image);
        }
        if (s.op === "alarmNew" || s.op === "elementMacro") visit(s.steps);
        if (s.op === "defineMacro" && s.steps) visit(s.steps);
      }
    };
    visit(this.scene.loadSequence);
    for (const m of this.scene.macros) visit(m.steps);
    for (const map of this.scene.walkmaps.maps) for (const c of map.cells) if (c.kind === "hotspot") { visit(c.enter.steps); visit(c.exit.steps); }
  }

  // ---------------------------------------------------------------- host interface

  /** `Do(command)` (§2.1). */
  do(command: string): string {
    return this.interp.do(command);
  }

  /** Runs `f` on the first tick after loading has finished (host reactions to `WORLDLOADED`). */
  whenLoaded(f: () => void): void {
    this.afterLoad.push(f);
  }

  fireEvent(text: string): void {
    this.events.onEvent?.(text);
  }

  /** Starts `load.3sc` (§2.2 step 2.3) from the converter's executed load sequence. */
  startLoad(): void {
    const macroSteps = new Map(this.scene.macros.map((m) => [`${m.name}@${m.origin}`, m.steps]));
    this.pending = this.scene.loadSequence.map((s) =>
      s.op === "defineMacro" && !s.steps ? { ...s, steps: macroSteps.get(`${s.name}@${s.origin}`) ?? [] } : s,
    );
    this.pc = 0;
    this.loading = true;
    this.loaded = false;
    this.waitingForModel = false;
    this.loadStartedAt = this.simTime;
    this.runPending(false);
  }

  /** Runs load steps until the model load suspends the script, or (resumed) until a top-level draw. */
  private runPending(resumed: boolean): void {
    while (this.pc < this.pending.length) {
      const step = this.pending[this.pc++];
      if (!step) break;
      this.interp.exec(step);
      if (step.op === "modelLoad") {
        this.waitingForModel = true;
        return;
      }
      if (resumed && step.op === "draw" && step.origin.startsWith("load.3sc:")) return;
    }
    this.loading = false;
    this.loaded = true;
    this.keys.clear();
  }

  // ---------------------------------------------------------------- the tick

  /** One simulation tick of `1/tickHz` seconds (§2.3). */
  tick(): void {
    const dt = 1 / this.options.tickHz;
    this.simTime += dt;
    this.tickCount++;
    if (this.loading) {
      if (this.waitingForModel) {
        if (this.modelReady() && this.simTime - this.loadStartedAt >= this.options.minLoadSeconds - 1e-9) {
          this.waitingForModel = false;
          this.modelLoaded = true;
          this.runPending(true);
        }
      } else {
        this.runPending(true);
      }
    }
    if (this.loaded && this.afterLoad.length) {
      const queue = this.afterLoad.splice(0);
      for (const f of queue) f();
    }
    if (!this.interp.timerOn) return;
    this.interp.pollAlarms(this.simTime);
    if (!this.loading) this.step(dt);
    this.advanceEffects(dt);
  }

  /** Input → turn/move → collision → ground → hotspots (§6.9). */
  private step(dt: number): void {
    const input = this.stepInput();
    stepPitch(this.camera, input);
    const k = timeFactor(dt);
    const walk = this.walk.enabled ? this.walk : null;
    stepMove(this.camera, input, k, walk);
    followGround(this.camera, walk, this.options.groundMode, this.gravity);
    this.hotspotCheck();
  }

  private hotspotCheck(): void {
    this.walk.hotspotCheck(this.camera.x, this.camera.z, (steps) => this.interp.run(steps));
  }

  stepInput(): StepInput {
    const k = this.keys;
    let move = 0, turn = 0;
    if (this.mouse.down && this.mouse.nav && !this.layer.pressedElement?.def.capturesMouse) {
      const v = mouseVector(this.mouse.x, this.mouse.y, this.viewport.w, this.viewport.h);
      move = v.move;
      turn = v.turn;
    }
    if (k.has("ArrowUp") || k.has("ArrowDown")) move = (k.has("ArrowUp") ? 1 : 0) - (k.has("ArrowDown") ? 1 : 0);
    if (k.has("ArrowLeft") || k.has("ArrowRight")) turn = (k.has("ArrowRight") ? 1 : 0) - (k.has("ArrowLeft") ? 1 : 0);
    return { move, turn, pitchUp: k.has("PageUp"), pitchDown: k.has("PageDown"), level: k.has("Home") };
  }

  private advanceEffects(dt: number): void {
    if (!this.modelLoaded || !this.modelOn) return;
    for (const l of this.lights) {
      if (l.flicker && l.flicker.advance(dt, this.rng)) this.lightsVersion++;
    }
    for (const o of this.objects) o.filmstrip?.advance(dt);
    for (const w of this.water.values()) {
      if (!w.size) continue;
      if (!w.ripple) w.ripple = new Ripple(w.size[0], w.size[1]);
      // Once per drawn frame, skipping frames only above 80 fps (§7.4.6): at 64 ticks/s, every tick.
      if (w.ripple.frame(1 / dt, this.rng)) w.version++;
    }
  }

  // ---------------------------------------------------------------- views and cursors

  /** The point of view for rendering and caption anchoring (§8.5.2). */
  view(): View {
    const c = this.camera;
    const y = (c.yawDeg * Math.PI) / 180, p = (c.pitchDeg * Math.PI) / 180;
    const fwd: Vec3 = [Math.cos(p) * Math.sin(y), Math.sin(p), -Math.cos(p) * Math.cos(y)];
    const right: Vec3 = [Math.cos(y), 0, Math.sin(y)];
    const up: Vec3 = [
      right[1] * fwd[2] - right[2] * fwd[1],
      right[2] * fwd[0] - right[0] * fwd[2],
      right[0] * fwd[1] - right[1] * fwd[0],
    ];
    const { w, h } = this.viewport;
    const half = Math.tan(((c.fovDeg * Math.PI) / 180) / 2);
    const k = this.options.fovAxis === "horizontal" ? w / 2 / half : h / 2 / half;
    const fovY = (2 * Math.atan(h / 2 / k) * 180) / Math.PI;
    return { eye: [c.x, c.y, c.z], fwd, right, up, k, cx: w / 2, cy: h / 2, fovY };
  }

  /** Cone test and projection of an anchor (§8.5.2). */
  projectAnchor(v: View, p: Vec3): { x: number; y: number } | null {
    const d: Vec3 = [p[0] - v.eye[0], p[1] - v.eye[1], p[2] - v.eye[2]];
    const len = Math.hypot(d[0], d[1], d[2]);
    if (len === 0) return null;
    const cos = (d[0] * v.fwd[0] + d[1] * v.fwd[1] + d[2] * v.fwd[2]) / len;
    if (!(cos > ((this.camera.fovDeg * Math.PI) / 180) * 0.5)) return null;
    const xc = d[0] * v.right[0] + d[1] * v.right[1] + d[2] * v.right[2];
    const yc = d[0] * v.up[0] + d[1] * v.up[1] + d[2] * v.up[2];
    const zc = d[0] * v.fwd[0] + d[1] * v.fwd[1] + d[2] * v.fwd[2];
    return { x: Math.trunc((v.k * xc) / zc + v.cx), y: Math.trunc(v.cy - (v.k * yc) / zc) };
  }

  /** Lays out the 2D layer for this frame. */
  layoutLayer(): View {
    const v = this.view();
    this.layer.layout({ project: (p) => this.projectAnchor(v, p) }, this.viewport.w, this.viewport.h);
    return v;
  }

  /** The cursor slot for the pointer (§6.9, §8.5.3), or null for the default cursor. */
  cursorSlot(): number | null {
    if (!this.cursorsEnabled || !this.mouse.inside) return null;
    const pressed = this.layer.pressedElement;
    if (pressed?.def.capturesMouse) return 5;
    if (this.layer.hoveredElement?.def.hasText) return 5;
    return cursorZone(this.mouse.x, this.mouse.y, this.viewport.w, this.viewport.h);
  }

  /** Pointer input from the page (client pixels inside the control). */
  pointer(x: number, y: number, down: boolean, inside = true): void {
    const wasDown = this.mouse.down;
    this.mouse.x = x;
    this.mouse.y = y;
    this.mouse.inside = inside;
    const hit = this.layer.pointer(x, y, down, (steps) => this.interp.run(steps));
    if (down && !wasDown) this.mouse.nav = !(hit && hit.def.capturesMouse);
    if (!down) this.mouse.nav = false;
    this.mouse.down = down;
  }

  // ---------------------------------------------------------------- handlers

  reset(): void {
    this.modelLoaded = false;
    this.modelOn = true;
    this.objects = [];
    this.byName.clear();
    this.byNode.clear();
    this.textureRecords.clear();
    this.textureVersion++;
    this.water.clear();
    this.lights = [];
    this.lightsOn = 1;
    this.lightsVersion++;
    this.walk.clear();
    this.layer.clear();
    this.layer.file = null;
    this.camera = newCamera();
    this.fog = { color: [75, 150, 230], start: 0, end: 20, enabled: false };
  }

  private createObjects(): void {
    this.objects = this.scene.objects.map((info) => ({
      info,
      visible: true,
      sprite: false,
      noIllumination: false,
      water: false,
      filmstrip: null,
      cloud: null,
      cloudOffset: 0,
      other: {},
    }));
    this.byNode = new Map(this.objects.map((o) => [o.info.node, o]));
    // Statements address the first mesh whose upper-cased name matches (§7.1).
    this.byName.clear();
    for (const m of this.scene.model.meshes) {
      const key = m.name.toUpperCase();
      if (this.byName.has(key)) continue;
      const o = m.node ? this.byNode.get(m.node) : undefined;
      this.byName.set(key, o ?? null);
    }
  }

  object(id: string): ObjectRt | null {
    const m = /^\[(\d+)\]$/.exec(id);
    if (m) {
      const mesh = this.scene.model.meshes[Number(m[1])];
      return mesh?.node ? (this.byNode.get(mesh.node) ?? null) : null;
    }
    return this.byName.get(id.toUpperCase()) ?? null;
  }

  objectByNode(node: string): ObjectRt | null {
    return this.byNode.get(node) ?? null;
  }

  light(id: string): LightRt | null {
    const m = /^\[(\d+)\]$/.exec(id);
    if (m) return this.lights[Number(m[1])] ?? null;
    return this.lights.find((l) => l.info.name.toUpperCase() === id) ?? null;
  }

  private loadLights(file: string): boolean {
    const L = this.scene.lights;
    if (!L || L.file.toLowerCase() !== file.toLowerCase()) return false;
    const slotOf = new Map(L.flickerSlots.map((s) => [s.light, s.slot]));
    this.lights = L.lights.map((info) => ({ info, on: true, colour: [...info.fileColour] as RGB, flicker: null, slot: slotOf.get(info.index) ?? null }));
    this.lightsVersion++;
    return true;
  }

  private image(file: string, given: string | null): string | null {
    return given ?? this.images.get(file.toUpperCase()) ?? null;
  }

  apply(step: Step): string {
    switch (step.op) {
      case "draw":
      case "device":
        return SUCCESS;
      case "loadScript":
        // The host's `script.load=load.3sc` starts the converted load sequence. Every nested
        // SCENE.LOAD of the disc was expanded in place by the converter; there are no script
        // files at run time.
        // The VM replies `Failed` to `script.load=…load.3sc` even though the load then runs (§2.2).
        if (/(^|\\)LOAD\.3SC$/.test(step.file.toUpperCase()) && !this.loading) this.startLoad();
        return FAILED;
      case "setScene":
        return this.setScene(step);
      case "ambient":
        if (!this.modelLoaded && !this.waitingForModel) return FAILED;
        this.ambient = step.rgb;
        return SUCCESS;
      case "fogColor":
        this.fog.color = step.rgb;
        return SUCCESS;
      case "cursor": {
        const f = this.scene.cursors.files[step.file.toLowerCase()];
        this.cursorSlots[step.slot] = f
          ? { slot: step.slot, meaning: "", file: step.file, image: f.image, hotspot: f.hotspot }
          : { slot: step.slot, meaning: "", file: step.file, image: step.image, hotspot: null };
        return SUCCESS;
      }
      case "camera":
        return this.cameraProp(step);
      case "cameraCommand":
        return this.cameraCommand(step.command);
      case "walkmapLoad": {
        const ok = this.walk.load(step.resolved ?? step.file, { x: this.camera.x, z: this.camera.z });
        if (ok && this.walk.active) this.events.onWalkmap?.(this.walk.active.name);
        return ok ? SUCCESS : FAILED;
      }
      case "walkmapEnable":
        this.walk.on = step.on ? 1 : 0;
        return SUCCESS;
      case "walkmapToggle":
        this.walk.on = -this.walk.on;
        return SUCCESS;
      case "walkmapDelete":
        return this.walk.active?.deleteKind(step.what) ? SUCCESS : FAILED;
      case "lightsLoad":
        return this.loadLights(step.file) ? SUCCESS : FAILED;
      case "lightsEnable":
        this.lightsOn = step.on ? 1 : 0;
        this.lightsVersion++;
        return SUCCESS;
      case "lightsToggle":
        this.lightsOn = -this.lightsOn;
        return SUCCESS;
      case "lightsDelete":
        this.lights = [];
        this.lightsVersion++;
        return SUCCESS;
      case "light":
        return this.lightProp(step);
      case "modelLoad":
        if (this.scene.model.file.toLowerCase() !== step.file.toLowerCase()) return FAILED;
        this.createObjects();
        this.modelLoaded = this.modelReady();
        return SUCCESS;
      case "modelEnable":
        this.modelOn = step.on;
        return SUCCESS;
      case "modelSet":
        if (step.prop === "calc64bit") this.calc64bit = step.value;
        else this.lodDistance = step.value;
        return SUCCESS;
      case "modelCommand":
        if (step.command === "unhideall") for (const o of this.objects) o.visible = true;
        else if (step.command === "togglehidden") for (const o of this.objects) o.visible = !o.visible;
        else if (step.command === "delete") {
          this.objects = [];
          this.byName.clear();
          this.byNode.clear();
          this.modelLoaded = false;
        }
        return SUCCESS;
      case "objectFlag":
      case "applyTexture":
      case "filmstrip":
      case "cloud":
      case "cloudOffset":
      case "objectOther":
        return this.objectProp(step);
      case "layerLoad":
        return this.layer.load(step.file) ? SUCCESS : FAILED;
      case "layerEnable":
        this.layer.on = step.on;
        return SUCCESS;
      case "layerToggle":
        return bool(this.layer.on);
      case "layerNew":
        this.layer.clear();
        return SUCCESS;
      case "iconFile":
        this.layer.iconImage = step.image ?? this.scene.hud.iconStrip?.image ?? null;
        for (const el of this.layer.elements) el.rasterVersion++;
        return this.layer.iconImage ? SUCCESS : FAILED;
      case "icons":
        this.layer.iconCount = step.count;
        return SUCCESS;
      case "layerSet":
        return this.layerSet(step);
      case "elementVisible":
      case "elementText":
      case "elementAnchor":
      case "elementAnchored":
      case "elementIcon":
      case "elementMacro":
      case "elementNextFrame":
      case "elementSky":
      case "elementOther":
        return this.elementProp(step);
      default:
        return FAILED;
    }
  }

  private setScene(step: StepOf<"setScene">): string {
    const v = step.value;
    switch (step.prop) {
      case "gravity": this.gravity = Number(v); break;
      case "specular": this.specular = Boolean(v); break;
      case "highSpecular": this.highSpecular = Boolean(v); break;
      // The VM replies `Failed` but the overlay does switch on.
      case "displayFps": this.displayFps = Boolean(v); return FAILED;
      case "cursors": this.cursorsEnabled = Boolean(v); break;
      case "halfResDriving": this.halfResDriving = Boolean(v); break;
      case "showCollision": this.showCollision = Boolean(v); break;
      case "fogStart": this.fog.start = Number(v); break;
      case "fogEnd": this.fog.end = Number(v); this.fog.enabled = this.fog.end > 0.01; break;
      // The control only checks that the file exists (§2.2): `1`, or `-1` for an empty value. Files come from
      // the converted container either way.
      case "itsFileUrl": return String(v).trim() ? "1" : "-1";
      default: break;
    }
    return SUCCESS;
  }

  private cameraProp(step: StepOf<"camera">): string {
    const c = this.camera;
    const v = step.values;
    const n = (i: number) => v[i] ?? 0;
    switch (step.prop) {
      case "pos":
      case "moveTo":
        c.x = n(0); c.y = Math.fround(n(1)); c.z = n(2);
        if (v.length > 3) c.yawDeg = n(3);
        aimLook(c, c.pitchDeg); // keeps the look vector, then reapplies the stored pitch (§6.10)
        this.hotspotCheck();
        break;
      case "move":
        c.x += n(0); c.z += n(2);
        c.y = Math.fround(c.y + n(1)); c.lookY = Math.fround(c.lookY + n(1));
        break;
      case "pitch":
        setPitch(c, Math.max(-PITCH_CLAMP, Math.min(PITCH_CLAMP, n(0))));
        break;
      case "fov": c.fovDeg = n(0); break;
      case "height": c.height = n(0); break;
      case "speedFly": c.speedFly = n(0); break;
      case "speedTurn": c.speedTurn = n(0); break;
      case "pitchSpeed": c.pitchSpeed = n(0); break;
      case "collisionRadius": c.collisionRadius = n(0); break;
      case "climbRate": c.climbRate = n(0); break;
      case "near": c.near = n(0); break;
      case "far": c.far = n(0); break;
      case "backgroundColor": c.backgroundColor = [n(0), n(1), n(2)]; break;
      case "clearBackground": c.clearBackground = n(0); break;
      default: return FAILED;
    }
    return SUCCESS;
  }

  private cameraCommand(command: string): string {
    const c = this.camera;
    if (command === "home" || command === "homePov") {
      c.x = this.home.x; c.y = this.home.y; c.z = this.home.z;
      c.yawDeg = this.home.yawDeg; setPitch(c, this.home.pitchDeg);
      c.height = CAMERA_DEFAULTS.height;
      this.hotspotCheck();
      return SUCCESS;
    }
    // POV is the only mode the tours use; OMNI (orbit) is not specified (§6.14).
    return command === "pov" ? SUCCESS : FAILED;
  }

  private lightProp(step: StepOf<"light">): string {
    const l = this.light(step.id);
    if (!l) return FAILED;
    switch (step.prop) {
      case "flicker": {
        const p = Number(step.value);
        if (p > 1e-6) {
          if (l.flicker) l.flicker.period = p;
          else l.flicker = new FlickerLight(p, l.colour);
        } else if (l.flicker) {
          l.colour = [...l.flicker.saved];
          l.flicker = null;
        }
        break;
      }
      case "on": l.on = Boolean(step.value); break;
      case "color": l.colour = step.value as RGB; break;
      default: break; // pos, static: no visible effect on the baked data
    }
    this.lightsVersion++;
    return SUCCESS;
  }

  private objectProp(step: Extract<Step, { id: string }>): string {
    const o = this.object(step.id);
    if (!o) return FAILED;
    switch (step.op) {
      case "objectFlag":
        if (step.prop === "water") this.setWater(o, step.value);
        else o[step.prop] = step.value;
        return SUCCESS;
      case "applyTexture": {
        const img = this.image(step.resolved ?? step.file, step.image);
        const rec = o.info.textureRecord;
        if (!img || rec === null) return FAILED;
        this.textureRecords.set(rec, img);
        this.textureVersion++;
        return SUCCESS;
      }
      case "filmstrip": {
        if (step.frameWidth <= 0) {
          o.filmstrip = null;
          return SUCCESS;
        }
        const img = this.image(step.resolved ?? step.file, step.image);
        const rec = o.info.textureRecord;
        if (img && rec !== null) {
          this.textureRecords.set(rec, img);
          this.textureVersion++;
        }
        o.filmstrip = new Filmstrip(o.info.filmstripFrames ?? 16, step.wait, this.rng);
        return SUCCESS;
      }
      case "cloud":
        o.cloud = { file: step.file, scale: step.scale === 0 ? 0.5 : step.scale };
        return SUCCESS;
      case "cloudOffset":
        if (!o.cloud) return FAILED;
        o.cloudOffset = step.radians;
        return SUCCESS;
      case "objectOther":
        o.other[step.prop] = step.value;
        return SUCCESS;
      default:
        return FAILED;
    }
  }

  private setWater(o: ObjectRt, on: boolean): void {
    o.water = on;
    const rec = o.info.textureRecord;
    if (rec === null) return;
    if (on) {
      // Turning water on restarts the height fields from zero (§7.4.6).
      const prev = this.water.get(rec);
      this.water.set(rec, { ripple: null, version: (prev?.version ?? 0) + 1, size: prev?.size ?? null });
    } else {
      this.water.delete(rec);
    }
  }

  private layerSet(step: StepOf<"layerSet">): string {
    this.layer.settings[step.prop] = step.value;
    if (step.prop === "captionmetrics") {
      const [voff, scale, adder] = step.value.split(",").map(Number);
      this.layer.metrics = {
        verticalOffset: Number.isFinite(voff) ? (voff as number) : this.layer.metrics.verticalOffset,
        scale: Number.isFinite(scale) ? (scale as number) : this.layer.metrics.scale,
        adder: Number.isFinite(adder) ? (adder as number) : this.layer.metrics.adder,
      };
      for (const el of this.layer.elements) this.layer.relayout(el);
    } else {
      for (const el of this.layer.elements) el.rasterVersion++;
    }
    return SUCCESS;
  }

  private elementProp(step: Extract<Step, { id: string }>): string {
    const el = this.layer.find(step.id);
    if (!el) return FAILED;
    switch (step.op) {
      case "elementVisible":
        el.visible = step.value === null ? !el.visible : step.value;
        break;
      case "elementText":
        this.layer.setText(el, step.text);
        break;
      case "elementAnchor":
        el.anchor = [...step.point];
        break;
      case "elementAnchored":
        el.anchored = step.value;
        break;
      case "elementIcon":
        this.layer.setIcon(el, step.icon);
        break;
      case "elementMacro":
        el.macros[step.event] = step.steps;
        el.macroText[step.event] = step.script;
        break;
      case "elementNextFrame":
        this.layer.nextFrame(el, step.frames);
        break;
      case "elementSky":
        el.sky = step.value;
        break;
      case "elementOther":
        el.other[step.prop] = step.value;
        break;
      default:
        return FAILED;
    }
    return SUCCESS;
  }

  // ---------------------------------------------------------------- queries

  query(step: StepOf<"query">): string {
    const what = step.what;
    const c = this.camera;
    switch (step.subsystem) {
      case "CAMERA": {
        const yaw = normYaw(c.yawDeg);
        const map: Record<string, string> = {
          POS: `${fmt(c.x)},${fmt(c.y)},${fmt(-c.z)},${fmt(yaw)}`,
          PITCH: fmt(c.pitchDeg),
          FOV: fmt(c.fovDeg),
          HEIGHT: fmt(c.height),
          "SPEED.FLY": fmt(c.speedFly),
          "SPEED.TURN": fmt(c.speedTurn),
          PITCHSPEED: fmt(c.pitchSpeed),
          COLLISIONRADIUS: fmt(c.collisionRadius),
          CLIMBRATE: fmt(c.climbRate),
          NEAR: fmt(c.near),
          FAR: fmt(c.far),
          VIEWMODE: "0",
        };
        return map[what] ?? FAILED;
      }
      case "WALKMAP": {
        const map = this.walk.active;
        if (what === "ON") return bool(this.walk.on !== 0);
        if (what === "OFF") return bool(this.walk.on === 0);
        if (!map) return FAILED;
        if (what === "CELL") {
          const g = map.cellOf(c.x, c.z);
          return g ? `${g[0]},${g[1]}` : FAILED;
        }
        const kinds: Record<string, "visibility" | "ground" | "collision" | "hotspot"> = {
          VISIBILITY: "visibility", GROUND: "ground", COLLISION: "collision", HOTSPOT: "hotspot",
        };
        const kind = kinds[what];
        if (kind) return map.find(kind, c.x, c.z) ? "" : FAILED;
        const over = /^\[(\d+)\]\.OVER$/.exec(what);
        if (over) {
          const idx = Number(over[1]);
          const cell = map.file.cells.find((x) => x.index === idx);
          return bool(!!cell && map.find(cell.kind, c.x, c.z) === cell);
        }
        return FAILED;
      }
      case "SCENE":
      case "SCRIPT":
      case "": {
        if (what.startsWith("WORLDINFO")) {
          const b = this.scene.model.worldBounds;
          const ctr = [0, 1, 2].map((i) => ((b.min[i] ?? 0) + (b.max[i] ?? 0)) / 2);
          return `${fmt(this.scene.model.worldSize)};${fmt(ctr[0] ?? 0)},${fmt(ctr[1] ?? 0)},${fmt(-(ctr[2] ?? 0))}`;
        }
        if (what === "GRAVITY") return fmt(this.gravity);
        if (what === "CURSORS") return bool(this.cursorsEnabled);
        if (what === "AMBIENT") return this.ambient ? this.ambient.join(",") : FAILED;
        if (what === "FOGCOLOR") return this.fog.color.join(",");
        if (what === "SHOWCOLLISION") return bool(this.showCollision);
        return FAILED;
      }
      case "LIGHTS": {
        if (what === "ON") return bool(this.lightsOn !== 0);
        if (what === "OFF") return bool(this.lightsOn === 0);
        const m = /^(.*)\.FLICKER$/.exec(what);
        if (m) {
          const l = this.light(m[1] ?? "");
          return l ? fmt(l.flicker?.period ?? 0) : FAILED;
        }
        return FAILED;
      }
      case "3D":
      case "MODEL": {
        if (what === "CLOUDS" || what === "CLOUD") return String(this.objects.filter((o) => o.cloud).length);
        const m = /^(.*)\.(VISIBLE|SPRITE|NOILLUMINATION|WATER)$/.exec(what);
        if (m) {
          const o = this.object(m[1] ?? "");
          if (!o) return FAILED;
          const p = m[2] === "VISIBLE" ? o.visible : m[2] === "SPRITE" ? o.sprite : m[2] === "WATER" ? o.water : o.noIllumination;
          return bool(p);
        }
        return FAILED;
      }
      case "2D": {
        if (what === "ON") return bool(this.layer.on);
        if (what === "OFF") return bool(!this.layer.on);
        if (what === "ICONS") return String(this.layer.iconCount);
        const m = /^(.*)\.(VISIBLE|TEXT|ANCHORED|ANCHORPOINT)$/.exec(what);
        if (m) {
          const el = this.layer.find(m[1] ?? "");
          if (!el) return FAILED;
          if (m[2] === "VISIBLE") return bool(el.visible);
          if (m[2] === "TEXT") return el.text;
          if (m[2] === "ANCHORED") return bool(el.anchored);
          return `${fmt(el.anchor[0])},${fmt(el.anchor[1])},${fmt(-el.anchor[2])}`;
        }
        return FAILED;
      }
      default:
        return FAILED;
    }
  }

  /** The current horizontal look direction (for the site map). */
  heading(): { x: number; z: number } {
    return lookDir(this.camera.yawDeg);
  }
}
