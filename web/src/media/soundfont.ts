/**
 * The General MIDI soundfont written by `encarta-convert soundfont` (docs/media.md §8), served under
 * `/out/soundfont/`: `index.json`, one `presets/{bank}-{program}.json` per preset and one FLAC per
 * sample. Presets and samples load on demand and stay cached for the session.
 */

export const SOUNDFONT_BASE = "/out/soundfont";

/** One flattened preset × instrument zone. Times in timecents, levels in centibels (SF2 units). */
export interface SfRegion {
  keys: [number, number];
  vels: [number, number];
  sample: number;
  root: number;
  /** Coarse + fine tune + the sample's pitch correction, cents. */
  tune?: number;
  /** Cents per key (default 100). */
  scale?: number;
  /** First frame played (default 0) and the frame playback ends at (default: the sample's end). */
  offset?: number;
  end?: number;
  /** 1: loop continuously; 3: loop until release (played as 1). */
  loop?: number;
  loopStart?: number;
  loopEnd?: number;
  attenuation?: number;
  /** −500 (left) … 500 (right). */
  pan?: number;
  /** Effects sends, 0.1 % units. */
  reverb?: number;
  chorus?: number;
  exclusive?: number;
  /** Lowpass cutoff (absolute cents) and resonance (cB). */
  filterFc?: number;
  filterQ?: number;
  /** delay, attack, hold, decay (timecents), sustain (cB of attenuation), release (timecents). */
  volEnv: number[];
  /** Timecents per key below 60 added to hold and decay. */
  volEnvKey?: [number, number];
  /** Like volEnv; sustain in 0.1 % below full. */
  modEnv?: number[];
  modEnvKey?: [number, number];
  modEnvToFilter?: number;
  modEnvToPitch?: number;
  /** Vibrato LFO: depth (cents), frequency (absolute cents), delay (timecents). */
  vibrato?: [number, number, number];
}

export interface SfSampleInfo {
  rate: number;
  length: number;
}

export interface SfPreset {
  bank: number;
  program: number;
  name: string;
  regions: SfRegion[];
  samples: Record<string, SfSampleInfo>;
}

export interface SfIndex {
  name: string | null;
  copyright: string | null;
  presets: { bank: number; program: number; name: string }[];
}

let indexPromise: Promise<SfIndex | null> | null = null;
const presetCache = new Map<string, Promise<SfPreset | null>>();
const sampleCache = new Map<number, Promise<AudioBuffer>>();
const decoded = new Map<number, AudioBuffer>();

async function fetchJson<T>(url: string): Promise<T> {
  const r = await fetch(url);
  if (!r.ok) throw new Error(`HTTP ${r.status} for ${url}`);
  return (await r.json()) as T;
}

/** The soundfont index, or null when the server has none (the synth then falls back). */
export function loadIndex(): Promise<SfIndex | null> {
  indexPromise ??= fetchJson<SfIndex>(`${SOUNDFONT_BASE}/index.json`).catch(() => {
    indexPromise = null; // retry on the next song
    return null;
  });
  return indexPromise;
}

/**
 * The preset a channel's bank and program select, as a General MIDI synth would: an unknown bank
 * falls back to bank 0 (GM2/GS variation banks), an unknown drum kit to the Standard kit (128:0).
 */
export function resolvePreset(index: SfIndex, bank: number, program: number): { bank: number; program: number } | null {
  const has = (b: number, p: number) => index.presets.some((x) => x.bank === b && x.program === p);
  if (bank === 128) return has(128, program) ? { bank, program } : has(128, 0) ? { bank: 128, program: 0 } : null;
  if (has(bank, program)) return { bank, program };
  return has(0, program) ? { bank: 0, program } : null;
}

export function presetKey(bank: number, program: number) {
  return `${bank}-${program}`;
}

export function loadPreset(bank: number, program: number): Promise<SfPreset | null> {
  const key = presetKey(bank, program);
  let p = presetCache.get(key);
  if (!p) {
    p = fetchJson<SfPreset>(`${SOUNDFONT_BASE}/presets/${key}.json`).catch(() => null);
    presetCache.set(key, p);
  }
  return p;
}

