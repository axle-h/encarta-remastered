import { useEffect, useMemo, useRef, useState, type MutableRefObject } from "react";
import { useNavigate, useSearchParams } from "react-router-dom";
import { api } from "../api/client";
import type { Item } from "../api/types";
import { useApi } from "../api/useApi";
import { ArticleInline } from "../article/ArticleInline";
import { useDocumentTitle } from "../components/useDocumentTitle";
import { fileByRole, fileUrl } from "../media/pick";
import { tourHfov, wedgeAngle, type PanoView } from "../panorama/geometry";
import { TOUR } from "../panorama/measured";
import { tourParams } from "../panorama/params";
import { PanoramaView, type PanoHotspot, type PanoLabel } from "../panorama/PanoramaView";
import { childIds, destOf, labelPoint, quadrantRect, shedRect } from "./panorama";

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

/** `?yaw=` as degrees, or null. */
export function yawParam(v: string | null): number | null {
  if (v === null || v.trim() === "") return null;
  const n = Number(v);
  return Number.isFinite(n) ? n : null;
}

/**
 * A panoramic virtual tour (pages/panorama.ts for the records): the scene's panorama in the native
 * cylindrical player (panorama/), with its labels and hotspots; the site map (current scene marked)
 * and the scene list pick scenes. A hotspot to another scene opens it; any other hotspot opens its
 * media page, as the 3D tours do in the VM (docs/vm-findings.md). The URL keeps the scene and the
 * yaw (`?scene=&yaw=`), so Back returns to the same view.
 */
