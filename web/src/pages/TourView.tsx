import { useCallback, useEffect, useRef, useState } from "react";
import { Link, useNavigate, useSearchParams } from "react-router-dom";
import { api } from "../api/client";
import { useApi } from "../api/useApi";
import { Html } from "../components/Html";
import { Icon } from "../components/Icons";
import { MenuButton } from "../components/Menu";
import { useDocumentTitle } from "../components/useDocumentTitle";
import {
  SITE_MAP_WINDOW,
  decodePose,
  drawSiteMap,
  encodePose,
  markerMoved,
  mountTour,
  parseWorldLoaded,
  siteMapMarker,
  type SiteMapConfig,
  type SiteMapMarker,
  type TourHandle,
  type TourScene,
} from "../tours";
import type { HotspotTarget, SiteMapImage, TourStop } from "../tours/types";
import { fileUrl, largeImage } from "../media/pick";
import "./TourView.css";

/**
 * A 3D virtual tour laid out like Encarta's tour page (VM captures): the site map with the
 * "you are here" marker and view wedge (tours-3d.md §4.2) above the stop text on the left, the
 * menu bar (labels toggle, Tour Stops, Credits) above the 616×420 control, the tour title below.
 */
export function TourView({ tourId }: { tourId: string }) {
  const index = useApi((s) => api.tours(s), []);
  const meta = index.data?.tours.find((t) => t.id === tourId);
  const stage = useRef<HTMLDivElement>(null);
  const handle = useRef<TourHandle | null>(null);
  const [scene, setScene] = useState<TourScene | null>(null);
  const [state, setState] = useState<"loading" | "ready" | "error">("loading");
  const [error, setError] = useState("");
  const [labels, setLabels] = useState(true);
  const [hotspot, setHotspot] = useState<number | null>(null);
  const [stopId, setStopId] = useState<number | null>(null);
  const [siteId, setSiteId] = useState<number | null>(null);
  const [mapCfg, setMapCfg] = useState<SiteMapConfig | null>(null);
  const [marker, setMarker] = useState<SiteMapMarker | null>(null);
  const navigate = useNavigate();
  const [search, setSearch] = useSearchParams();
  // `?pose=` and `?stop=` bring the visitor back where they left for a hotspot's media (read once per mount).
  const initial = useRef({ pose: search.get("pose"), stop: Number(search.get("stop")) || null });
  const title = scene?.tour.title ?? meta?.name ?? "3D Virtual Tour";
  useDocumentTitle(meta || scene ? `${title}: 3D Virtual Tour` : "3D Virtual Tour");

  // Values the control's event callback reads (it is created once per mount).
  const live = useRef({ cfg: null as SiteMapConfig | null, marker: null as SiteMapMarker | null, stop: null as number | null });
  live.current.stop = stopId;

  const openMedia = useCallback(
    (destMedia: number) => {
      const h = handle.current;
      const keep: Record<string, string> = {};
      if (h) keep.pose = encodePose(h.pose());
      if (live.current.stop) keep.stop = String(live.current.stop);
      setSearch(keep, { replace: true });
      navigate(`/item/${destMedia}`);
    },
    [navigate, setSearch],
  );

  useEffect(() => {
    const el = stage.current;
    if (!el) return;
    let cancelled = false;
    let h: TourHandle | null = null;
    setState("loading");
    setScene(null);
    setMarker(null);
    setMapCfg(null);
    live.current.cfg = null;
    live.current.marker = null;
    // §4: on USERLOCUPDATE the host reads camera.pos? and moves the marker (redrawn past 2°/2 px).
    const updateMarker = () => {
      const cfg = live.current.cfg;
      const pos = h?.do("camera.pos?").split(",").map(Number);
      if (!cfg || !pos || pos.length < 4 || pos.some((v) => !Number.isFinite(v))) return;
      const next = siteMapMarker(cfg, pos[0] ?? 0, pos[2] ?? 0, pos[3] ?? 0);
      if (markerMoved(live.current.marker, next)) {
        live.current.marker = next;
        setMarker(next);
      }
    };
    mountTour(el, {
      baseUrl: "/out/tours",
      tourId,
      controlSize: [616, 420], // Encarta's control size (VM): layer, loading screen and FOV are laid out for it
      initialPose: decodePose(initial.current.pose),
      onEvent: (e) => {
        if (e.name === "WORLDLOADED") {
          live.current.cfg = parseWorldLoaded(e.pairs);
          setMapCfg(live.current.cfg);
          updateMarker();
        } else if (e.name === "USERLOCUPDATE") {
          updateMarker();
        } else if (e.name === "USERLEVELCHANGE") {
          // The floor shown on the map is that of the stop named by QTSRefID (§4).
          const id = Number(e.pairs.find(([k]) => k.toUpperCase() === "QTSREFID")?.[1]);
          const site = h?.scene.stops?.find((s) => s.id === id)?.site;
          if (site) setSiteId(site);
        } else if (e.name === "HOTSPOTCLICKED") {
          // As in Encarta: the hotspot's media page replaces the main view; Back returns to this
          // pose and stop, which are kept in the tour's URL.
          if (e.destMedia) {
            openMedia(e.destMedia);
            return;
          }
          // Converted without the catalog: show the hotspot record in the text pane.
          const id = Number(e.pairs.find(([k]) => k.toUpperCase() === "HOTSPOTID")?.[1]);
          if (Number.isFinite(id) && id > 0) setHotspot(id);
        }
      },
    }).then(
      (t) => {
        if (cancelled) {
          t.destroy();
          return;
        }
        h = t;
        handle.current = t;
        setScene(t.scene);
        const stops = t.scene.stops ?? [];
        const first = stops.find((s) => s.id === initial.current.stop) ?? stops.find((s) => s.id === t.scene.defaultStop) ?? stops[0];
        setStopId(first?.id ?? null);
        setSiteId(first?.site ?? t.scene.sites?.[0]?.id ?? null);
        t.loaded.then(
          () => !cancelled && setState("ready"),
          (err: unknown) => {
            if (!cancelled) {
              setError(String(err));
              setState("error");
            }
          },
        );
      },
      (err: unknown) => {
        if (!cancelled) {
          setError(err instanceof Error ? err.message : String(err));
          setState("error");
        }
      },
    );
    return () => {
      cancelled = true;
      h?.destroy();
      handle.current = null;
    };
  }, [tourId, openMedia]);

  useEffect(() => {
    if (state === "ready") handle.current?.do(labels ? "2D.ON" : "2D.OFF");
  }, [labels, state]);

  const goToStop = (stop: TourStop) => {
    const h = handle.current;
    if (!h) return;
    h.goToStop(stop.id);
    setStopId(stop.id);
    setHotspot(null);
    if (stop.site) setSiteId(stop.site);
    (stage.current?.querySelector(".encarta-tour") as HTMLElement | null)?.focus();
  };

  const stops = scene?.stops ?? [];
  const stop = stops.find((s) => s.id === stopId) ?? null;
  const site = scene?.sites?.find((s) => s.id === siteId) ?? null;

  return (
    <div className="tv">
      <aside className="tv-left">
        <SiteMap
          image={site?.image ? `/out/tours/${tourId}/${site.image}` : null}
          size={site?.size ?? [0, 0]}
          cfg={mapCfg}
          marker={marker}
          locations={state === "ready" ? (site?.locations ?? []) : []}
          onPick={(id) => {
            const s = stops.find((x) => x.id === id);
            if (s) goToStop(s);
          }}
        />
        <div className="tv-text">
          {state === "error" && <p className="enc-error">The tour could not be loaded: {error}</p>}
          {hotspot ? <Hotspot id={hotspot} onClose={() => setHotspot(null)} /> : <StopText stop={stop} />}
        </div>
      </aside>
      <section className="tv-main">
        <nav className="tv-menu" aria-label="Tour">
          <Link to="/tours" className="tv-menu-item tv-back">
            <Icon name="back" size={12} /> All tours
          </Link>
          <span className="tv-menu-gap" />
          <button type="button" className={`tv-menu-item${labels ? " tv-on" : ""}`} aria-pressed={labels} onClick={() => setLabels((v) => !v)}>
            Labels and Hot Spots
          </button>
          <TourStopsMenu stops={stops} targets={scene?.hotspotTargets ?? []} disabled={state !== "ready"} onStop={goToStop} onHotspot={openMedia} />
          <button
            type="button"
            className="tv-menu-item"
            disabled={state !== "ready" || stops.length === 0}
            onClick={() => {
              const first = stops.find((s) => s.id === scene?.defaultStop) ?? stops[0];
              if (first) goToStop(first);
            }}
          >
            Start Over
          </button>
          {meta?.contentId ? <AppearsInMenu contentId={meta.contentId} /> : null}
          {scene?.article ? (
            <Link to={`/item/${scene.article}`} className="tv-menu-item">
              Credits
            </Link>
          ) : null}
        </nav>
        <div className="tv-stagewrap">
          <div className="tv-stage" ref={stage} />
        </div>
        <h1 className="tv-title">{title}</h1>
      </section>
    </div>
  );
}

