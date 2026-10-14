import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { Link, useLocation, useNavigate } from "react-router-dom";
import { api } from "../api/client";
import type { AssocItem, Item } from "../api/types";
import { useApi } from "../api/useApi";
import { Html } from "../components/Html";
import { Icon } from "../components/Icons";
import { ArticleInline } from "../article/ArticleInline";
import { fileUrl, galleryImage, largeImage, mediaClass, tourIdFor, uniqueByRef } from "./pick";
import { FlashInteractive } from "./FlashPlayer";
import { GameInteractive } from "../games/GameInteractive";
import { AudioPlayer, VideoPlayer } from "./Players";
import { itemHref } from "../content/links";
import { isPanoramicTour } from "../pages/panorama";

interface ViewerState {
  /** Opens the viewer on `id`; `list` is the sequence prev/next walk through. */
  open: (id: number, list?: AssocItem[]) => void;
}

const Ctx = createContext<ViewerState>({ open: () => undefined });

export function useMediaViewer() {
  return useContext(Ctx);
}

/** Holds the viewer's sequence; the open item is the `media` URL parameter, so Back closes it. */
export function MediaViewerProvider({ children, kids = false }: { children: ReactNode; kids?: boolean }) {
  const [list, setList] = useState<AssocItem[]>([]);
  const location = useLocation();
  const navigate = useNavigate();
  const params = new URLSearchParams(location.search);
  const current = Number(params.get("media")) || null;

  const open = useCallback(
    (id: number, l?: AssocItem[]) => {
      if (l) setList(l);
      const p = new URLSearchParams(location.search);
      const replace = p.has("media");
      p.set("media", String(id));
      navigate({ pathname: location.pathname, search: `?${p}`, hash: location.hash }, { replace, state: location.state });
    },
    [location, navigate],
  );

  const close = useCallback(() => {
    const p = new URLSearchParams(location.search);
    p.delete("media");
    const s = p.toString();
    navigate({ pathname: location.pathname, search: s ? `?${s}` : "", hash: location.hash }, { replace: true });
  }, [location, navigate]);

  const value = useMemo(() => ({ open }), [open]);
  const seq = current !== null && list.some((x) => x.ref_id === current) ? list : current !== null ? [{ ref_id: current, group: "media" }] : [];
  return (
    <Ctx.Provider value={value}>
      {children}
      {current !== null && <MediaViewer id={current} list={seq} onClose={close} onGo={(id) => open(id)} kids={kids} />}
    </Ctx.Provider>
  );
}

