import { afterEach, describe, expect, it, vi } from "vitest";
import { parseMidi } from "./midi";
import { absCents, centibels, regionPitchCents, regionsFor, resolvePreset, timecents, volumeEnvelope, type SfIndex, type SfPreset, type SfRegion } from "./soundfont";
import { gsRelease, panGains, scheduleUntil, SoundfontEngine, volumePoints } from "./synth";

/** A Standard MIDI File, format 0, 480 ppq at the default tempo (0.5 s per quarter); synthetic. */
function smf0(track: number[]): Uint8Array {
  const u32 = (n: number) => [(n >>> 24) & 255, (n >>> 16) & 255, (n >>> 8) & 255, n & 255];
  const ascii = (s: string) => [...s].map((c) => c.charCodeAt(0));
  const t = [...track, 0x00, 0xff, 0x2f, 0x00];
  return new Uint8Array([...ascii("MThd"), ...u32(6), 0, 0, 0, 1, 0x01, 0xe0, ...ascii("MTrk"), ...u32(t.length), ...t]);
}

describe("MIDI reader: controls", () => {
  it("reads controllers, pitch bend, bank select and the percussion bank", () => {
    const song = parseMidi(
      smf0([
        0x00, 0xb0, 0, 121, // bank select 121 (GM2 variation)
        0x00, 0xc0, 57, // trombone
        0x00, 0xb0, 7, 90, // volume
        0x00, 0xb0, 11, 64, // expression
        0x00, 0xe0, 0x00, 0x60, // pitch bend +4096 → +1 semitone at ±2
        0x00, 0x90, 60, 100,
        0x00, 0x99, 38, 90, // snare on channel 10
        0x83, 0x60, 0x80, 60, 0, // +480: off
        0x00, 0x89, 38, 0,
      ]),
    );
    expect(song.controls.map((c) => [c.kind, c.value])).toEqual([
      ["volume", 90],
      ["expression", 64],
      ["bend", 100],
    ]);
    const [tbn, snare] = [song.notes.find((n) => n.channel === 0)!, song.notes.find((n) => n.channel === 9)!];
    expect(tbn).toMatchObject({ program: 57, bank: 121 });
    expect(snare).toMatchObject({ bank: 128, program: 0 });
    expect(tbn.d).toBeCloseTo(0.5);
  });

  it("holds notes released under the sustain pedal until it lifts", () => {
    const song = parseMidi(
      smf0([
        0x00, 0xb0, 64, 127, // pedal down
        0x00, 0x90, 60, 100,
        0x83, 0x60, 0x80, 60, 0, // released at 0.5 s, held by the pedal
        0x87, 0x40, 0xb0, 64, 0, // pedal up at 1.5 s
      ]),
    );
    expect(song.notes).toHaveLength(1);
    expect(song.notes[0]!.d).toBeCloseTo(1.5);
  });

  it("treats any non-zero sustain pedal value as down, as the GS synth does", () => {
    const song = parseMidi(
      smf0([
        0x00, 0xb0, 64, 32, // pedal 32: down
        0x00, 0x90, 60, 100,
        0x83, 0x60, 0x80, 60, 0, // released at 0.5 s
        0x87, 0x40, 0xb0, 64, 0, // pedal up at 1.5 s
      ]),
    );
    expect(song.notes[0]!.d).toBeCloseTo(1.5);
  });

  it("follows RPN 0 pitch bend sensitivity", () => {
    const song = parseMidi(smf0([0x00, 0xb0, 101, 0, 0x00, 0xb0, 100, 0, 0x00, 0xb0, 6, 12, 0x00, 0xe0, 0x7f, 0x7f]));
    expect(song.controls[0]!.value).toBeCloseTo((8191 / 8192) * 1200);
  });
});

const region = (r: Partial<SfRegion>): SfRegion => ({ keys: [0, 127], vels: [0, 127], sample: 0, root: 60, volEnv: [-12000, -12000, -12000, -12000, 0, -12000], ...r });

