/**
 * Behaviour measured in the Windows XP reference VM (docs/vm-findings.md, tours section), checked
 * on synthetic scenes. Each test names the observation it reproduces.
 */
import { describe, expect, it } from "vitest";
import { TourControl } from "./control";
import { decodePose, encodePose, hotspotDestination, playHostStart, poseCommand } from "./host";
import { parseCommand } from "./script/parse";
import { stepPitch, newCamera, NO_INPUT } from "./sim/camera";
import { FixedTicker } from "./sim/clock";
import { FlickerLight, lcg } from "./sim/effects";
import { coversGridCell } from "./sim/walkmap";
import type { Cell, ElementDef, LayerFile, SeqStep, TourScene, WalkMapFile } from "./types";

// ------------------------------------------------------------------ synthetic scene

const square = (x0: number, z0: number, x1: number, z1: number, y = 0) => [x0, y, z0, x1, y, z0, x1, y, z1, x0, y, z1];

function element(name: string, rect: [number, number, number, number], extra: Partial<ElementDef> = {}): ElementDef {
  return {
    index: 0, name, class: "hotspot", kind: "hotspot", rect, uv: [0, 0, 1, 1], primitive: 6, additive: false, blend: 2,
    textured: true, capturesMouse: true, colorKey: false, isLabel: true, hasText: true, diffuse: "#FFFFFF",
    textColor: "#C0D1E3", frameColor: "#C0D1E3", labelFill: "#FF0000", icons: { normal: 0, pressed: 1, hover: 2 },
    font: { face: "Tahoma", height: -11, weight: 400, charset: 1 }, fileText: "ERS HotSpot", sourceImage: null, image: null,
    state: null, ...extra,
  };
}

const loadingLayer: LayerFile = {
  file: "begin.32d", version: 1, on: true, authoringIconFile: "", final: false,
  elements: [element("_0", [39, 6, 167, 134], { class: "image", kind: "plain", isLabel: false, hasText: false, capturesMouse: false, uv: [0, 0, 1, 1 / 16] })],
} as unknown as LayerFile;
const finalLayer: LayerFile = {
  file: "lh.32d", version: 1, on: true, authoringIconFile: "", final: true,
  elements: [element("_H1", [10, 10, 101, 32])],
} as unknown as LayerFile;

/** A walk map: ground and collision everywhere in −5…5, one hotspot cell around (0, 0), walls given. */
function walkmap(walls: [number, number, number, number][] = []): WalkMapFile {
  const cells: Cell[] = [
    { index: 0, kind: "ground", polygons: [square(-5, -5, 5, 5)], plane: [0, 1, 0, 0] },
    { index: 1, kind: "collision", level: 0, polygons: [square(-5, -5, 5, 5)], segments: walls.map(([a, b, c, d]) => [a, b, c, d, 1, 0]) },
    { index: 2, kind: "hotspot", polygons: [square(-1, -1, 1, 1)],
      enter: { script: "2D._H1.VISIBLE=1;", steps: parseCommand("2D._H1.VISIBLE=1;") },
      exit: { script: "2D._H1.VISIBLE=0;", steps: parseCommand("2D._H1.VISIBLE=0;") } },
  ];
  return { file: "w.3wm", version: 3, bbox: { min: [-10, -1, -10], max: [10, 1, 10] }, cells };
}

/** A load.3sc in the disc's shape: loading layer and spinner, model, walk map, final layer, events. */
const LOAD = [
  "2D.LOAD(begin.32d)",
  "SCENE.ALARM.NEW(1,-1,0.1,LOADALARM,2d._0.nextframe(16);scene.draw)",
  "SCENE.ALARM.ACTIVATE",
  "3D.LOAD(t.x)",
  "SCENE.DRAW",
  "CAMERA.SPEED.FLY=.002",
  "CAMERA.SPEED.TURN=.5",
  "CAMERA.PITCHSPEED=6",
  "CAMERA.HEIGHT=.1",
  "CAMERA.COLLISIONRADIUS=.015",
  "CAMERA.POS=0,0,0,0",
  "WALKMAP.LOAD(w.3wm)",
  "WALKMAP.ON",
  "2D.LOAD(lh.32d)",
  "2D._H1.VISIBLE=0",
  "SCENE.ALARM.LOADALARM.DELETE",
  "SCRIPT.EVENT(WorldLoaded:xOffset=1.00,fieldOfView=80)",
  "SCENE.ALARM.NEW(1,-1,1,USERLOCUPDATE,SCRIPT.EVENT(UserLocUpdate))",
];

