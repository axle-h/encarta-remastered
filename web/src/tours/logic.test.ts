/** Unit tests of the pure runtime logic on synthetic data (no disc content). */
import { describe, expect, it } from "vitest";
import { captionWidth, systemTextWidth } from "./hud/caption";
import { Layer2D } from "./hud/layer";
import { Interpreter, type Handlers } from "./script/interp";
import { atof, atoi, foldCase, parseCommand } from "./script/parse";
import { cursorZone, followGround, mouseVector, newCamera, stepMove, stepPitch, timeFactor, NO_INPUT } from "./sim/camera";
import { collide, slideMove } from "./sim/collision";
import { chunkVisible, cloudPhi, Filmstrip, flickerColour, flickerFactor, FlickerLight, lcg, rasteriseCloud, Ripple } from "./sim/effects";
import { WalkMap, WalkMapSet, type Segment } from "./sim/walkmap";
import { markerMoved, parseWorldLoaded, siteMapMarker, wedgeAngles } from "./sitemap";
import type { Cell, ElementDef, LayerFile, Step, StepOf, WalkMapFile } from "./types";

// ------------------------------------------------------------------ synthetic fixtures

const square = (x0: number, z0: number, x1: number, z1: number, y = 0) => [x0, y, z0, x1, y, z0, x1, y, z1, x0, y, z1];
const hotspot = (index: number, poly: number[], enter: string, exit = ""): Cell => ({
  index,
  kind: "hotspot",
  polygons: [poly],
  enter: { script: enter, steps: parseCommand(enter) },
  exit: { script: exit, steps: parseCommand(exit) },
});

function map(file: string, cells: Cell[], bbox: [number, number, number, number] = [-10, -10, 10, 10]): WalkMapFile {
  return { file, version: 3, bbox: { min: [bbox[0], -1, bbox[1]], max: [bbox[2], 1, bbox[3]] }, cells };
}

function element(name: string, rect: [number, number, number, number], extra: Partial<ElementDef> = {}): ElementDef {
  return {
    index: 0, name, class: "hotspot", kind: "hotspot", rect, uv: [0, 0, 1, 1], primitive: 6, additive: false, blend: 2,
    textured: true, capturesMouse: true, colorKey: false, isLabel: true, hasText: true, diffuse: "#FFFFFF",
    textColor: "#C0D1E3", frameColor: "#C0D1E3", labelFill: "#FF0000", icons: { normal: 0, pressed: 1, hover: 2 },
    font: { face: "Tahoma", height: -11, weight: 400, charset: 1 }, fileText: "ERS HotSpot", sourceImage: null, image: null,
    state: null, ...extra,
  };
}

// ------------------------------------------------------------------ script

describe("script parsing (§3.2)", () => {
  it("parses numbers like C", () => {
    expect(atof("  .5x")).toBe(0.5);
    expect(atof("-1.5e2,3")).toBe(-150);
    expect(atof("abc")).toBe(0);
    expect(atoi("12.9")).toBe(12);
    expect(atoi("x")).toBe(0);
  });

  it("folds case only before the first caret", () => {
    expect(foldCase("2d._h1.text=^Map of Rome^")).toBe("2D._H1.TEXT=^Map of Rome^");
  });

  it("splits at every ; and keeps the unclosed forms", () => {
    const steps = parseCommand("SCENE.ALARM.NEW(1,-1,0.1,TICK,2d._0.nextframe(16);scene.draw)");
    expect(steps.map((s) => s.op)).toEqual(["alarmNew", "draw"]);
    const a = steps[0] as StepOf<"alarmNew">;
    expect([a.name, a.interval, a.maxExecutions, a.macroText]).toEqual(["TICK", 0.1, -1, "2D._0.NEXTFRAME(16"]);
    expect(a.steps[0]).toMatchObject({ op: "elementNextFrame", id: "_0", frames: 16 });
  });

  it("3D flags look at the first character only (the glued HIDEALL statement)", () => {
    const [s] = parseCommand("3d.n_water.WATER=03d.Box.VISIBLE=02D._dot.VISIBLE=0");
    expect(s).toMatchObject({ op: "objectFlag", id: "N_WATER", prop: "water", value: false });
  });

  it("negates z of world points and ignores the anchor yaw", () => {
    const [s] = parseCommand("2D._L1.ANCHORPOINT=1,2,3,183.6");
    expect(s).toMatchObject({ op: "elementAnchor", point: [1, 2, -3], ignored: ",183.6" });
    const [p] = parseCommand("CAMERA.POS=1,2,3,90");
    expect(p).toMatchObject({ op: "camera", prop: "pos", values: [1, 2, -3, 90] });
  });

  it("events arrive upper-cased; ^text^ keeps its case", () => {
    const [e] = parseCommand("SCRIPT.EVENT(HotSpotClicked:hotSpotID=701504853)");
    expect(e).toMatchObject({ op: "event", text: "HOTSPOTCLICKED:HOTSPOTID=701504853", name: "HOTSPOTCLICKED", pairs: [["HOTSPOTID", "701504853"]] });
  });
});

