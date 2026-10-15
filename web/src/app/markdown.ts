/**
 * Article text as Markdown for the clipboard's plain-text flavour: headings, paragraphs, bold and
 * italic, lists, quotes, tables and web links survive a paste into a plain-text editor. Walks the
 * live DOM so what is hidden on screen (section numbers in Kids, toggles, margin boxes) stays out,
 * and, given a range, only the selected part of it.
 */

/** No-break spaces (U+00A0, U+202F) as plain spaces. */
export function plainSpaces(s: string): string {
  return s.replace(/[\u00a0\u202f]/g, " ");
}

const SKIP = new Set(["BUTTON", "SCRIPT", "STYLE", "AUDIO", "VIDEO", "CANVAS", "IFRAME", "IMG", "SVG", "svg", "INPUT", "SELECT", "TEXTAREA", "COLGROUP"]);
const BLOCK = new Set([
  "ADDRESS", "ARTICLE", "ASIDE", "BLOCKQUOTE", "DD", "DIV", "DL", "DT", "FIGCAPTION", "FIGURE", "FOOTER", "H1", "H2", "H3", "H4", "H5", "H6",
  "HEADER", "HR", "LI", "MAIN", "NAV", "OL", "P", "SECTION", "TABLE", "UL",
]);

/** Media boxes and sidebar teasers in the text column: pictures and links, not article text. */
const NOT_TEXT = ".enc-media, .enc-mbox";

function hidden(el: Element): boolean {
  if (SKIP.has(el.tagName) || el.getAttribute("aria-hidden") === "true" || el.matches(NOT_TEXT)) return true;
  const view = el.ownerDocument.defaultView;
  return !!view && view.getComputedStyle(el).display === "none";
}