function scene(opts: { walls?: [number, number, number, number][]; stopCommand?: string } = {}): TourScene {
  const loadSequence: SeqStep[] = LOAD.flatMap((line, i) => parseCommand(line).map((s) => ({ ...s, origin: `load.3sc:${i + 1}` }) as SeqStep));
  const stops = opts.stopCommand ? [{ id: 11, title: "Stop", command: opts.stopCommand, hotspots: [501] }] : [];
  return {
    format: "encarta-tour-scene", version: 1, tour: { id: "99999", contentId: 701599999, name: "Test" },
    files: { glb: "scene.glb", pvs: "pvs.bin" },
    model: { file: "t.x", meshes: [], materials: [], drawUnits: [], worldBounds: { min: [-10, -1, -10], max: [10, 1, 10] }, worldSize: 20 },
    objects: [], lights: null, clouds: [],
    walkmaps: { loadOrder: ["w.3wm"], start: "w.3wm", maps: [walkmap(opts.walls)] },
    layers: [loadingLayer, finalLayer],
    hud: { iconStrip: null }, cursors: { enabled: false, slots: [], files: {} },
    hotspots: [{ element: "_H1", event: "mouseUp", contentId: 501, contentIdText: "501", title: "A", icon: 0, anchor: [0, 0, 0], anchored: false, destMedia: 681500001, destGroup: "media" }],
    hotspotTargets: [{ id: 502, title: "B", destMedia: 681500002, destGroup: "flash", destKind: "interactive", destTitle: "B", stops: [11] }],
    stops, defaultStop: stops.length ? 11 : null,
    macros: [], alarms: [], events: [], loadSequence,
  } as unknown as TourScene;
}

function loaded(s = scene(), tickHz = 64): { c: TourControl; events: [number, string][] } {
  const c = new TourControl(s, { tickHz, rng: lcg(1) });
  const events: [number, string][] = [];
  c.events.onEvent = (e) => events.push([c.simTime, e]);
  c.do("SCENE.ALARM.ACTIVATE");
  c.do("script.load=load.3sc");
  for (let i = 0; i < 1000 && c.loading; i++) c.tick();
  c.tick();
  return { c, events };
}

const holding = (c: TourControl, key: string, seconds: number) => {
  c.keys.add(key);
  const n = Math.round(seconds * c.options.tickHz);
  for (let i = 0; i < n; i++) c.tick();
  c.keys.delete(key);
};

// ------------------------------------------------------------------ tick and speeds

describe("tick and speeds (VM: 100·SPEED.FLY units/s, 200·SPEED.TURN °/s)", () => {
  it("walks 100 × SPEED.FLY units per second at the idle (64) and walking (47) tick rates", () => {
    for (const hz of [64, 47]) {
      const { c } = loaded(scene(), hz);
      c.do("WALKMAP.OFF");
      const z0 = c.camera.z;
      holding(c, "ArrowUp", 3);
      // Yaw 0 walks along original +z (glTF −z): 0.2 units/s at SPEED.FLY .002.
      expect(z0 - c.camera.z).toBeCloseTo(0.6, 6);
      expect(c.camera.x).toBeCloseTo(0, 12);
    }
  });

  it("turns 200 × SPEED.TURN degrees per second, right increases yaw, without drifting", () => {
    const { c } = loaded();
    const p = [c.camera.x, c.camera.z];
    holding(c, "ArrowRight", 3);
    expect(c.camera.yawDeg).toBeCloseTo(300, 6);
    expect([c.camera.x, c.camera.z]).toEqual(p);
    holding(c, "ArrowLeft", 1);
    expect(c.camera.yawDeg).toBeCloseTo(200, 6);
  });

  it("the tick does not depend on the display frame rate", () => {
    for (const fps of [24, 30, 60, 75, 144, 240]) {
      const t = new FixedTicker(64);
      let ticks = 0;
      for (let i = 0; i < 3 * fps; i++) ticks += t.frame(1 / fps);
      expect(Math.abs(ticks - 192), `${fps} fps`).toBeLessThanOrEqual(1);
    }
    // Jittery frames still sum to real time.
    const t = new FixedTicker(64);
    const rng = lcg(5);
    let ticks = 0, time = 0;
    while (time < 3) {
      const dt = 0.004 + rng() * 0.03;
      time += dt;
      ticks += t.frame(dt);
    }
    expect(Math.abs(ticks - time * 64)).toBeLessThanOrEqual(1);
  });
});