describe("interpreter (§3.4–§3.6)", () => {
  function harness() {
    const applied: Step[] = [];
    const events: string[] = [];
    let resets = 0;
    const h: Handlers = {
      apply: (s) => (applied.push(s), "Success"),
      query: () => "42",
      fireEvent: (t) => events.push(t),
      reset: () => resets++,
    };
    const i = new Interpreter(h);
    return { i, applied, events, resets: () => resets };
  }

  it("defines, runs and deletes macros", () => {
    const { i, applied } = harness();
    i.do("BEGIN(SHOW);2D._L1.VISIBLE=1;2D._L2.VISIBLE=1;END(SHOW);");
    expect(i.do("RUN(SHOW)")).toBe("Success;Success");
    expect(applied.map((s) => s.op)).toEqual(["elementVisible", "elementVisible"]);
    expect(i.do("RUN(NOPE)")).toBe("Failed");
    i.do("DELETE(SHOW)");
    expect(i.do("RUN(SHOW)")).toBe("Failed");
  });

  it("ignores statements with no subsystem and joins results", () => {
    const { i } = harness();
    expect(i.do(")")).toBe("");
    expect(i.do("CAMERA.FOV=80;camera.pos?")).toBe("Success;42");
  });

  it("alarms fire at most once per poll, count executions and are deleted by DELETEALL", () => {
    const { i, events, resets } = harness();
    i.do("SCENE.ALARM.NEW(1,2,0.5,PING,SCENE.EVENT(PING))");
    i.pollAlarms(0); // starts the clock
    expect(events).toEqual([]);
    i.pollAlarms(0.4);
    expect(events).toEqual([]);
    i.do("SCENE.ALARM.ACTIVATE");
    i.pollAlarms(1);
    i.pollAlarms(3); // late: still only one firing
    expect(events).toEqual(["PING"]);
    i.pollAlarms(3.6);
    expect(events).toEqual(["PING", "PING"]);
    i.pollAlarms(10); // maxExecutions 2 reached
    expect(events.length).toBe(2);
    i.do("SCENE.DELETEALL");
    expect(i.alarms.length).toBe(0);
    expect(resets()).toBe(1);
  });

  it("alarm sequences substitute @", () => {
    const { i, events } = harness();
    i.do("SCENE.ALARM.ACTIVATE");
    i.do("SCENE.ALARM.NEW(1,-1,1,SEQ,SCENE.EVENT(@))");
    i.do("SCENE.ALARM.SEQ.ADDSEQUENCE=A");
    i.do("SCENE.ALARM.SEQ.ADDSEQUENCE=B");
    for (let t = 0; t <= 3; t++) i.pollAlarms(t);
    expect(events).toEqual(["A", "B", "@"]);
  });
});

// ------------------------------------------------------------------ walk maps

