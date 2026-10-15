/**
 * A General MIDI synthesizer over the soundfont of `soundfont.ts`, on WebAudio. Notes are scheduled
 * slightly ahead of time; each sounds one voice per matching region (a sample source with its root
 * pitch, loop, lowpass filter, volume and modulation envelopes, pan, and reverb and chorus sends).
 *
 * Channel controls follow the SoundFont 2 default modulators: volume (CC 7) and expression (CC 11)
 * as a squared gain, pan (CC 10) added to the region's pan, reverb (CC 91) and chorus (CC 93) each
 * adding up to 20 % send, the mod wheel (CC 1) adding up to 50 cents of vibrato, and the pitch wheel
 * (±2 semitones unless the song sets RPN 0). Note velocity is a squared gain. Not modelled: the
 * modulation LFO, "loop until release" (played as a continuous loop) and the soundfont's own
 * modulators. Levels and effect returns were set against FluidSynth renders (web-client.md).
 */
import { PERCUSSION_CHANNEL, type MidiControl, type MidiNote, type MidiSong } from "./midi";
import {
  ATTENUATION_FACTOR,
  absCents,
  centibels,
  loadIndex,
  loadPreset,
  loadSample,
  modulationEnvelope,
  presetKey,
  regionPitchCents,
  regionsFor,
  resolvePreset,
  sampleBuffer,
  timecents,
  volumeEnvelope,
  type SfPreset,
  type SfRegion,
} from "./soundfont";

/** Output level before the caller's volume: about 3 dB above FluidSynth at its default gain 0.2. */
export const SYNTH_GAIN = 0.3;
/** Effect return levels: FluidSynth's effects are stronger still (its chorus by about 2 dB). */
const REVERB_LEVEL = 1.4;
const CHORUS_LEVEL = 1.2;
/** How far ahead notes are scheduled, seconds. */
const LOOKAHEAD = 0.5;

/** The presets a song needs, keyed by the bank and program its notes ask for. */
export interface Instruments {
  presets: Map<string, SfPreset>;
  samples: number;
}

/** (bank, program) a note asks for; drum notes use bank 128 and the kit number. */
function noteKey(n: MidiNote): string {
  return presetKey(n.channel === PERCUSSION_CHANNEL ? 128 : n.bank, n.program);
}

/**
 * Loads the presets and every sample the song's notes can sound. Null when there is no soundfont
 * (the caller falls back to a simpler synth). `onProgress` gets (samples loaded, total).
 */
export async function prepareInstruments(song: MidiSong, onProgress?: (done: number, total: number) => void): Promise<Instruments | null> {
  const index = await loadIndex();
  if (!index) return null;
  const presets = new Map<string, SfPreset>();
  const wanted = new Map<string, { bank: number; program: number }>();
  for (const n of song.notes) wanted.set(noteKey(n), { bank: n.channel === PERCUSSION_CHANNEL ? 128 : n.bank, program: n.program });
  await Promise.all(
    [...wanted].map(async ([key, w]) => {
      const r = resolvePreset(index, w.bank, w.program);
      const p = r && (await loadPreset(r.bank, r.program));
      if (p) presets.set(key, p);
    }),
  );
  if (presets.size === 0) return null;
  const samples = new Map<number, SfPreset>();
  for (const n of song.notes) {
    const p = presets.get(noteKey(n));
    if (p) for (const r of regionsFor(p, n.note, n.velocity)) samples.set(r.sample, p);
  }
  let done = 0;
  onProgress?.(0, samples.size);
  await Promise.allSettled(
    [...samples].map(([id, p]) =>
      loadSample(id, p.samples[String(id)]!).finally(() => {
        done++;
        onProgress?.(done, samples.size);
      }),
    ),
  );
  return { presets, samples: samples.size };
}

interface ChannelState {
  volume: number;
  expression: number;
  pan: number;
  modulation: number;
  reverb: number;
  chorus: number;
  bend: number;
}

const initialChannel = (): ChannelState => ({ volume: 100, expression: 127, pan: 64, modulation: 0, reverb: 40, chorus: 0, bend: 0 });

interface Channel {
  state: ChannelState;
  dry: GainNode;
  wet: GainNode;
  chorus: GainNode;
  bend: ConstantSourceNode;
}

