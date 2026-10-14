/**
 * The dictionary word wheel (api.md `lookup?around=`): headwords before and after the typed word,
 * labelled as the reference VM lists them ("Paris (1)", "Paris (2)" for homographs).
 */
import type { DictLookupEntry } from "../api/types";

/** Words the wheel shows before the typed one (the VM's list has eight above the selection). */
export const WHEEL_BEFORE = 8;

export interface WheelRow {
  entry: DictLookupEntry;
  label: string;
  /** True for the rows from the typed word's position on. */
  after: boolean;
}

/** Rows of the wheel; runs of equal headwords get homograph numbers. */
export function wheelRows(before: DictLookupEntry[], after: DictLookupEntry[]): WheelRow[] {
  const all = [...before.map((e) => ({ e, after: false })), ...after.map((e) => ({ e, after: true }))];
  const word = (e: DictLookupEntry) => e.headword ?? e.form ?? "";
  return all.map(({ e, after: a }, i) => {
    const w = word(e);
    let start = i;
    while (start > 0 && word(all[start - 1]!.e) === w) start--;
    let end = i;
    while (end < all.length - 1 && word(all[end + 1]!.e) === w) end++;
    // Numbering is only certain when the whole run is in view; a run cut by the list edge
    // still counts from the first visible one.
    const label = end > start ? `${w} (${i - start + 1})` : w;
    return { entry: e, label, after: a };
  });
}
