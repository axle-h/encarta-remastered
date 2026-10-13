import { useState } from "react";
import { Link, useParams, useSearchParams } from "react-router-dom";
import { api } from "../api/client";
import type { AssocItem, SearchHit } from "../api/types";
import { useApi } from "../api/useApi";
import { Html } from "../components/Html";
import { Icon, mediaIcon } from "../components/Icons";
import { useDocumentTitle } from "../components/useDocumentTitle";
import { useArticleModel } from "../article/useArticleModel";
import { useMediaViewer } from "../media/MediaViewer";
import { fileUrl, mediaClass } from "../media/pick";

const TYPES: { kind: string; label: string }[] = [
  { kind: "media", label: "All Multimedia" },
  { kind: "video", label: "Videos" },
  { kind: "audio", label: "Sounds" },
  { kind: "audio-set", label: "Sound Sets" },
  { kind: "interactive", label: "Interactivities" },
  { kind: "table", label: "Charts & Tables" },
  { kind: "panorama", label: "Panoramas" },
  { kind: "map", label: "Maps" },
];

const PAGE = 48;

function hitToAssoc(h: SearchHit): AssocItem {
  return { ref_id: h.ref_id, group: h.category, kind: h.kind, media_type: h.media_type, title: h.title, title_html: h.title_html, thumb: h.thumb ?? null };
}

/** A grid of media tiles that open the viewer on the whole grid. */
export function MediaGrid({ items }: { items: AssocItem[] }) {
  const viewer = useMediaViewer();
  return (
    <ul className="mgrid">
      {items.map((m) => {
        const cls = mediaClass(m);
        return (
          <li key={m.ref_id}>
            <button type="button" className={`mgrid-tile mgrid-${cls}`} onClick={() => viewer.open(m.ref_id, items)}>
              <span className="mgrid-pic">
                {m.thumb ? <img src={fileUrl(m.thumb)} alt="" loading="lazy" /> : <Icon name={mediaIcon(cls)} size={32} />}
                {cls !== "image" && (
                  <span className={`enc-badge enc-badge-${cls}`}>
                    <Icon name={mediaIcon(cls)} size={12} />
                  </span>
                )}
              </span>
              <span className="mgrid-title">
                <Html as="span" html={m.title_html ?? m.title ?? ""} />
              </span>
            </button>
          </li>
        );
      })}
    </ul>
  );
}

/** `/media`: the media gallery, a search restricted to multimedia, by type. */
export function MediaBrowserPage() {
  const [params, setParams] = useSearchParams();
  const q = (params.get("q") ?? "").trim();
  const kind = params.get("type") ?? "media";
  const offset = Number(params.get("offset") ?? 0) || 0;
  const [draft, setDraft] = useState(q);
  useDocumentTitle("Media Gallery");
  const res = useApi(q ? (s) => api.search({ q, kind, limit: PAGE, offset }, s) : null, [q, kind, offset]);
  const set = (p: Record<string, string | null>) => {
    const n = new URLSearchParams(params);
    for (const [k, v] of Object.entries(p)) {
      if (v === null || v === "") n.delete(k);
      else n.set(k, v);
    }
    setParams(n);
  };
  const items = res.data?.hits.map(hitToAssoc) ?? [];
  return (
    <div className="mbrowse">
      <h1 className="page-title">Media Gallery</h1>
      <form
        className="mbrowse-form"
        onSubmit={(e) => {
          e.preventDefault();
          set({ q: draft.trim(), offset: null });
        }}
      >
        <input value={draft} onChange={(e) => setDraft(e.target.value)} placeholder="Find pictures, sounds, videos…" aria-label="Find media" />
        <button type="submit" className="enc-button">Find</button>
      </form>
      <div className="search-tabs" role="tablist">
        {TYPES.map((t) => (
          <button key={t.kind} type="button" role="tab" aria-selected={kind === t.kind} className={kind === t.kind ? "current" : ""} onClick={() => set({ type: t.kind === "media" ? null : t.kind, offset: null })}>
            {t.label}
          </button>
        ))}
      </div>
      {!q && (
        <p className="enc-muted">
          Type a subject to see its pictures, sounds, videos and interactive media. Try{" "}
          {["volcano", "jazz", "space shuttle", "birds"].map((w, i) => (
            <span key={w}>
              {i > 0 && ", "}
              <Link to={`/media?q=${encodeURIComponent(w)}`}>{w}</Link>
            </span>
          ))}
          .
        </p>
      )}
      {res.error && <p className="enc-error">Search is not available ({res.error.message}).</p>}
      {res.loading && !res.data && <div className="enc-spinner" />}
      {res.data && res.data.hits.length === 0 && <p>No media found.</p>}
      {items.length > 0 && <MediaGrid items={items} />}
      {res.data && res.data.total > PAGE && (
        <div className="search-pager">
          <button type="button" disabled={offset === 0} onClick={() => set({ offset: String(Math.max(0, offset - PAGE)) })}>
            Previous
          </button>
          <span>
            {offset + 1}–{Math.min(res.data.total, offset + PAGE)} of {res.data.total.toLocaleString("en-US")}
          </span>
          <button type="button" disabled={offset + PAGE >= res.data.total} onClick={() => set({ offset: String(offset + PAGE) })}>
            Next
          </button>
        </div>
      )}
    </div>
  );
}

/** `/item/{id}/media`: "Show All Media" for one article. */
export function ArticleMediaPage({ kids = false }: { kids?: boolean }) {
  const id = Number(useParams().id);
  const res = useApi((s) => api.article(id, kids ? "kids" : undefined, s), [id, kids]);
  const m = useArticleModel(res.data);
  useDocumentTitle(res.data ? `${res.data.title}: All Media` : null);
  if (res.error) return <div className="enc-page-error">Could not load the article ({res.error.message}).</div>;
  if (!res.data) return <div className="enc-loading"><div className="enc-spinner" /></div>;
  return (
    <div className={`mbrowse${kids ? " kids" : ""}`}>
      <h1 className="art-title">
        <Html as="span" html={res.data.title_html ?? res.data.title} />
      </h1>
      <p className="mbrowse-back">
        <Link to={`${kids ? "/kids" : ""}/item/${id}`}>
          <Icon name="back" size={14} /> Back to the article
        </Link>
      </p>
      <MediaGrid items={m.sequence} />
    </div>
  );
}