describe("walk-map lookup (§6.3)", () => {
  const ground = (index: number, poly: number[], y: number): Cell => ({ index, kind: "ground", polygons: [poly], plane: [0, 1, 0, -y] });
  const m = new WalkMap(
    map("t.3wm", [
      ground(0, square(-5, -5, 0, 5), 0.5),
      ground(1, square(-1, -5, 5, 5), 1.5), // overlaps cell 0 for x in −1…0: file order wins
      hotspot(2, square(2, 2, 3, 3), "2D._L1.VISIBLE=1;"),
    ]),
  );

  it("returns the first cell of a kind in file order", () => {
    expect(m.find("ground", -0.5, 0)?.index).toBe(0);
    expect(m.find("ground", 2, 0)?.index).toBe(1);
    expect(m.find("hotspot", 2.5, 2.5)?.index).toBe(2);
    expect(m.find("hotspot", 0, 0)).toBeNull();
    expect(m.find("ground", 50, 0)).toBeNull();
  });

  it("quantises on the 4096 grid with original z rows", () => {
    expect(m.cellOf(-10, 10)).toEqual([0, 0]);
    expect(m.cellOf(10, -10)).toEqual([4095, 4095]);
    expect(m.cellOf(11, 0)).toBeNull();
  });

  it("closes gaps of 1–3 grid units between cells of a kind (left cell wins)", () => {
    const u = 20 / 4095;
    const g = new WalkMap(map("g.3wm", [ground(0, square(-5, -5, 0, 5), 0), ground(1, square(0 + 3 * u, -5, 5, 5), 1)]));
    expect(g.find("ground", 0 + 1.5 * u, 0)?.index).toBe(0);
    const wide = new WalkMap(map("w.3wm", [ground(0, square(-5, -5, 0, 5), 0), ground(1, square(0 + 6 * u, -5, 5, 5), 1)]));
    expect(wide.find("ground", 0 + 3.5 * u, 0)).toBeNull();
  });

  it("gives the ground height from the cell plane", () => {
    expect(m.groundY(-3, 4, 0)).toBeCloseTo(0.5);
    expect(m.groundY(3, 0, 0)).toBeCloseTo(1.5);
  });

  it("runs exit then enter macros when the eye changes hotspot cell (§6.5)", () => {
    const set = new WalkMapSet((n) =>
      n === "a.3wm" ? map("a.3wm", [hotspot(0, square(0, 0, 1, 1), "A_IN", "A_OUT"), hotspot(1, square(1, 0, 2, 1), "B_IN", "B_OUT")]) : null,
    );
    set.load("a.3wm", { x: 0.5, z: 0.5 });
    set.on = 1;
    const ran: string[] = [];
    const run = (steps: Step[]) => ran.push(...steps.map((s) => s.src));
    set.hotspotCheck(0.5, 0.5, run); // initialised on load without running enter
    expect(ran).toEqual([]);
    set.hotspotCheck(1.5, 0.5, run);
    set.hotspotCheck(5, 5, run);
    expect(ran).toEqual(["A_OUT", "B_IN", "B_OUT"]);
  });
});

// ------------------------------------------------------------------ collision and camera

describe("collision (§6.7)", () => {
  const wall: Segment = { x1: 0, z1: -5, x2: 0, z2: 5, top: 1, climbable: false };
  const R = 0.1;

  it("stops at the inflated wall when moving straight into it", () => {
    const p = slideMove({ x: -1, z: 0 }, 2, 0, [wall], R);
    expect(p.x).toBeCloseTo(-R, 5);
    expect(p.z).toBeCloseTo(0, 5);
  });

  it("slides along the wall by the tangential component", () => {
    const p = slideMove({ x: -0.5, z: 0 }, 1, 1, [wall], R);
    expect(p.x).toBeCloseTo(-R, 5);
    expect(p.z).toBeCloseTo(1.0, 5); // 0.4 to the wall, then the 0.6 tangential remainder
  });

  it("stops in a corner (second sweep)", () => {
    const wall2: Segment = { x1: -5, z1: 1, x2: 5, z2: 1, top: 1, climbable: false };
    const p = slideMove({ x: -0.5, z: 0 }, 1, 2, [wall, wall2], R);
    expect(p.x).toBeCloseTo(-R, 5);
    expect(p.z).toBeCloseTo(1 - R, 5);
  });

  it("pushes out radially from wall endpoints", () => {
    const p = slideMove({ x: 0.5, z: 5.05 }, -0.5, 0, [wall], R);
    expect(Math.hypot(p.x - 0, p.z - 5)).toBeCloseTo(R, 5);
  });

  it("cancels a move that leaves the collision coverage when no wall is involved", () => {
    const coll: Cell = { index: 0, kind: "collision", level: 0, polygons: [square(-2, -2, 2, 2)], segments: [] };
    const m = new WalkMap(map("c.3wm", [coll]));
    expect(collide(m, { x: 0, z: 0 }, 0.5, 0, R)).toEqual({ x: 0.5, z: 0 });
    expect(collide(m, { x: 1.8, z: 0 }, 0.15, 0, R)).toEqual({ x: 1.8, z: 0 });
  });
});

