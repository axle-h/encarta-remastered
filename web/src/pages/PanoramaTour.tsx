import { useEffect, useLayoutEffect, useRef, useState, type PointerEvent as ReactPointerEvent } from "react";
import { useSearchParams } from "react-router-dom";
import { api } from "../api/client";
import type { AssocItem, Item } from "../api/types";
import { useApi } from "../api/useApi";
import { ArticleInline } from "../article/ArticleInline";
import { useDocumentTitle } from "../components/useDocumentTitle";
import { useMediaViewer } from "../media/MediaViewer";
import { fileByRole, fileUrl } from "../media/pick";
import { childIds, destOf, labelPoint, shedRect, wrapScroll } from "./panorama";

interface Scene {
  item: Item;
  labels: Item[];
  hotspots: Item[];
  site: Item | null;
  siteHotspots: Item[];
}

async function loadScene(id: number, s: AbortSignal): Promise<Scene> {
  const item = await api.item(id, s);
  const all = (ids: number[]) => Promise.all(ids.map((x) => api.item(x, s).catch(() => null))).then((l) => l.filter((x): x is Item => !!x));
  const [labels, hotspots, sites] = await Promise.all([all(childIds(item, "label")), all(childIds(item, "hotspot")), all(childIds(item, "site"))]);
  const site = sites[0] ?? null;
  const siteHotspots = site ? await all(childIds(site, "hotspot")) : [];
  return { item, labels, hotspots, site, siteHotspots };
}

/**
 * A panoramic virtual tour (pages/panorama.ts): the scene's panorama scrolls round (drag, the
 * arrow buttons or ←/→ when focused), with its labels and hotspots; the site map and the scene list
 * pick scenes; a hotspot opens its scene or its picture in the media viewer. The original's
 * perspective correction of the cylinder is not reproduced (flat strip).
 */
export function PanoramaTour({ tour }: { tour: Item }) {
  useDocumentTitle(tour.title ? `${tour.title}: Virtual Tour` : "Virtual Tour");
  const scenes = (tour.children ?? []).filter((c) => c.kind === "media");
  const sceneSet = new Set(scenes.map((c) => c.ref_id));
  const intro = childIds(tour, "article")[0];
  const [search, setSearch] = useSearchParams();
  const wanted = Number(search.get("scene"));
  const current = sceneSet.has(wanted) ? wanted : (scenes[0]?.ref_id ?? 0);
  const scene = useApi(current ? (s) => loadScene(current, s) : null, [current]);
  const viewer = useMediaViewer();
  const go = (id: number) => setSearch({ scene: String(id) }, { replace: false });
  const open = (dest: number | null) => {
    if (!dest) return;
    if (sceneSet.has(dest)) go(dest);
    else {
      const pics: AssocItem[] = (scene.data?.item.children ?? []).filter((c) => c.kind === "media").map((c) => ({ ref_id: c.ref_id, group: "media", title: c.title }));
      viewer.open(dest, pics.some((p) => p.ref_id === dest) ? pics : undefined);
    }
  };
  const sc = scene.data;
  const titles = new Map<number, string>();
  for (const c of [...(tour.children ?? []), ...(sc?.item.children ?? [])]) if (c.title) titles.set(c.ref_id, c.title);
  const article = sc ? childIds(sc.item, "article")[0] : undefined;
  return (
    <div className="pano">
      <aside className="pano-side">
        <p className="itempage-kicker">Virtual Tour</p>
        <h1 className="trek-title">{tour.title}</h1>
        {sc?.site && <SiteMap site={sc.site} hotspots={sc.siteHotspots} current={current} titles={titles} onOpen={open} />}
        <ol className="trek-stops" aria-label="Scenes">
          {scenes.map((c, i) => (
            <li key={c.ref_id}>
              <button type="button" className={c.ref_id === current ? "current" : ""} aria-current={c.ref_id === current ? "step" : undefined} onClick={() => go(c.ref_id)}>
                <span className="trek-num">{i + 1}</span> {c.title}
              </button>
            </li>
          ))}
        </ol>
        <div className="trek-text">{article ? <ArticleInline key={article} id={article} kids={false} /> : intro ? <ArticleInline id={intro} kids={false} /> : null}</div>
      </aside>
      <section className="pano-main">
        {scene.error && <p className="enc-error">This scene could not be loaded ({scene.error.message}).</p>}
        {!sc && !scene.error && <div className="enc-loading"><div className="enc-spinner" /></div>}
        {sc && <Panorama key={sc.item.ref_id} scene={sc} onOpen={open} />}
      </section>
    </div>
  );
}

/**
 * The site map: like the 3D tours' maps, the picture holds two halves (plain and tinted); the left
 * half is shown. Its hotspots are points (`x`, `y`) leading to scenes and pictures; the current
 * scene's point is highlighted.
 */
