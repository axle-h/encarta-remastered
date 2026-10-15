/**
 * Edit › Read Aloud (adult) and Tools › Read Aloud (Kids) with the browser's speech synthesis
 * (Encarta used SAPI; the voice is ours). The submenu follows the VM (docs/vm-findings.md,
 * "Read Aloud"): Whole Article and Selected Text only while idle (Selected Text needs a
 * selection), Stop Reading and Pause only while reading, Pause turns into Continue when paused.
 * Nothing in the page is highlighted while it reads.
 */
import type { MenuEntry } from "../components/Menu";
import { articleText, selectedText } from "./commands";

function synthesis(): SpeechSynthesis | undefined {
  return typeof window !== "undefined" ? window.speechSynthesis : undefined;
}

export const canReadAloud = () => !!synthesis();

/** Reads the whole article, or only the selection. False when there is nothing to read. */
export function readAloud(what: "article" | "selection"): boolean {
  const synth = synthesis();
  if (!synth) return false;
  const text = what === "selection" ? selectedText().trim() : articleText();
  if (!text) return false;
  synth.cancel();
  synth.speak(new SpeechSynthesisUtterance(text));
  return true;
}

export function stopReading() {
  synthesis()?.cancel();
}

/** Pauses reading, or continues it when paused. */
export function togglePause() {
  const synth = synthesis();
  if (!synth) return;
  if (synth.paused) synth.resume();
  else synth.pause();
}

/** True while an utterance is queued, playing or paused. */
export const isReading = () => !!synthesis()?.speaking;

/** The Read Aloud submenu entries for the current state. */
export function readAloudSubmenu(read: (what: "article" | "selection") => void): MenuEntry[] {
  const reading = isReading();
  return [
    { label: "Whole Article", disabled: reading, onSelect: () => read("article"), key: "ra-all" },
    { label: "Selected Text", disabled: reading || !selectedText().trim(), onSelect: () => read("selection"), key: "ra-sel" },
    { separator: true, key: "ra-s" },
    { label: "Stop Reading", shortcut: "F9", disabled: !reading, onSelect: stopReading, key: "ra-stop" },
    { label: synthesis()?.paused ? "Continue" : "Pause", disabled: !reading, onSelect: togglePause, key: "ra-pause" },
  ];
}
