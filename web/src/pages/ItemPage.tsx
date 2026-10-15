import { Link, Navigate, useParams } from "react-router-dom";
import { api } from "../api/client";
import type { Item } from "../api/types";
import { useApi } from "../api/useApi";
import { ArticlePage } from "../article/ArticlePage";
import { Html } from "../components/Html";
import { useDocumentTitle } from "../components/useDocumentTitle";
import { itemHref } from "../content/links";
import { MediaStage } from "../media/MediaViewer";
import { mediaClass, tourIdFor, uniqueByRef } from "../media/pick";
import { BrowseContent } from "./BrowsePage";
import { eventDateText } from "./timelineDates";
import { AuthorView, ContributorView, MapTrekView, OnlineFeatureView, ReferencedBy, TourNodeView, WebLinkView } from "./ItemKinds";
import { itemRedirect } from "./itemRoute";
import { isPanoramicTour } from "./panorama";
import { PanoramaTour } from "./PanoramaTour";

const GEOQUIZ_FEATURE_ID = 701712336;

/** `/item/{id}`: the target of every content link. Picks the view from the item's kind. */
export function ItemPage({ kids = false }: { kids?: boolean }) {
  const id = Number(useParams().id);
  const res = useApi((s) => api.item(id, s), [id]);
  const item = res.data;
  if (!Number.isFinite(id)) return <Navigate to="/" replace />;
  // The Geography Quiz feature item opens the game.
  if (id === GEOQUIZ_FEATURE_ID) return <Navigate to="/games/geoquiz" replace />;
  if (res.error) {
    return (
      <div className="enc-page-error">
        <h1>Item not found</h1>
        <p>Encarta has no item {id} ({res.error.message}).</p>
      </div>
    );
  }
  if (!item) return <div className="enc-loading"><div className="enc-spinner" /></div>;
  // An item only in Encarta Kids opens in Kids mode; a Premium-only item outside it.
  const eds = item.editions ?? [];
  if (!kids && eds.length === 1 && eds[0] === "kids") return <Navigate to={`/kids/item/${id}`} replace />;
  if (item.kind === "home-page") return kids ? <Navigate to={`/kids/browse/${id}`} replace /> : <BrowseContent item={item} />;
  const to = itemRedirect(item);
  if (to) return <Navigate to={to} replace />;
  switch (item.kind) {
    case "feature":
      return <OnlineFeatureView item={item} />;
    case "weblink":
    case "live-link":
      return <WebLinkView item={item} kids={kids} />;
    case "contributor":
      return <ContributorView item={item} kids={kids} />;
    case "quotation-author":
      return <AuthorView item={item} kids={kids} />;
    case "virtual-tour-node":
    case "virtual-tour": {
      if (isPanoramicTour(item)) return <PanoramaTour tour={item} />;
      // The 7 3D tours redirect in ItemView; pictures that hotspots open show as pictures.
      const isScene = (item.files ?? []).some((f) => f.role === "surround");
      const isPicture = !isScene && (item.files ?? []).some((f) => f.role === "image" || f.role === "picon");
      if (item.kind === "virtual-tour-node" && isPicture) break;
      if (item.kind === "virtual-tour" && !isScene && (item.children?.length ?? 0) === 0) break;
      return <TourNodeView item={item} />;
    }
    case "maptrek-tour":
      return <MapTrekView item={item} kids={kids} />;
  }
  if (item.has_article) {
    // The article picks its page layout (encyclopedia, Kids, sidebar) from `article.layout`.
    return <ArticlePage id={id} kids={kids} />;
  }
  return <ItemView item={item} kids={kids} />;
}

function ItemView({ item, kids }: { item: Item; kids: boolean }) {
  useDocumentTitle(item.title);
  const cls = mediaClass(item);
  const tours = useApi(cls === "tour" ? (s) => api.tours(s) : null, [cls]);
  const tid = tourIdFor(item.ref_id, tours.data?.tours);
  if (tid) return <Navigate to={`/tours/${tid}`} replace />;
  const ev = item.timeline;
  return (
    <div className="itempage">
      <h1 className="art-title">
        <Html as="span" html={item.title_html ?? item.title ?? ""} />
      </h1>
      {ev?.start && (
        <p className="itempage-date">
          {eventDateText(ev)} · <Link to={`/timeline?year=${ev.start.year}`}>Show on the Timeline</Link>
        </p>
      )}
      <div className="itempage-stage">
        <MediaStage item={item} kids={kids} />
      </div>
      {item.caption_html && <Html className="itempage-caption" html={item.caption_html} kids={kids} />}
      {item.preview_html && !item.caption_html && <Html className="itempage-caption" html={item.preview_html} kids={kids} />}
      {item.credit && <p className="mv-credit">{item.credit}</p>}
      {item.data_table && cls !== "table" && <Html className={`mv-table enc-dt-${item.data_table.kind}`} html={item.data_table.html} kids={kids} />}
      {(item.appears_in?.length ?? 0) === 0 && <ReferencedBy items={item.referenced_by} title="Appears in" kids={kids} />}
      {(item.appears_in?.length ?? 0) > 0 && (
        <div className="itempage-appears">
          <h2>Appears in</h2>
          <ul>
            {uniqueByRef(item.appears_in!).slice(0, 60).map((a) => (
              <li key={a.ref_id}>
                <Link to={itemHref(a.ref_id, null, kids)}>{a.title ?? a.ref_id}</Link>
              </li>
            ))}
          </ul>
        </div>
      )}
    </div>
  );
}