function escapeText(s: string): string {
  return s.replace(/([\\`*_])/g, "\\$1");
}

class Walker {
  constructor(private range?: Range) {}

  private inRange(n: Node): boolean {
    if (!this.range) return true;
    try {
      return this.range.intersectsNode(n);
    } catch {
      return false;
    }
  }

  private text(n: Text): string {
    let s = n.data;
    const r = this.range;
    if (r) {
      const end = r.endContainer === n ? r.endOffset : s.length;
      const start = r.startContainer === n ? r.startOffset : 0;
      s = s.slice(start, end);
    }
    return escapeText(plainSpaces(s).replace(/[ \t\r\n]+/g, " "));
  }

  /** The inline content of `el` on one line (line breaks kept as Markdown hard breaks). */
  inline(el: Node): string {
    let out = "";
    el.childNodes.forEach((c) => {
      if (!this.inRange(c)) return;
      if (c.nodeType === Node.TEXT_NODE) out += this.text(c as Text);
      else if (c.nodeType === Node.ELEMENT_NODE) out += this.inlineEl(c as Element);
    });
    return out;
  }

  private inlineEl(el: Element): string {
    if (hidden(el)) return "";
    if (BLOCK.has(el.tagName)) return ` ${this.inline(el).trim()} `;
    const inner = this.inline(el);
    switch (el.tagName) {
      case "BR":
        return "  \n";
      case "B":
      case "STRONG":
        return wrap(inner, "**");
      case "I":
      case "EM":
        return wrap(inner, "*");
      case "SUP":
      case "SUB":
        return inner.trim() ? `<${el.tagName.toLowerCase()}>${inner.trim()}</${el.tagName.toLowerCase()}>` : "";
      case "A": {
        const href = (el as HTMLAnchorElement).href;
        const web = /^https?:/i.test(href) && new URL(href).origin !== el.ownerDocument.location?.origin;
        return web && inner.trim() ? `[${inner.trim()}](${href})` : inner;
      }
      default:
        return inner;
    }
  }

  /** The blocks of `el`, each a run of lines; inline runs between blocks become paragraphs. */
  blocks(el: Node): string[] {
    const out: string[] = [];
    let run = "";
    const flush = () => {
      const p = tidy(run);
      if (p) out.push(p);
      run = "";
    };
    el.childNodes.forEach((c) => {
      if (!this.inRange(c)) return;
      if (c.nodeType === Node.TEXT_NODE) run += this.text(c as Text);
      else if (c.nodeType === Node.ELEMENT_NODE) {
        const e = c as Element;
        if (hidden(e)) return;
        if (!BLOCK.has(e.tagName)) {
          run += this.inlineEl(e);
          return;
        }
        flush();
        out.push(...this.block(e));
      }
    });
    flush();
    return out;
  }

  private block(el: Element): string[] {
    const tag = el.tagName;
    if (/^H[1-6]$/.test(tag)) {
      const t = tidy(this.inline(el));
      return t ? [`${"#".repeat(Number(tag[1]))} ${t}`] : [];
    }
    switch (tag) {
      case "HR":
        return ["---"];
      case "UL":
      case "OL":
        return this.list(el, tag === "OL");
      case "BLOCKQUOTE":
        return this.blocks(el).map((b) => b.replace(/^/gm, "> "));
      case "TABLE":
        return this.table(el as HTMLTableElement);
      default:
        return this.blocks(el);
    }
  }

  private list(el: Element, ordered: boolean): string[] {
    const items: string[] = [];
    let n = Number(el.getAttribute("start") ?? 1);
    for (const li of Array.from(el.children)) {
      if (li.tagName !== "LI" || !this.inRange(li) || hidden(li)) continue;
      const marker = ordered ? `${n++}. ` : "- ";
      const body = this.blocks(li).join("\n\n");
      if (!body) continue;
      items.push(marker + body.replace(/\n/g, `\n${" ".repeat(marker.length)}`));
    }
    return items.length ? [items.join("\n")] : [];
  }

  private table(el: HTMLTableElement): string[] {
    const rows = Array.from(el.rows)
      .filter((r) => this.inRange(r) && !hidden(r))
      .map((r) =>
        Array.from(r.cells)
          .filter((c) => !hidden(c))
          .map((c) => tidy(this.blocks(c).join(" ")).replace(/\|/g, "\\|").replace(/\n/g, " ")),
      )
      .filter((r) => r.length > 0);
    if (rows.length === 0) return [];
    const width = Math.max(...rows.map((r) => r.length));
    const line = (r: string[]) => `| ${Array.from({ length: width }, (_, i) => r[i] ?? "").join(" | ")} |`;
    const [head, ...body] = rows;
    return [[line(head!), `|${" --- |".repeat(width)}`, ...body.map(line)].join("\n")];
  }
}

/** `**x**` around the text, with its outer spaces kept outside the markers. */
function wrap(s: string, mark: string): string {
  const m = /^(\s*)([\s\S]*?)(\s*)$/.exec(s)!;
  return m[2] ? `${m[1]}${mark}${m[2]}${mark}${m[3]}` : s;
}

/** One paragraph: spaces collapsed and trimmed on every line (hard breaks kept). */
function tidy(s: string): string {
  return s
    .split("  \n")
    .map((l) => l.replace(/ {2,}/g, " ").trim())
    .filter(Boolean)
    .join("  \n");
}

/** `root` (or the part of it inside `range`) as Markdown; "" when nothing is in it. */
export function toMarkdown(root: Node, range?: Range): string {
  return new Walker(range).blocks(root).join("\n\n");
}

/** The selection's range as Markdown (its common ancestor walked, clipped to the range). */
export function rangeToMarkdown(range: Range): string {
  let root: Node = range.commonAncestorContainer;
  if (root.nodeType === Node.TEXT_NODE) {
    const r = range;
    const s = plainSpaces((root as Text).data.slice(r.startOffset, r.endOffset)).replace(/[ \t\r\n]+/g, " ").trim();
    return escapeText(s);
  }
  // A selection inside one block (a heading, list item, emphasis) keeps that block's form.
  while (root.parentNode && !BLOCK.has((root as Element).tagName)) root = root.parentNode;
  if (root.parentNode) root = root.parentNode;
  return toMarkdown(root, range);
}