/** Pixel size of a location's box on the map art (its record's x, y is the top-left corner). */
const LOCATION_BOX = 11;

/**
 * The site-map window (172×194, §4.2): map image, wedge and marker on a pixel canvas. Clicking a
 * location's box moves to its stop (the loading screen: "Click the highlighted points in the map
 * ... to jump to specific tour stops").
 */
function SiteMap({
  image,
  size,
  cfg,
  marker,
  locations,
  onPick,
}: {
  image: string | null;
  size: [number, number];
  cfg: SiteMapConfig | null;
  marker: SiteMapMarker | null;
  locations: SiteMapImage["locations"];
  onPick: (stopId: number) => void;
}) {
  const canvas = useRef<HTMLCanvasElement>(null);
  const [img, setImg] = useState<HTMLImageElement | null>(null);
  useEffect(() => {
    setImg(null);
    if (!image) return;
    const el = new Image();
    let alive = true;
    el.onload = () => alive && setImg(el);
    el.src = image;
    return () => {
      alive = false;
    };
  }, [image]);
  useEffect(() => {
    const g = canvas.current?.getContext("2d");
    if (g) drawSiteMap(g, img, size, cfg, marker);
  }, [img, size, cfg, marker]);
  const [hover, setHover] = useState(false);
  const hit = (e: React.MouseEvent<HTMLCanvasElement>) => {
    const r = e.currentTarget.getBoundingClientRect();
    const x = ((e.clientX - r.left) * SITE_MAP_WINDOW[0]) / r.width;
    const y = ((e.clientY - r.top) * SITE_MAP_WINDOW[1]) / r.height;
    return locations.find((l) => l.stop && l.x !== null && l.y !== null && x >= l.x && x < l.x + LOCATION_BOX && y >= l.y && y < l.y + LOCATION_BOX) ?? null;
  };
  return (
    <canvas
      ref={canvas}
      className="tv-map"
      width={SITE_MAP_WINDOW[0]}
      height={SITE_MAP_WINDOW[1]}
      aria-label="Site map"
      role="img"
      style={{ cursor: hover ? "pointer" : "default" }}
      onMouseMove={(e) => setHover(!!hit(e))}
      onMouseLeave={() => setHover(false)}
      onClick={(e) => {
        const l = hit(e);
        if (l?.stop) onPick(l.stop);
      }}
    />
  );
}