interface Voice {
  channel: number;
  exclusive: number;
  amp: GainNode;
  src: AudioBufferSourceNode;
  stopAt: number;
}

type Point = [time: number, value: number, kind: "set" | "lin" | "exp"];

/**
 * Schedules envelope points on `param` up to time `off` and returns the value there; there is
 * always an event at `off`, so the release ramp the caller adds starts there. Points must be in
 * time order.
 */
export function scheduleUntil(param: AudioParam | null, points: Point[], off: number): number {
  let prev = points[0]!;
  param?.setValueAtTime(prev[1], prev[0]);
  for (const p of points.slice(1)) {
    const [t, v, kind] = p;
    if (t <= off) {
      if (kind === "set") param?.setValueAtTime(v, t);
      else if (kind === "lin") param?.linearRampToValueAtTime(v, t);
      else param?.exponentialRampToValueAtTime(v, t);
      prev = p;
      continue;
    }
    const f = t > prev[0] ? (off - prev[0]) / (t - prev[0]) : 1;
    let at: number;
    if (kind === "set") at = prev[1];
    else if (kind === "lin") at = prev[1] + (v - prev[1]) * f;
    else at = prev[1] * (v / prev[1]) ** f;
    if (kind === "lin") param?.linearRampToValueAtTime(at, off);
    else if (kind === "exp") param?.exponentialRampToValueAtTime(at, off);
    else param?.setValueAtTime(at, off);
    return at;
  }
  // An event at `off` anchors the release ramp that follows (a ramp starts at the previous event).
  param?.setValueAtTime(prev[1], off);
  return prev[1];
}

/** Volume envelope points (linear attack, decay linear in dB) for a voice of peak gain `peak`. */
export function volumePoints(r: SfRegion, key: number, at: number, peak: number): Point[] {
  const e = volumeEnvelope(r, key);
  const tD = at + e.delay;
  const tA = tD + e.attack;
  const tH = tA + e.hold;
  const pts: Point[] = [
    [at, 0, "set"],
    [tD, 0, "set"],
    [tA, peak, e.attack > 0 ? "lin" : "set"],
    [tH, peak, "set"],
  ];
  // The decay time covers 96 dB; sustain is reached after its share of that.
  if (e.sustainCb > 0) pts.push([tH + (e.decay * Math.min(e.sustainCb, 960)) / 960, peak * e.sustain, "exp"]);
  return pts;
}

/** Modulation envelope points, 0–1, all stages linear. */
function modPoints(r: SfRegion, key: number, at: number): Point[] {
  const e = modulationEnvelope(r, key);
  const tD = at + e.delay;
  const tA = tD + e.attack;
  const tH = tA + e.hold;
  return [
    [at, 0, "set"],
    [tD, 0, "set"],
    [tA, 1, e.attack > 0 ? "lin" : "set"],
    [tH, 1, "set"],
    [tH + e.decay * (1 - e.sustain), e.sustain, "lin"],
  ];
}

/**
 * A stereo chorus like FluidSynth's default (three voices, 0.3 Hz sine, 8 ms depth): three
 * modulated delay lines spread left, centre and right. Returns its input and its LFOs.
 */
function chorusBus(ctx: BaseAudioContext, out: AudioNode, at: number): { input: GainNode; lfos: OscillatorNode[] } {
  const input = ctx.createGain();
  const level = ctx.createGain();
  level.gain.value = CHORUS_LEVEL;
  level.connect(out);
  const lfos: OscillatorNode[] = [];
  for (let i = 0; i < 3; i++) {
    const delay = ctx.createDelay(0.05);
    delay.delayTime.value = 0.012;
    const lfo = ctx.createOscillator();
    lfo.frequency.value = 0.3;
    const depth = ctx.createGain();
    depth.gain.value = 0.004;
    lfo.connect(depth).connect(delay.delayTime);
    lfo.start(at + i / (3 * 0.3)); // a third of a cycle apart
    lfos.push(lfo);
    const pan = ctx.createStereoPanner();
    pan.pan.value = i - 1;
    input.connect(delay).connect(pan).connect(level);
  }
  return { input, lfos };
}

