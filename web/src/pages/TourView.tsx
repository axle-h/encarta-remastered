import { useEffect, useRef, useState } from "react";
import { Link } from "react-router-dom";
import { api } from "../api/client";
import { useApi } from "../api/useApi";
import { Html } from "../components/Html";
import { Icon } from "../components/Icons";
import { useDocumentTitle } from "../components/useDocumentTitle";
import { mountTour, type TourHandle } from "../tours";
import { fileUrl, largeImage } from "../media/pick";

/** A 3D virtual tour (plan Step 6 runtime) with the host's labels toggle and hotspot panel. */
export function TourView({ tourId }: { tourId: string }) {
  const index = useApi((s) => api.tours(s), []);
  const meta = index.data?.tours.find((t) => t.id === tourId);
  useDocumentTitle(meta ? `${meta.name}: 3D Virtual Tour` : "3D Virtual Tour");
  const stage = useRef<HTMLDivElement>(null);
  const handle = useRef<TourHandle | null>(null);
  const [state, setState] = useState<"loading" | "ready" | "error">("loading");
  const [error, setError] = useState("");
  const [labels, setLabels] = useState(true);
  const [hotspot, setHotspot] = useState<number | null>(null);

  useEffect(() => {
    const el = stage.current;
    if (!el) return;
    let cancelled = false;
    let h: TourHandle | null = null;
    setState("loading");
    mountTour(el, {
      baseUrl: "/out/tours",
      tourId,
      onEvent: (e) => {
        if (e.name === "HOTSPOTCLICKED") {
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
  }, [tourId]);

  useEffect(() => {
    if (state === "ready") handle.current?.do(labels ? "2D.ON" : "2D.OFF");
  }, [labels, state]);

  return (
    <div className="tourpage">
      <div className="tourpage-bar">
        <Link to="/tours" className="tourpage-back">
          <Icon name="back" size={14} /> All tours
        </Link>
        <h1>{meta?.name ?? "3D Virtual Tour"}</h1>
        <label>
          <input type="checkbox" checked={labels} onChange={(e) => setLabels(e.target.checked)} /> Show Labels and Hot Spots
        </label>
      </div>
      <div className="tourpage-main">
        <div className="tourpage-stage" ref={stage} />
        <aside className="tourpage-side">
          {state === "error" && <p className="enc-error">The tour could not be loaded: {error}</p>}
          {hotspot ? (
            <Hotspot id={hotspot} onClose={() => setHotspot(null)} />
          ) : (
            <div className="tourpage-help">
              <h2>How to explore</h2>
              <ul>
                <li>Use the arrow keys to walk and turn.</li>
                <li>Page Up and Page Down look up and down; Home levels the view.</li>
                <li>Drag in the view to move.</li>
                <li>Click a hot spot to learn about what you see.</li>
              </ul>
            </div>
          )}
        </aside>
      </div>
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
    <div className="tourpage-hotspot">
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