function StopText({ stop }: { stop: TourStop | null }) {
  const id = stop?.article ?? null;
  const article = useApi(id ? (s) => api.article(id, undefined, s) : null, [id]);
  if (!stop) return null;
  return (
    <div className="tv-stoptext">
      <h2>{stop.title}</h2>
      {article.data?.html ? <Html html={article.data.html} className="tv-article" /> : article.loading ? <div className="enc-spinner" /> : null}
    </div>
  );
}

/**
 * Encarta's Tour Stops menu (§4.3, VM): the stops in menu order. A stop with hotspots opens a
 * submenu whose first entry moves to the stop and whose other entries are its hotspots;
 * a stop without hotspots is a single entry.
 */
function TourStopsMenu({
  stops,
  targets,
  disabled,
  onStop,
  onHotspot,
}: {
  stops: TourStop[];
  targets: HotspotTarget[];
  disabled: boolean;
  onStop: (s: TourStop) => void;
  onHotspot: (destMedia: number) => void;
}) {
  const [open, setOpen] = useState(false);
  const [sub, setSub] = useState<number | null>(null);
  const root = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!open) return;
    const close = (e: Event) => {
      if (e instanceof KeyboardEvent ? e.key === "Escape" : !root.current?.contains(e.target as Node)) {
        setOpen(false);
        setSub(null);
      }
    };
    document.addEventListener("mousedown", close);
    document.addEventListener("keydown", close);
    return () => {
      document.removeEventListener("mousedown", close);
      document.removeEventListener("keydown", close);
    };
  }, [open]);
  if (!stops.length) return null;
  const pick = (f: () => void) => {
    setOpen(false);
    setSub(null);
    f();
  };
  return (
    <div className="tv-dropdown" ref={root}>
      <button type="button" className={`tv-menu-item${open ? " tv-open" : ""}`} aria-haspopup="menu" aria-expanded={open} disabled={disabled} onClick={() => setOpen((v) => !v)}>
        Tour Stops
      </button>
      {open && (
        <ul className="tv-popup" role="menu">
          {stops.map((s) => {
            const hs = s.hotspots.map((id) => targets.find((t) => t.id === id)).filter((t): t is HotspotTarget => !!t);
            if (!hs.length) {
              return (
                <li key={s.id} role="none">
                  <button type="button" role="menuitem" onClick={() => pick(() => onStop(s))} onMouseEnter={() => setSub(null)}>
                    {s.title}
                  </button>
                </li>
              );
            }
            return (
              <li key={s.id} role="none" className="tv-hassub" onMouseEnter={() => setSub(s.id)}>
                <button type="button" role="menuitem" aria-haspopup="menu" aria-expanded={sub === s.id} onClick={() => setSub(sub === s.id ? null : s.id)}>
                  {s.title}
                </button>
                {sub === s.id && (
                  <ul className="tv-popup tv-sub" role="menu">
                    <li role="none">
                      <button type="button" role="menuitem" onClick={() => pick(() => onStop(s))}>
                        {s.title}
                      </button>
                    </li>
                    <li role="separator" className="tv-sep" />
                    {hs.map((t) => (
                      <li key={t.id} role="none">
                        <button type="button" role="menuitem" disabled={!t.destMedia} onClick={() => t.destMedia && pick(() => onHotspot(t.destMedia as number))}>
                          {t.title}
                        </button>
                      </li>
                    ))}
                  </ul>
                )}
              </li>
            );
          })}
        </ul>
      )}
    </div>
  );
}

