import { createContext, Fragment, useContext, type ReactNode } from "react";
import type { AssocItem, DataTable } from "../api/types";
import { Html } from "../components/Html";
import { Icon } from "../components/Icons";
import { Inlines, hasBlockContent, type RenderOptions } from "../content/Inlines";
import { type BibBlock, type Block, type Inline, type SectionBlock } from "../content/model";
import { MediaBox } from "./MediaBox";
import { firstParagraph, runsOf } from "./layout";

export interface BodyContext {
  assoc: Map<number, AssocItem>;
  tables: Map<number, DataTable>;
  opts: RenderOptions;
  /** Section ids that are collapsed (outline view). */
  collapsed: Set<number>;
  toggle: (id: number) => void;
  /** Id of the section that shows the "more…" preview while collapsed (the introduction). */
  previewSection: number | null;
  /** Whether to draw the left media column (Kids articles list their media separately). */
  mediaColumn: boolean;
  /** The viewer sequence for this article. */
  sequence: AssocItem[];
}

const Ctx = createContext<BodyContext | null>(null);

function useBody(): BodyContext {
  const c = useContext(Ctx);
  if (!c) throw new Error("ArticleBody context missing");
  return c;
}

export function ArticleBody({ blocks, ctx }: { blocks: Block[]; ctx: BodyContext }) {
  return (
    <Ctx.Provider value={ctx}>
      <div className="enc-article">
        <Blocks blocks={blocks} />
      </div>
    </Ctx.Provider>
  );
}

function Blocks({ blocks }: { blocks: Block[] }) {
  const ctx = useBody();
  const runs = runsOf(blocks, ctx.assoc, ctx.tables);
  return (
    <>
      {runs.map((r, i) => {
        if (r.t === "block") return <BlockView key={i} b={r.block} />;
        if (r.placement === "hidden" || r.placement === "right") return null;
        if (r.placement === "inline")
          return (
            <Fragment key={i}>
              {r.items.map((m) => {
                const tb = ctx.tables.get(m.ref);
                return tb ? <InlineTable key={m.ref} table={tb} /> : null;
              })}
            </Fragment>
          );
        if (!ctx.mediaColumn) return null;
        return <MediaBox key={i} items={r.items.map((m) => ctx.assoc.get(m.ref) ?? { ref_id: m.ref, group: m.group })} />;
      })}
    </>
  );
}

function InlineTable({ table }: { table: DataTable }) {
  const ctx = useBody();
  return (
    <div className="enc-inline-table">
      {table.title && <div className="enc-inline-table-title">{table.title}</div>}
      <Html html={table.html} kids={ctx.opts.kids} />
    </div>
  );
}

function Para({ tag, id, idPrefix, className, c }: { tag?: "p"; id?: number; idPrefix: string; className: string; c: Inline[] }) {
  const ctx = useBody();
  const Tag = tag && !hasBlockContent(c) ? "p" : "div";
  return (
    <Tag id={id !== undefined ? `${idPrefix}${id}` : undefined} data-para={idPrefix === "p" ? id : undefined} className={className}>
      <Inlines c={c} opts={ctx.opts} />
    </Tag>
  );
}

function BlockView({ b }: { b: Block }): ReactNode {
  switch (b.t) {
    case "p":
      return <Para tag="p" id={b.id} idPrefix="p" className="enc-p" c={b.c} />;
    case "intro":
      return <Para tag="p" id={b.id} idPrefix="i" className="enc-intro" c={b.c} />;
    case "byline":
      return <Para tag="p" id={b.id} idPrefix="b" className="enc-byline" c={b.c} />;
    case "heading": {
      const level = Math.min(6, Math.max(3, (b.level ?? 1) + 2));
      const H = `h${level}` as "h3";
      return (
        <H id={b.id !== undefined ? `h${b.id}` : undefined} className={`enc-heading enc-heading-${b.level ?? 1} enc-align-${b.align ?? "left"}`}>
          <Inlines c={b.c} opts={useBody().opts} />
        </H>
      );
    }
    case "rule":
      return <hr className="enc-rule" />;
    case "bib":
      return <Bib b={b} />;
    case "section":
      return <Section s={b} />;
    case "media":
      return null;
    default:
      return null;
  }
}

function Bib({ b }: { b: BibBlock }) {
  const { opts } = useBody();
  const part = (c: Inline[] | undefined, cls: string, after = " ") =>
    c && c.length ? (
      <>
        <span className={cls}>
          <Inlines c={c} opts={opts} />
        </span>
        {after}
      </>
    ) : null;
  return (
    <div className="enc-bib">
      <p className="enc-bib-ref">
        {part(b.author, "enc-bib-author")}
        {b.title && b.title.length > 0 && (
          <>
            <em className="enc-bib-title">
              <Inlines c={b.title} opts={opts} />
            </em>{" "}
          </>
        )}
        {part(b.series, "enc-bib-series")}
        {part(b.edition, "enc-bib-edition")}
        {part(b.volumes, "enc-bib-volumes")}
        {part(b.publisher, "enc-bib-publisher", b.date ? ", " : "")}
        {part(b.date, "enc-bib-date", "")}
      </p>
      {b.annotation && b.annotation.length > 0 && (
        <p className="enc-bib-annotation">
          <Inlines c={b.annotation} opts={opts} />
        </p>
      )}
    </div>
  );
}

function SectionTitle({ s, level }: { s: SectionBlock; level: number }) {
  const { opts } = useBody();
  const title = s.title ?? "";
  const text = typeof title === "string" ? title : <Inlines c={title} opts={opts} />;
  const H = `h${Math.min(6, level + 1)}` as "h2";
  const upper = s.kind === "introduction" || s.kind === "major";
  // The number is the full path ("III", "A", "1"); only the last label is printed.
  const num = Array.isArray(s.number) ? s.number[s.number.length - 1] : s.number;
  return (
    <H className={`enc-section-title${upper ? " upper" : ""}`}>
      {num && <span className="enc-secnum">{num}.</span>} {text}
    </H>
  );
}

function Section({ s }: { s: SectionBlock }) {
  const ctx = useBody();
  const collapsed = ctx.collapsed.has(s.id);
  const level = s.level ?? 1;
  const childSections = s.blocks.filter((b): b is SectionBlock => b.t === "section");
  const preview = collapsed && ctx.previewSection === s.id ? firstParagraph(s.blocks) : undefined;
  return (
    <section className={`enc-section enc-${s.kind ?? "section"} enc-level-${level}${collapsed ? " collapsed" : ""}`} id={`s${s.id}`} data-section={s.id}>
      <div className="enc-section-head">
        <button
          type="button"
          className="enc-toggle"
          aria-expanded={!collapsed}
          aria-label={collapsed ? "Expand section" : "Collapse section"}
          onClick={() => ctx.toggle(s.id)}
        >
          <Icon name={collapsed ? "plus" : "minus"} size={12} />
        </button>
        <SectionTitle s={s} level={level} />
      </div>
      {!collapsed && <Blocks blocks={s.blocks} />}
      {collapsed && preview && "c" in preview && (
        <div className="enc-preview">
          <p className="enc-p enc-clamp">
            <Inlines c={preview.c as Inline[]} opts={ctx.opts} />
          </p>
          <button type="button" className="enc-more" onClick={() => ctx.toggle(s.id)}>
            more…
          </button>
        </div>
      )}
      {collapsed && childSections.map((c) => <Section key={c.id} s={c} />)}
    </section>
  );
}