describe("camera step and tick (§6.8–§6.9)", () => {
  it("time factor k = clamp(100·dt, 1, 5)", () => {
    expect(timeFactor(1 / 64)).toBeCloseTo(1.5625);
    expect(timeFactor(0.001)).toBe(1);
    expect(timeFactor(0.2)).toBe(5);
  });

  it("turns 2·k·SPEED.TURN degrees and moves k·SPEED.FLY along the yaw", () => {
    const c = newCamera();
    c.x = 0; c.z = 0; c.yawDeg = 0; c.speedFly = 0.01; c.speedTurn = 0.5;
    stepMove(c, { ...NO_INPUT, move: 1 }, 2, null);
    expect(c.z).toBeCloseTo(-0.02); // yaw 0 looks along original +z = glTF −z
    stepMove(c, { ...NO_INPUT, turn: 1 }, 2, null);
    expect(c.yawDeg).toBeCloseTo(2);
  });

  it("pitch is per tick and refused at ±45°", () => {
    const c = newCamera();
    c.pitchSpeed = 6;
    for (let i = 0; i < 20; i++) stepPitch(c, { ...NO_INPUT, pitchUp: true });
    expect(c.pitchDeg).toBe(42);
    stepPitch(c, { ...NO_INPUT, level: true });
    expect(c.pitchDeg).toBe(0);
  });

  it("snaps to the ground, or falls with gravity", () => {
    const set = new WalkMapSet(() => map("g.3wm", [{ index: 0, kind: "ground", polygons: [square(-5, -5, 5, 5)], plane: [0, 1, 0, 0] }]));
    set.load("g.3wm", { x: 0, z: 0 });
    set.on = 1;
    const c = newCamera();
    c.x = 0; c.z = 0; c.y = 1; c.height = 0.1;
    followGround(c, set, "snap", 0.01);
    expect(c.y).toBeCloseTo(0.1);
    c.y = 1;
    followGround(c, set, "gravity", 0.01);
    expect(c.y).toBeCloseTo(0.99);
    followGround(c, set, "gravity", 0.01);
    expect(c.y).toBeCloseTo(0.97);
    for (let i = 0; i < 50; i++) followGround(c, set, "gravity", 0.01);
    expect(c.y).toBeCloseTo(0.1);
    expect(c.vfall).toBe(0);
  });

  it("mouse vector has dead zones and a v·|v|^0.5 curve; cursor zones", () => {
    expect(mouseVector(320, 240, 640, 480)).toEqual({ turn: 0, move: 0 });
    const v = mouseVector(640, 0, 640, 480);
    expect(v.turn).toBeCloseTo(1);
    expect(v.move).toBeCloseTo(1);
    expect(cursorZone(330, 250, 640, 480)).toBe(0);
    expect(cursorZone(600, 250, 640, 480)).toBe(1);
    expect(cursorZone(330, 20, 640, 480)).toBe(2);
    expect(cursorZone(20, 250, 640, 480)).toBe(3);
    expect(cursorZone(330, 470, 640, 480)).toBe(4);
  });
});

// ------------------------------------------------------------------ lights and effects

