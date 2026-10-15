import { useEffect, useId, useRef, useState, type KeyboardEvent as ReactKeyboardEvent, type PointerEvent as ReactPointerEvent } from "react";
import { Link, useNavigate } from "react-router-dom";
import { api } from "../api/client";
import { useApi } from "../api/useApi";
import { Html } from "../components/Html";
import { Icon, mediaIcon, type IconName } from "../components/Icons";
import { fileUrl, mediaClass } from "../media/pick";
import { useSettings } from "../settings/settings";
import type { SearchHit } from "../api/types";
import type { QuickKind } from "./featuresData";
import { PANEL_CATEGORIES, PANEL_WIDTH, QUICK_LABEL, clampPanelWidth, panelCategories } from "./searchPanel";

/** What the panel shows: a query, optionally only one category (Quick Search). */
export interface PanelQuery {
  q: string;
  only?: QuickKind;
}

const LIMIT = 25;
const BEST = "best";

/** The panel width the reader dragged to, kept for the session. */
let panelWidth: number = PANEL_WIDTH.initial;

/** One hit: 56 px row, 34 px picture, bold title, snippet under it (the Articles list shows titles only). */
function HitRow({ h, selected, snippet, onPick }: { h: SearchHit; selected: boolean; snippet: boolean; onPick: () => void }) {
  const cls = mediaClass({ kind: h.kind, media_type: h.media_type });
  const icon: IconName = h.category === "article" ? "article" : h.category === "dictionary" ? "book" : h.category === "map" ? "map" : mediaIcon(cls);
  return (
    <li>
      <Link to={h.url} onClick={onPick} className={selected ? "sel" : undefined} aria-current={selected ? "true" : undefined}>
        <span className="sp-thumb">{h.thumb ? <img src={fileUrl(h.thumb)} alt="" width={34} height={34} /> : <Icon name={icon} size={20} />}</span>
        <span className="sp-hit-text">
          <Html as="span" className="sp-hit-title" html={h.title_html ?? h.title} />
          {snippet && h.snippet_html && <Html as="span" className="sp-snippet" html={h.snippet_html} />}
        </span>
      </Link>
    </li>
  );
}

function Bar({
  id,
  label,
  kind,
  all,
  icon,
  q,
  open,
  onToggle,
  onPick,
  onBest,
}: {
  id: string;
  label: string;
  kind: string;
  all: string;
  icon: IconName;
  q: string;
  open: boolean;
  onToggle: () => void;
  onPick: () => void;
  onBest?: (url: string) => void;
}) {
  const res = useApi((s) => api.search({ q, kind: kind || undefined, limit: LIMIT }, s), [q, kind]);
  const data = res.data;
  const body = useId();
  useEffect(() => {
    if (onBest && data?.hits[0]) onBest(data.hits[0].url);
  }, [data]); // eslint-disable-line react-hooks/exhaustive-deps
  const count = data ? data.total : null;
  // VM: the first hit starts highlighted; a click moves the highlight.
  const [selected, setSelected] = useState(0);
  useEffect(() => setSelected(0), [data]);
  return (
    <section className={`sp-cat${open ? " open" : ""}`} data-cat={id}>
      <h3>
        <button type="button" aria-expanded={open} aria-controls={body} onClick={onToggle}>
          <Icon name={icon} size={18} className="sp-cat-icon" />
          <span>
            {label}
            {count !== null && ` (${count.toLocaleString("en-US")})`}
          </span>
          <span className="sp-toggle" aria-hidden="true">
            {open ? "−" : "+"}
          </span>
        </button>
      </h3>
      {open && (
        <div className="sp-body" id={body}>
          {res.loading && !data && <div className="enc-spinner" />}
          {res.error && <p className="sp-none">Search is not available.</p>}
          {data && data.hits.length === 0 && (
            <div className="sp-none">
              <p>No results were found in {label}</p>
              <p>Check the spelling or try a new search.</p>
            </div>
          )}
          {data && data.hits.length > 0 && (
            <ul className="sp-hits">
              {data.hits.map((h, i) => (
                <HitRow key={`${h.ref_id}-${h.url}`} h={h} selected={i === selected} snippet={id !== "article"} onPick={() => setSelected(i)} />
              ))}
            </ul>
          )}
          {data && kind && data.total > data.hits.length && (
            <p className="sp-more">
              <Link to={`/search?q=${encodeURIComponent(q)}&kind=${kind}`} onClick={onPick}>
                Search all {all}
              </Link>
            </p>
          )}
        </div>
      )}
    </section>
  );
}