describe("soundfont", () => {
  it("converts SF2 units", () => {
    expect(timecents(0)).toBe(1);
    expect(timecents(1200)).toBe(2);
    expect(timecents(-12000)).toBe(0);
    expect(absCents(6900)).toBeCloseTo(440, 0);
    expect(centibels(200)).toBeCloseTo(0.1);
  });

  it("picks presets as a GM synth does", () => {
    const index: SfIndex = { name: "t", copyright: null, presets: [{ bank: 0, program: 57, name: "Trombone" }, { bank: 128, program: 0, name: "Standard" }] };
    expect(resolvePreset(index, 121, 57)).toEqual({ bank: 0, program: 57 });
    expect(resolvePreset(index, 128, 6)).toEqual({ bank: 128, program: 0 });
    expect(resolvePreset(index, 0, 1)).toBeNull();
  });

  it("selects regions by key and velocity, and pitches them from the root key", () => {
    const preset: SfPreset = {
      bank: 0,
      program: 0,
      name: "p",
      samples: {},
      regions: [region({ keys: [0, 59], sample: 1 }), region({ keys: [60, 127], vels: [0, 63], sample: 2 }), region({ keys: [60, 127], vels: [64, 127], sample: 3 })],
    };
    expect(regionsFor(preset, 72, 100).map((r) => r.sample)).toEqual([3]);
    expect(regionsFor(preset, 40, 10).map((r) => r.sample)).toEqual([1]);
    expect(regionPitchCents(region({ root: 60, tune: -3 }), 72)).toBe(1197);
    expect(regionPitchCents(region({ root: 60, scale: 50 }), 72)).toBe(600);
  });

  it("scales hold and decay with the key", () => {
    const r = region({ volEnv: [-12000, -12000, 0, 0, 960, 0], volEnvKey: [0, 100] });
    expect(volumeEnvelope(r, 60).decay).toBeCloseTo(1);
    expect(volumeEnvelope(r, 48).decay).toBeCloseTo(2);
  });
});

describe("GS synth fits", () => {
  it("pans as a balance: the near side keeps its level, the far side falls as a square root", () => {
    expect(panGains(64)).toEqual([1, 1]);
    const db = (g: [number, number]) => 20 * Math.log10(g[1] / g[0]);
    // Measured R − L on the Microsoft GS synth: −2, −3.7, −6.7 dB at CC 48, 32, 16 (within 1 dB).
    expect(db(panGains(48))).toBeCloseTo(-1.25, 0);
    expect(db(panGains(32))).toBeCloseTo(-3, 0);
    expect(db(panGains(16))).toBeCloseTo(-6.0, 0);
    expect(panGains(96)[1]).toBe(1);
    expect(panGains(0)).toEqual([1, 0]);
    expect(panGains(127)).toEqual([0, 1]);
  });

  it("shortens releases by instrument family and leaves the rest", () => {
    expect(gsRelease(56, 0.4)).toBe(0.05); // trumpet
    expect(gsRelease(73, 0.02)).toBe(0.02); // flute with a release already shorter
    expect(gsRelease(48, 0.8)).toBeCloseTo(0.4); // strings
    expect(gsRelease(19, 1)).toBeCloseTo(0.2); // church organ
    expect(gsRelease(0, 1)).toBe(1); // piano
  });
});

/** Records automation calls like an AudioParam. */
function fakeParam() {
  const calls: [string, number, number][] = [];
  const p = {
    setValueAtTime: (v: number, t: number) => calls.push(["set", v, t]),
    linearRampToValueAtTime: (v: number, t: number) => calls.push(["lin", v, t]),
    exponentialRampToValueAtTime: (v: number, t: number) => calls.push(["exp", v, t]),
  };
  return { param: p as unknown as AudioParam, calls };
}

