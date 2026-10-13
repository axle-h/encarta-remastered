import { useCallback, useEffect, useRef, useState } from "react";
import { Link, useLocation } from "react-router-dom";
import { api } from "../api/client";
import type { Article, AssocItem } from "../api/types";
import { useApi } from "../api/useApi";
import { Html } from "../components/Html";
import { Icon, mediaIcon } from "../components/Icons";
import { useDocumentTitle } from "../components/useDocumentTitle";
import type { SectionBlock } from "../content/model";
import { useMediaViewer } from "../media/MediaViewer";
import { fileUrl, mediaClass } from "../media/pick";
import { ArticleBody, type BodyContext } from "./ArticleBody";
import { FindBar } from "./FindBar";
import { sectionPathOf } from "./layout";
import { useArticleModel } from "./useArticleModel";
import { Pronunciation } from "./Pronunciation";
import { SequenceContext } from "./sequence";
import { AtlasPanel, LinksPanel, QuickFacts, RelatedPanel } from "./SidePanels";

export function ArticlePage({ id, kids = false }: { id: number; kids?: boolean }) {
  const res = useApi((s) => api.article(id, kids ? "kids" : undefined, s), [id, kids]);
  const a = res.data;
  useDocumentTitle(a?.title);
  if (res.error) return <div className="enc-page-error">Could not load the article ({res.error.message}).</div>;
  if (!a) return <div className="enc-loading"><div className="enc-spinner" /></div>;
  return <ArticleView key={a.ref_id} article={a} kids={kids} />;
}

function ArticleView({ article: a, kids }: { article: Article; kids: boolean }) {
  const m = useArticleModel(a);
  const location = useLocation();
  const viewer = useMediaViewer();
  const [collapsed, setCollapsed] = useState<Set<number>>(new Set());
  const [findOpen, setFindOpen] = useState(false);
  const textRef = useRef<HTMLDivElement>(null);
  const outline = collapsed.size > 0 && collapsed.size === m.sections.length;

  const toggle = useCallback((sid: number) => {
    setCollapsed((c) => {
      const n = new Set(c);
      if (n.has(sid)) n.delete(sid);
      else n.add(sid);
      return n;
    });
  }, []);

  const showOutline = () => setCollapsed(new Set(m.sections.map((s) => s.section.id)));
  const showFull = () => setCollapsed(new Set());

  // Deep links: `#p12` (paragraph), `#s4` (section). Expand the path, then scroll and flash.
  useEffect(() => {
    const h = location.hash;
    const pm = /^#p(\d+)$/.exec(h);
    if (pm) {
      const path = sectionPathOf(m.body, Number(pm[1]));
      if (path && path.some((s) => collapsed.has(s))) {
        setCollapsed((c) => {
          const n = new Set(c);
          path.forEach((s) => n.delete(s));
          return n;
        });
      }
    }
    if (!h) {
      window.scrollTo(0, 0);
      return;
    }
    const t = window.setTimeout(() => {
      const el = document.getElementById(h.slice(1));
      if (el) {
        el.scrollIntoView({ block: "start" });
        el.classList.add("enc-flash");
        window.setTimeout(() => el.classList.remove("enc-flash"), 1800);
      }
    }, 30);
    return () => window.clearTimeout(t);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [location.hash, a.ref_id]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "f") {
        e.preventDefault();
        setFindOpen(true);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  const firstSection = m.body.find((b): b is SectionBlock => b.t === "section");
  const ctx: BodyContext = {
    assoc: m.assoc,
    tables: m.tables,
    opts: { kids },
    collapsed,
    toggle,
    previewSection: firstSection?.id ?? null,
    mediaColumn: true,
    sequence: m.sequence,
  };

  const maps = m.media.filter((x) => x.group === "maps");
  const further = m.media.filter((x) => x.group === "furtherread");
  const experts = m.media.filter((x) => x.group === "expert");
  const hasSections = m.sections.length > 0;

  return (
    <SequenceContext.Provider value={m.sequence}>
      <div className="art">
        <header className="art-head">
          <h1 className="art-title">
            <Html as="span" html={a.title_html ?? a.title} />
            <Pronunciation title={a.title} />
          </h1>
        </header>
        {m.sequence.length > 0 && (
          <div className="art-strip" role="list" aria-label="Media in this article">
            {m.sequence.map((x) => (
              <button
                type="button"
                role="listitem"
                key={x.ref_id}
                className={`art-strip-item art-strip-${mediaClass(x)}`}
                onClick={() => viewer.open(x.ref_id, m.sequence)}
                title={x.title}
              >
                {x.thumb ? <img src={fileUrl(x.thumb)} alt={x.title ?? ""} width={64} height={64} /> : <Icon name={mediaIcon(mediaClass(x))} size={28} />}
              </button>
            ))}
          </div>
        )}
        <div className="art-tools">
          {hasSections && (
            <>
              <button type="button" className="art-tool" disabled={outline} onClick={showOutline}>
                <span className="art-tool-icon"><Icon name="outline" size={12} /></span>
                Show <u>O</u>utline
              </button>
              <button type="button" className="art-tool" disabled={collapsed.size === 0} onClick={showFull}>
                <span className="art-tool-icon"><Icon name="article" size={12} /></span>
                Show <u>F</u>ull Article
              </button>
            </>
          )}
          <button type="button" className="art-tool" onClick={() => setFindOpen(true)}>
            <span className="art-tool-icon"><Icon name="find" size={12} /></span>
            Find in Article (Ctrl+F)
          </button>
          {m.sequence.length > 0 && (
            <Link className="art-allmedia" to={`${kids ? "/kids" : ""}/item/${a.ref_id}/media`}>
              <span className="art-tool-icon"><Icon name="grid" size={12} /></span>
              Show All <u>M</u>edia
            </Link>
          )}
          {findOpen && <FindBar root={textRef} onClose={() => setFindOpen(false)} />}
        </div>
        <div className="art-cols">
          <div className="art-text" ref={textRef}>
            <ArticleBody blocks={m.body} ctx={ctx} />
            {experts.length > 0 && <Contributors items={experts} />}
          </div>
          <aside className="art-side">
            {maps[0] && <AtlasPanel map={maps[0]} />}
            {m.right.map((t) => (
              <QuickFacts key={t.ref_id} table={t} sequence={m.sequence} kids={kids} />
            ))}
            {a.related && a.related.length > 0 && <RelatedPanel tree={a.related} kids={kids} />}
            {further.length > 0 && <LinksPanel title="Further Reading" items={further} kids={kids} />}
          </aside>
        </div>
      </div>
    </SequenceContext.Provider>
  );
}

function Contributors({ items }: { items: AssocItem[] }) {
  return (
    <div className="art-contrib">
      {items.map((x) => (
        <Contributor key={x.ref_id} id={x.ref_id} />
      ))}
    </div>
  );
}

function Contributor({ id }: { id: number }) {
  const it = useApi((s) => api.item(id, s), [id]);
  if (!it.data) return null;
  const meta = it.data.meta ?? {};
  const cred = flat(meta.credentials) || flat(meta.info);
  return (
    <p className="art-contrib-line">
      <span className="art-contrib-label">Contributed By:</span>
      <br />
      {it.data.title}
      {cred && <>, {cred}</>}
    </p>
  );
}

/** Flattens a meta value that may be a string, inline array or nested structure into text. */
function flat(v: unknown): string {
  if (typeof v === "string") return v;
  if (Array.isArray(v)) return v.map(flat).join("");
  if (v && typeof v === "object" && "c" in v) return flat((v as { c: unknown }).c);
  return "";
}
