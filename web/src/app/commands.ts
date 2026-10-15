/**
 * Commands of the File and Edit menus. Copy and print end with Encarta's source line
 * (docs/vm-findings.md, "Copy source line"): a selection gets it on the next line, a whole
 * article or dictionary entry after a blank line.
 */

/** Event the article reader listens to for Edit › Find in Article. */
export const FIND_EVENT = "encarta:find";

/** The line Encarta 2009 put under everything copied or printed. */
export const SOURCE_NAME = "Microsoft® Encarta® 2009.";
export const COPYRIGHT = "© 1993-2008 Microsoft Corporation. All rights reserved.";
export const SOURCE_LINE = `${SOURCE_NAME} ${COPYRIGHT}`;

/** Selector of the elements whose text is "the article" (adult reader, Kids reader). */
export const ARTICLE_TEXT = ".art-text, .kids-text";

/**
 * Text with the source line: `whole` (an entire article or entry) leaves a blank line before it,
 * a selection only a line break. Null when there is nothing to copy.
 */
export function withSourceLine(text: string, whole = false): string | null {
  const s = text.trim();
  return s ? `${s}${whole ? "\n\n" : "\n"}${SOURCE_LINE}` : null;
}

/** The same as HTML: the copied markup, then the source line with its name in bold. */
export function sourceLineHtml(): string {
  return `<p><b>${SOURCE_NAME}</b> ${COPYRIGHT}</p>`;
}

export function selectedText(): string {
  return window.getSelection?.()?.toString() ?? "";
}

/** Puts text on the clipboard; resolves false when the browser refuses. */
export async function writeClipboard(text: string): Promise<boolean> {
  try {
    await navigator.clipboard.writeText(text);
    return true;
  } catch {
    return false;
  }
}

/** Edit › Copy with a selection: the selection and the source line. Resolves false when nothing is selected. */
export async function copySelection(): Promise<boolean> {
  const text = withSourceLine(selectedText());
  return text ? writeClipboard(text) : false;
}

/**
 * Ctrl+C inside an article: Encarta added the source line to plain copies too. Returns true when
 * it filled the clipboard (the caller then cancels the browser's own copy).
 */
export function addSourceToCopy(e: ClipboardEvent): boolean {
  const sel = window.getSelection?.();
  if (!e.clipboardData || !sel || sel.isCollapsed || sel.rangeCount === 0) return false;
  const anchor = sel.anchorNode instanceof Element ? sel.anchorNode : sel.anchorNode?.parentElement;
  if (!anchor?.closest(ARTICLE_TEXT)) return false;
  const text = withSourceLine(sel.toString());
  if (!text) return false;
  const box = document.createElement("div");
  box.appendChild(sel.getRangeAt(0).cloneContents());
  e.clipboardData.setData("text/plain", text);
  e.clipboardData.setData("text/html", `${box.innerHTML}${sourceLineHtml()}`);
  return true;
}

/** Copy events get the source line inside articles; returns the cleanup. */
export function installCopySourceLine(): () => void {
  const on = (e: ClipboardEvent) => {
    if (addSourceToCopy(e)) e.preventDefault();
  };
  document.addEventListener("copy", on);
  return () => document.removeEventListener("copy", on);
}

export function printPage() {
  printWith("media");
}

/** Asks the print host (the Print dialog of whichever shell is on screen) to start File › Print. */
export function requestPrint() {
  window.dispatchEvent(new CustomEvent(PRINT_EVENT));
}

/** Events the menu bar listens to, fired by keyboard shortcuts. */
export const PRINT_EVENT = "encarta:print";
export const COPY_EVENT = "encarta:copy";
export const OPTIONS_EVENT = "encarta:options";
export const FEATURES_EVENT = "encarta:features";

export type ArticleScope = "text" | "media" | "selection";

/** Everything the Print dialogs offer: the article scopes, the picture's two and the timeline's two. */
export type PrintScope = ArticleScope | "picture" | "caption" | "timeline-page" | "timeline-all";