function MediaViewer({
  id,
  list,
  onClose,
  onGo,
  kids,
}: {
  id: number;
  list: AssocItem[];
  onClose: () => void;
  onGo: (id: number) => void;
  kids: boolean;
}) {
  const idx = list.findIndex((x) => x.ref_id === id);
  const prev = idx > 0 ? list[idx - 1] : undefined;
  const next = idx >= 0 && idx < list.length - 1 ? list[idx + 1] : undefined;
  const item = useApi((s) => api.item(id, s), [id]);
  const dialog = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
      else if (e.key === "ArrowLeft" && prev && !(e.target instanceof HTMLMediaElement)) onGo(prev.ref_id);
      else if (e.key === "ArrowRight" && next && !(e.target instanceof HTMLMediaElement)) onGo(next.ref_id);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose, onGo, prev, next]);

  useEffect(() => {
    dialog.current?.focus();
  }, []);

  useEffect(() => {
    const prevOverflow = document.body.style.overflow;
    document.body.style.overflow = "hidden";
    return () => {
      document.body.style.overflow = prevOverflow;
    };
  }, []);

  const it = item.data;
  return (
    <div className="mv-backdrop" onMouseDown={(e) => e.target === e.currentTarget && onClose()}>
      <div className="mv" role="dialog" aria-modal="true" aria-label={it?.title ?? "Media"} tabIndex={-1} ref={dialog}>
        <div className="mv-bar">
          <span className="mv-count">{list.length > 1 && idx >= 0 ? `${idx + 1} of ${list.length}` : ""}</span>
          <h2 className="mv-title">{it ? <Html html={it.title_html ?? it.title ?? ""} /> : " "}</h2>
          <button type="button" className="mv-close" onClick={onClose} aria-label="Close">
            <Icon name="close" />
          </button>
        </div>
        <div className="mv-main">
          <button type="button" className="mv-nav prev" disabled={!prev} onClick={() => prev && onGo(prev.ref_id)} aria-label="Previous">
            <Icon name="back" />
          </button>
          <div className="mv-stage">
            {item.loading && !it && <div className="enc-spinner" />}
            {item.error && <p className="enc-error">Could not load this item ({item.error.message}).</p>}
            {it && <MediaStage item={it} kids={kids} />}
          </div>
          <button type="button" className="mv-nav next" disabled={!next} onClick={() => next && onGo(next.ref_id)} aria-label="Next">
            <Icon name="forward" />
          </button>
        </div>
        {it && (
          <div className="mv-info">
            {it.caption_html && <Html className="mv-caption" html={it.caption_html} />}
            {it.credit && <p className="mv-credit">{it.credit_html ? <Html html={it.credit_html} /> : it.credit}</p>}
            <AppearsIn item={it} kids={kids} />
          </div>
        )}
        {list.length > 1 && (
          <div className="mv-strip">
            {list.map((m) => (
              <button
                type="button"
                key={m.ref_id}
                className={m.ref_id === id ? "current" : ""}
                onClick={() => onGo(m.ref_id)}
                title={m.title}
              >
                {m.thumb ? <img src={fileUrl(m.thumb)} alt={m.title ?? ""} loading="lazy" /> : <span className="mv-noimg">{m.title}</span>}
              </button>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}

function AppearsIn({ item, kids }: { item: Item; kids: boolean }) {
  const articles = uniqueByRef((item.appears_in ?? []).filter((a) => a.kind === "article"));
  if (articles.length === 0) return null;
  return (
    <p className="mv-appears">
      Appears in:{" "}
      {articles.slice(0, 8).map((a, i) => (
        <span key={a.ref_id}>
          {i > 0 && ", "}
          <Link to={itemHref(a.ref_id, null, kids)}>{a.title}</Link>
        </span>
      ))}
    </p>
  );
}

/** The body of the viewer for one item, by kind. */
export function MediaStage({ item, kids }: { item: Item; kids: boolean }) {
  const cls = mediaClass(item);
  const tours = useApi(cls === "tour" ? (s) => api.tours(s) : null, [cls]);
  switch (cls) {
    case "video":
      return <VideoPlayer item={item} autoPlay />;
    case "audio": {
      const pic = item.kind === "audio-set" ? undefined : galleryImage(item);
      return (
        <div className="mv-audio">
          {pic && <img src={fileUrl(pic)} alt="" />}
          <AudioPlayer item={item} autoPlay />
        </div>
      );
    }
    case "table":
      if (item.data_table) return <Html className={`mv-table enc-dt-${item.data_table.kind}`} html={item.data_table.html} />;
      break;
    case "text":
      return <ArticleInline id={item.ref_id} kids={kids} />;
    case "tour": {
      const tid = tourIdFor(item.ref_id, tours.data?.tours);
      const pic = largeImage(item);
      const panoramic = isPanoramicTour(item);
      // A picture inside a panoramic tour (a hotspot's destination) is shown like any picture.
      if (!tid && !panoramic && pic) break;
      return (
        <div className="mv-tour">
          {pic && <img src={fileUrl(pic)} alt="" />}
          {tid ? (
            <Link className="enc-button" to={`/tours/${tid}`}>
              Start the 3D virtual tour
            </Link>
          ) : panoramic ? (
            <Link className="enc-button" to={`/item/${item.ref_id}`}>
              Start the virtual tour
            </Link>
          ) : (
            <Link className="enc-button" to={`/item/${item.ref_id}`}>
              Open in its virtual tour
            </Link>
          )}
        </div>
      );
    }
    case "interactive": {
      const iax = item.files?.find((f) => f.role === "item" && /\.iax$/i.test(f.file_name));
      if (iax) {
        return (
          <div className="mv-interactive">
            <GameInteractive iax={iax.file_name} kids={kids} title={item.title} fallback={<FlashInteractive iax={iax.file_name} width={iax.width ?? 556} height={iax.height ?? 400} kids={kids} />} />
          </div>
        );
      }
      const pic = largeImage(item);
      return (
        <div className="mv-interactive">
          {pic && <img src={fileUrl(pic)} alt="" />}
          <p className="enc-muted">This interactive has no Flash definition on the disc.</p>
        </div>
      );
    }
    case "map": {
      const pic = largeImage(item);
      return (
        <div className="mv-map">
          {pic && <img src={fileUrl(pic)} alt="" />}
          <Link className="enc-button" to={item.geocode ? `/atlas/place/${item.geocode}` : "/atlas"}>
            Open in the World Atlas
          </Link>
        </div>
      );
    }
    default:
      break;
  }
  const img = largeImage(item);
  if (img) {
    const wide = (img.width ?? 0) > 2 * (img.height ?? 1);
    return (
      <div className={`mv-image${wide ? " wide" : ""}`}>
        <img src={fileUrl(img)} alt={item.title ?? ""} width={img.width} height={img.height} />
      </div>
    );
  }
  if (item.has_article) return <ArticleInline id={item.ref_id} kids={kids} />;
  return <p className="enc-muted">Nothing to display for this item.</p>;
}
