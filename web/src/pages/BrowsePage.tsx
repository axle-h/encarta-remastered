import { Link, useParams } from "react-router-dom";
import { api } from "../api/client";
import { useApi } from "../api/useApi";
import { useDocumentTitle } from "../components/useDocumentTitle";
import { fileUrl } from "../media/pick";
import { background, homeLinkHref, homeSections } from "./home";
import { useThemeImage } from "./HomePage";
import type { AssocItem, Item } from "../api/types";

/** A category screen (home-page item, subgroup `grid` or `list`): its links in titled groups. */
export function BrowsePage({ kids = false }: { kids?: boolean }) {
  const id = Number(useParams().id);
  return <BrowseView key={id} id={id} kids={kids} />;
}

function BrowseView({ id, kids }: { id: number; kids: boolean }) {
  const res = useApi((s) => api.item(id, s), [id]);
  const item = res.data;
  useDocumentTitle(item?.title);
  const bg = useThemeImage(item ? background(item) : undefined);
  if (res.error) return <div className="enc-page-error">Could not load this page ({res.error.message}).</div>;
  if (!item) return <div className="enc-loading"><div className="enc-spinner" /></div>;
  return <BrowseContent item={item} kids={kids} bg={bg.url} color={bg.color} />;
}

/** Captions of separators come from their own items. */
function SeparatorCaption({ sep }: { sep: AssocItem }) {
  const it = useApi((s) => api.item(sep.ref_id, s), [sep.ref_id]);
  const cap = it.data?.meta?.caption;
  const text = Array.isArray(cap) ? cap.filter((x): x is string => typeof x === "string").join("") : "";
  return text ? <p className="browse-caption">{text}</p> : null;
}

export function BrowseContent({ item, kids, bg, color }: { item: Item; kids: boolean; bg?: string; color?: string }) {
  const sections = homeSections(item);
  const list = item.subgroup === "list";
  const parents = (item.appears_in ?? []).filter((a) => a.group === "home" || a.kind === "home-page").slice(0, 1);
  return (
    <div className={`browse${list ? " browse-list" : ""}${kids ? " kids" : ""}`} style={{ backgroundImage: bg ? `url(${bg})` : undefined, backgroundColor: color }}>
      <div className="browse-panel">
        <nav className="browse-crumbs" aria-label="Breadcrumb">
          <Link to={kids ? "/kids" : "/"}>Home</Link>
          {parents.map((p) => (
            <span key={p.ref_id}>
              {" › "}
              <Link to={homeLinkHref(p, kids)}>{p.title}</Link>
            </span>
          ))}
          {" › "}
          <span>{item.title}</span>
        </nav>
        <h1 className="browse-title">{item.title}</h1>
        {sections.map((s, i) => (
          <section key={i} className="browse-section">
            {s.title && <h2 className="browse-sep">{s.title}</h2>}
            {s.separator && <SeparatorCaption sep={s.separator} />}
            <ul className={list ? "browse-items-list" : "browse-items"}>
              {s.links.map((l) => (
                <li key={l.ref_id}>
                  <Link to={homeLinkHref(l, kids)} className="browse-tile">
                    {!list && (l.thumb ? <img src={fileUrl(l.thumb)} alt="" width={64} height={64} loading="lazy" /> : <span className="browse-noimg" />)}
                    <span>{l.title}</span>
                  </Link>
                </li>
              ))}
            </ul>
          </section>
        ))}
      </div>
    </div>
  );
}
