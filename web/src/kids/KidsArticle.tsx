import { useEffect, useRef } from "react";
import { Link, useLocation } from "react-router-dom";
import type { Article, AssocItem } from "../api/types";
import { ArticleBody, type BodyContext } from "../article/ArticleBody";
import { useArticleModel } from "../article/useArticleModel";
import { Html } from "../components/Html";
import { Inlines } from "../content/Inlines";
import { itemHref } from "../content/links";
import { fileUrl, mediaClass } from "../media/pick";

/**
 * An Encarta Kids article (layout `kids`, content-schema §2), laid out as the disc's Kids article page
 * (docs/vm-findings.md, Kids articles): the orange swoosh background fixed behind everything, a 62-px
 * header with the orange title and the "Look at Pictures" button, and one scrolling panel holding the
 * text column (floated right) and the 170×206 media tiles, two to a row, on its left.
 */
export function KidsArticleView({ article }: { article: Article }) {
  const m = useArticleModel(article);
  const location = useLocation();
  const scroller = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const el = scroller.current;
    if (!el) return;
    const target = location.hash ? document.getElementById(location.hash.slice(1)) : null;
    if (target) target.scrollIntoView();
    else el.scrollTop = 0;
  }, [location.hash, article]);
  const tiles = m.tiles.length > 0 ? m.tiles : m.sequence;
  const ctx: BodyContext = {
    assoc: m.assoc,
    tables: m.tables,
    opts: { kids: true },
    collapsed: new Set(),
    toggle: () => undefined,
    previewSection: null,
    mediaColumn: false,
    sequence: tiles,
    layout: "kids",
  };
  return (
    <div className="kids-art">
      <div className="kids-art-head">
        <h1>
          <Html as="span" html={article.title_html ?? article.title} />
        </h1>
        {tiles.length > 0 && <Link to={`/kids/item/${article.ref_id}/media`} className="kids-pictures-button" aria-label="Look at Pictures" />}
      </div>
      <div className="kids-art-scroll" id="textscrollarea" ref={scroller} tabIndex={-1}>
        <div className="kids-text">
          <ArticleBody blocks={m.body} ctx={ctx} />
          <KidsRelated article={article} />
        </div>
        <ul className="kids-media">
          {tiles.map((x) => (
            <KidsMediaTile key={x.ref_id} m={x} articleId={article.ref_id} />
          ))}
        </ul>
      </div>
    </div>
  );
}

/**
 * The end of a Kids article: "Also in Encarta Kids" (the related tree's top level, `sort_top` entries
 * first) and "On the Web" (the web links by their captions, bulleted), as the Kids template ends it.
 */
function KidsRelated({ article }: { article: Article }) {
  const related = [...(article.related ?? [])].filter((n) => n.ref !== undefined).sort((a, b) => Number(!!b.sort_top) - Number(!!a.sort_top));
  const web = (article.media ?? []).filter((x) => x.group === "weblink" && x.url);
  return (
    <>
      {related.length > 0 && (
        <div className="kids-related">
          <h2 className="kids-sectitle">Also in Encarta Kids</h2>
          <ul>
            {related.map((n) => (
              <li key={n.ref}>
                <Link to={itemHref(n.ref!, n.para ?? null, true)}>
                  <Inlines c={n.title} opts={{ kids: true }} />
                </Link>
              </li>
            ))}
          </ul>
        </div>
      )}
      {web.length > 0 && (
        <div className="kids-related kids-weblinks">
          <h2 className="kids-sectitle">On the Web</h2>
          <ul>
            {web.map((w) => (
              <li key={w.ref_id}>
                <a href={w.url!} target="_blank" rel="noreferrer">
                  <Html as="span" html={w.caption_html ?? w.title_html ?? w.title ?? ""} />
                </a>
              </li>
            ))}
          </ul>
        </div>
      )}
    </>
  );
}

/** Corner badges on the disc, by `ticon` (our `media_type`); other kinds have none. */
const TICONS = new Set(["anim", "audio", "game", "ia", "image", "map", "narr", "pan", "video"]);

/**
 * A media tile: the gallery picture scaled to 150 px on its longer side in an orange frame, the
 * `ticon` badge at its lower right and the title under it. Interactives open as items; the rest open
 * full size in the Look at Pictures view.
 */
function KidsMediaTile({ m, articleId }: { m: AssocItem; articleId: number }) {
  const pic = m.picon ?? m.image ?? m.thumb ?? undefined;
  const wide = !pic || (pic.width ?? 0) > (pic.height ?? 0);
  const to = mediaClass(m) === "interactive" ? `/kids/item/${m.ref_id}` : `/kids/item/${articleId}/media?max=${m.ref_id}`;
  const ticon = m.media_type && TICONS.has(m.media_type) ? m.media_type : undefined;
  return (
    <li>
      <Link to={to} className="kids-tile">
        <span className="kids-tile-pic">
          <span className="kids-tile-wrap">
            {pic && <img src={fileUrl(pic)} alt="" loading="lazy" className={`kids-tile-img ${wide ? "wide" : "tall"}`} />}
            {ticon && <img className="kids-ticon" src={`/ui/kids-ticon-${ticon}.gif`} alt="" width={28} height={28} />}
          </span>
        </span>
        <span className="kids-tile-title">
          <Html as="span" html={m.title_html ?? m.title ?? ""} />
        </span>
      </Link>
    </li>
  );
}