/** The regions that sound for a key and velocity (all of them: layers and stereo pairs). */
export function regionsFor(preset: SfPreset, key: number, velocity: number): SfRegion[] {
  return preset.regions.filter((r) => key >= r.keys[0] && key <= r.keys[1] && velocity >= r.vels[0] && velocity <= r.vels[1]);
}

/** Decodes a sample at its own rate (an offline context of that rate does not resample). */
export function loadSample(id: number, info: SfSampleInfo): Promise<AudioBuffer> {
  let p = sampleCache.get(id);
  if (!p) {
    p = fetch(`${SOUNDFONT_BASE}/samples/${id}.flac`)
      .then((r) => (r.ok ? r.arrayBuffer() : Promise.reject(new Error(`HTTP ${r.status} for sample ${id}`))))
      .then((b) => new OfflineAudioContext(1, Math.max(1, info.length), info.rate).decodeAudioData(b))
      .then((buf) => {
        decoded.set(id, buf);
        return buf;
      });
    p.catch(() => sampleCache.delete(id));
    sampleCache.set(id, p);
  }
  return p;
}

/** A sample already decoded by `loadSample`. */
export function sampleBuffer(id: number): AudioBuffer | undefined {
  return decoded.get(id);
}

// --- SF2 unit conversions (SoundFont 2.04 §8.1.3) -------------------------------------------------

/** Timecents → seconds; the −12000 floor means "instant". */
export function timecents(tc: number): number {
  return tc <= -12000 ? 0 : 2 ** (tc / 1200);
}

/** Absolute cents → Hz (8.176 Hz is cent 0). */
export function absCents(c: number): number {
  return 8.176 * 2 ** (c / 1200);
}

/** Centibels of attenuation → linear gain. */
export function centibels(cb: number): number {
  return 10 ** (-cb / 200);
}

/**
 * Initial attenuation is scaled by 0.4 as E-mu hardware and FluidSynth do: soundfonts such as
 * Fluid R3 were voiced on that hardware, and the specification's full value makes them too quiet.
 */
export const ATTENUATION_FACTOR = 0.4;

/** Playback rate of a region for a key, before pitch bend and modulation. */
export function regionPitchCents(r: SfRegion, key: number): number {
  return (key - r.root) * (r.scale ?? 100) + (r.tune ?? 0);
}

/**
 * The volume envelope's stages for a key, in seconds; `sustain` is a linear gain (0–1).
 * Hold and decay scale with the key (keynumToVolEnvHold/Decay, relative to key 60).
 */
export function volumeEnvelope(r: SfRegion, key: number) {
  const [delay = -12000, attack = -12000, hold = -12000, decay = -12000, sustain = 0, release = -12000] = r.volEnv;
  const [holdKey, decayKey] = r.volEnvKey ?? [0, 0];
  return {
    delay: timecents(delay),
    attack: timecents(attack),
    hold: timecents(hold + holdKey * (60 - key)),
    decay: timecents(decay + decayKey * (60 - key)),
    /** Attenuation in cB at sustain (≥ 0), and the matching gain. */
    sustainCb: Math.max(0, Math.min(1440, sustain)),
    sustain: centibels(Math.max(0, Math.min(1440, sustain))),
    release: Math.max(0.006, timecents(release)),
  };
}

/** The modulation envelope's stages; `sustain` is a level 0–1. */
export function modulationEnvelope(r: SfRegion, key: number) {
  const [delay = -12000, attack = -12000, hold = -12000, decay = -12000, sustain = 0, release = -12000] = r.modEnv ?? [];
  const [holdKey, decayKey] = r.modEnvKey ?? [0, 0];
  return {
    delay: timecents(delay),
    attack: timecents(attack),
    hold: timecents(hold + holdKey * (60 - key)),
    decay: timecents(decay + decayKey * (60 - key)),
    sustain: 1 - Math.max(0, Math.min(1000, sustain)) / 1000,
    release: timecents(release),
  };
}
