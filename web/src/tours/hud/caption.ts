/**
 * Caption sizing (tours-3d.md §8.4.2). The original measures the caption text in the window DC's
 * stock System font (a font-selection bug) and corrects the width with
 * `w = Wm + 8 − trunc((Wm − adder) × scale)`, `+10` for outlined (`color_key`) captions.
 *
 * SYSTEM_WIDTHS approximates the XP System font advance widths: Helvetica-Bold widths × 14.6/1000,
 * floored. That table reproduces the two widths the authoring tool saved on the disc ("ERS Label"
 * = 68 px → 89-px boxes, "ERS HotSpot" = 83 px → 91-px boxes) but is not a capture of the real
 * font; replace it with GetCharWidth32 values from the reference VM when they exist (§8.8).
 */

/** Helvetica-Bold advance widths (1/1000 em) for 0x20…0x7E. */
const HELVETICA_BOLD = [
  278, 333, 474, 556, 556, 889, 722, 238, 333, 333, 389, 584, 278, 333, 278, 278, // 0x20–0x2F
  556, 556, 556, 556, 556, 556, 556, 556, 556, 556, 333, 333, 584, 584, 584, 611, // 0x30–0x3F
  975, 722, 722, 722, 722, 667, 611, 778, 722, 278, 556, 722, 611, 833, 722, 778, // 0x40–0x4F
  667, 778, 722, 667, 611, 722, 667, 944, 667, 667, 611, 333, 278, 333, 584, 556, // 0x50–0x5F
  333, 556, 611, 556, 611, 556, 333, 611, 611, 278, 278, 556, 278, 889, 611, 611, // 0x60–0x6F
  611, 611, 389, 556, 333, 611, 556, 778, 556, 556, 500, 389, 280, 389, 584, // 0x70–0x7E
];

const SCALE = 14.6 / 1000;

/** Base letters for Latin-1 accented characters (width of the base letter). */
const LATIN1_BASE: Record<string, string> = {
  À: "A", Á: "A", Â: "A", Ã: "A", Ä: "A", Å: "A", Ç: "C", È: "E", É: "E", Ê: "E", Ë: "E",
  Ì: "I", Í: "I", Î: "I", Ï: "I", Ñ: "N", Ò: "O", Ó: "O", Ô: "O", Õ: "O", Ö: "O", Ù: "U", Ú: "U", Û: "U", Ü: "U", Ý: "Y",
  à: "a", á: "a", â: "a", ã: "a", ä: "a", å: "a", ç: "c", è: "e", é: "e", ê: "e", ë: "e",
  ì: "i", í: "i", î: "i", ï: "i", ñ: "n", ò: "o", ó: "o", ô: "o", õ: "o", ö: "o", ù: "u", ú: "u", û: "u", ü: "u", ý: "y", ÿ: "y",
  "‘": "'", "’": "'", "“": '"', "”": '"', "–": "-", "—": "-",
};

export function systemCharWidth(ch: string): number {
  const base = LATIN1_BASE[ch] ?? ch;
  const code = base.charCodeAt(0);
  const w = code >= 0x20 && code <= 0x7e ? HELVETICA_BOLD[code - 0x20] : 556;
  return Math.floor((w ?? 556) * SCALE);
}

/** Text width in the (approximated) System font, the `Wm` of the width rule. */
export function systemTextWidth(text: string): number {
  let w = 0;
  for (const ch of text) w += systemCharWidth(ch);
  return w;
}

export interface CaptionMetrics {
  verticalOffset: number;
  scale: number;
  adder: number;
}

export const DEFAULT_METRICS: CaptionMetrics = { verticalOffset: 4, scale: 0.26, adder: 82 };
export const CAPTION_HEIGHT = 22;
export const CAPTION_SURFACE: [number, number] = [512, 32];

/** The caption box width rule of §8.4.2. */
export function captionWidth(text: string, colorKey: boolean, metrics: CaptionMetrics = DEFAULT_METRICS, measure = systemTextWidth): number {
  const wm = measure(text);
  let w = wm + 8 - Math.trunc((wm - metrics.adder) * metrics.scale);
  if (colorKey) w += 10;
  return w;
}