/**
 * Encarta asks "The article you are printing is longer than 5 pages" before printing. Which count it
 * uses is not known (the VM only shows the prompt and the printed results: zebra text 16,300 characters
 * on 5 pages, no prompt; with media 7 pages), so the pages are estimated from the printed length:
 * about 3,300 characters of text per A4 page (zebra's 16,316 characters filled 5 pages) and a third
 * of a page per picture (zebra's media print took 2 more pages for its pictures).
 */
export const LONG_PRINT_PAGES = 5;
export const CHARS_PER_PAGE = 3300;
export const PICTURE_PAGES = 0.35;

/** Estimated printed pages of the article (or its selection) for a scope; 0 with no article on screen. */
export function estimatePrintPages(scope: PrintScope): number {
  const chars = scope === "selection" ? selectedText().length : articleText().length;
  const pictures = scope === "media" ? document.querySelectorAll(".art-cols img").length : 0;
  return chars / CHARS_PER_PAGE + pictures * PICTURE_PAGES;
}

/** The article's title and text (the reading column), or "" when the page has none. */
export function articleText(): string {
  const el = document.querySelector<HTMLElement>(ARTICLE_TEXT);
  if (!el) return "";
  const body = (el.innerText ?? el.textContent ?? "").trim();
  const title = (document.querySelector<HTMLElement>("main h1")?.innerText ?? "").trim();
  return title && !body.startsWith(title) ? `${title}\n\n${body}` : body;
}

/**
 * The article as HTML with its pictures (the text column and the side column beside it), links
 * made absolute and controls dropped; "" when the page has no article.
 */
export function articleHtmlWithMedia(): string {
  const cols = document.querySelector<HTMLElement>(".art-cols");
  if (!cols) return "";
  const box = cols.cloneNode(true) as HTMLElement;
  box.querySelectorAll("button, script, style, audio, video, canvas, iframe").forEach((n) => n.remove());
  box.querySelectorAll<HTMLImageElement>("img").forEach((img) => {
    const src = img.getAttribute("src");
    if (src) img.setAttribute("src", new URL(src, document.baseURI).href);
  });
  box.querySelectorAll<HTMLAnchorElement>("a[href]").forEach((a) => a.setAttribute("href", new URL(a.getAttribute("href")!, document.baseURI).href));
  const title = (document.querySelector<HTMLElement>("main h1")?.innerText ?? "").trim();
  const head = title ? `<h1>${title.replace(/[&<]/g, (c) => (c === "&" ? "&amp;" : "&lt;"))}</h1>` : "";
  return `${head}${box.innerHTML}${sourceLineHtml()}`;
}

/** Puts HTML and its plain text on the clipboard; falls back to the text alone. */
export async function writeClipboardHtml(html: string, text: string): Promise<boolean> {
  try {
    if (typeof ClipboardItem !== "undefined") {
      await navigator.clipboard.write([
        new ClipboardItem({ "text/html": new Blob([html], { type: "text/html" }), "text/plain": new Blob([text], { type: "text/plain" }) }),
      ]);
      return true;
    }
  } catch {
    /* fall through to plain text */
  }
  return writeClipboard(text);
}

/** Edit › Copy for a scope: the selection, the whole article text, or the whole article with its pictures. */
export async function copyScope(scope: ArticleScope): Promise<boolean> {
  if (scope === "media") {
    const text = withSourceLine(articleText(), true);
    const html = articleHtmlWithMedia();
    return text && html ? writeClipboardHtml(html, text) : false;
  }
  const text = scope === "selection" ? withSourceLine(selectedText()) : withSourceLine(articleText(), true);
  return text ? writeClipboard(text) : false;
}

/** The Kids Copy button: the selection if there is one, else the whole article (VM: no dialog). */
export async function copySelectionOrArticle(): Promise<boolean> {
  return selectedText().trim() ? copySelection() : copyScope("text");
}

/**
 * Prints with a copyright line after the content. `text` leaves the pictures out, `selection`
 * prints only what is selected (copied into a temporary block), `media` is the whole page.
 */
export function printWith(scope: PrintScope) {
  const root = document.documentElement;
  const added: HTMLElement[] = [];
  const line = document.createElement("p");
  line.className = "print-copyright";
  line.textContent = SOURCE_LINE;
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