// ------------------------------------------------------------------ pitch

describe("pitch keys (VM: exactly PITCHSPEED per tick, a step reaching |pitch| ≥ 45 is refused)", () => {
  const run = (from: number, up: boolean, ticks: number) => {
    const c = newCamera();
    c.pitchSpeed = 6;
    c.pitchDeg = from;
    const seen: number[] = [];
    for (let i = 0; i < ticks; i++) {
      stepPitch(c, { ...NO_INPUT, pitchUp: up, pitchDown: !up });
      if (seen.at(-1) !== c.pitchDeg) seen.push(c.pitchDeg);
    }
    return seen;
  };

  it("from −40 holding Page Up: −34, −28, …, 38, 44, then stops", () => {
    const seen = run(-40, true, 40);
    expect(seen[0]).toBe(-34);
    expect(seen.at(-1)).toBe(44);
    expect(seen.length).toBe(14);
    expect(seen.every((p, i) => i === 0 || Math.abs(p - (seen[i - 1] ?? 0) - 6) < 1e-9)).toBe(true);
  });

  it("from 44.998962 holding Page Down it stops at −39.001038", () => {
    expect(run(44.998962, false, 40).at(-1)).toBeCloseTo(-39.001038, 6);
  });

  it("is per tick, not time-scaled, and Home sets pitch 0", () => {
    for (const hz of [47, 64]) {
      const { c } = loaded(scene(), hz);
      c.do("CAMERA.PITCH=0");
      holding(c, "PageUp", 3 / hz); // three ticks
      expect(c.camera.pitchDeg).toBe(18);
      holding(c, "Home", 1 / hz);
      expect(c.camera.pitchDeg).toBe(0);
    }
  });

  it("positive pitch looks up", () => {
    const { c } = loaded();
    c.do("CAMERA.PITCH=30");
    expect(c.view().fwd[1]).toBeGreaterThan(0);
  });
});

// ------------------------------------------------------------------ collision

describe("walls (VM: the move is projected onto the wall, no bounce, a corner stops it)", () => {
  it("slides along a wall hit at yaw 93.4 with the tangential speed 0.2·sin 3.4°", () => {
    // A wall along z at x = 1 (glTF), walked into almost head-on.
    const { c } = loaded(scene({ walls: [[1, -5, 1, 5]] }));
    c.do("CAMERA.MOVETO=0.5,0,0,93.4");
    const xs: number[] = [], zs: number[] = [];
    c.keys.add("ArrowUp");
    for (let i = 0; i < 192; i++) {
      c.tick();
      xs.push(c.camera.x);
      zs.push(c.camera.z);
    }
    const R = 0.015;
    const settled = xs.findIndex((x) => Math.abs(x - (1 - R)) < 1e-8);
    expect(settled).toBeGreaterThan(0);
    // Pressed against the wall: x constant (no jitter), z moving at 0.2·|cos 93.4°| units/s.
    expect(new Set(xs.slice(settled).map((x) => x.toFixed(9))).size).toBe(1);
    const n = xs.length - 1 - settled;
    const rate = (((zs.at(-1) ?? 0) - (zs[settled] ?? 0)) / n) * 64;
    expect(rate).toBeCloseTo(0.2 * Math.sin((3.4 * Math.PI) / 180), 6);
  });

  it("stops completely in a corner", () => {
    const { c } = loaded(scene({ walls: [[1, -5, 1, 5], [-5, 0.04, 5, 0.04]] }));
    c.do("CAMERA.MOVETO=0.5,0,0,93.4");
    holding(c, "ArrowUp", 5);
    const p = [c.camera.x, c.camera.z];
    holding(c, "ArrowUp", 1);
    expect([c.camera.x, c.camera.z]).toEqual(p);
    expect(p[0]).toBeCloseTo(1 - 0.015, 7);
    expect(p[1]).toBeCloseTo(0.04 - 0.015, 7);
  });
});

// ------------------------------------------------------------------ events and replies