/** A stereo impulse response: decaying noise, about 1.8 s, a little darker as it fades. */
function reverbImpulse(ctx: BaseAudioContext): AudioBuffer {
  const len = Math.floor(ctx.sampleRate * 1.8);
  const buf = ctx.createBuffer(2, len, ctx.sampleRate);
  for (let c = 0; c < 2; c++) {
    const d = buf.getChannelData(c);
    let lp = 0;
    for (let i = 0; i < len; i++) {
      const t = i / ctx.sampleRate;
      const k = 0.5 + 0.45 * Math.min(1, t / 1.2); // smoothing grows with time
      lp = lp * k + (Math.random() * 2 - 1) * (1 - k);
      d[i] = t < 0.012 ? 0 : lp * Math.exp(-3.4 * t) * 2.2;
    }
  }
  return buf;
}

export interface EngineOptions {
  /** Start position, seconds. */
  from?: number;
  /** Start again from the beginning at the end (game music). */
  loop?: boolean;
  /** Gain after `SYNTH_GAIN`. */
  volume?: number;
  /** Reverb and chorus on (default); off for dry comparisons with a reference synth. */
  effects?: boolean;
}

/** Plays a song on an AudioContext until stopped (or its end). */
export class SoundfontEngine {
  private readonly out: GainNode;
  private readonly reverb: ConvolverNode;
  private readonly lfos: OscillatorNode[];
  private readonly channels: Channel[] = [];
  private readonly voices = new Set<Voice>();
  private t0: number;
  private nextNote = 0;
  private nextControl = 0;
  private readonly timer: ReturnType<typeof setInterval>;
  private stopped = false;
  private readonly length: number;

  constructor(
    private readonly ctx: BaseAudioContext,
    destination: AudioNode,
    private readonly song: MidiSong,
    private readonly instruments: Instruments,
    private readonly opts: EngineOptions = {},
  ) {
    const from = opts.from ?? 0;
    this.length = Math.max(song.duration, 1);
    this.out = ctx.createGain();
    this.out.gain.value = SYNTH_GAIN * (opts.volume ?? 1);
    // A safety limiter: the disc's loudest songs peak near −4 dBFS at this gain.
    const limiter = ctx.createDynamicsCompressor();
    limiter.threshold.value = -2;
    limiter.knee.value = 0;
    limiter.ratio.value = 20;
    limiter.attack.value = 0.003;
    limiter.release.value = 0.1;
    this.out.connect(limiter).connect(destination);
    this.reverb = ctx.createConvolver();
    this.reverb.buffer = reverbImpulse(ctx);
    const reverbLevel = ctx.createGain();
    reverbLevel.gain.value = REVERB_LEVEL;
    this.reverb.connect(reverbLevel).connect(this.out);
    const now = ctx.currentTime;
    const chorus = chorusBus(ctx, this.out, now);
    this.lfos = chorus.lfos;
    for (let c = 0; c < 16; c++) {
      const dry = ctx.createGain();
      const wet = ctx.createGain();
      const cho = ctx.createGain();
      dry.connect(this.out);
      wet.connect(this.reverb);
      cho.connect(chorus.input);
      const bend = ctx.createConstantSource();
      bend.offset.value = 0;
      bend.start(now);
      this.channels.push({ state: initialChannel(), dry, wet, chorus: cho, bend });
    }
    // Controls before the start position set the channels' state; earlier notes are skipped.
    while (this.nextControl < song.controls.length && song.controls[this.nextControl]!.t < from) this.applyControl(song.controls[this.nextControl++]!, now);
    for (let c = 0; c < 16; c++) this.applyLevels(c, now, true);
    while (this.nextNote < song.notes.length && song.notes[this.nextNote]!.t < from) this.nextNote++;
    this.t0 = now + 0.1 - from;
    this.timer = setInterval(() => this.schedule(), 50);
    this.schedule();
  }

  /** Song position in seconds (wraps when looping). */
  position(): number {
    const p = Math.max(0, this.ctx.currentTime - this.t0);
    return this.opts.loop ? p % this.length : p;
  }

