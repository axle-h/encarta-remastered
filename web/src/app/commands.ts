/**
 * Commands of the File and Edit menus. Encarta's Copy put a source line under copied text; the
 * wording here is ours (the VM captures do not show the clipboard).
 */

/** Event the article reader listens to for Edit › Find in Article. */
export const FIND_EVENT = "encarta:find";

/** Appended to everything copied or printed (Encarta did the same; the wording is generic). */
export const COPYRIGHT = "© Microsoft Corporation. All rights reserved.";

export function citation(title: string, url: string): string {
  const t = title.trim();
  return `${t ? `"${t}." ` : ""}${COPYRIGHT} Microsoft® Encarta® Premium 2009 (resurrected). ${url}`;
}

/** The selected text followed by a blank line and the citation; null when nothing is selected. */
export function textWithCitation(selected: string, title: string, url: string): string | null {
  const s = selected.trim();
  return s ? `${s}\n\n${citation(title, url)}` : null;
}

export function selectedText(): string {
  return window.getSelection?.()?.toString() ?? "";
}

/** Edit › Copy: the selection with its citation. Resolves false when nothing could be copied. */
export async function copySelection(title: string): Promise<boolean> {
  const text = textWithCitation(selectedText(), title, window.location.href);
  if (!text) return false;
  try {
    await navigator.clipboard.writeText(text);
    return true;
  } catch {
    // Older browsers or no clipboard permission: the browser's own copy of the selection.
    try {
      return document.execCommand("copy");
    } catch {
      return false;
    }
  }
}

export function printPage() {
  printWith("media", "");
}

/** Events the menu bar listens to, fired by keyboard shortcuts. */
export const PRINT_EVENT = "encarta:print";
export const COPY_EVENT = "encarta:copy";
export const OPTIONS_EVENT = "encarta:options";
export const FEATURES_EVENT = "encarta:features";

export type ArticleScope = "text" | "media" | "selection";

/** The article's text (the reading column), or "" when the page has none. */
export function articleText(): string {
  const el = document.querySelector<HTMLElement>(".art-text");
  return (el?.innerText ?? el?.textContent ?? "").trim();
}

/** Edit › Copy for a scope: the article text or the selection, plus the citation. */
export async function copyScope(scope: ArticleScope, title: string): Promise<boolean> {
  const body = scope === "selection" ? selectedText() : articleText();
  const text = textWithCitation(body, title, window.location.href);
  if (!text) return false;
  try {
    await navigator.clipboard.writeText(text);
    return true;
  } catch {
    return false;
  }
}

/**
 * Prints with a copyright line after the content. `text` leaves the pictures out, `selection`
 * prints only what is selected (copied into a temporary block), `media` is the whole page.
 */
export function printWith(scope: ArticleScope, title: string) {
  const root = document.documentElement;
  const added: HTMLElement[] = [];
  const line = document.createElement("p");
  line.className = "print-copyright";
  line.textContent = `${title.trim() ? `${title.trim()}. ` : ""}${COPYRIGHT}`;
  if (scope === "selection") {
    const sel = window.getSelection();
    if (sel && sel.rangeCount > 0 && !sel.isCollapsed) {
      const box = document.createElement("div");
      box.className = "print-selection";
      box.appendChild(sel.getRangeAt(0).cloneContents());
      document.body.appendChild(box);
      added.push(box);
    }
  }
  document.body.appendChild(line);
  added.push(line);
  root.dataset.printScope = scope;
  const done = () => {
    delete root.dataset.printScope;
    added.forEach((n) => n.remove());
    window.removeEventListener("afterprint", done);
  };
  window.addEventListener("afterprint", done);
  window.print();
}

export function openFind() {
  window.dispatchEvent(new CustomEvent(FIND_EVENT));
}
