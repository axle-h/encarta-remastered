import { useEffect, useRef, useState } from "react";
import { Icon } from "../components/Icons";
import { midiFrequency, parseMidi, type MidiNote, type MidiSong } from "./midi";

/**
 * Plays a MIDI file with a small WebAudio synthesizer (oscillator voices per General MIDI family,
 * noise for percussion). PLAN Step 2 intends a soundfont synth; this is the self-contained
 * stand-in so music items play without extra downloads.
 */
export function MidiPlayer({ src, captions }: { src: string; captions?: string }) {
  const [song, setSong] = useState<MidiSong | null>(null);
  const [error, setError] = useState("");
  const [playing, setPlaying] = useState(false);
  const [pos, setPos] = useState(0);
  const [cues, setCues] = useState<Cue[]>([]);
  const engine = useRef<Engine | null>(null);

  useEffect(() => {
    const ctl = new AbortController();
    fetch(src, { signal: ctl.signal })
      .then((r) => {
        if (!r.ok) throw new Error(`HTTP ${r.status}`);
        return r.arrayBuffer();
      })
      .then((b) => setSong(parseMidi(b)))
      .catch((e: unknown) => {
        if (!ctl.signal.aborted) setError(e instanceof Error ? e.message : String(e));
      });
    if (captions) {
      fetch(captions, { signal: ctl.signal })
        .then((r) => (r.ok ? r.text() : ""))
        .then((t) => setCues(parseVtt(t)))
        .catch(() => undefined);
    }
    return () => {
      ctl.abort();
      engine.current?.stop();
      engine.current = null;
    };
  }, [src, captions]);

  useEffect(() => {
    if (!playing) return;
    let raf = 0;
    const tick = () => {
      const e = engine.current;
      if (e) {
        const p = e.position();
        setPos(p);
        if (song && p >= song.duration + 0.3) {
          e.stop();
          engine.current = null;
          setPlaying(false);
          setPos(0);
          return;
        }
      }
      raf = requestAnimationFrame(tick);
    };
    raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
  }, [playing, song]);

  const toggle = () => {
    if (!song) return;
    if (playing) {
      engine.current?.stop();
      engine.current = null;
      setPlaying(false);
      return;
    }
    engine.current = new Engine(song, pos >= song.duration ? 0 : pos);
    setPlaying(true);
  };

  if (error) return <p className="enc-muted">This music could not be played ({error}).</p>;
  const cue = cues.find((c) => pos >= c.start && pos < c.end);
  return (
    <div className="enc-midi">
      <div className="enc-midi-bar">
        <button type="button" className="enc-midi-play" onClick={toggle} disabled={!song} aria-label={playing ? "Stop" : "Play"}>
          {playing ? <span className="enc-midi-stop" /> : <Icon name="play" size={14} />}
        </button>
        <input
          type="range"
          min={0}
          max={song?.duration ?? 1}
          step={0.1}
          value={pos}
          aria-label="Position"
          onChange={(e) => {
            const p = Number(e.target.value);
            setPos(p);
            if (playing && song) {
              engine.current?.stop();
              engine.current = new Engine(song, p);
            }
          }}
        />
        <span className="enc-midi-time">
          {fmt(pos)} / {song ? fmt(song.duration) : "–"}
        </span>
      </div>
      {cues.length > 0 && <div className="enc-audio-cue">{cue?.text ?? ""}</div>}
    </div>
  );
}

function fmt(s: number) {
  const m = Math.floor(s / 60);
  return `${m}:${String(Math.floor(s % 60)).padStart(2, "0")}`;
}

export interface Cue {
  start: number;
  end: number;
  text: string;
}

/** Minimal WebVTT cue reader (timings and text; tags stripped). */
export function parseVtt(text: string): Cue[] {
  const out: Cue[] = [];
  const time = (s: string) => {
    const p = s.trim().split(":").map(Number);
    return p.reduce((a, b) => a * 60 + b, 0);
  };
  for (const block of text.replace(/\r/g, "").split(/\n\n+/)) {
    const lines = block.split("\n");
    const i = lines.findIndex((l) => l.includes("-->"));
    if (i < 0) continue;
    const [a, b] = lines[i]!.split("-->");
    const start = time(a ?? "0");
    const end = time((b ?? "0").trim().split(/\s+/)[0] ?? "0");
    const body = lines
      .slice(i + 1)
      .join("\n")
      .replace(/<[^>]+>/g, "")
      .trim();
    if (body) out.push({ start, end, text: body });
  }
  return out;
}

