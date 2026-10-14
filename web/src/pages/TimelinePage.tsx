import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { Link, useSearchParams } from "react-router-dom";
import { api } from "../api/client";
import type { TimelineEvent } from "../api/types";
import { useApi } from "../api/useApi";
import { Html } from "../components/Html";
import { Icon } from "../components/Icons";
import { useDocumentTitle } from "../components/useDocumentTitle";
import { eventDateText, yearLabel } from "./timelineDates";
import { categoryIndex, displayOf, ERAS, pan, placeEvents, prominenceOf, ticks, titleSize, topicWindow, zoom, type Placed } from "./timelineLayout";

const LANE_H = 30;
const BAND_LIMIT = 400;
const LIST_PAGE = 200;

/** Encarta Timeline: a zoomable, pannable band of events with category and region filters. */
export function TimelinePage() {
  useDocumentTitle("Timeline");
  const [params, setParams] = useSearchParams();
  const meta = useApi((s) => api.timelineMeta(s), []);
  const min = meta.data?.min_year ?? -4_700_000_000;
  const max = meta.data?.max_year ?? 2010;
  const year = params.get("year");
  const initial = (): [number, number] => {
    const f = Number(params.get("from"));
    const t = Number(params.get("to"));
    if (params.has("from") && params.has("to") && t > f) return [f, t];
    if (year !== null && Number.isFinite(Number(year))) {
      const y = Number(year);
      const half = Math.abs(y) > 100_000 ? Math.abs(y) / 4 : 50;
      return [y - half, y + half];
    }
    return [1900, 2010];
  };
  const [[from, to], setWin] = useState<[number, number]>(initial);
  const [category, setCategory] = useState(params.get("category") ?? "");
  const [region, setRegion] = useState(params.get("region") ?? "");
  const [text, setText] = useState(params.get("q") ?? "");
  const [q, setQ] = useState(text);
  // A topic timeline (`?term=`, timeline.md §5): only that term's events, the window fitted to them.
  const [term, setTerm] = useState(params.get("term") ?? "");
  const topic = useApi(term ? (s) => api.timeline({ term, limit: 5000 }, s) : null, [term]);
  const fitted = useRef("");
  // Whether the address named a window when the page opened (the effect below writes one at once).
  const explicitWindow = useRef(params.has("from") && params.has("to"));
  useEffect(() => {
    const evs = topic.data?.events ?? [];
    if (!term || fitted.current === term || evs.length === 0) return;
    fitted.current = term;
    if (explicitWindow.current) {
      explicitWindow.current = false;
      return;
    }
    setWin(topicWindow(evs, min, max));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [topic.data, term]);
  const [selected, setSelected] = useState<TimelineEvent | null>(null);
  const bandRef = useRef<HTMLDivElement>(null);
  const [width, setWidth] = useState(900);

  useLayoutEffect(() => {
    const el = bandRef.current;
    if (!el) return;
    const ro = new ResizeObserver(() => setWidth(el.clientWidth));
    ro.observe(el);
    setWidth(el.clientWidth);
    return () => ro.disconnect();
  }, []);

  useEffect(() => {
    const t = window.setTimeout(() => setQ(text.trim()), 250);
    return () => window.clearTimeout(t);
  }, [text]);

  useEffect(() => {
    const p = new URLSearchParams();
    p.set("from", String(from));
    p.set("to", String(to));
    if (category) p.set("category", category);
    if (region) p.set("region", region);
    if (q) p.set("q", q);
    if (term) p.set("term", term);
    setParams(p, { replace: true });
  }, [from, to, category, region, q, term, setParams]);

  const filters = { from, to, category: category || undefined, region: region || undefined, q: q || undefined, term: term || undefined };
  // The band shows the window's most important events (one request, `order=importance`).
  const band = useApi((s) => api.timeline({ ...filters, order: "importance", limit: BAND_LIMIT, text: true }, s), [from, to, category, region, q, term]);
  // The list below pages through every event of the window in date order.
  const [listOffset, setListOffset] = useState(0);
  useEffect(() => {
    setListOffset(0);
  }, [from, to, category, region, q, term]);
  const list = useApi((s) => api.timeline({ ...filters, limit: LIST_PAGE, offset: listOffset, text: true }, s), [from, to, category, region, q, term, listOffset]);
  const res = list;
  const events = list.data?.events ?? [];
  const lanes = 12;
  const { placed, hidden } = placeEvents(band.data?.events ?? [], from, to, width, 170, lanes);

  const set = (w: [number, number]) => {
    setWin(w);
    setSelected(null);
  };
  const onWheel = (e: React.WheelEvent) => {
    if (Math.abs(e.deltaX) > Math.abs(e.deltaY)) set(pan(from, to, e.deltaX / Math.max(1, width), min, max));
  };

  return (
    <div className="tl">
      <div className="tl-head">
        <h1 className="page-title">Encarta Timeline</h1>
        {term && (
          <p className="tl-topic">
            Topic: <strong>{term}</strong> ({topic.data ? `${topic.data.total} events` : "…"}){" "}
            <button type="button" className="enc-panel-link" onClick={() => setTerm("")}>
              Show all events
            </button>
          </p>
        )}
        <div className="tl-eras" role="tablist" aria-label="Eras">
          {ERAS.map((e) => {
            const on = from >= e.from && to <= e.to;
            return (
              <button key={e.label} type="button" className={on ? "current" : ""} onClick={() => set([e.from, e.to])}>
                {e.label}
              </button>
            );
          })}
        </div>
        <div className="tl-filters">
          <label>
            Category{" "}
            <select value={category} onChange={(e) => setCategory(e.target.value)}>
              <option value="">All</option>
              {meta.data?.categories.map((c) => (
                <option key={c.bit} value={c.name}>
                  {c.name}
                </option>
              ))}
            </select>
          </label>
          <label>
            Region{" "}
            <select value={region} onChange={(e) => setRegion(e.target.value)}>
              <option value="">All</option>
              {meta.data?.regions.map((r) => (
                <option key={r.bit} value={r.name}>
                  {r.name}
                </option>
              ))}
            </select>
          </label>
          <input type="search" value={text} onChange={(e) => setText(e.target.value)} placeholder="Find events" aria-label="Find events" />
        </div>
      </div>
      <div className="tl-controls">
        <button type="button" onClick={() => set(pan(from, to, -0.5, min, max))} aria-label="Earlier">
          <Icon name="back" size={16} />
        </button>
        <span className="tl-range">
          {yearLabel(from)} – {yearLabel(to)}
        </span>
        <button type="button" onClick={() => set(pan(from, to, 0.5, min, max))} aria-label="Later">
          <Icon name="forward" size={16} />
        </button>
        <button type="button" onClick={() => set(zoom(from, to, 0.5, min, max))} aria-label="Zoom in">
          <Icon name="plus" size={16} />
        </button>
        <button type="button" onClick={() => set(zoom(from, to, 2, min, max))} aria-label="Zoom out">
          <Icon name="minus" size={16} />
        </button>
        <span className="tl-count">
          {res.data ? `${res.data.total.toLocaleString("en-US")} events` : "…"}
          {band.data && res.data && band.data.events.length - hidden.length < res.data.total && " · the band shows the most important; zoom in for more, or see the list below"}
        </span>
      </div>
      <div className="tl-band" ref={bandRef} onWheel={onWheel} style={{ height: lanes * LANE_H + 40 }}>
        <div className="tl-axis">
          {ticks(from, to, Math.max(3, Math.floor(width / 110))).map((y) => (
            <span key={y} className="tl-tick" style={{ left: ((y - from) / (to - from)) * width }}>
              {yearLabel(y)}
            </span>
          ))}
        </div>
        {placed.map((p) => (
          <BandEvent key={p.ev.seq} p={p} top={30 + p.lane * LANE_H} current={selected?.seq === p.ev.seq} onSelect={setSelected} />
        ))}
        {band.loading && <div className="tl-loading enc-spinner" />}
      </div>
      <div className="tl-legend">
        {meta.data?.categories.map((c, i) => (
          <span key={c.bit} className={`tl-cat-${i}`}>
            <i /> {c.name}
          </span>
        ))}
      </div>
      <div className="tl-lower">
        {selected && <EventDetail ev={selected} onClose={() => setSelected(null)} />}
        <ol className="tl-list">
          {events.map((ev) => (
            <li key={ev.seq} className={selected?.seq === ev.seq ? "current" : ""}>
              <button type="button" onClick={() => setSelected(ev)}>
                <span className="tl-list-date">{eventDateText(ev)}</span>
                <span className={`tl-dot tl-cat-${categoryIndex(ev.category_mask)}`} />
                <Html as="span" html={ev.title_html ?? ev.title} />
              </button>
            </li>
          ))}
        </ol>
      </div>
      {res.data && res.data.total > LIST_PAGE && (
        <div className="search-pager">
          <button type="button" disabled={listOffset === 0} onClick={() => setListOffset(Math.max(0, listOffset - LIST_PAGE))}>
            Earlier
          </button>
          <span>
            {listOffset + 1}–{Math.min(res.data.total, listOffset + LIST_PAGE)} of {res.data.total.toLocaleString("en-US")}
          </span>
          <button type="button" disabled={listOffset + LIST_PAGE >= res.data.total} onClick={() => setListOffset(listOffset + LIST_PAGE)}>
            Later
          </button>
        </div>
      )}
    </div>
  );
}

/**
 * One event on the band, drawn as the VM does (timeline.md §3.2): a point is a square and its title,
 * sized by prominence; a period an orange bar from start to end with a boxed upper-case label; an
 * essay a two-tone striped bar labelled "HISTORICAL ESSAY: …".
 */
function BandEvent({ p, top, current, onSelect }: { p: Placed; top: number; current: boolean; onSelect: (ev: TimelineEvent) => void }) {
  const ev = p.ev;
  const kind = displayOf(ev);
  const title = `${ev.title} (${eventDateText(ev)})`;
  if (kind !== "point" && p.w > 0) {
    const label = ev.label ?? (kind === "essay" ? `HISTORICAL ESSAY: ${ev.title}` : ev.title).toUpperCase();
    return (
      <button
        type="button"
        className={`tl-bar tl-${kind}${current ? " current" : ""}`}
        style={{ left: p.x, top, width: p.w }}
        onClick={() => onSelect(ev)}
        title={title}
      >
        <span className="tl-bar-line" />
        <span className="tl-bar-label" style={{ maxWidth: Math.max(p.w, 170) }}>
          {label}
        </span>
      </button>
    );
  }
  return (
    <button
      type="button"
      className={`tl-ev tl-cat-${categoryIndex(ev.category_mask)}${current ? " current" : ""}`}
      style={{ left: Math.max(0, p.x), top, maxWidth: p.lw, fontSize: titleSize(prominenceOf(ev)) }}
      onClick={() => onSelect(ev)}
      title={title}
    >
      <i className="tl-sq" />
      <Html as="span" html={ev.title_html ?? ev.title} />
    </button>
  );
}

function EventDetail({ ev, onClose }: { ev: TimelineEvent; onClose: () => void }) {
  const item = useApi(ev.ref_id ? (s) => api.item(ev.ref_id!, s) : null, [ev.ref_id]);
  // The event's text comes with the event; the article is fetched only for older servers.
  const text = useApi(ev.text === undefined && (ev.has_article ?? item.data?.has_article) ? (s) => api.article(ev.ref_id!, undefined, s) : null, [ev.seq, item.data]);
  const articles = (item.data?.media ?? []).filter((m) => m.kind === "article" || m.group === "article");
  return (
    <aside className="tl-detail">
      <button type="button" className="tl-detail-close" onClick={onClose} aria-label="Close">
        <Icon name="close" size={14} />
      </button>
      <p className="tl-detail-date">{eventDateText(ev)}</p>
      <h2>
        <Html as="span" html={ev.title_html ?? ev.title} />
      </h2>
      {ev.text ? (
        <div className="tl-detail-text">
          {ev.text.split("\n").map((t, i) => (
            <p key={i}>{t}</p>
          ))}
        </div>
      ) : text.data?.html ? (
        <Html className="tl-detail-text" html={text.data.html} />
      ) : (
        item.data?.preview_html && <Html className="tl-detail-text" html={item.data.preview_html} />
      )}
      {item.data?.caption_html && <Html className="tl-detail-text" html={item.data.caption_html} />}
      <p className="tl-detail-tags">
        {[...(ev.categories ?? []), ...(ev.regions ?? [])].join(" · ")}
      </p>
      {articles.length > 0 && (
        <ul className="tl-detail-links">
          {articles.map((a) => (
            <li key={a.ref_id}>
              <Link to={`/item/${a.ref_id}`}>{a.title}</Link>
            </li>
          ))}
        </ul>
      )}
    </aside>
  );
}
