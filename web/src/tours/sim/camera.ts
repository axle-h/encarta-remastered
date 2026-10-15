/**
 * The walking camera (tours-3d.md §6.1, §6.8–§6.10): eye position (glTF space), yaw and pitch in
 * the original degrees, the per-tick step with its time factor, collision, ground following and
 * the mouse/cursor rules.
 */
import { collide } from "./collision";
import type { WalkMapSet } from "./walkmap";

/** §6.10 defaults (control initialisation). Eye (0, 0.2, −3) original = (0, 0.2, 3) glTF. */
export const CAMERA_DEFAULTS = {
  x: 0,
  y: 0.2,
  z: 3,
  yawDeg: 0,
  pitchDeg: 0,
  height: 0.05,
  speedFly: 0.03,
  speedTurn: 1.3,
  pitchSpeed: 1.0,
  collisionRadius: 0.05,
  climbRate: 0.01,
  gravity: 0.01,
  fovDeg: 80,
  near: 0.05,
  far: -1,
} as const;

export const PITCH_LIMIT = 45;
export const PITCH_CLAMP = 44.9;

export interface CameraState {
  x: number;
  y: number;
  z: number;
  yawDeg: number;
  pitchDeg: number;
  /** Fall speed (gravity mode). */
  vfall: number;
  /**
   * The look vector as the original keeps it (§6.9): the target point's Y as a float32 (eye Y plus the
   * unit look vector's rise) and the vector's horizontal length. Ground following moves both eye and
   * target in float32, so the pitch derived from them drifts away from `pitchDeg`; the ±45° test of
   * the pitch keys uses the derived value.
   */
  lookY: number;
  lookH: number;
  height: number;
  speedFly: number;
  speedTurn: number;
  pitchSpeed: number;
  collisionRadius: number;
  climbRate: number;
  fovDeg: number;
  near: number;
  far: number;
  backgroundColor: [number, number, number];
  clearBackground: number;
}

export function newCamera(): CameraState {
  return {
    x: CAMERA_DEFAULTS.x,
    y: CAMERA_DEFAULTS.y,
    z: CAMERA_DEFAULTS.z,
    yawDeg: 0,
    pitchDeg: 0,
    vfall: 0,
    lookY: Math.fround(CAMERA_DEFAULTS.y),
    lookH: 1,
    height: CAMERA_DEFAULTS.height,
    speedFly: CAMERA_DEFAULTS.speedFly,
    speedTurn: CAMERA_DEFAULTS.speedTurn,
    pitchSpeed: CAMERA_DEFAULTS.pitchSpeed,
    collisionRadius: CAMERA_DEFAULTS.collisionRadius,
    climbRate: CAMERA_DEFAULTS.climbRate,
    fovDeg: CAMERA_DEFAULTS.fovDeg,
    near: CAMERA_DEFAULTS.near,
    far: CAMERA_DEFAULTS.far,
    backgroundColor: [0, 0, 0],
    clearBackground: 1,
  };
}

/** §6.9: `k = clamp(100 · dt, 1, 5)`, speeds are "per 1/100 s". */
export function timeFactor(dtSeconds: number, rate = 100, min = 1, max = 5): number {
  return Math.min(max, Math.max(min, rate * dtSeconds));
}

/** Horizontal look direction in glTF space for an original yaw: (sin θ, 0, −cos θ). */
export function lookDir(yawDeg: number): { x: number; z: number } {
  const r = (yawDeg * Math.PI) / 180;
  return { x: Math.sin(r), z: -Math.cos(r) };
}

/** `CAMERA.POS?` yaw: normalised to [0, 360). */
export function normYaw(yawDeg: number): number {
  const y = yawDeg % 360;
  return y < 0 ? y + 360 : y;
}

export interface StepInput {
  /** Move input m in −1…1 (forward positive). */
  move: number;
  /** Turn input t in −1…1 (right positive). */
  turn: number;
  pitchUp: boolean;
  pitchDown: boolean;
  /** Home: `CAMERA.PITCH=0`. */
  level: boolean;
}

export const NO_INPUT: StepInput = { move: 0, turn: 0, pitchUp: false, pitchDown: false, level: false };

/** How the eye follows the ground (§6.8): gravity (hardware device; software until the first walk step) or snap. */
export type GroundMode = "snap" | "gravity";

/** Points the look vector at `deg` (`CAMERA.PITCH=`, POS/MOVETO, Home): target = eye + unit vector, stored as float32. */
export function aimLook(cam: CameraState, deg: number): void {
  const r = (deg * Math.PI) / 180;
  cam.lookY = Math.fround(cam.y + Math.sin(r));
  cam.lookH = Math.cos(r);
}

/** Sets the pitch as `CAMERA.PITCH=` does: the stored value (what `PITCH?` reports) and the look vector. */
export function setPitch(cam: CameraState, deg: number): void {
  cam.pitchDeg = Math.fround(deg);
  aimLook(cam, deg);
}