/** GM program family → oscillator type and envelope. */
function voiceFor(program: number): { type: OscillatorType; attack: number; release: number; sustain: number; gain: number } {
  const fam = Math.floor(program / 8);
  switch (fam) {
    case 0: // piano
    case 1: // chromatic percussion
      return { type: "triangle", attack: 0.005, release: 0.3, sustain: 0.35, gain: 0.5 };
    case 2: // organ
      return { type: "square", attack: 0.01, release: 0.08, sustain: 0.9, gain: 0.18 };
    case 3: // guitar
      return { type: "triangle", attack: 0.004, release: 0.25, sustain: 0.3, gain: 0.45 };
    case 4: // bass
      return { type: "triangle", attack: 0.01, release: 0.12, sustain: 0.8, gain: 0.6 };
    case 5: // strings
    case 6: // ensemble
      return { type: "sawtooth", attack: 0.08, release: 0.3, sustain: 0.85, gain: 0.14 };
    case 7: // brass
    case 8: // reed
      return { type: "sawtooth", attack: 0.03, release: 0.12, sustain: 0.8, gain: 0.16 };
    case 9: // pipe
      return { type: "sine", attack: 0.04, release: 0.15, sustain: 0.85, gain: 0.45 };
    default:
      return { type: "square", attack: 0.02, release: 0.2, sustain: 0.6, gain: 0.15 };
  }
}

/** Schedules the song's notes slightly ahead of time on an AudioContext. */
class Engine {
  private ctx: AudioContext;
  private out: GainNode;
  private t0: number;
  private next = 0;
  private timer: number;
  private noise: AudioBuffer;
  private notes: MidiNote[];

  constructor(song: MidiSong, from: number) {
    this.ctx = new AudioContext();
    const comp = this.ctx.createDynamicsCompressor();
    this.out = this.ctx.createGain();
    this.out.gain.value = 0.5;
    this.out.connect(comp).connect(this.ctx.destination);
    this.t0 = this.ctx.currentTime + 0.1 - from;
    this.notes = song.notes;
    while (this.next < this.notes.length && this.notes[this.next]!.t < from) this.next++;
    this.noise = this.ctx.createBuffer(1, this.ctx.sampleRate * 0.5, this.ctx.sampleRate);
    const d = this.noise.getChannelData(0);
    for (let i = 0; i < d.length; i++) d[i] = Math.random() * 2 - 1;
    this.timer = window.setInterval(() => this.schedule(), 50);
    this.schedule();
  }

  position(): number {
    return Math.max(0, this.ctx.currentTime - this.t0);
  }

  private schedule() {
    const horizon = this.ctx.currentTime + 0.4;
    while (this.next < this.notes.length) {
      const n = this.notes[this.next]!;
      const at = this.t0 + n.t;
      if (at > horizon) break;
      if (at >= this.ctx.currentTime - 0.05) this.play(n, Math.max(at, this.ctx.currentTime));
      this.next++;
    }
  }

  private play(n: MidiNote, at: number) {
    const ctx = this.ctx;
    const vel = n.velocity / 127;
    const g = ctx.createGain();
    g.connect(this.out);
    if (n.channel === 9) {
      // Percussion: filtered noise burst, pitch region from the drum note.
      const src = ctx.createBufferSource();
      src.buffer = this.noise;
      const f = ctx.createBiquadFilter();
      const low = n.note <= 36;
      f.type = low ? "lowpass" : n.note >= 42 && n.note <= 46 ? "highpass" : "bandpass";
      f.frequency.value = low ? 150 : n.note >= 42 && n.note <= 46 ? 7000 : 1800;
      const len = low ? 0.25 : 0.12;
      g.gain.setValueAtTime(0.6 * vel, at);
      g.gain.exponentialRampToValueAtTime(0.001, at + len);
      src.connect(f).connect(g);
      src.start(at);
      src.stop(at + len + 0.02);
      return;
    }
    const v = voiceFor(n.program);
    const osc = ctx.createOscillator();
    osc.type = v.type;
    osc.frequency.value = midiFrequency(n.note);
    const peak = v.gain * vel;
    const end = at + n.d;
    g.gain.setValueAtTime(0, at);
    g.gain.linearRampToValueAtTime(peak, at + v.attack);
    g.gain.setTargetAtTime(peak * v.sustain, at + v.attack, 0.15);
    g.gain.setValueAtTime(peak * v.sustain, Math.max(at + v.attack, end));
    g.gain.linearRampToValueAtTime(0, Math.max(at + v.attack, end) + v.release);
    osc.connect(g);
    osc.start(at);
    osc.stop(Math.max(at + v.attack, end) + v.release + 0.02);
  }

  stop() {
    window.clearInterval(this.timer);
    void this.ctx.close();
  }
}