describe("synth envelopes", () => {
  // Attack 1 s, hold 0, decay 2 s to 480 cB (half of 96 dB, so reached after 1 s), release 1 s.
  const r = region({ volEnv: [-12000, 0, -12000, 1200, 480, 0] });

  it("ends a note during the attack at the level reached", () => {
    const { param, calls } = fakeParam();
    const level = scheduleUntil(param, volumePoints(r, 60, 0, 1), 0.25);
    expect(level).toBeCloseTo(0.25);
    expect(calls.at(-1)).toEqual(["lin", 0.25, 0.25]);
  });

  it("ends a note during the decay on the dB-linear curve", () => {
    const { param } = fakeParam();
    // Halfway through the decay to −48 dB: −24 dB.
    expect(scheduleUntil(param, volumePoints(r, 60, 0, 1), 1.5)).toBeCloseTo(10 ** (-24 / 20), 4);
  });

  it("anchors the release at note-off after the envelope has settled", () => {
    const { param, calls } = fakeParam();
    expect(scheduleUntil(param, volumePoints(r, 60, 0, 1), 5)).toBeCloseTo(10 ** (-48 / 20), 4);
    expect(calls.at(-1)).toEqual(["set", expect.closeTo(10 ** (-48 / 20), 4), 5]);
  });
});

/** A stand-in AudioContext whose nodes record every AudioParam call as [method, value, time]. */
function fakeContext() {
  const calls: [string, number, number][] = [];
  const param = () => ({
    value: 0,
    setValueAtTime: (v: number, t: number) => calls.push(["set", v, t]),
    setTargetAtTime: (v: number, t: number) => calls.push(["target", v, t]),
    linearRampToValueAtTime: (v: number, t: number) => calls.push(["lin", v, t]),
    exponentialRampToValueAtTime: (v: number, t: number) => calls.push(["exp", v, t]),
    cancelScheduledValues: () => {},
  });
  const node = (): unknown => {
    const params = new Map<string | symbol, unknown>();
    return new Proxy(
      {},
      {
        get: (_, k) => {
          if (k === "connect") return (n: unknown) => n;
          if (k === "disconnect" || k === "start" || k === "stop") return () => {};
          if (!params.has(k)) params.set(k, param());
          return params.get(k);
        },
        set: () => true,
      },
    );
  };
  const ctx = new Proxy(
    { currentTime: 0, sampleRate: 8000, createBuffer: (_c: number, n: number) => ({ getChannelData: () => new Float32Array(n) }) },
    { get: (t, k) => (k in t ? t[k as keyof typeof t] : node) },
  );
  return { ctx: ctx as unknown as BaseAudioContext, calls };
}

describe("synth engine", () => {
  afterEach(() => vi.useRealTimers());

  it("starts each pass of a looped song from the initial channel levels", () => {
    vi.useFakeTimers();
    // Volume 50 at 0.25 s; the 0.5 s song loops as 1 s passes, the first starting at 0.1 s.
    const song = parseMidi(smf0([0x00, 0x90, 60, 100, 0x81, 0x70, 0xb0, 7, 50, 0x81, 0x70, 0x80, 60, 0]));
    const { ctx, calls } = fakeContext();
    const engine = new SoundfontEngine(ctx, ctx.destination, song, { presets: new Map(), samples: 0 }, { loop: true });
    engine.schedule(1.2);
    const initial = (100 / 127) ** 2;
    expect(calls).toContainEqual(["target", (50 / 127) ** 2, expect.closeTo(0.35)]);
    expect(calls).toContainEqual(["set", initial, expect.closeTo(1.1)]);
    engine.stop();
  });

  it("drives the mod wheel's vibrato from one 6 Hz LFO per channel, so held notes follow the wheel", () => {
    vi.useFakeTimers();
    // Mod wheel to 127 at 0.25 s and back to 0 at 0.5 s.
    const song = parseMidi(smf0([0x00, 0x90, 60, 100, 0x81, 0x70, 0xb0, 1, 127, 0x81, 0x70, 0xb0, 1, 0, 0x00, 0x80, 60, 0]));
    const { ctx, calls } = fakeContext();
    const engine = new SoundfontEngine(ctx, ctx.destination, song, { presets: new Map(), samples: 0 });
    engine.schedule(1);
    expect(calls).toContainEqual(["set", 42, expect.closeTo(0.35)]);
    expect(calls).toContainEqual(["set", 0, expect.closeTo(0.6)]);
    engine.stop();
  });
});
