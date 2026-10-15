/**
 * Standard MIDI File reader (public SMF 1.0 format): formats 0 and 1, PPQ or SMPTE time division,
 * running status, tempo changes. Produces a flat, time-sorted list of notes in seconds, plus the
 * channel controls the soundfont synth (`synth.ts`) automates. The sustain pedal is applied here:
 * a note released while the pedal is down ends when the pedal lifts.
 */

export interface MidiNote {
  /** Start time in seconds. */
  t: number;
  /** Duration in seconds. */
  d: number;
  note: number;
  velocity: number;
  channel: number;
  /** General MIDI program (0–127) in force on the channel at note-on. */
  program: number;
  /** Bank select (CC 0) in force at the last program change; 128 on the percussion channel 10. */
  bank: number;
}

/** Controllers the synth follows; `bend` is the pitch wheel already scaled to cents. */
export type MidiControlKind = "volume" | "expression" | "pan" | "modulation" | "reverb" | "chorus" | "bend";

export interface MidiControl {
  t: number;
  channel: number;
  kind: MidiControlKind;
  /** 0–127 for controllers; cents for `bend`. */
  value: number;
}

export interface MidiSong {
  format: number;
  tracks: number;
  duration: number;
  notes: MidiNote[];
  /** Time-sorted controller changes. */
  controls: MidiControl[];
}

const CONTROLLERS: Record<number, MidiControlKind> = { 1: "modulation", 7: "volume", 10: "pan", 11: "expression", 91: "reverb", 93: "chorus" };
export const PERCUSSION_CHANNEL = 9;

class Reader {
  pos = 0;
  constructor(private readonly b: Uint8Array) {}
  get eof() {
    return this.pos >= this.b.length;
  }
  u8() {
    const v = this.b[this.pos];
    if (v === undefined) throw new Error("MIDI: unexpected end of data");
    this.pos++;
    return v;
  }
  u16() {
    return (this.u8() << 8) | this.u8();
  }
  u32() {
    return ((this.u8() << 24) >>> 0) + (this.u8() << 16) + (this.u8() << 8) + this.u8();
  }
  u32le() {
    return this.u8() + (this.u8() << 8) + (this.u8() << 16) + ((this.u8() << 24) >>> 0);
  }
  str(n: number) {
    let s = "";
    for (let i = 0; i < n; i++) s += String.fromCharCode(this.u8());
    return s;
  }
  vlq() {
    let v = 0;
    for (let i = 0; i < 4; i++) {
      const c = this.u8();
      v = (v << 7) | (c & 0x7f);
      if (!(c & 0x80)) return v;
    }
    return v;
  }
  skip(n: number) {
    this.pos += n;
  }
}

interface RawEvent {
  tick: number;
  order: number;
  kind: "on" | "off" | "tempo" | "program" | "cc" | "bend";
  channel: number;
  a: number;
  b: number;
}

