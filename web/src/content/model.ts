/**
 * The converter's article model (content-schema.md §8): inline nodes and blocks of
 * `article.body_json`. Text is a plain JSON string; everything else is an object with `t`.
 */

export type LinkKind =
  | "qv"
  | "web"
  | "see-also"
  | "see-also-next"
  | "see-also-paren"
  | "see-also-embed"
  | "see"
  | "see-next"
  | "paren"
  | "see-embed"
  | "sidebar-see"
  | string;

export type InlineNode =
  | { t: "em" | "strong" | "sub" | "sup" | "u" | "smallcaps" | "title-ref" | "formula"; c: Inline[] }
  | { t: "br" }
  | { t: "link"; kind: LinkKind; ref?: number; para?: number; url?: string; c: Inline[] }
  | { t: "img"; id: string; placement?: string }
  | { t: "frac"; num: string; den: string }
  | { t: "stack"; top: string; bottom: string }
  | { t: "radical" | "overline" | "vector"; c: string | Inline[] }
  | { t: "place"; gid?: number; rid?: number; c: Inline[] }
  | { t: "quote"; align?: string; c: Inline[] }
  | { t: "list"; style?: string; items: Inline[][] }
  | { t: "unknown"; name?: string; c: Inline[] };

export type Inline = string | InlineNode;

export interface ParagraphBlock {
  t: "p" | "intro" | "byline";
  id?: number;
  c: Inline[];
}

export interface HeadingBlock {
  t: "heading";
  id?: number;
  level?: number;
  align?: string;
  c: Inline[];
}

export interface BibBlock {
  t: "bib";
  author?: Inline[];
  title?: Inline[];
  series?: Inline[];
  publisher?: Inline[];
  edition?: Inline[];
  volumes?: Inline[];
  date?: Inline[];
  annotation?: Inline[];
}

export interface MediaBlock {
  t: "media";
  ref: number;
  group: string;
  seq?: number;
}

export interface SectionBlock {
  t: "section";
  id: number;
  kind?: "introduction" | "major" | "minor" | "subminor" | string;
  type?: number;
  level?: number;
  /** Number labels from the outermost section down, e.g. ["III", "A"]. */
  number?: string[] | string;
  title?: string | Inline[];
  blocks: Block[];
}

export type Block = ParagraphBlock | HeadingBlock | BibBlock | MediaBlock | SectionBlock | { t: "rule" };

/** Text the reader prints before a cross-reference (content-schema §2). */
export function linkPrefix(kind: LinkKind): string {
  switch (kind) {
    case "see-also":
    case "see-also-embed":
      return "See also ";
    case "see":
      return "See ";
    case "see-embed":
      return "see ";
    case "see-also-paren":
    case "paren":
      return "(";
    default:
      return "";
  }
}

export function linkSuffix(kind: LinkKind): string {
  return kind === "see-also-paren" || kind === "paren" ? ")" : "";
}

/** Plain text of inline content (for titles, alt text, search). */
export function plainText(c: Inline[] | string | undefined): string {
  if (c === undefined) return "";
  if (typeof c === "string") return c;
  let out = "";
  for (const n of c) {
    if (typeof n === "string") out += n;
    else if (n.t === "br") out += " ";
    else if (n.t === "frac") out += `${n.num}/${n.den}`;
    else if (n.t === "stack") out += `${n.top}/${n.bottom}`;
    else if (n.t === "list") out += n.items.map((i) => plainText(i)).join(" ");
    else if (n.t === "img") out += "";
    else if ("c" in n) out += plainText(n.c as Inline[] | string);
  }
  return out;
}

/** Every section of a body, depth first, with its nesting depth (1 = top level). */
export function walkSections(blocks: Block[], depth = 1, out: { section: SectionBlock; depth: number }[] = []) {
  for (const b of blocks) {
    if (b.t === "section") {
      out.push({ section: b, depth });
      walkSections(b.blocks, depth + 1, out);
    }
  }
  return out;
}