function SiteMap({ site, hotspots, current, titles, onOpen }: { site: Item; hotspots: Item[]; current: number; titles: Map<number, string>; onOpen: (dest: number | null) => void }) {
  const img = fileByRole(site, "image", "picon");
  if (!img?.width || !img.height) return null;
  const half = Math.round(img.width / 2);
  return (
    <div className="pano-site" style={{ width: half, height: img.height }}>
      <img src={fileUrl(img)} alt="Site map" width={img.width} height={img.height} />
      {hotspots.map((h) => {
        const p = labelPoint(h.meta);
        const dest = destOf(h);
        if (!p || !dest) return null;
        const title = titles.get(dest) ?? (h.title && h.title !== "TBD" ? h.title : `Item ${dest}`);
        return (
          <button
            key={h.ref_id}
            type="button"
            className={`pano-site-pt${dest === current ? " current" : ""}`}
            style={{ left: p.x, top: p.y }}
            title={title}
            aria-label={title}
            aria-current={dest === current ? "location" : undefined}
            onClick={() => onOpen(dest)}
          />
        );
      })}
    </div>
  );
}

function Panorama({ scene, onOpen }: { scene: Scene; onOpen: (dest: number | null) => void }) {
  const file = fileByRole(scene.item, "surround");
  const orig = file?.media?.variants.original;
  const pano = scene.item.meta?.panorama as { width?: number } | undefined;
  const width = orig?.width ?? pano?.width ?? file?.width ?? 0;
  const height = orig?.height ?? file?.height ?? 380;
  const box = useRef<HTMLDivElement>(null);
  const drag = useRef<{ x: number; left: number } | null>(null);
  const [focusTitle, setFocusTitle] = useState("");

  // Start in the middle copy, at the panorama's left edge.
  useLayoutEffect(() => {
    if (box.current && width) box.current.scrollLeft = width;
  }, [width]);
  useEffect(() => {
    const el = box.current;
    if (!el || !width) return;
    const onScroll = () => {
      const w = wrapScroll(el.scrollLeft, width);
      if (w !== el.scrollLeft) el.scrollLeft = w;
    };
    el.addEventListener("scroll", onScroll, { passive: true });
    return () => el.removeEventListener("scroll", onScroll);
  }, [width]);

  const by = (dx: number) => box.current?.scrollBy({ left: dx, behavior: "smooth" });
  const down = (e: ReactPointerEvent) => {
    if (e.button !== 0 || (e.target as HTMLElement).closest("button")) return;
    drag.current = { x: e.clientX, left: box.current?.scrollLeft ?? 0 };
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
  };
  const move = (e: ReactPointerEvent) => {
    if (drag.current && box.current) box.current.scrollLeft = drag.current.left - (e.clientX - drag.current.x);
  };
  const up = () => {
    drag.current = null;
  };

  if (!file || !width) return <p className="enc-muted">This scene has no panorama picture.</p>;
  const src = fileUrl(file);
  const copies = [0, 1, 2];
  return (
    <div className="pano-view">
      <h2 className="pano-title">{scene.item.title}</h2>
      <div
        className="pano-strip"
        ref={box}
        tabIndex={0}
        role="group"
        aria-label={`Panorama: ${scene.item.title ?? ""}. Use the left and right arrow keys to look around.`}
        style={{ height: height + 16 }}
        onKeyDown={(e) => {
          if (e.key === "ArrowLeft") (e.preventDefault(), by(-120));
          else if (e.key === "ArrowRight") (e.preventDefault(), by(120));
        }}
        onPointerDown={down}
        onPointerMove={move}
        onPointerUp={up}
        onPointerCancel={up}
      >
        <div className="pano-track" style={{ width: width * 3, height }}>
          {copies.map((k) => (
            <div key={k} className="pano-copy" style={{ left: k * width, width, height }} aria-hidden={k !== 1}>
              <img src={src} alt="" width={width} height={height} draggable={false} />
              {scene.labels.map((l) => {
                const p = labelPoint(l.meta);
                return p ? (
                  <span key={l.ref_id} className="pano-label" style={{ left: p.x, top: p.y }}>
                    {l.title}
                  </span>
                ) : null;
              })}
              {scene.hotspots.map((h) => {
                const r = shedRect(h.meta);
                return r ? (
                  <button
                    key={h.ref_id}
                    type="button"
                    className="pano-hot"
                    tabIndex={k === 1 ? 0 : -1}
                    style={{ left: r.x, top: r.y, width: r.w, height: r.h }}
                    title={h.title}
                    aria-label={h.title}
                    onFocus={() => setFocusTitle(h.title ?? "")}
                    onClick={() => onOpen(destOf(h))}
                  />
                ) : null;
              })}
            </div>
          ))}
        </div>
      </div>
      <div className="pano-bar">
        <button type="button" className="enc-button" onClick={() => by(-300)} aria-label="Look left">
          ◀
        </button>
        <button type="button" className="enc-button" onClick={() => by(300)} aria-label="Look right">
          ▶
        </button>
        <span className="enc-muted">{focusTitle || "Drag the picture to look around; click a highlighted area to see more."}</span>
      </div>
    </div>
  );
}