  /** Schedules everything up to the lookahead horizon. Public for offline rendering. */
  schedule(horizon = this.ctx.currentTime + LOOKAHEAD) {
    if (this.stopped) return;
    const { notes, controls } = this.song;
    for (;;) {
      const n = notes[this.nextNote];
      const c = controls[this.nextControl];
      if (!n && !c) {
        if (!this.opts.loop) return;
        this.t0 += this.length;
        this.nextNote = 0;
        this.nextControl = 0;
        // Each pass starts from the initial channel state, gains and pitch wheel included.
        const at = Math.max(this.t0, this.ctx.currentTime);
        this.channels.forEach((ch, c) => {
          ch.state = initialChannel();
          ch.bend.offset.setValueAtTime(0, at);
          this.applyLevels(c, at, true);
        });
        continue;
      }
      const control = c && (!n || c.t <= n.t);
      const at = this.t0 + (control ? c.t : n!.t);
      if (at > horizon) return;
      const when = Math.max(at, this.ctx.currentTime);
      if (control) {
        this.applyControl(c, when);
        this.nextControl++;
      } else {
        if (at >= this.ctx.currentTime - 0.05) this.noteOn(n!, when, at + n!.d);
        this.nextNote++;
      }
    }
  }

  private applyControl(c: MidiControl, at: number) {
    const ch = this.channels[c.channel];
    if (!ch) return;
    if (c.kind === "bend") {
      ch.state.bend = c.value;
      ch.bend.offset.setValueAtTime(c.value, at);
      return;
    }
    ch.state[c.kind] = c.value;
    if (c.kind === "volume" || c.kind === "expression") this.applyLevels(c.channel, at, false);
  }

  /** Channel gain: volume and expression, each a squared curve (the SF2 concave 960 cB). */
  private applyLevels(channel: number, at: number, immediate: boolean) {
    const ch = this.channels[channel]!;
    const g = (ch.state.volume / 127) ** 2 * (ch.state.expression / 127) ** 2;
    for (const node of [ch.dry, ch.wet, ch.chorus]) {
      if (immediate) node.gain.setValueAtTime(g, at);
      else node.gain.setTargetAtTime(g, at, 0.008);
    }
  }

  private noteOn(n: MidiNote, at: number, off: number) {
    const preset = this.instruments.presets.get(noteKey(n));
    if (!preset) return;
    const ch = this.channels[n.channel]!;
    for (const r of regionsFor(preset, n.note, n.velocity)) {
      const buf = sampleBuffer(r.sample);
      if (buf) this.voice(r, buf, n, ch, at, Math.max(off, at + 0.005));
    }
  }

