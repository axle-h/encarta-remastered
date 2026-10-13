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

/** How the eye follows the ground (§6.8): snap (software device) or gravity (hardware device). */
export type GroundMode = "snap" | "gravity";

/** Pitch keys: per tick, not time-scaled, refused when |pitch| would reach the limit. */
export function stepPitch(cam: CameraState, input: StepInput): void {
  if (input.level) cam.pitchDeg = 0;
  let p = cam.pitchDeg;
  if (input.pitchUp) p += cam.pitchSpeed;
  if (input.pitchDown) p -= cam.pitchSpeed;
  if (Math.abs(p) < PITCH_LIMIT) cam.pitchDeg = p;
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

/** Ground following, every tick (§6.8). */
export function followGround(cam: CameraState, walk: WalkMapSet | null, mode: GroundMode, gravity: number): void {
  const map = walk?.enabled ? walk.active : null;
  if (!map) return;
  const gy = map.groundY(cam.x, cam.y, cam.z);
  if (gy === null) return;
  const target = gy + cam.height;
  if (mode === "gravity") {
    if (cam.y >= target) {
      cam.vfall += gravity;
      cam.y -= cam.vfall;
    }
    if (cam.y < target) {
      cam.vfall = 0;
      cam.y = target;
    }
  } else {
    cam.y = target;
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
