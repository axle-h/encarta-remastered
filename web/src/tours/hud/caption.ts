/**
 * Caption sizing (tours-3d.md §8.4.2). The original measures the caption text in the window DC's
 * stock System font (a font-selection bug) and corrects the width with
 * `w = Wm + 8 − trunc((Wm − adder) × scale)`, `+10` for outlined (`color_key`) captions.
 *
 * SYSTEM_WIDTHS is the XP System font as the reference VM reports it (GetCharWidth32 on the
 * control's window DC, face "System", 0x20–0xFF, Windows-1252). With it the rule gives every
 * hotspot box width in the VM captures (55 boxes), and the authoring widths "ERS Label" = 68 px
 * (89-px boxes) and "ERS HotSpot" = 83 px (91-px boxes).
 */
const SYSTEM_WIDTHS = [
  4, 4, 6, 8, 8, 11, 9, 4, 4, 4, 6, 8, 4, 4, 4, 4,  // 0x20–0x2F
  8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 4, 4, 8, 8, 8, 8,  // 0x30–0x3F
  14, 8, 10, 9, 10, 9, 8, 10, 10, 4, 7, 9, 8, 12, 10, 10,  // 0x40–0x4F
  9, 10, 10, 9, 8, 10, 8, 14, 9, 10, 9, 4, 4, 4, 5, 8,  // 0x50–0x5F
  5, 8, 8, 7, 8, 8, 4, 8, 8, 4, 4, 7, 4, 12, 8, 8,  // 0x60–0x6F
  8, 8, 5, 8, 4, 8, 8, 10, 8, 8, 8, 5, 4, 5, 5, 4,  // 0x70–0x7F
  4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4,  // 0x80–0x8F
  4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4,  // 0x90–0x9F
  9, 4, 8, 8, 8, 8, 4, 8, 5, 10, 5, 7, 8, 4, 10, 8,  // 0xA0–0xAF
  5, 8, 4, 4, 5, 8, 7, 4, 5, 4, 5, 7, 11, 11, 11, 8,  // 0xB0–0xBF
  8, 8, 8, 8, 8, 8, 13, 9, 9, 9, 9, 9, 4, 4, 4, 4,  // 0xC0–0xCF
  10, 10, 10, 10, 10, 10, 10, 8, 10, 10, 10, 10, 10, 10, 9, 8,  // 0xD0–0xDF
  8, 8, 8, 8, 8, 8, 12, 7, 8, 8, 8, 8, 4, 4, 4, 4,  // 0xE0–0xEF
  8, 8, 8, 8, 8, 8, 8, 6, 8, 8, 8, 8, 8, 8, 8, 8,  // 0xF0–0xFF
];

/** Windows-1252 bytes 0x80–0x9F for the Unicode characters that have them. */
const CP1252: Record<string, number> = {
  "€": 0x80, "‚": 0x82, "ƒ": 0x83, "„": 0x84, "…": 0x85, "†": 0x86, "‡": 0x87, "ˆ": 0x88, "‰": 0x89, "Š": 0x8a,
  "‹": 0x8b, "Œ": 0x8c, "Ž": 0x8e, "‘": 0x91, "’": 0x92, "“": 0x93, "”": 0x94, "•": 0x95, "–": 0x96, "—": 0x97,
  "˜": 0x98, "™": 0x99, "š": 0x9a, "›": 0x9b, "œ": 0x9c, "ž": 0x9e, "Ÿ": 0x9f,
};

export function systemCharWidth(ch: string): number {
  let code = CP1252[ch] ?? ch.charCodeAt(0);
  if (code < 0x20 || code > 0xff) code = 0x3f; // not in the code page: "?"
  return SYSTEM_WIDTHS[code - 0x20] ?? 8;
}

/** Text width in the System font, the `Wm` of the width rule. */
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
  // The scale is a float32 (0.26 → 0.25999999): "Titus" (Wm 32) gives 52 px in the VM, not 53.
  let w = wm + 8 - Math.trunc((wm - metrics.adder) * Math.fround(metrics.scale));
  if (colorKey) w += 10;
  return w;
}
