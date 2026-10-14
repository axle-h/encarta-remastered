import { useEffect } from "react";
import { useLocation } from "react-router-dom";
import type { Article, AssocItem } from "../api/types";
import { ArticleBody, type BodyContext } from "../article/ArticleBody";
import { useArticleModel } from "../article/useArticleModel";
import { Html } from "../components/Html";
import { Icon, mediaIcon } from "../components/Icons";
import { useMediaViewer } from "../media/MediaViewer";
import { fileUrl, mediaClass } from "../media/pick";

/**
 * An Encarta Kids article (layout `kids`, content-schema §2): orange title and "Look at Pictures",
 * large text on the right without section numbers, and the stored tile grid (`media-grid`) in two
 * columns on the left, scrolling with the text.
 */
export function KidsArticleView({ article }: { article: Article }) {
  const m = useArticleModel(article);
  const viewer = useMediaViewer();
  const location = useLocation();
  useEffect(() => {
    if (!location.hash) window.scrollTo(0, 0);
    else document.getElementById(location.hash.slice(1))?.scrollIntoView();
  }, [location.hash, article]);
  // The viewer steps through the tiles first, then anything else the article shows.
  const tiles = m.tiles.length > 0 ? m.tiles : m.sequence;
  const tileRefs = new Set(tiles.map((t) => t.ref_id));
  const list = [...tiles, ...m.sequence.filter((x) => !tileRefs.has(x.ref_id))];
  const ctx: BodyContext = {
    assoc: m.assoc,
    tables: m.tables,
    opts: { kids: true },
    collapsed: new Set(),
    toggle: () => undefined,
    previewSection: null,
    mediaColumn: false,
    sequence: list,
    layout: "kids",
  };
  return (
    <div className="kids-art">
      <div className="kids-art-head">
        <h1>
          <Html as="span" html={article.title_html ?? article.title} />
        </h1>
        {list.length > 0 && (
          <button type="button" className="kids-pictures" onClick={() => viewer.open(list[0]!.ref_id, list)}>
            <Icon name="image" size={22} /> Look at Pictures
          </button>
        )}
      </div>
      <div className="kids-art-cols">
        <ul className="kids-media">
          {tiles.map((x) => (
            <KidsMediaTile key={x.ref_id} m={x} list={list} />
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
  const cls = mediaClass(m);
  const pic = m.picon ?? m.image ?? m.thumb ?? undefined;
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
          <Html as="span" html={m.title_html ?? m.title ?? ""} />
        </span>
      </button>
    </li>
  );
}