export function parseMidi(data: ArrayBuffer | Uint8Array): MidiSong {
  const bytes = data instanceof Uint8Array ? data : new Uint8Array(data);
  const r = new Reader(bytes);
  // RIFF-wrapped MIDI (RMID) puts the SMF inside a "data" chunk.
  if (r.str(4) === "RIFF") {
    r.skip(8);
    while (!r.eof) {
      const id = r.str(4);
      const len = r.u32le();
      if (id === "data") return parseMidi(bytes.subarray(r.pos, r.pos + len));
      r.skip(len + (len & 1));
    }
    throw new Error("MIDI: RMID without data chunk");
  }
  r.pos = 0;
  if (r.str(4) !== "MThd") throw new Error("MIDI: missing MThd header");
  const hlen = r.u32();
  const format = r.u16();
  const ntracks = r.u16();
  const division = r.u16();
  r.skip(hlen - 6);

  const events: RawEvent[] = [];
  let order = 0;
  for (let t = 0; t < ntracks && !r.eof; t++) {
    const id = r.str(4);
    const len = r.u32();
    if (id !== "MTrk") {
      r.skip(len);
      t--;
      continue;
    }
    const end = r.pos + len;
    let tick = 0;
    let status = 0;
    while (r.pos < end) {
      tick += r.vlq();
      let b = r.u8();
      if (b === 0xff) {
        const type = r.u8();
        const l = r.vlq();
        if (type === 0x51 && l === 3) {
          const us = (r.u8() << 16) | (r.u8() << 8) | r.u8();
          events.push({ tick, order: order++, kind: "tempo", channel: 0, a: us, b: 0 });
        } else r.skip(l);
        if (type === 0x2f) break;
        continue;
      }
      if (b === 0xf0 || b === 0xf7) {
        r.skip(r.vlq());
        continue;
      }
      let first: number;
      if (b & 0x80) {
        status = b;
        first = r.u8();
      } else {
        if (!status) throw new Error("MIDI: running status without a status byte");
        first = b;
        b = status;
      }
      const hi = status & 0xf0;
      const ch = status & 0x0f;
      if (hi === 0xc0 || hi === 0xd0) {
        if (hi === 0xc0) events.push({ tick, order: order++, kind: "program", channel: ch, a: first, b: 0 });
        continue;
      }
      const second = r.u8();
      if (hi === 0xb0) events.push({ tick, order: order++, kind: "cc", channel: ch, a: first, b: second });
      else if (hi === 0xe0) events.push({ tick, order: order++, kind: "bend", channel: ch, a: ((second << 7) | first) - 8192, b: 0 });
      else if (hi === 0x90 && second > 0) events.push({ tick, order: order++, kind: "on", channel: ch, a: first, b: second });
      else if (hi === 0x80 || hi === 0x90) events.push({ tick, order: order++, kind: "off", channel: ch, a: first, b: 0 });
    }
    r.pos = end;
  }

  events.sort((x, y) => x.tick - y.tick || x.order - y.order);
  const smpte = (division & 0x8000) !== 0;
  const fps = smpte ? 256 - (division >> 8) : 0;
  const tpf = division & 0xff;
  let usPerQuarter = 500_000;
  let lastTick = 0;
  let seconds = 0;
  const toSec = (tick: number) => {
    const dt = tick - lastTick;
    if (smpte) seconds += dt / (fps * tpf);
    else seconds += (dt * usPerQuarter) / 1e6 / division;
    lastTick = tick;
    return seconds;
  };
  const programs = new Array<number>(16).fill(0);
  const banks = new Array<number>(16).fill(0);
  const bankSelect = new Array<number>(16).fill(0);
  const bendRange = new Array<number>(16).fill(2);
  const rpn = new Array<number>(16).fill(0x3fff);
  const pedal = new Array<boolean>(16).fill(false);
  type Open = { t: number; velocity: number; program: number; bank: number; released: boolean };
  const open = new Map<number, Open[]>();
  const notes: MidiNote[] = [];
  const controls: MidiControl[] = [];
  const finish = (key: number, s: Open, now: number) =>
    notes.push({ t: s.t, d: Math.max(0.02, now - s.t), note: key % 128, velocity: s.velocity, channel: Math.floor(key / 128), program: s.program, bank: s.bank });
  for (const e of events) {
    const now = toSec(e.tick);
    const ch = e.channel;
    if (e.kind === "tempo") usPerQuarter = e.a;
    else if (e.kind === "program") {
      programs[ch] = e.a;
      banks[ch] = bankSelect[ch]!;
    } else if (e.kind === "bend") controls.push({ t: now, channel: ch, kind: "bend", value: (e.a / 8192) * bendRange[ch]! * 100 });
    else if (e.kind === "cc") {
      const kind = CONTROLLERS[e.a];
      if (kind) controls.push({ t: now, channel: ch, kind, value: e.b });
      else if (e.a === 0) bankSelect[ch] = e.b;
      else if (e.a === 101) rpn[ch] = (e.b << 7) | (rpn[ch]! & 0x7f);
      else if (e.a === 100) rpn[ch] = (rpn[ch]! & 0x3f80) | e.b;
      else if (e.a === 6 && rpn[ch] === 0) bendRange[ch] = e.b; // RPN 0: pitch bend sensitivity
      else if (e.a === 64) {
        pedal[ch] = e.b > 0; // the Microsoft GS synth treats any non-zero value as pedal down
        if (!pedal[ch]) {
          for (let k = ch * 128; k < ch * 128 + 128; k++) {
            const stack = open.get(k);
            if (!stack?.some((s) => s.released)) continue;
            for (const s of stack.filter((s) => s.released)) finish(k, s, now);
            open.set(k, stack.filter((s) => !s.released));
          }
        }
      }
    } else {
      const key = ch * 128 + e.a;
      if (e.kind === "on") {
        const stack = open.get(key) ?? [];
        // A key struck again while held by the pedal: the old note ends here.
        for (const s of stack.filter((s) => s.released)) finish(key, s, now);
        const held = stack.filter((s) => !s.released);
        held.push({ t: now, velocity: e.b, program: programs[ch] ?? 0, bank: ch === PERCUSSION_CHANNEL ? 128 : banks[ch]!, released: false });
        open.set(key, held);
      } else {
        const stack = open.get(key);
        const s = stack?.find((s) => !s.released);
        if (!stack || !s) continue;
        if (pedal[ch]) s.released = true;
        else {
          stack.splice(stack.indexOf(s), 1);
          finish(key, s, now);
        }
      }
    }
  }
  // Notes never switched off end with the song.
  const endSec = seconds;
  for (const [key, stack] of open) {
    for (const s of stack) finish(key, s, Math.max(endSec, s.t + 0.1));
  }
  notes.sort((a, b) => a.t - b.t || a.note - b.note);
  const duration = notes.reduce((m, n) => Math.max(m, n.t + n.d), endSec);
  return { format, tracks: ntracks, duration, notes, controls };
}

export function midiFrequency(note: number): number {
  return 440 * 2 ** ((note - 69) / 12);
}
