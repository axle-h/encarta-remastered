/**
 * Item pages for the kinds that are neither articles nor media: web links, contributors,
 * quotation authors, 3D-tour nodes, Map Treks and the online-only features. Without them these
 * 40,000-odd items (all searchable) opened on "Nothing to display".
 */
import { Suspense, useState } from "react";
import { Link, Navigate, useSearchParams } from "react-router-dom";
import { api } from "../api/client";
import type { Item, ItemRef } from "../api/types";
import { useApi } from "../api/useApi";
import { AtlasLazy } from "../app/atlasModule";
import { ArticleInline } from "../article/ArticleInline";
import { Html } from "../components/Html";
import { useDocumentTitle } from "../components/useDocumentTitle";
import { Inlines } from "../content/Inlines";
import { itemHref } from "../content/links";
import type { Inline } from "../content/model";
import { tourIdFor, uniqueByRef } from "../media/pick";
import { ONLINE_FEATURES, mapTrekUrl } from "./itemRoute";
import { isPanoramicTour } from "./panorama";

/** First string of a DATA meta value (`["text"]`, `"text"` or inline content). */
export function metaText(v: unknown): string {
  if (typeof v === "string") return v;
  if (Array.isArray(v)) return v.map(metaText).join("");
  if (v && typeof v === "object" && "c" in v) return metaText((v as { c: unknown }).c);
  return "";
}

function inlines(v: unknown): Inline[] {
  return Array.isArray(v) ? (v as Inline[]) : typeof v === "string" ? [v] : [];
}

/** The external address of a web or live link (`meta.url`, plus `url_params` when present). */
export function webLinkUrl(item: Pick<Item, "meta">): string | null {
  const url = metaText(item.meta?.url).trim();
  return /^https?:\/\//i.test(url) ? url : null;
}

/** The Internet Archive's copy of a 2008 address (the sites were live when the disc shipped). */
export function archiveUrl(url: string): string {
  return `https://web.archive.org/web/2008/${url}`;
}

/** Items that name this one (`referenced_by`), as links. */
export function ReferencedBy({ items, title, kids }: { items: ItemRef[] | undefined; title: string; kids: boolean }) {
  const list = uniqueByRef(items ?? []);
  if (list.length === 0) return null;
  return (
    <section className="itempage-appears">
      <h2>{title}</h2>
      <ul>
        {list.map((a) => (
          <li key={a.ref_id}>
            <Link to={itemHref(a.ref_id, null, kids)}>{a.title_html ? <Html as="span" html={a.title_html} /> : (a.title ?? a.ref_id)}</Link>
          </li>
        ))}
      </ul>
    </section>
  );
}

export function WebLinkView({ item, kids }: { item: Item; kids: boolean }) {
  useDocumentTitle(item.title);
  const url = webLinkUrl(item);
  const live = item.kind === "live-link";
  return (
    <div className="itempage itempage-text">
      <p className="itempage-kicker">{live ? "Live link" : "Web site"}</p>
      <h1 className="art-title">
        <Html as="span" html={item.title_html ?? item.title ?? ""} />
      </h1>
      {item.caption_html && <Html className="itempage-caption" html={item.caption_html} kids={kids} />}
      {url ? (
        <p className="itempage-url">
          <a href={url} target="_blank" rel="noopener noreferrer">
            {url}
          </a>
          <br />
          <a className="itempage-archive" href={archiveUrl(url)} target="_blank" rel="noopener noreferrer">
            The page as archived in 2008 (Internet Archive)
          </a>
        </p>
      ) : (
        <p className="enc-muted">This link has no address on the disc.</p>
      )}
      <p className="enc-muted">
        {live
          ? "Encarta 2009 fetched this from an MSN service that no longer exists."
          : "Encarta 2009 opened this site in the web browser; many of the 2008 sites have since moved or closed."}
      </p>
      <ReferencedBy items={item.referenced_by} title="Listed in" kids={kids} />
    </div>
  );
}

export function ContributorView({ item, kids }: { item: Item; kids: boolean }) {
  useDocumentTitle(item.title);
  const m = item.meta ?? {};
  const cred = metaText(m.credentials);
  const kind = metaText(m.expert_type);
  const area = metaText(m.area);
  return (
    <div className="itempage itempage-text">
      <p className="itempage-kicker">{kind || "Contributor"}</p>
      <h1 className="art-title">
        {item.title}
        {cred && <span className="itempage-cred">, {cred}</span>}
      </h1>
      {inlines(m.info).length > 0 && (
        <p className="itempage-caption">
          <Inlines c={inlines(m.info)} opts={{ kids }} />
        </p>
      )}
      {area && <p className="enc-muted">Area: {area}</p>}
      <ReferencedBy items={item.referenced_by} title="Articles" kids={kids} />
    </div>
  );
}

/** Life dates of a quotation author: "1935–", "1657–1757", "fl. 1200". */
export function lifeDates(meta: Record<string, unknown>): string {
  const born = metaText(meta.born).trim();
  const died = metaText(meta.died).trim();
  const fl = metaText(meta.flourished).trim();
  if (born || died) return `${born || "?"}–${died}`;
  if (fl && fl !== "0") return `fl. ${fl}`;
  return "";
}

export function AuthorView({ item, kids }: { item: Item; kids: boolean }) {
  const m = item.meta ?? {};
  const name = metaText(m.name) || item.title || "";
  useDocumentTitle(name);
  const facts = [lifeDates(m), metaText(m.nationality), metaText(m.designation)].filter(Boolean).join(" · ");
  return (
    <div className="itempage itempage-text">
      <p className="itempage-kicker">Quotations</p>
      <h1 className="art-title">{name}</h1>
      {facts && <p className="itempage-caption">{facts}</p>}
      <ReferencedBy items={item.referenced_by} title="Quotations" kids={kids} />
    </div>
  );
}

