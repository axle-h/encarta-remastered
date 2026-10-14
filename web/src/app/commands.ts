/**
 * Commands of the File and Edit menus. Encarta's Copy put a source line under copied text; the
 * wording here is ours (the VM captures do not show the clipboard).
 */

/** Event the article reader listens to for Edit › Find in Article. */
export const FIND_EVENT = "encarta:find";

export function citation(title: string, url: string): string {
  const t = title.trim();
  return `${t ? `"${t}." ` : ""}Microsoft® Encarta® Premium 2009 (resurrected). ${url}`;
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
  window.print();
}

export function openFind() {
  window.dispatchEvent(new CustomEvent(FIND_EVENT));
}
