import { useEffect } from "react";
import { useNavigate } from "react-router-dom";
import { FEATURES_EVENT, OPTIONS_EVENT, PRINT_EVENT } from "./commands";

/** What a key combination does, or null when the browser keeps it (Ctrl+F, Ctrl+C, …). */
export type ShortcutAction =
  | { go: string }
  | { back: -1 | 1 }
  | { event: string }
  | { focusSearch: true };

interface KeyLike {
  key: string;
  ctrlKey: boolean;
  metaKey: boolean;
  altKey: boolean;
  shiftKey: boolean;
}

export function shortcutFor(e: KeyLike): ShortcutAction | null {
  const k = e.key.toLowerCase();
  if (e.altKey && !e.ctrlKey && !e.metaKey && !e.shiftKey) {
    if (k === "home") return { go: "/" };
    if (k === "arrowleft") return { back: -1 };
    if (k === "arrowright") return { back: 1 };
    if (k === "t") return { event: FEATURES_EVENT };
    return null;
  }
  if ((e.ctrlKey || e.metaKey) && !e.altKey && !e.shiftKey) {
    switch (k) {
      case "s":
        return { focusSearch: true };
      case "p":
        return { event: PRINT_EVENT };
      case "o":
        return { event: OPTIONS_EVENT };
      case "m":
        return { go: "/atlas" };
      case "d":
        return { go: "/dictionary" };
      case "q":
        return { go: "/games/geoquiz" };
      case "t":
        return { go: "/timeline" };
    }
  }
  return null;
}

/** Encarta's keyboard shortcuts (docs/vm-findings.md, "Menu bar"), for the adult shell. */
export function useShortcuts() {
  const navigate = useNavigate();
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const a = shortcutFor(e);
      if (!a) return;
      const t = e.target as HTMLElement | null;
      const typing = !!t && (t.isContentEditable || /^(INPUT|TEXTAREA|SELECT)$/.test(t.tagName));
      // Alt+arrows belong to a text field's caret handling no more than the browser's history; leave them there.
      if ("back" in a && typing) return;
      e.preventDefault();
      if ("go" in a) navigate(a.go);
      else if ("back" in a) navigate(a.back);
      else if ("event" in a) window.dispatchEvent(new CustomEvent(a.event));
      else {
        const box = document.querySelector<HTMLInputElement>(".topbar-search input");
        box?.focus();
        box?.select();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [navigate]);
}
