import { Link, useSearchParams } from "react-router-dom";
import { api } from "../api/client";
import type { SearchHit } from "../api/types";
import { useApi } from "../api/useApi";
import { Html } from "../components/Html";
import { Icon, mediaIcon } from "../components/Icons";
import { kidsUrl, SearchBox } from "../components/SearchBox";
import { useDocumentTitle } from "../components/useDocumentTitle";
import { fileUrl, mediaClass } from "../media/pick";

export const SEARCH_TABS: { kind: string; label: string }[] = [
  { kind: "", label: "All" },
  { kind: "article", label: "Articles" },
  { kind: "media", label: "Multimedia" },
  { kind: "dictionary", label: "Dictionary" },
  { kind: "timeline", label: "Timeline" },
  { kind: "map", label: "Maps" },
  { kind: "quotation", label: "Quotations" },
  { kind: "sidebar", label: "Sidebars" },
  { kind: "web", label: "Web Links" },
  { kind: "other", label: "Other" },
];

const PAGE = 20;

export function SearchPage({ kids = false }: { kids?: boolean }) {
  const [params, setParams] = useSearchParams();
  const q = (params.get("q") ?? "").trim();
  const kind = params.get("kind") ?? "";
  const offset = Number(params.get("offset") ?? 0) || 0;
  useDocumentTitle(q ? `Search: ${q}` : "Search");
  const res = useApi(
    q ? (s) => api.search({ q, kind: kind || undefined, edition: kids ? "kids" : undefined, limit: PAGE, offset }, s) : null,
    [q, kind, offset, kids],
  );
  const spell = useApi(q && res.data && res.data.total === 0 && !res.data.did_you_mean ? (s) => api.spell({ q, edition: kids ? "kids" : undefined }, s) : null, [q, res.data]);
  const data = res.data;
  const facets = data?.facets?.kind ?? {};
  const allCount = Object.values(facets).reduce((a, b) => a + b, 0);
  const suggestion = data?.did_you_mean && data.did_you_mean.toLowerCase() !== q.toLowerCase() ? data.did_you_mean : spell.data?.corrected && spell.data.corrected !== q ? spell.data.corrected : null;

  const set = (p: Record<string, string | null>) => {
    const n = new URLSearchParams(params);
    for (const [k, v] of Object.entries(p)) {
      if (v === null || v === "") n.delete(k);
      else n.set(k, v);
    }
    setParams(n);
  };

  return (
    <div className={`search${kids ? " kids" : ""}`}>
      {!q && (
        <div className="search-empty">
          <h1>Search Encarta</h1>
          <SearchBox kids={kids} autoFocus />
        </div>
      )}
      {q && (
        <>
          <h1 className="search-title">
            Results for <q>{q}</q>
            {data && <span className="search-total">{data.total.toLocaleString("en-US")} found</span>}
          </h1>
          {suggestion && (
            <p className="search-dym">
              Did you mean{" "}
              <Link to={`${kids ? "/kids" : ""}/search?q=${encodeURIComponent(suggestion)}`}>
                <strong>{suggestion}</strong>
              </Link>
              ?
            </p>
          )}
          {data?.mode === "any" && data.total > 0 && <p className="search-note">No item contains all of your words; showing items that contain any of them.</p>}
          {!kids && (
            <div className="search-tabs" role="tablist">
              {SEARCH_TABS.filter((t) => t.kind === "" || (facets[t.kind] ?? 0) > 0 || t.kind === kind).map((t) => (
                <button
                  key={t.kind}
                  type="button"
                  role="tab"
                  aria-selected={kind === t.kind}
                  className={kind === t.kind ? "current" : ""}
                  onClick={() => set({ kind: t.kind || null, offset: null })}
                >
                  {t.label}
                  <span className="search-tab-count">{t.kind === "" ? allCount || "" : facets[t.kind] ?? 0}</span>
                </button>
              ))}
            </div>
          )}
          {res.error && <p className="enc-error">Search is not available ({res.error.message}).</p>}
          {res.loading && !data && <div className="enc-spinner" />}
          {data && data.hits.length === 0 && <p className="search-none">No results. Check the spelling or try other words.</p>}
          <ol className="search-hits" start={offset + 1}>
            {data?.hits.map((h) => <Hit key={`${h.ref_id}-${h.url}`} h={h} kids={kids} />)}
          </ol>
          {data && data.total > PAGE && (
            <div className="search-pager">
              <button type="button" disabled={offset === 0} onClick={() => set({ offset: String(Math.max(0, offset - PAGE)) })}>
                Previous
              </button>
              <span>
                {offset + 1}–{Math.min(data.total, offset + PAGE)} of {data.total.toLocaleString("en-US")}
              </span>
              <button type="button" disabled={offset + PAGE >= data.total} onClick={() => set({ offset: String(offset + PAGE) })}>
                Next
              </button>
            </div>
          )}
        </>
      )}
    </div>
  );
}

const CATEGORY: Record<string, string> = {
  article: "Article",
  media: "Multimedia",
  dictionary: "Dictionary",
  timeline: "Timeline",
  map: "Map",
  quotation: "Quotation",
  sidebar: "Sidebar",
  web: "Web Link",
  other: "Other",
};

function Hit({ h, kids }: { h: SearchHit; kids: boolean }) {
  const cls = mediaClass({ kind: h.kind, media_type: h.media_type });
  return (
    <li className={`search-hit search-hit-${h.category}`}>
      <span className="search-hit-icon">
        {h.thumb ? <img src={fileUrl(h.thumb)} alt="" width={40} height={40} /> : <Icon name={h.category === "article" ? "article" : h.category === "dictionary" ? "book" : h.category === "timeline" ? "clock" : mediaIcon(cls)} size={20} />}
      </span>
      <div className="search-hit-body">
        <Link className="search-hit-title" to={kidsUrl(h.url, kids)}>
          <Html as="span" html={h.title_html ?? h.title} />
        </Link>
        {h.para?.section_title && <span className="search-hit-section"> › {h.para.section_title}</span>}
        <span className="search-hit-cat">{CATEGORY[h.category] ?? h.category}</span>
        {h.snippet_html && <Html className="search-hit-snippet" html={h.snippet_html} />}
      </div>
    </li>
  );
}