  private voice(r: SfRegion, buf: AudioBuffer, n: MidiNote, ch: Channel, at: number, off: number) {
    const ctx = this.ctx;
    const rate = buf.sampleRate;
    if (r.exclusive) {
      for (const v of this.voices) {
        if (v.channel === n.channel && v.exclusive === r.exclusive && v.stopAt > at) {
          v.amp.gain.cancelScheduledValues(at);
          v.amp.gain.setTargetAtTime(0, at, 0.005);
          try {
            v.src.stop(at + 0.05);
          } catch {
            /* already stopped */
          }
          v.stopAt = at + 0.05;
        }
      }
    }
    const src = ctx.createBufferSource();
    src.buffer = buf;
    src.playbackRate.value = 2 ** (regionPitchCents(r, n.note) / 1200);
    if (r.loop && r.loopEnd !== undefined && r.loopStart !== undefined) {
      src.loop = true;
      src.loopStart = r.loopStart / rate;
      src.loopEnd = r.loopEnd / rate;
    }
    ch.bend.connect(src.detune);

    // Volume envelope and release.
    const peak = (n.velocity / 127) ** 2 * centibels((r.attenuation ?? 0) * ATTENUATION_FACTOR);
    const amp = ctx.createGain();
    const level = scheduleUntil(amp.gain, volumePoints(r, n.note, at, peak), off);
    const env = volumeEnvelope(r, n.note);
    let stopAt: number;
    if (level <= peak * 1e-5 || level <= 0) {
      amp.gain.setValueAtTime(0, off);
      stopAt = off + 0.01;
    } else {
      // Release covers 96 dB from full level; a quieter voice needs only its share of it.
      const below = Math.min(960, 200 * Math.log10(peak / level));
      stopAt = off + env.release * Math.max(0.02, (960 - below) / 960);
      amp.gain.exponentialRampToValueAtTime(level * 1e-4, stopAt);
      amp.gain.setValueAtTime(0, stopAt);
    }

    // Filter with the modulation envelope on its cutoff; the same envelope can bend pitch.
    let head: AudioNode = src;
    const mod = r.modEnv ? modPoints(r, n.note, at) : null;
    // A sample played below its pitch has nothing above its own Nyquist frequency times the rate;
    // the browser's linear interpolation puts images there, which this cutoff removes.
    const imageCut = 0.45 * rate * src.playbackRate.value;
    const cutoff = Math.min(r.filterFc !== undefined ? absCents(r.filterFc) : Infinity, imageCut);
    if (cutoff < ctx.sampleRate * 0.45 || (mod && r.modEnvToFilter)) {
      const f = ctx.createBiquadFilter();
      f.type = "lowpass";
      f.frequency.value = Math.min(cutoff, ctx.sampleRate / 2 - 100);
      f.Q.value = (r.filterQ ?? 0) / 10;
      if (mod && r.modEnvToFilter) this.modulate(f.detune, mod, r.modEnvToFilter, off, modulationEnvelope(r, n.note).release);
      head.connect(f);
      head = f;
    }
    if (mod && r.modEnvToPitch) this.modulate(src.detune, mod, r.modEnvToPitch, off, modulationEnvelope(r, n.note).release);

    // Vibrato: the region's own depth plus the mod wheel's (up to 50 cents).
    const depth = (r.vibrato?.[0] ?? 0) + (50 * ch.state.modulation) / 127;
    let lfo: OscillatorNode | null = null;
    if (depth !== 0) {
      lfo = ctx.createOscillator();
      lfo.frequency.value = absCents(r.vibrato?.[1] ?? 0);
      const g = ctx.createGain();
      g.gain.value = depth;
      lfo.connect(g).connect(src.detune);
      lfo.start(at + timecents(r.vibrato?.[2] ?? -12000));
      lfo.stop(stopAt);
    }

    const pan = ctx.createStereoPanner();
    pan.pan.value = Math.max(-1, Math.min(1, (r.pan ?? 0) / 500 + (ch.state.pan - 64) / 64));
    head.connect(amp).connect(pan).connect(ch.dry);
    const sends: GainNode[] = [];
    const send = (gen: number | undefined, cc: number, bus: GainNode) => {
      const level = ((gen ?? 0) + (200 * cc) / 127) / 1000;
      if (level <= 0 || this.opts.effects === false) return;
      const g = ctx.createGain();
      g.gain.value = Math.min(1, level);
      pan.connect(g).connect(bus);
      sends.push(g);
    };
    send(r.reverb, ch.state.reverb, ch.wet);
    send(r.chorus, ch.state.chorus, ch.chorus);

    const offset = (r.offset ?? 0) / rate;
    if (!src.loop && r.end !== undefined) src.start(at, offset, Math.max(0, (r.end - (r.offset ?? 0)) / rate));
    else src.start(at, offset);
    src.stop(stopAt);
    const v: Voice = { channel: n.channel, exclusive: r.exclusive ?? 0, amp, src, stopAt };
    this.voices.add(v);
    src.onended = () => {
      this.voices.delete(v);
      try {
        ch.bend.disconnect(src.detune);
      } catch {
        /* engine stopped */
      }
      pan.disconnect();
      for (const g of sends) g.disconnect();
    };
  }

  /** Drives `param` (cents) with an envelope scaled by `amount`, released linearly after `off`. */
  private modulate(param: AudioParam, pts: Point[], amount: number, off: number, release: number) {
    const scaled = pts.map(([t, v, k]) => [t, v * amount, k] as Point);
    const at = scheduleUntil(param, scaled, off);
    param.linearRampToValueAtTime(0, off + Math.max(0.005, release * Math.abs(at / amount)));
  }

  stop() {
    if (this.stopped) return;
    this.stopped = true;
    clearInterval(this.timer);
    const now = this.ctx.currentTime;
    this.out.gain.cancelScheduledValues(now);
    this.out.gain.setTargetAtTime(0, now, 0.03);
    for (const v of this.voices) {
      try {
        v.src.stop(now + 0.2);
      } catch {
        /* already stopped */
      }
    }
    for (const ch of this.channels) ch.bend.stop(now + 0.2);
    for (const lfo of this.lfos) lfo.stop(now + 0.2);
    setTimeout(() => this.out.disconnect(), 300);
  }
}
