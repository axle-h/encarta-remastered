/**
 * What ENCARTA.EXE does around the control (tours-3d.md §4), as observed in the reference VM:
 * after `WORLDLOADED` it sends the default Tour Stop's camera command; a hotspot click replaces
 * the main view with the hotspot's `destmedia` page, and Back returns to the tour at the same
 * pose. Pure functions over the control; the page decides how to navigate.
 */
import type { TourControl } from "./control";
import { normYaw } from "./sim/camera";
import type { TourScene, TourStop } from "./types";

/** A camera pose in the disc's coordinates, as `camera.pos?` / `camera.pitch?` report it. */
export interface Pose {
  x: number;
  y: number;
  z: number;
  yaw: number;
  pitch: number;
  /** Active walk map (floor), or null. */
  walkmap: string | null;
}

const fmt = (n: number) => n.toFixed(6);

/** The control's current pose. */
export function currentPose(control: TourControl): Pose {
  const c = control.camera;
  return { x: c.x, y: c.y, z: -c.z, yaw: normYaw(c.yawDeg), pitch: c.pitchDeg, walkmap: control.walk.active?.name ?? null };
}

/** `x,y,z,yaw,pitch[,walkmap]`, for a URL parameter. */
export function encodePose(p: Pose): string {
  const nums = [p.x, p.y, p.z, p.yaw, p.pitch].map(fmt).join(",");
  return p.walkmap ? `${nums},${p.walkmap}` : nums;
}

export function decodePose(text: string | null | undefined): Pose | null {
  if (!text) return null;
  const parts = text.split(",");
  if (parts.length < 5) return null;
  const n = parts.slice(0, 5).map(Number);
  if (!n.every(Number.isFinite)) return null;
  const [x = 0, y = 0, z = 0, yaw = 0, pitch = 0] = n;
  const map = parts.slice(5).join(",").trim();
  return { x, y, z, yaw, pitch, walkmap: /^[\w.-]+$/.test(map) ? map : null };
}

/**
 * The command that puts the camera at `pose` the way the stop commands of the disc do: a move far
 * outside the world first (so the hotspot cell of the pose is entered again and its captions show,
 * and the final move is never one to the current position, which the original handles badly),
 * then the walk map, the position and the pitch.
 */
export function poseCommand(scene: TourScene, pose: Pose): string {
  const b = scene.model.worldBounds;
  const far = Math.max(...b.max.map(Math.abs), ...b.min.map(Math.abs)) + scene.model.worldSize + 10;
  const away = fmt(Math.round(far));
  const out = [`CAMERA.MOVETO=${away},${away},${away},${fmt(pose.yaw)}`];
  if (pose.walkmap) out.push(`WALKMAP.LOAD(${pose.walkmap})`);
  out.push(`CAMERA.MOVETO=${fmt(pose.x)},${fmt(pose.y)},${fmt(pose.z)},${fmt(pose.yaw)}`, `CAMERA.PITCH=${fmt(pose.pitch)}`);
  return `${out.join(";")};`;
}

/** The stop the host moves to after loading (the tour record's default stop), if known. */
export function defaultStop(scene: TourScene): TourStop | null {
  const stops = scene.stops ?? [];
  return stops.find((s) => s.id === scene.defaultStop) ?? null;
}

/** The media id a hotspot click opens (`destmedia`), or null when the converted data lacks it. */
export function hotspotDestination(scene: TourScene, hotspotId: number): number | null {
  const link = scene.hotspots.find((h) => h.contentId === hotspotId && h.destMedia);
  if (link?.destMedia) return link.destMedia;
  return scene.hotspotTargets?.find((t) => t.id === hotspotId)?.destMedia ?? null;
}

/**
 * The host's reaction to `WORLDLOADED`: restore `pose` (coming back to the tour) or move to the
 * default stop. Runs on the first tick after loading, as the host handles the event
 * asynchronously. Returns the command that will be sent, or null.
 */
export function playHostStart(control: TourControl, pose: Pose | null): string | null {
  const command = pose ? poseCommand(control.scene, pose) : defaultStop(control.scene)?.command ?? null;
  if (command) control.whenLoaded(() => void control.do(command));
  return command;
}