/** The pitch derived from the eye and the float32 target (what the original tests against the limit). */
export function lookPitch(cam: CameraState): number {
  return (Math.atan2(cam.lookY - cam.y, cam.lookH) * 180) / Math.PI;
}

/**
 * Pitch keys: per tick, not time-scaled. Each step starts from the pitch derived from the look vector,
 * not from the stored value, and is refused when |derived + step| would reach the limit (§6.9).
 */
export function stepPitch(cam: CameraState, input: StepInput): void {
  if (input.level) setPitch(cam, 0);
  const step = (delta: number) => {
    const p = lookPitch(cam) + delta;
    if (Math.abs(p) < PITCH_LIMIT) setPitch(cam, p);
  };
  if (input.pitchUp) step(cam.pitchSpeed);
  if (input.pitchDown) step(-cam.pitchSpeed);
}

/**
 * The step (§6.9): turn first, then move along the horizontal look direction through collision.
 * Returns true when the eye moved.
 */
export function stepMove(cam: CameraState, input: StepInput, k: number, walk: WalkMapSet | null): boolean {
  if (input.turn !== 0) cam.yawDeg += 2 * k * cam.speedTurn * input.turn;
  if (input.move === 0) return false;
  const dist = k * cam.speedFly * input.move;
  const d = lookDir(cam.yawDeg);
  const dx = d.x * dist, dz = d.z * dist;
  const map = walk?.enabled ? walk.active : null;
  if (map && map.hasKind("collision")) {
    const p = collide(map, { x: cam.x, z: cam.z }, dx, dz, cam.collisionRadius);
    const moved = p.x !== cam.x || p.z !== cam.z;
    cam.x = p.x;
    cam.z = p.z;
    return moved;
  }
  cam.x += dx;
  cam.z += dz;
  return true;
}

/**
 * Ground following, every tick (§6.8), in the original's float32 arithmetic (§6.9: the x87 runs at single
 * precision, so every operation rounds to float32; a replay of the VM's gravity log matches bit for bit).
 * `target = HEIGHT + ground`. Gravity: unless the target is above the eye, `vfall += GRAVITY` and eye and
 * target point fall by it. Snap: `δ = target − E.y` is added to both. Then, in both modes, if the target is
 * still above the eye, `vfall = 0` and `δ = target − E.y` is added to both again (the landing).
 * Each addition rounds eye and target point separately, so the derived pitch can drift. In gravity mode the
 * fall and landing round every tick; in snap mode the eye settles on the target and δ becomes 0, except on
 * some sloped planes, where the ground point depends on E.y and the eye keeps alternating between two
 * float32 values (§6.9 "Snap mode").
 */
export function followGround(cam: CameraState, walk: WalkMapSet | null, mode: GroundMode, gravity: number): void {
  const map = walk?.enabled ? walk.active : null;
  if (!map) return;
  const gy = map.groundY(cam.x, cam.y, cam.z);
  if (gy === null) return;
  const f = Math.fround;
  const target = f(f(cam.height) + gy);
  const add = (delta: number) => {
    cam.y = f(delta + cam.y);
    cam.lookY = f(delta + cam.lookY);
  };
  if (mode === "gravity") {
    if (!(target > cam.y)) {
      cam.vfall = f(cam.vfall + f(gravity));
      add(-cam.vfall);
    }
  } else {
    add(f(target - cam.y));
  }
  if (target > cam.y) {
    cam.vfall = 0;
    add(f(target - cam.y));
  }
}

/**
 * Mouse vector while dragging in the 3D view (§6.9): offsets from the view centre with dead zones
 * of 15 % (x) and 8 % (y) of the half sizes, then the response curve v·|v|^0.5.
 */
export function mouseVector(x: number, y: number, w: number, h: number): { turn: number; move: number } {
  const hw = w / 2, hh = h / 2;
  const dx = x - hw;
  const dy = h - y - hh;
  const zx = 0.15 * hw, zy = 0.08 * hh;
  const axis = (d: number, half: number, zone: number) => {
    if (Math.abs(d) <= zone || half - zone <= 0) return 0;
    const v = (Math.sign(d) * (Math.abs(d) - zone)) / (half - zone);
    return Math.max(-1, Math.min(1, v));
  };
  const vx = axis(dx, hw, zx), vy = axis(dy, hh, zy);
  return { turn: vx * Math.sqrt(Math.abs(vx)), move: vy * Math.sqrt(Math.abs(vy)) };
}

/** Navigation cursor slot for a pointer position (§6.9): 0 centre, 1 right, 2 up, 3 left, 4 down. */
export function cursorZone(x: number, y: number, w: number, h: number): number {
  const dx = x - w / 2;
  const dy = h / 2 - y;
  if (Math.abs(dx) < 30 && Math.abs(dy) < 60) return 0;
  if (Math.abs(dy) <= Math.abs(dx)) return dx > 0 ? 1 : 3;
  return dy > 0 ? 2 : 4;
}