/**
 * The results panel docked at the right of the window (VM: Best Matches, Articles, Photos & More,
 * Web Links, Dictionaries, Maps; one category open at a time; two check boxes at the bottom; a
 * splitter at the left edge sets the width between 308 and 498 px).
 */
export function SearchPanel({ query, onClose, id, top }: { query: PanelQuery | null; onClose: () => void; id: string; top?: number }) {
  const navigate = useNavigate();
  const { settings, update } = useSettings();
  const only = query?.only;
  const [openCat, setOpenCat] = useState<string>(BEST);
  const auto = useRef<string>("");
  // VM: a new search keeps the category that was open last; a Quick Search opens its own.
  useEffect(() => {
    if (query?.only) setOpenCat(PANEL_CATEGORIES.find((c) => c.id === query.only)?.id ?? BEST);
  }, [query?.q, query?.only]); // eslint-disable-line react-hooks/exhaustive-deps
  const bestMatch = (url: string) => {
    if (!settings.autoBestMatch || !query || auto.current === query.q) return;
    auto.current = query.q;
    navigate(url);
  };
  // VM: opening a hit leaves the panel open (it hides on a click in the page unless "keep open" is set);
  // the "Search all" links lead to the full results page, so they close it.
  const pick = () => {
    if (!settings.keepSearchOpen) onClose();
  };
  const cats = panelCategories(only);
  const toggle = (c: string) => setOpenCat(c);
  const [width, setWidth] = useState(panelWidth);
  const drag = (e: ReactPointerEvent<HTMLDivElement>) => {
    e.preventDefault();
    const startX = e.clientX;
    const startW = width;
    const move = (m: PointerEvent) => {
      panelWidth = clampPanelWidth(startW + startX - m.clientX);
      setWidth(panelWidth);
    };
    const up = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
  };
  const nudge = (e: ReactKeyboardEvent<HTMLDivElement>) => {
    const by = e.key === "ArrowLeft" ? 10 : e.key === "ArrowRight" ? -10 : 0;
    if (!by) return;
    e.preventDefault();
    panelWidth = clampPanelWidth(width + by);
    setWidth(panelWidth);
  };
  return (
    <aside className="search-panel" id={id} aria-label="Search results" style={{ ...(top ? { top } : {}), width: `min(${width}px, 100vw)` }}>
      <div
        className="sp-splitter"
        role="separator"
        aria-orientation="vertical"
        aria-label="Resize search results"
        aria-valuemin={PANEL_WIDTH.min}
        aria-valuemax={PANEL_WIDTH.max}
        aria-valuenow={width}
        tabIndex={0}
        onPointerDown={drag}
        onKeyDown={nudge}
      />
      {!query && <p className="sp-hint">Type a search term and press Enter.</p>}
      {query && (
        <>
          <h2 className="sp-title">
            {only ? QUICK_LABEL[only] : "Search"}: <q>{query.q}</q>
          </h2>
          <div className="sp-cats">
            {!only && <Bar id={BEST} label="Best Matches" kind="" all="" icon="search" q={query.q} open={openCat === BEST} onToggle={() => toggle(BEST)} onPick={pick} onBest={bestMatch} />}
            {cats.map((c) => (
              <Bar key={c.id} id={c.id} label={c.label} kind={c.kind} all={c.all} icon={c.icon} q={query.q} open={openCat === c.id} onToggle={() => toggle(c.id)} onPick={pick} />
            ))}
          </div>
        </>
      )}
      <div className="sp-options">
        <label>
          <input type="checkbox" checked={settings.autoBestMatch} onChange={(e) => update({ autoBestMatch: e.target.checked })} /> Automatically display best match
        </label>
        <label>
          <input type="checkbox" checked={settings.keepSearchOpen} onChange={(e) => update({ keepSearchOpen: e.target.checked })} /> Keep search results list open
        </label>
      </div>
    </aside>
  );
}