function Hotspot({ id, onClose }: { id: number; onClose: () => void }) {
  const item = useApi((s) => api.item(id, s), [id]);
  const it = item.data;
  const pic = largeImage(it);
  const articles = [...(it?.media ?? []), ...(it?.children ?? []).map((c) => ({ ref_id: c.ref_id, group: c.kind, title: c.title, kind: c.item_kind }))].filter(
    (m) => m.kind === "article" || m.group === "article",
  );
  return (
    <div className="tv-hotspot">
      <button type="button" className="tl-detail-close" onClick={onClose} aria-label="Close">
        <Icon name="close" size={14} />
      </button>
      {!it && <div className="enc-spinner" />}
      {it && (
        <>
          <h2>
            <Html as="span" html={it.title_html ?? it.title ?? ""} />
          </h2>
          {pic && <img src={fileUrl(pic)} alt="" />}
          {it.caption_html && <Html html={it.caption_html} />}
          {it.preview_html && <Html html={it.preview_html} />}
          {it.has_article && <Link to={`/item/${id}`}>Read more</Link>}
          {articles.map((a) => (
            <p key={a.ref_id}>
              <Link to={`/item/${a.ref_id}`}>{a.title ?? "Related article"}</Link>
            </p>
          ))}
        </>
      )}
    </div>
  );
}

/** Encarta's "Appears in" menu: the articles that link this tour. */
function AppearsInMenu({ contentId }: { contentId: number }) {
  const item = useApi((s) => api.item(contentId, s), [contentId]);
  const list = [...(item.data?.appears_in ?? []), ...(item.data?.referenced_by ?? [])].filter(
    (a, i, all) => (a.kind === "article" || a.kind === undefined) && all.findIndex((b) => b.ref_id === a.ref_id) === i,
  );
  return (
    <MenuButton
      label="Appears in"
      buttonClassName="tv-menu-item"
      menuClassName="tv-popup"
      entries={() =>
        list.length > 0 ? list.map((a) => ({ label: a.title ?? String(a.ref_id), to: `/item/${a.ref_id}`, key: String(a.ref_id) })) : [{ label: "No articles link this tour", disabled: true }]
      }
    />
  );
}
