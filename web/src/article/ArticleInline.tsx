import { Link } from "react-router-dom";
import { api } from "../api/client";
import { useApi } from "../api/useApi";
import { Html } from "../components/Html";
import { itemHref } from "../content/links";
import { ArticleBody, type BodyContext } from "./ArticleBody";
import { useArticleModel } from "./useArticleModel";

/**
 * A compact article body (sidebars, further reading, quotations) for the media viewer: text only,
 * no media column, with a link to open it as a page.
 */
export function ArticleInline({ id, kids }: { id: number; kids: boolean }) {
  const res = useApi((s) => api.article(id, kids ? "kids" : undefined, s), [id, kids]);
  const m = useArticleModel(res.data);
  if (res.error) return <p className="enc-error">This item has no text ({res.error.message}).</p>;
  if (!res.data) return <div className="enc-spinner" />;
  const ctx: BodyContext = {
    assoc: m.assoc,
    tables: m.tables,
    opts: { kids },
    collapsed: new Set(),
    toggle: () => undefined,
    previewSection: null,
    mediaColumn: false,
    sequence: m.sequence,
  };
  return (
    <div className="art-inline">
      <h3 className="art-inline-title">
        <Html as="span" html={res.data.title_html ?? res.data.title} />
      </h3>
      <ArticleBody blocks={m.body} ctx={ctx} />
      <p className="art-inline-open">
        <Link to={itemHref(id, null, kids)}>Open as a page</Link>
      </p>
    </div>
  );
}