describe("events and command replies (VM)", () => {
  it("events arrive upper-cased; USERLOCUPDATE every 1.00 s; FinishedInitializing keeps its case", () => {
    const { c, events } = loaded();
    expect(events[0]?.[1]).toBe("WORLDLOADED:XOFFSET=1.00,FIELDOFVIEW=80");
    for (let i = 0; i < 64 * 5; i++) c.tick();
    const t = events.filter(([, e]) => e === "USERLOCUPDATE").map(([s]) => s);
    expect(t.length).toBe(5);
    for (let i = 1; i < t.length; i++) expect((t[i] ?? 0) - (t[i - 1] ?? 0)).toBeCloseTo(1.0, 9);
    c.do("SCRIPT.EVENT(HotSpotClicked:hotSpotID=701500001)");
    expect(events.at(-1)?.[1]).toBe("HOTSPOTCLICKED:HOTSPOTID=701500001");
  });

  it("replies: load and DISPLAYFPS say Failed but act, HALFREZDRIVING fails, ITSFILEURL 1 or −2", () => {
    const c = new TourControl(scene(), { rng: lcg(1) });
    expect(c.do("SCENE.HALFREZDRIVING=0")).toBe("Failed");
    expect(c.do("SCENE.DISPLAYFPS=1")).toBe("Failed");
    expect(c.displayFps).toBe(true);
    expect(c.do("script.itsfileurl=C:\\PROGRA~1\\ENC\\04257.eit")).toBe("1");
    expect(c.do("script.itsfileurl=C:\\Program Files\\ENC\\04257.eit")).toBe("-2");
    expect(c.do("script.load=load.3sc")).toBe("Failed");
    expect(c.loading).toBe(true);
    for (let i = 0; i < 1000 && c.loading; i++) c.tick();
    expect(c.loaded).toBe(true);
    expect(c.do("CAMERA.MOVETO=1,0,1,10;CAMERA.PITCH=5")).toBe("Success;Success");
  });

  it("a MOVETO to the current position leaves a valid pose (the original's NaN quirk is not reproduced)", () => {
    const { c } = loaded();
    c.do("CAMERA.MOVETO=0.5,0,0.5,20");
    c.do("CAMERA.MOVETO=0.5,0,0.5,20");
    const pos = c.do("CAMERA.POS?").split(",").map(Number);
    expect(pos.every(Number.isFinite)).toBe(true);
    expect(Number.isFinite(Number(c.do("CAMERA.PITCH?")))).toBe(true);
  });
});

// ------------------------------------------------------------------ captions

describe("captions (VM: 0xC0D1E3, white on hover; first shown by the host's stop command)", () => {
  it("normal C0D1E3, hovered and pressed white, back to C0D1E3, hand cursor over it", () => {
    const { c } = loaded();
    c.do("CAMERA.MOVETO=3,0,3,0");
    c.do("CAMERA.MOVETO=0,0,0,0"); // enter the hotspot cell: _H1 shows
    c.cursorsEnabled = true;
    c.cursorSlots[5] = { slot: 5, meaning: "hotspot", file: "vthot360.cur", image: "x.png", hotspot: [0, 0] };
    c.layoutLayer();
    const h = c.layer.find("_H1")!;
    expect(h.visible).toBe(true);
    expect([h.textColor, h.frameColor]).toEqual(["#C0D1E3", "#C0D1E3"]);
    c.pointer(20, 20, false);
    expect([h.textColor, h.frameColor]).toEqual(["#FFFFFF", "#FFFFFF"]);
    expect(c.cursorSlot()).toBe(5);
    c.pointer(20, 20, true);
    expect(h.textColor).toBe("#FFFFFF");
    c.pointer(300, 300, false);
    expect([h.textColor, h.frameColor]).toEqual(["#C0D1E3", "#C0D1E3"]);
  });

  it("none after loading; a plain MOVETO to the start pose keeps them hidden", () => {
    const { c } = loaded(scene({ stopCommand: "CAMERA.MOVETO=0,0,0,0;CAMERA.PITCH=10;" }));
    expect(c.layer.find("_H1")?.visible).toBe(false);
    playHostStart(c, null);
    c.tick();
    expect(c.camera.pitchDeg).toBe(10);
    expect(c.layer.find("_H1")?.visible).toBe(false);
  });

  it("a stop command that moves away and back (Colosseum's) shows them at once", () => {
    const { c } = loaded(scene({ stopCommand: "CAMERA.MOVETO=350.0,350.0,350.0,350.0;CAMERA.PITCH=10;WALKMAP.LOAD(w.3wm);CAMERA.MOVETO=0,0,0,0;" }));
    playHostStart(c, null);
    c.tick();
    expect(c.layer.find("_H1")?.visible).toBe(true);
  });

  it("the host's command waits for the end of loading", () => {
    const c = new TourControl(scene({ stopCommand: "CAMERA.PITCH=7;" }), { rng: lcg(1) });
    c.do("script.load=load.3sc");
    playHostStart(c, null);
    c.tick();
    expect(c.camera.pitchDeg).toBe(0);
    for (let i = 0; i < 1000 && c.loading; i++) c.tick();
    c.tick();
    expect(c.camera.pitchDeg).toBe(7);
  });
});