describe("flicker (§7.2.6)", () => {
  it("draws k in [0.4, 1.0] and holds it for the period", () => {
    const rng = lcg(5);
    for (let i = 0; i < 1000; i++) {
      const k = flickerFactor(rng);
      expect(k).toBeGreaterThanOrEqual(0.4);
      expect(k).toBeLessThanOrEqual(1);
    }
    const l = new FlickerLight(0.1, [200, 100, 10]);
    expect(l.current).toEqual([200, 100, 10]);
    expect(l.advance(0.05, rng)).toBe(false);
    expect(l.advance(0.05, rng)).toBe(true);
    const held = [...l.current];
    expect(l.advance(0.05, rng)).toBe(false);
    expect(l.current).toEqual(held);
    expect(held[0]).toBeGreaterThanOrEqual(80);
    expect(held[0]).toBeLessThanOrEqual(200);
  });

  it("adds trunc(current·w) to the baked colour and clamps at 255", () => {
    expect(flickerColour(60, [[100, 0.5], [5, 0.5]])).toBe(112);
    expect(flickerColour(250, [[100, 1]])).toBe(255);
  });

  it("film strips advance one frame per wait and wrap", () => {
    const f = new Filmstrip(4, 0.1, () => 0);
    expect(f.offset).toBe(0);
    const seen: number[] = [];
    for (let i = 0; i < 6; i++) {
      f.advance(0.1);
      seen.push(f.offset);
    }
    expect(seen).toEqual([0, 0.25, 0.5, 0.75, 0, 0.25]);
  });

  it("water ripple spreads and damps a drop", () => {
    const r = new Ripple(16, 16);
    r.drop(8, 8, 1);
    expect(r.cur[8 * 16 + 8]).toBe(750);
    r.step();
    expect(r.cur[8 * 16 + 5]).not.toBe(0);
    for (let i = 0; i < 400; i++) r.step();
    expect(Math.max(...r.cur.map(Math.abs))).toBeLessThan(16); // integer damping leaves |h| < 16
    const src = new Uint8Array(16 * 16 * 4).fill(200);
    const dst = new Uint8Array(src.length);
    r.render(src, dst);
    expect(dst[0]).toBe(200);
  });

  it("cloud impostor: chunk culling and the reference rasteriser", () => {
    expect(chunkVisible(0, 90, 120)).toBe(true);
    expect(chunkVisible(0, 180, 200)).toBe(false);
    expect(chunkVisible(0, -1, 200)).toBe(true);
    expect(cloudPhi(1, 0, 0, 0, 0)).toBeCloseTo(0);
    expect(cloudPhi(0, 1, 0, 0, Math.PI)).toBeCloseTo((3 * Math.PI) / 2);
    const pts = [
      { x: 0, y: 0, z: 0, key: -1 },
      { x: 0, y: 0, z: 5, key: -1 }, // behind point 0 at phi = 0 (larger depth)
      { x: 3, y: 2, z: 0, key: -1 },
      { x: 0, y: 1, z: 0, key: -1 }, // last point: never drawn
    ];
    const img = rasteriseCloud(pts, 1, 0, 8, 4);
    expect(img[0 * 8 + 4]).toBe(0);
    expect(img[2 * 8 + 7]).toBe(2);
    expect(img[1 * 8 + 4]).toBe(-1);
  });
});

// ------------------------------------------------------------------ 2D layer

