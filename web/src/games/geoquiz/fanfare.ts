/**
 * The Geography Quiz's right-answer fanfare (about 2 s). Encarta plays a sound for it that is not on the
 * disc, so this one is our own, synthesised with Web Audio: a rising C-major arpeggio of brass-like
 * sawtooth voices that ends on a held chord. Nothing is fetched.
 */
const NOTES: [start: number, length: number, midi: number[]][] = [
  [0, 0.16, [60]],
  [0.16, 0.16, [64]],
  [0.32, 0.16, [67]],
  [0.48, 0.3, [72]],
  [0.84, 0.16, [67]],
  [1.0, 1.0, [60, 64, 67, 72]],
];

/** Total length in seconds. */
export const FANFARE_SECONDS = NOTES.reduce((m, [s, l]) => Math.max(m, s + l), 0);

const hz = (midi: number) => 440 * 2 ** ((midi - 69) / 12);

let ctx: AudioContext | null = null;

/** Plays the fanfare; silent where Web Audio is missing or refused. */
export function playFanfare() {
  try {
    const AC = typeof window === "undefined" ? undefined : window.AudioContext;
    if (!AC) return;
    ctx ??= new AC();
    void ctx.resume?.();
    const t0 = ctx.currentTime + 0.02;
    const master = ctx.createGain();
    master.gain.value = 0.16;
    master.connect(ctx.destination);
    for (const [start, length, midis] of NOTES) {
      for (const m of midis) {
        const osc = ctx.createOscillator();
        const env = ctx.createGain();
        osc.type = "sawtooth";
        osc.frequency.value = hz(m);
        const a = t0 + start;
        const end = a + length;
        env.gain.setValueAtTime(0, a);
        env.gain.linearRampToValueAtTime(1 / midis.length, a + 0.02);
        env.gain.setValueAtTime(1 / midis.length, Math.max(a + 0.02, end - 0.08));
        env.gain.linearRampToValueAtTime(0, end);
        osc.connect(env).connect(master);
        osc.start(a);
        osc.stop(end + 0.01);
      }
    }
  } catch {
    /* no sound */
  }
}
