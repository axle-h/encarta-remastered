import { useEffect } from "react";
import { useLocation } from "react-router-dom";
import { api } from "../api/client";
import type { AssocItem } from "../api/types";
import { useApi } from "../api/useApi";
import { ArticleBody, type BodyContext } from "../article/ArticleBody";
import { useArticleModel } from "../article/useArticleModel";
import { Html } from "../components/Html";
import { Icon, mediaIcon } from "../components/Icons";
import { useDocumentTitle } from "../components/useDocumentTitle";
import { useMediaViewer } from "../media/MediaViewer";
import { fileUrl, galleryImage, mediaClass } from "../media/pick";

/**
 * An Encarta Kids article: large text on the right and the article's media as a grid of framed
 * pictures on the left (Kids articles list their media after the text, content-schema §2).
 */
export function KidsArticle({ id }: { id: number }) {
  const res = useApi((s) => api.article(id, "kids", s), [id]);
  const m = useArticleModel(res.data);
  const viewer = useMediaViewer();
  const location = useLocation();
  useDocumentTitle(res.data?.title);
  useEffect(() => {
    if (!location.hash) window.scrollTo(0, 0);
    else document.getElementById(location.hash.slice(1))?.scrollIntoView();
  }, [location.hash, res.data]);
  if (res.error) return <div className="enc-page-error">Could not load the article ({res.error.message}).</div>;
  if (!res.data) return <div className="enc-loading"><div className="enc-spinner" /></div>;
  const ctx: BodyContext = {
    assoc: m.assoc,
    tables: m.tables,
    opts: { kids: true },
    collapsed: new Set(),
    toggle: () => undefined,
    previewSection: null,
    mediaColumn: false,
    sequence: m.sequence,
  };
  return (
    <div className="kids-art">
      <div className="kids-art-head">
        <h1>
          <Html as="span" html={res.data.title_html ?? res.data.title} />
        </h1>
        {m.sequence.length > 0 && (
          <button type="button" className="kids-pictures" onClick={() => viewer.open(m.sequence[0]!.ref_id, m.sequence)}>
            <Icon name="image" size={22} /> Look at Pictures
          </button>
        )}
      </div>
      <div className="kids-art-cols">
        <ul className="kids-media">
          {m.sequence.map((x) => (
            <KidsMediaTile key={x.ref_id} m={x} list={m.sequence} />
          ))}
        </ul>
        <div className="kids-text">
          <ArticleBody blocks={m.body} ctx={ctx} />
        </div>
      </div>
    </div>
  );
}

function KidsMediaTile({ m, list }: { m: AssocItem; list: AssocItem[] }) {
  const viewer = useMediaViewer();
  const item = useApi((s) => api.item(m.ref_id, s), [m.ref_id]);
  const cls = mediaClass(item.data ?? m);
  const pic = galleryImage(item.data) ?? m.thumb ?? undefined;
  return (
    <li>
      <button type="button" className="kids-tile" onClick={() => viewer.open(m.ref_id, list)}>
        <span className="kids-tile-pic">
          {pic ? <img src={fileUrl(pic)} alt="" loading="lazy" /> : <Icon name={mediaIcon(cls)} size={40} />}
          <span className={`kids-badge kids-badge-${cls}`}>
            <Icon name={cls === "image" ? "plus" : mediaIcon(cls)} size={16} />
          </span>
        </span>
        <span className="kids-tile-title">
          <Html as="span" html={m.title_html ?? m.title ?? item.data?.title ?? ""} />
        </span>
      </button>
    </li>
  );
}