/** The item and up to four ancestors through `parents` (a site hotspot → site → scene → tour). */
async function ancestry(item: Item, s: AbortSignal): Promise<Item[]> {
  const chain = [item];
  for (let i = 0; i < 4; i++) {
    const p = chain[chain.length - 1]!.parents?.[0];
    if (!p || chain.some((c) => c.ref_id === p.ref_id)) break;
    chain.push(await api.item(p.ref_id, s));
  }
  return chain;
}

/**
 * A virtual-tour node (stop, scene, hotspot, label, site map): opens its tour, the 3D runtime at
 * the stop, or the panoramic tour at the scene. Nodes hang under a stop or scene, and those under
 * the tour, through `children`.
 */
export function TourNodeView({ item }: { item: Item }) {
  const tours = useApi((s) => api.tours(s), []);
  const chain = useApi((s) => ancestry(item, s), [item.ref_id]);
  if (!tours.data || !chain.data) return <div className="enc-loading"><div className="enc-spinner" /></div>;
  const c = chain.data;
  const own = tourIdFor(item.ref_id, tours.data.tours);
  if (own) return <Navigate to={`/tours/${own}`} replace />;
  for (let i = 1; i < c.length; i++) {
    const tid = tourIdFor(c[i]!.ref_id, tours.data.tours);
    // The node just below the tour: a stop (the tour page moves there) or a hotspot of the tour.
    if (tid) return <Navigate to={`/tours/${tid}?stop=${c[i - 1]!.ref_id}`} replace />;
    if (isPanoramicTour(c[i])) return <Navigate to={`/item/${c[i]!.ref_id}?scene=${c[i - 1]!.ref_id}`} replace />;
  }
  return (
    <div className="itempage itempage-text">
      <p className="itempage-kicker">Virtual Tour</p>
      <h1 className="art-title">{item.title}</h1>
      <p className="enc-muted">
        This tour node belongs to none of the tours on the disc. <Link to="/tours">All 3D Virtual Tours</Link>
      </p>
    </div>
  );
}

/** College Prep Online and Encarta on the Web: web services of 2009. */
export function OnlineFeatureView({ item }: { item: Item }) {
  useDocumentTitle(item.title);
  return (
    <div className="itempage itempage-text">
      <p className="itempage-kicker">Feature</p>
      <h1 className="art-title">{item.title}</h1>
      <p className="itempage-caption">
        {ONLINE_FEATURES.has(item.subtype ?? "")
          ? "This feature was a Microsoft web service in 2009 and is not on the disc. Encarta's online services closed in 2009."
          : "This feature has no page in this edition."}
      </p>
      <p>
        <Link to="/">Home</Link>
      </p>
    </div>
  );
}

/**
 * Map Trek (`maptrektour`, 171 treks reached from the Geography pages): a guided series of atlas
 * stops. Each stop names a place, an altitude and a map style, and has a short article. The atlas
 * opens each stop's view; the stop list and the text are on the left.
 */
export function MapTrekView({ item, kids }: { item: Item; kids: boolean }) {
  useDocumentTitle(item.title);
  const [search, setSearch] = useSearchParams();
  const stops = (item.children ?? []).filter((c) => c.item_kind === "maptrek-stop" || c.kind === "stop");
  const wanted = Number(search.get("stop"));
  const [fallback, setFallback] = useState(stops[0]?.ref_id ?? 0);
  const current = stops.some((s) => s.ref_id === wanted) ? wanted : fallback;
  const idx = stops.findIndex((s) => s.ref_id === current);
  const stop = useApi(current ? (s) => api.item(current, s) : null, [current]);
  const url = mapTrekUrl(stop.data?.meta?.source_attrs as Record<string, unknown> | undefined);
  const go = (id: number) => {
    setFallback(id);
    setSearch({ stop: String(id) }, { replace: true });
  };
  return (
    <div className="trek">
      <aside className="trek-side">
        <p className="itempage-kicker">Map Trek</p>
        <h1 className="trek-title">{item.title}</h1>
        <ol className="trek-stops">
          {stops.map((s, i) => (
            <li key={s.ref_id}>
              <button type="button" className={s.ref_id === current ? "current" : ""} aria-current={s.ref_id === current ? "step" : undefined} onClick={() => go(s.ref_id)}>
                <span className="trek-num">{i + 1}</span> {s.title}
              </button>
            </li>
          ))}
        </ol>
        <div className="trek-nav">
          <button type="button" className="enc-button" disabled={idx <= 0} onClick={() => go(stops[idx - 1]!.ref_id)}>
            Previous stop
          </button>
          <button type="button" className="enc-button" disabled={idx < 0 || idx >= stops.length - 1} onClick={() => go(stops[idx + 1]!.ref_id)}>
            Next stop
          </button>
        </div>
        <div className="trek-text">{current ? <ArticleInline key={current} id={current} kids={kids} /> : <p className="enc-muted">This Map Trek has no stops.</p>}</div>
      </aside>
      <section className="trek-map" aria-label="Map">
        {AtlasLazy && url ? (
          <Suspense fallback={<div className="enc-loading"><div className="enc-spinner" /></div>}>
            <AtlasLazy key={url} className="atlas-route-map" initialPlace={url} />
          </Suspense>
        ) : (
          <div className="enc-loading">{stop.loading ? <div className="enc-spinner" /> : <p className="enc-muted">No map view for this stop.</p>}</div>
        )}
      </section>
    </div>
  );
}
