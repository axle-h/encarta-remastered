/**
 * Game sounds and music: one-shot effects through <audio> (the converter's Opus/AAC/MP3 variants)
 * and looping MIDI music through the shared soundfont synth (a small oscillator synth when the
 * server has no soundfont).
 */
import type { MediaInfo } from "../api/types";
import { parseMidi, midiFrequency, type MidiSong } from "../media/midi";
import { prepareInstruments, SoundfontEngine } from "../media/synth";
import { playableAudio } from "./api";

let muted = false;
let musicOn = true;
const listeners = new Set<() => void>();

export function soundState() {
  return { muted, musicOn };
}

export function onSoundChange(f: () => void): () => void {
  listeners.add(f);
  return () => listeners.delete(f);
}

function changed() {
  for (const f of listeners) f();
}

export function setMuted(m: boolean) {
  muted = m;
  if (m) stopMusic();
  changed();
}

export function setMusic(on: boolean) {
  musicOn = on;
  if (!on) stopMusic();
  changed();
}

/** Plays an effect; resolves when it ends (or at once when muted or unplayable). */
export function playEffect(info: MediaInfo | null | undefined, volume = 0.8): Promise<void> {
  const src = playableAudio(info);
  if (muted || !src || typeof Audio === "undefined") return Promise.resolve();
  return new Promise((resolve) => {
    const a = new Audio(src);
    a.volume = volume;
    a.onended = () => resolve();
    a.onerror = () => resolve();
    a.play().catch(() => resolve());
  });
}

// --- MIDI music --------------------------------------------------------------------------------

let ctx: AudioContext | null = null;
let current: { url: string; stop: () => void } | null = null;
const songs = new Map<string, Promise<MidiSong | null>>();

function audioContext(): AudioContext | null {
  if (typeof window === "undefined") return null;
  const C = window.AudioContext ?? (window as unknown as { webkitAudioContext?: typeof AudioContext }).webkitAudioContext;
  if (!C) return null;
  ctx ??= new C();
  if (ctx.state === "suspended") void ctx.resume();
  return ctx;
}

function loadSong(url: string): Promise<MidiSong | null> {
  let p = songs.get(url);
  if (!p) {
    p = fetch(url)
      .then((r) => (r.ok ? r.arrayBuffer() : Promise.reject(new Error(String(r.status)))))
      .then((b) => parseMidi(b))
      .catch(() => null);
    songs.set(url, p);
  }
  return p;
}

/** A voice per General MIDI family: waveform and level. */
function voice(program: number, channel: number): { type: OscillatorType; gain: number } {
  if (channel === 9) return { type: "square", gain: 0 }; // drums: left out of this quiet synth
  const family = Math.floor(program / 8);
  if (family === 0 || family === 1) return { type: "triangle", gain: 0.5 }; // piano, chromatic percussion
  if (family === 2) return { type: "sine", gain: 0.45 }; // organ
  if (family === 4 || family === 5) return { type: "sawtooth", gain: 0.18 }; // bass, strings
  if (family >= 7 && family <= 9) return { type: "square", gain: 0.16 }; // brass, reed, pipe
  return { type: "triangle", gain: 0.4 };
}

/**
 * The soundfont synth's gain under `volume`: at the default 0.12 the music plays about 6 dB under
 * the media player's level, near the oscillator fallback's.
 */
const SOUNDFONT_MUSIC_GAIN = 4;

/** Loops a MIDI file until stopped or replaced. */
export function playMusic(url: string | undefined, volume = 0.12) {
  if (!url || muted || !musicOn) return;
  if (current?.url === url) return;
  stopMusic();
  const ac = audioContext();
  if (!ac) return;
  let stopped = false;
  const master = ac.createGain();
  master.gain.value = volume;
  master.connect(ac.destination);
  let timer: ReturnType<typeof setTimeout> | undefined;
  const nodes: OscillatorNode[] = [];
  const schedule = (song: MidiSong, at: number) => {
    for (const n of song.notes) {
      const v = voice(n.program, n.channel);
      if (v.gain === 0 || n.d <= 0) continue;
      const osc = ac.createOscillator();
      const g = ac.createGain();
      osc.type = v.type;
      osc.frequency.value = midiFrequency(n.note);
      const t0 = at + n.t;
      const t1 = t0 + Math.min(n.d, 4);
      const peak = v.gain * (n.velocity / 127);
      g.gain.setValueAtTime(0, t0);
      g.gain.linearRampToValueAtTime(peak, t0 + 0.01);
      g.gain.setTargetAtTime(peak * 0.6, t0 + 0.05, 0.2);
      g.gain.setTargetAtTime(0, t1, 0.05);
      osc.connect(g).connect(master);
      osc.start(t0);
      osc.stop(t1 + 0.3);
      nodes.push(osc);
      osc.onended = () => nodes.splice(nodes.indexOf(osc), 1);
    }
    const len = Math.max(song.duration, 1);
    timer = setTimeout(() => !stopped && schedule(song, at + len), Math.max(0, (at + len - ac.currentTime - 1) * 1000));
  };
  let engine: SoundfontEngine | null = null;
  void loadSong(url).then(async (song) => {
    if (!song || stopped) return;
    const instruments = await prepareInstruments(song).catch(() => null);
    if (stopped) return;
    if (instruments) engine = new SoundfontEngine(ac, master, song, instruments, { loop: true, volume: SOUNDFONT_MUSIC_GAIN });
    else schedule(song, ac.currentTime + 0.1);
  });
  current = {
    url,
    stop: () => {
      stopped = true;
      if (timer) clearTimeout(timer);
      engine?.stop();
      master.gain.setTargetAtTime(0, ac.currentTime, 0.05);
      for (const o of nodes.slice()) {
        try {
          o.stop(ac.currentTime + 0.2);
        } catch {
          /* already stopped */
        }
      }
      setTimeout(() => master.disconnect(), 400);
    },
  };
}

export function stopMusic() {
  current?.stop();
  current = null;
}
