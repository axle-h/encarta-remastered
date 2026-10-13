/**
 * Standard MIDI File reader (public SMF 1.0 format): formats 0 and 1, PPQ or SMPTE time division,
 * running status, tempo changes. Produces a flat, time-sorted list of notes in seconds for the
 * WebAudio synth in `MidiPlayer`.
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
}

export interface MidiSong {
  format: number;
  tracks: number;
  duration: number;
  notes: MidiNote[];
}

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
  kind: "on" | "off" | "tempo" | "program";
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
      if (hi === 0x90 && second > 0) events.push({ tick, order: order++, kind: "on", channel: ch, a: first, b: second });
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
  const open = new Map<number, { t: number; velocity: number; program: number }[]>();
  const notes: MidiNote[] = [];
  for (const e of events) {
    const now = toSec(e.tick);
    if (e.kind === "tempo") usPerQuarter = e.a;
    else if (e.kind === "program") programs[e.channel] = e.a;
    else {
      const key = e.channel * 128 + e.a;
      if (e.kind === "on") {
        const stack = open.get(key) ?? [];
        stack.push({ t: now, velocity: e.b, program: programs[e.channel] ?? 0 });
        open.set(key, stack);
      } else {
        const s = open.get(key)?.shift();
        if (s) notes.push({ t: s.t, d: Math.max(0.02, now - s.t), note: e.a, velocity: s.velocity, channel: e.channel, program: s.program });
      }
    }
  }
  // Notes never switched off end with the song.
  const endSec = seconds;
  for (const [key, stack] of open) {
    for (const s of stack) notes.push({ t: s.t, d: Math.max(0.1, endSec - s.t), note: key % 128, velocity: s.velocity, channel: Math.floor(key / 128), program: s.program });
  }
  notes.sort((a, b) => a.t - b.t || a.note - b.note);
  const duration = notes.reduce((m, n) => Math.max(m, n.t + n.d), endSec);
  return { format, tracks: ntracks, duration, notes };
}

export function midiFrequency(note: number): number {
  return 440 * 2 ** ((note - 69) / 12);
}