export function PanoramaTour({ tour }: { tour: Item }) {
  useDocumentTitle(tour.title ? `${tour.title}: Virtual Tour` : "Virtual Tour");
  const scenes = (tour.children ?? []).filter((c) => c.kind === "media");
  const sceneSet = new Set(scenes.map((c) => c.ref_id));
  const intro = childIds(tour, "article")[0];
  const [search, setSearch] = useSearchParams();
  const navigate = useNavigate();
  const wanted = Number(search.get("scene"));
  const current = sceneSet.has(wanted) ? wanted : (scenes[0]?.ref_id ?? 0);
  const yaw = yawParam(search.get("yaw"));
  const scene = useApi(current ? (s) => loadScene(current, s) : null, [current]);
  const view = useRef<PanoView | null>(null);
  // Tour Settings (vm-findings, Panoramas layout): both on by default, kept while the tour is open.
  const [showMarks, setShowMarks] = useState(true);
  const [autoRotate, setAutoRotate] = useState(true);

  /** Records the current yaw in this history entry, so Back comes back to it. */
  const keepYaw = () => {
    const v = view.current;
    if (!v || !current) return;
    setSearch({ scene: String(current), yaw: v.yaw.toFixed(1) }, { replace: true });
  };
  const go = (id: number) => {
    keepYaw();
    setSearch({ scene: String(id) });
  };
  const open = (dest: number | null) => {
    if (!dest) return;
    if (sceneSet.has(dest)) go(dest);
    else {
      keepYaw();
      navigate(`/item/${dest}`);
    }
  };
  const sc = scene.data && scene.data.item.ref_id === current ? scene.data : undefined;
  const titles = new Map<number, string>();
  for (const c of [...(tour.children ?? []), ...(sc?.item.children ?? [])]) if (c.title) titles.set(c.ref_id, c.title);
  const article = sc ? childIds(sc.item, "article")[0] : undefined;
  return (
    <div className="pano">
      <aside className="pano-side">
        <p className="itempage-kicker">Virtual Tour</p>
        <h1 className="trek-title">{tour.title}</h1>
        {sc?.site && <SiteMap site={sc.site} hotspots={sc.siteHotspots} current={current} scenes={sceneSet} titles={titles} onOpen={open} viewRef={view} panangle={panangleOf(sc.item)} pictureWidth={pictureWidth(sc.item)} />}
        <ol className="trek-stops" aria-label="Scenes">
          {scenes.map((c, i) => (
            <li key={c.ref_id}>
              <button type="button" className={c.ref_id === current ? "current" : ""} aria-current={c.ref_id === current ? "step" : undefined} onClick={() => c.ref_id !== current && go(c.ref_id)}>
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
        <div className="pano-settings" role="group" aria-label="Tour Settings">
          <label>
            <input type="checkbox" checked={showMarks} onChange={(e) => setShowMarks(e.target.checked)} /> Show labels and hot spots
          </label>
          <label>
            <input type="checkbox" checked={autoRotate} onChange={(e) => setAutoRotate(e.target.checked)} /> Auto-rotate 360 views
          </label>
        </div>
        {sc && (
          <ScenePanorama
            marksOn={showMarks}
            onMarksChange={setShowMarks}
            autoOn={autoRotate}
            onAutoChange={setAutoRotate}
            key={sc.item.ref_id}
            scene={sc}
            scenes={sceneSet}
            titles={titles}
            yaw={yaw}
            viewRef={view}
            onOpen={open}
            onSettle={(y) => setSearch({ scene: String(current), yaw: y.toFixed(1) }, { replace: true })}
          />
        )}
      </section>
    </div>
  );
}

/**
 * The site map: like the 3D tours' maps, the picture holds two halves (plain and tinted); the left
 * half is shown. Its hotspots are points (`x`, `y`) leading to scenes and pictures; the current
 * scene's point is the "you are here" marker, with a view wedge that follows the live yaw
 * (oriented by the scene's `panangle`).
 */
function SiteMap({
  site,
  hotspots,
  current,
  scenes,
  titles,
  onOpen,
  viewRef,
  panangle,
  pictureWidth,
}: {
  viewRef: MutableRefObject<PanoView | null>;
  panangle: number;
  pictureWidth: number;
  site: Item;
  hotspots: Item[];
  current: number;
  scenes: Set<number>;
  titles: Map<number, string>;
  onOpen: (dest: number | null) => void;
}) {
  const img = fileByRole(site, "image", "picon");
  if (!img?.width || !img.height) return null;
  const half = Math.round(img.width / 2);
  const here = hotspots.map((h) => ({ p: labelPoint(h.meta), dest: destOf(h) })).find((h) => h.dest === current && h.p)?.p;
  return (
    <div className="pano-site" style={{ width: half, height: img.height }}>
      <img src={fileUrl(img)} alt="Site map" width={img.width} height={img.height} />
      {here && pictureWidth > 0 && <ViewWedge at={here} viewRef={viewRef} panangle={panangle} pictureWidth={pictureWidth} size={Math.max(half, img.height)} />}
      {hotspots.map((h) => {
        const p = labelPoint(h.meta);
        const dest = destOf(h);
        if (!p || !dest) return null;
        const here = dest === current;
        const title = (titles.get(dest) ?? (h.title && h.title !== "TBD" ? h.title : `Item ${dest}`)) + (here ? " (you are here)" : "");
        return (
          <button
            key={h.ref_id}
            type="button"
            className={`pano-site-pt${here ? " current" : ""}${scenes.has(dest) ? "" : " picture"}`}
            style={{ left: p.x, top: p.y }}
            title={title}
            aria-label={title}
            aria-current={here ? "location" : undefined}
            onClick={() => !here && onOpen(dest)}
          />
        );
      })}
    </div>
  );
}

function ScenePanorama({
  scene,
  scenes,
  titles,
  yaw,
  viewRef,
  onOpen,
  onSettle,
  marksOn,
  onMarksChange,
  autoOn,
  onAutoChange,
}: {
  marksOn: boolean;
  onMarksChange: (on: boolean) => void;
  autoOn: boolean;
  onAutoChange: (on: boolean) => void;
  scene: Scene;
  scenes: Set<number>;
  titles: Map<number, string>;
  yaw: number | null;
  viewRef: MutableRefObject<PanoView | null>;
  onOpen: (dest: number | null) => void;
  onSettle: (yaw: number) => void;
}) {
  const file = fileByRole(scene.item, "surround");
  const orig = file?.media?.variants.original;
  const pano = scene.item.meta?.panorama as { width?: number } | undefined;
  const width = orig?.width ?? pano?.width ?? file?.width ?? 0;
  const height = orig?.height ?? file?.height ?? 0;
  const hotspots = useMemo<PanoHotspot[]>(
    () =>
      scene.hotspots.flatMap((h) => {
        const rect = shedRect(h.meta);
        const dest = destOf(h);
        if (!rect || !dest) return [];
        const title = h.title && h.title !== "TBD" ? h.title : (titles.get(dest) ?? `Item ${dest}`);
        return [{ id: h.ref_id, rect, marker: quadrantRect(h.meta), title, kind: scenes.has(dest) ? ("scene" as const) : ("media" as const) }];
      }),
    [scene.hotspots, scenes, titles],
  );
  const labels = useMemo<PanoLabel[]>(
    () =>
      scene.labels.flatMap((l) => {
        const p = labelPoint(l.meta);
        return p && l.title ? [{ id: l.ref_id, x: p.x, y: p.y, text: l.title }] : [];
      }),
    [scene.labels],
  );
  const dests = useMemo(() => new Map(scene.hotspots.map((h) => [h.ref_id, destOf(h)])), [scene.hotspots]);
  if (!file || !width || !height) return <p className="enc-muted">This scene has no panorama picture.</p>;
  const params = tourParams(scene.item.meta, { width, height });
  return (
    <div className="pano-view">
      <h2 className="pano-title">{scene.item.title}</h2>
      <PanoramaView
        src={fileUrl(file)!}
        width={width}
        height={height}
        horizon={null}
        params={params}
        initialYaw={yaw}
        title={`Panorama: ${scene.item.title ?? ""}`}
        hotspots={hotspots}
        labels={labels}
        viewRef={viewRef}
        onHotspot={(h) => onOpen(dests.get(h.id) ?? null)}
        onSettle={onSettle}
        marksOn={marksOn}
        onMarksChange={onMarksChange}
        autoOn={autoOn}
        onAutoChange={onAutoChange}
        maxScale={2}
      />
    </div>
  );
}

/** The `panangle` of a scene record (0 when absent). */
function panangleOf(scene: Item): number {
  const n = Number((scene.meta?.panorama as { panangle?: number | string } | undefined)?.panangle);
  return Number.isFinite(n) ? n : 0;
}

function pictureWidth(scene: Item): number {
  const file = fileByRole(scene, "surround");
  return file?.media?.variants.original?.width ?? file?.width ?? Number((scene.meta?.panorama as { width?: number } | undefined)?.width) ?? 0;
}

/** Pie slice for a view wedge: apex, direction (degrees clockwise from map-right) and width in degrees. */
export function wedgePath(x: number, y: number, dir: number, hfov: number, r: number): string {
  const pt = (a: number) => `${(x + r * Math.cos((a * Math.PI) / 180)).toFixed(1)} ${(y + r * Math.sin((a * Math.PI) / 180)).toFixed(1)}`;
  return `M ${x} ${y} L ${pt(dir - hfov / 2)} A ${r} ${r} 0 ${hfov > 180 ? 1 : 0} 1 ${pt(dir + hfov / 2)} Z`;
}

/** The current scene's view wedge on the site map (vm-findings (e)): as wide as the hfov, pointing at yaw − panangle. */
function ViewWedge({ at, viewRef, panangle, pictureWidth: w, size }: { at: { x: number; y: number }; viewRef: MutableRefObject<PanoView | null>; panangle: number; pictureWidth: number; size: number }) {
  const [yaw, setYaw] = useState(0);
  useEffect(() => {
    const read = () => setYaw(Math.round((viewRef.current?.yaw ?? TOUR.openYaw) * 2) / 2);
    read();
    const id = window.setInterval(read, 100);
    return () => window.clearInterval(id);
  }, [viewRef]);
  return (
    <svg className="pano-wedge" width="100%" height="100%" aria-hidden="true" data-testid="site-wedge">
      <path d={wedgePath(at.x, at.y, wedgeAngle(yaw, panangle), tourHfov(w), Math.min(60, size / 2))} fill={TOUR.wedgeColour} fillOpacity="0.85" />
    </svg>
  );
}
