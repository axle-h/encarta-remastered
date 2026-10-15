import { useEffect } from "react";
import { useNavigate } from "react-router-dom";

/** Where a double-click looks a word up: article text and the Map Trek stop text. */
export const LOOKUP_AREAS = ".enc-article, .art-text, .mt-stop-text";

/** The word a double-click selected, trimmed of punctuation, or "" when it is not a plain word. */
export function lookupWord(selected: string): string {
  const w = selected.trim().replace(/^[^\p{L}\p{N}]+|[^\p{L}\p{N}]+$/gu, "");
  return w && !/\s/.test(w) && w.length <= 40 ? w : "";
}

/**
 * Encarta's general double-click lookup (vm-findings "Map Trek"): a double-clicked word opens the
 * dictionary on it. Encarta Kids has none (a double-click only selects the word).
 */
export function useDoubleClickLookup() {
  const navigate = useNavigate();
  useEffect(() => {
    const on = (e: MouseEvent) => {
      const t = e.target;
      if (!(t instanceof Element) || !t.closest(LOOKUP_AREAS) || t.closest("a, button, input, textarea")) return;
      const word = lookupWord(window.getSelection()?.toString() ?? "");
      if (word) navigate(`/dictionary?q=${encodeURIComponent(word)}`);
    };
    document.addEventListener("dblclick", on);
    return () => document.removeEventListener("dblclick", on);
  }, [navigate]);
}