describe("captions and the 2D layer (§8.4–§8.5)", () => {
  it("reproduces the authoring tool's placeholder widths", () => {
    expect(systemTextWidth("ERS Label")).toBe(68);
    expect(systemTextWidth("ERS HotSpot")).toBe(83);
    expect(captionWidth("ERS Label", true)).toBe(89);
    expect(captionWidth("ERS HotSpot", false)).toBe(91);
  });

  it("applies the width rule with custom metrics", () => {
    expect(captionWidth("", false, { verticalOffset: 4, scale: 0.5, adder: 10 })).toBe(13);
  });

  const file: LayerFile = {
    file: "t.32d", version: 4, on: true, final: true,
    elements: [
      element("_H1", [10, 10, 101, 32], { state: { visible: true, text: "", anchor: [0, 0, 0], anchored: false, icon: 0, uv: [0, 0, 1, 1], sky: false, macros: { mouseUp: ["SCRIPT.EVENT(HIT)", parseCommand("SCRIPT.EVENT(HIT)")] } } }),
      element("_0", [0, 0, 128, 128], { class: "image", kind: "plain", isLabel: false, hasText: false, uv: [0, 0, 1, 1] }),
    ],
  };

  it("film strip frames advance by NEXTFRAME", () => {
    const l = new Layer2D([file]);
    l.load("T.32D");
    const img = l.find("_0");
    if (!img) throw new Error("missing");
    l.nextFrame(img, 16);
    expect(img.uv).toEqual([0, 0, 1, 1 / 16]);
    l.nextFrame(img, 16);
    expect(img.uv[1]).toBeCloseTo(1 / 16);
    for (let i = 0; i < 14; i++) l.nextFrame(img, 16);
    expect(img.uv[3]).toBe(1);
    l.nextFrame(img, 16);
    expect(img.uv[1]).toBe(0);
  });

  it("hit tests last to first and fires MOUSEUP only over the pressed element", () => {
    const l = new Layer2D([file]);
    l.load("t.32d");
    const h = l.find("_H1");
    if (!h) throw new Error("missing");
    l.layout({ project: () => null }, 640, 480);
    const ran: string[] = [];
    const run = (s: Step[]) => ran.push(...s.map((x) => x.op));
    // _0 (later in the array) covers _H1: it wins the hit test.
    expect(l.hitTest(20, 20)?.name).toBe("_0");
    l.find("_0")!.visible = false;
    l.layout({ project: () => null }, 640, 480);
    l.pointer(20, 20, false, run);
    expect(h.iconState).toBe("hover");
    l.pointer(20, 20, true, run);
    expect(h.iconState).toBe("pressed");
    l.pointer(20, 20, false, run);
    expect(ran).toEqual(["event"]);
    l.pointer(20, 20, true, run);
    l.pointer(300, 300, false, run); // released elsewhere: nothing
    expect(ran).toEqual(["event"]);
    expect(h.iconState).toBe("normal");
  });

  it("anchored captions follow the projection; out-of-cone and edge-straddling captions are not drawn", () => {
    const l = new Layer2D([file]);
    l.load("t.32d");
    const h = l.find("_H1")!;
    h.anchored = true;
    l.layout({ project: () => ({ x: 100, y: 50 }) }, 640, 480);
    expect(h.rect.slice(0, 2)).toEqual([100, 50]);
    expect(l.isDrawn(h, 640, 480)).toBe(true);
    l.layout({ project: () => ({ x: 600, y: 50 }) }, 640, 480);
    expect(l.isDrawn(h, 640, 480)).toBe(false);
    l.layout({ project: () => null }, 640, 480);
    expect(h.coneHidden).toBe(true);
    expect(l.hitTest(601, 51)).toBeNull();
    l.on = false;
    l.layout({ project: () => ({ x: 100, y: 50 }) }, 640, 480);
    expect(l.isDrawn(h, 640, 480)).toBe(false);
  });
});

describe("site map (§4.2)", () => {
  it("places the marker and heading, with rotated frames", () => {
    const cfg = parseWorldLoaded([["XOFFSET", "0"], ["ZOFFSET", "0"], ["XEXTENT", "10"], ["ZEXTENT", "20"], ["SM_RECTLEFT", "0"], ["SM_RECTTOP", "0"], ["SM_RECTRIGHT", "100"], ["SM_RECTBOTTOM", "200"]]);
    expect(siteMapMarker(cfg, 0, 0, 0)).toEqual({ px: 50, py: 100, heading: 90 });
    expect(siteMapMarker(cfg, 5, 10, 90)).toEqual({ px: 100, py: 0, heading: 0 });
    const rot = { ...cfg, rotateFrame: -90 };
    expect(siteMapMarker(rot, 0, 5, 0)).toEqual({ px: 25, py: 100, heading: 180 });
    expect(siteMapMarker({ ...cfg, xExtent: 0 }, 0, 0, 0)).toBeNull();
    expect(parseWorldLoaded([]).fieldOfViewDeg).toBe(120);
  });

  it("redraws only past 2° or 2 px, and opens the wedge around the heading", () => {
    const m = { px: 50, py: 50, heading: 90 };
    expect(markerMoved(null, m)).toBe(true);
    expect(markerMoved(m, { px: 52, py: 48, heading: 92 })).toBe(false);
    expect(markerMoved(m, { px: 53, py: 50, heading: 90 })).toBe(true);
    expect(markerMoved(m, { px: 50, py: 50, heading: 93 })).toBe(true);
    expect(markerMoved({ ...m, heading: 359 }, { ...m, heading: 1 })).toBe(false);
    // Yaw 0 looks up the map: heading 90, so the wedge is centred on canvas angle −90° (up).
    const [a0, a1] = wedgeAngles(90, 80);
    expect(((a0 + a1) / 2) * (180 / Math.PI)).toBeCloseTo(-90);
    expect((a1 - a0) * (180 / Math.PI)).toBeCloseTo(80);
  });
});