// ------------------------------------------------------------------ flicker and loading

describe("flicker and the loading screen (VM)", () => {
  it("flicker holds each brightness for its period: steps about 0.1 s apart at 64 ticks/s", () => {
    const f = new FlickerLight(0.1, [200, 150, 100]);
    const rng = lcg(9);
    const changes: number[] = [];
    let prev = f.current.join();
    for (let i = 1; i <= 640; i++) {
      f.advance(1 / 64, rng);
      if (f.current.join() !== prev) changes.push(i);
      prev = f.current.join();
    }
    const gaps = changes.slice(1).map((t, i) => t - (changes[i] ?? 0));
    // 7 ticks = 0.109 s (a repeated factor can hide a step, never shorten one).
    expect(gaps.every((g) => g % 7 === 0)).toBe(true);
    expect(gaps.filter((g) => g === 7).length).toBeGreaterThan(80);
  });

  it("the loading screen stays about 2 s, its spinner turning at 10 fps", () => {
    const c = new TourControl(scene(), { rng: lcg(1) });
    let frames = 0;
    const spinner = () => c.layer.find("_0");
    c.do("SCENE.ALARM.ACTIVATE");
    c.do("script.load=load.3sc");
    let last = spinner()?.uv[1];
    let ticks = 0;
    while (c.loading && ticks < 1000) {
      c.tick();
      ticks++;
      const v = spinner()?.uv[1];
      if (v !== undefined && v !== last) frames++;
      last = v;
    }
    expect(ticks / 64).toBeCloseTo(2, 1);
    expect(frames).toBeGreaterThanOrEqual(18);
    expect(frames).toBeLessThanOrEqual(20);
    // The 3D scene stays hidden while loading; the final layer replaced the loading layer.
    expect(c.layer.file).toBe("lh.32d");
  });
});

// ------------------------------------------------------------------ host

describe("host: hotspot destination and the pose kept for Back", () => {
  it("finds destmedia from the element links and the stop hotspot list", () => {
    const s = scene();
    expect(hotspotDestination(s, 501)).toBe(681500001);
    expect(hotspotDestination(s, 502)).toBe(681500002);
    expect(hotspotDestination(s, 999)).toBeNull();
  });

  it("round-trips a pose through the URL and restores it with captions", () => {
    const { c } = loaded();
    c.do("CAMERA.MOVETO=0.25,0,0.5,123.5;CAMERA.PITCH=-12");
    c.tick();
    const text = encodePose({ x: c.camera.x, y: c.camera.y, z: -c.camera.z, yaw: 123.5, pitch: -12, walkmap: c.walk.active?.name ?? null });
    const pose = decodePose(text);
    expect(pose).toMatchObject({ x: 0.25, z: 0.5, yaw: 123.5, pitch: -12 });
    expect(pose?.walkmap?.toLowerCase()).toBe("w.3wm");
    expect(decodePose("1,2,x")).toBeNull();

    const { c: back } = loaded();
    expect(back.layer.find("_H1")?.visible).toBe(false);
    back.do(poseCommand(back.scene, pose!));
    back.tick();
    expect(back.do("CAMERA.POS?")).toBe("0.250000,0.100000,0.500000,123.500000");
    expect(back.do("CAMERA.PITCH?")).toBe("-12.000000");
    expect(back.layer.find("_H1")?.visible).toBe(true);
  });
});

// ------------------------------------------------------------------ walk-map grid

describe("walk-map grid cells (VM: scan-line span rule, §6.14)", () => {
  it("a grid cell belongs to a polygon whose row span overlaps it, even left of the cell's corner", () => {
    // Two thin slanted strips sharing the edge (49,44)–(54,76); on row 66 it crosses x = 52.44.
    const right = Int32Array.from([49, 44, 54, 76, 58, 75, 53, 43]);
    const left = Int32Array.from([45, 45, 50, 77, 54, 76, 49, 44]);
    expect(coversGridCell(right, 52, 66)).toBe(true); // the VM puts column 52 on the right strip
    expect(coversGridCell(left, 52, 66)).toBe(true); // shared column: the first cell in file order wins
    expect(coversGridCell(right, 51, 66)).toBe(false);
    expect(coversGridCell(left, 44, 66)).toBe(false);
    expect(coversGridCell(right, 52, 80)).toBe(false);
  });
});
