/**
 * Integration checks against the user's converted tours (`out/tours/`), skipped when absent.
 * No disc-derived data is stored in the repository: these tests only read the local output.
 */
import { existsSync, readdirSync, readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";
import { TourControl } from "./control";
import { parseCommand } from "./script/parse";
import { lcg } from "./sim/effects";
import type { Step, TourScene } from "./types";

const TOURS = resolve(__dirname, "../../../out/tours");
const ids = existsSync(TOURS) ? readdirSync(TOURS).filter((d) => /^\d{5}$/.test(d)).sort() : [];
const load = (id: string) => JSON.parse(readFileSync(resolve(TOURS, id, "scene.json"), "utf8")) as TourScene;

/** Fields that only the converter can fill (name resolution against files and the final state). */
const IGNORED = new Set(["src", "target", "resolved", "image", "body", "origin"]);

function normalise(step: Step): unknown {
  const out: Record<string, unknown> = {};
  for (const [k, v] of Object.entries(step)) {
    if (IGNORED.has(k)) continue;
    if (k === "steps" && step.op === "defineMacro") continue;
    if (k === "steps" && Array.isArray(v)) out[k] = (v as Step[]).map(normalise);
    else if (k === "file" && typeof v === "string") out[k] = v.toUpperCase();
    else out[k] = v;
  }
  return out;
}

describe.skipIf(ids.length === 0)("converted tours", () => {
  for (const id of ids) {
    describe(id, () => {
      const scene = load(id);

      it("the TS parser converts every statement like the converter", () => {
        const checks: [string, Step[]][] = [];
        for (const s of scene.loadSequence) checks.push([s.src, [s]]);
        for (const m of scene.macros) checks.push([m.body, m.steps]);
        for (const a of scene.alarms) checks.push([a.macroText, a.steps]);
        for (const map of scene.walkmaps.maps) {
          for (const c of map.cells) {
            if (c.kind !== "hotspot") continue;
            checks.push([c.enter.script, c.enter.steps], [c.exit.script, c.exit.steps]);
          }
        }
        let n = 0;
        for (const [src, expected] of checks) {
          const got = parseCommand(src);
          expect(got.map(normalise), src).toEqual(expected.map(normalise));
          n += expected.length;
        }
        expect(n).toBeGreaterThan(100);
      });

      it("load.3sc leaves the converter's final object and element state", () => {
        const events: string[] = [];
        const c = new TourControl(scene, { rng: lcg(7) });
        c.events.onEvent = (e) => events.push(e);
        c.startLoad();
        for (let i = 0; i < 2000 && c.loading; i++) c.tick();
        expect(c.loading).toBe(false);
        expect(events[0]?.startsWith("WORLDLOADED:")).toBe(true);
        expect(events[0]).toBe(scene.events.find((e) => e.name === "WORLDLOADED")?.text);
        for (const info of scene.objects) {
          const o = c.objectByNode(info.node);
          expect(o, info.node).not.toBeNull();
          if (!o) continue;
          const st = info.state;
          expect([o.visible, o.sprite, o.noIllumination, o.water], info.node).toEqual([st.visible, st.sprite, st.noIllumination, st.water]);
          expect(o.cloud ? [o.cloud.file.toUpperCase(), o.cloud.scale] : null, info.node).toEqual(st.cloud ? [st.cloud[0].toUpperCase(), st.cloud[1]] : null);
          expect(!!o.filmstrip, info.node).toBe(!!st.filmstrip);
          if (st.appliedTexture && info.textureRecord !== null) expect(c.textureRecords.get(info.textureRecord)).toBe(st.appliedTexture[1]);
        }
        const final = scene.layers.find((l) => l.final);
        expect(c.layer.file).toBe(final?.file);
        for (const def of final?.elements ?? []) {
          const el = c.layer.find(def.name);
          const st = def.state;
          if (!el || !st) continue;
          expect([el.visible, el.text, el.anchored, el.icon], def.name).toEqual([st.visible, st.text, st.anchored, st.icon]);
          expect(el.anchor.map((v) => +v.toFixed(5)), def.name).toEqual(st.anchor.map((v) => +v.toFixed(5)));
          if (!def.isLabel) expect(el.uv).toEqual(st.uv);
          for (const [ev, [script]] of Object.entries(st.macros)) expect(el.macroText[ev as keyof typeof el.macroText]).toBe(script);
        }
        // The camera ends at the start pose of scene.json.
        expect(c.camera.x).toBe(scene.camera.start.position[0]);
        expect(c.camera.z).toBe(scene.camera.start.position[2]);
        expect(c.camera.yawDeg).toBe(scene.camera.start.yawDeg);
        expect(c.walk.active?.name.toLowerCase()).toBe(scene.walkmaps.start.toLowerCase());
        expect(c.interp.alarms.map((a) => a.name)).toEqual(scene.alarms.filter((a) => a.live).map((a) => a.name));
        // Labels stay hidden until the first move into a different hotspot cell (§8.6).
        expect(c.layer.elements.filter((e) => e.visible && e.name !== "_dot").length).toBe(0);
      });

      it("walking forward from the start stays on walkable ground and follows it", () => {
        const c = new TourControl(scene, { rng: lcg(3) });
        c.startLoad();
        for (let i = 0; i < 2000 && c.loading; i++) c.tick();
        c.tick();
        const y0 = c.camera.y;
        const start = { x: c.camera.x, z: c.camera.z };
        c.keys.add("ArrowUp");
        for (let i = 0; i < 200; i++) c.tick();
        c.keys.clear();
        const moved = Math.hypot(c.camera.x - start.x, c.camera.z - start.z);
        expect(moved).toBeGreaterThan(0);
        // At most k·speed per tick (k = 1.5625 at 64 Hz).
        expect(moved).toBeLessThanOrEqual(200 * 1.5625 * scene.camera.speedFly + 1e-9);
        const ground = c.walk.active?.groundY(c.camera.x, c.camera.y, c.camera.z);
        if (ground !== null && ground !== undefined) expect(c.camera.y).toBeCloseTo(ground + c.camera.height, 3);
        expect(Number.isFinite(y0)).toBe(true);
      });
    });
  }
});

describe.skipIf(!ids.includes("05916"))("Beaumaris stair strips (§6.4–§6.5)", () => {
  it("crossing the overlapping strips switches level2 → level1 and back without oscillating", () => {
    const scene = load("05916");
    const c = new TourControl(scene, { rng: lcg(1) });
    c.startLoad();
    for (let i = 0; i < 2000 && c.loading; i++) c.tick();
    const x = -16.95;
    c.do("WALKMAP.LOAD=level2.3wm");
    const path = (z0: number, z1: number) => {
      const maps: string[] = [];
      const n = 200;
      for (let i = 0; i <= n; i++) {
        c.camera.x = x;
        c.camera.z = z0 + ((z1 - z0) * i) / n;
        c.walk.hotspotCheck(c.camera.x, c.camera.z, (s) => c.interp.run(s));
        const name = c.walk.active?.name ?? "";
        if (maps.at(-1) !== name) maps.push(name);
      }
      // A few more checks standing still at the end.
      for (let i = 0; i < 5; i++) c.walk.hotspotCheck(c.camera.x, c.camera.z, (s) => c.interp.run(s));
      return { maps, end: c.walk.active?.name };
    };
    const down = path(-10.5, -9.95);
    expect(down.end?.toLowerCase()).toBe("level1.3wm");
    const up = path(-9.95, -10.5);
    expect(up.end?.toLowerCase()).toBe("level2.3wm");
    // Flapping is limited to the middle strip (a few switches at most per crossing).
    expect(down.maps.length).toBeLessThanOrEqual(6);
    expect(up.maps.length).toBeLessThanOrEqual(6);
  });
});
