import { Link, Navigate, useLocation, useParams } from "react-router-dom";
import { api } from "../api/client";
import type { AssocItem, Item } from "../api/types";
import { useApi } from "../api/useApi";
import { useDocumentTitle } from "../components/useDocumentTitle";
import { fileByRole, fileUrl } from "../media/pick";
import { background, bySequence, homeLinkHref, homeLinks, KIDS_HOME_ID } from "../pages/home";
import { useThemeImage } from "../pages/HomePage";
import { KidsCategory, SLOTS } from "./KidsHome";

const NO_PICTURE = "/ui/kids-no-picture.jpg";

/** Which top category a subcategory screen was opened from (Critter Corner and Science Zone are in two). */
interface FromCategory {
  cat?: number;
}

/**
 * `/kids/browse/{id}`: a top category (a home-screen panel) or one of its subcategories, the two
 * screens of docs/vm-findings.md "Kids chrome and home".
 */
export function KidsBrowsePage() {
  const id = Number(useParams().id);
  const res = useApi((s) => api.item(id, s), [id]);
  const item = res.data;
  useDocumentTitle(item?.title);
  if (id === KIDS_HOME_ID) return <Navigate to="/kids" replace />;
  if (res.error) return <div className="enc-page-error">Could not load this page ({res.error.message}).</div>;
  if (!item) return <div className="enc-loading"><div className="enc-spinner" /></div>;
  const top = (item.appears_in ?? []).some((a) => a.ref_id === KIDS_HOME_ID);
  return top ? <KidsTopCategory key={id} item={item} /> : <KidsSubcategory key={id} item={item} />;
}

/** The subcategory's own icon: its `hthm` picture ("Animals: Mammals", 44×42). */
function useIcon(id: number) {
  const page = useApi((s) => api.item(id, s), [id]);
  const theme = page.data?.media?.find((m) => m.group === "hthm");
  const themeItem = useApi(theme ? (s) => api.item(theme.ref_id, s) : null, [theme?.ref_id]);
  return fileByRole(themeItem.data, "default");
}

function TopicIcon({ id }: { id: number }) {
  const f = useIcon(id);
  return f ? <img className="kids-topic-icon" src={fileUrl(f)} alt="" width={f.width} height={f.height} /> : <span className="kids-topic-icon" />;
}

/**
 * A top category: its 1024×600 background (the home panels with this one highlighted and a blue
 * board in the middle), the other nine category buttons on their panels and the subcategories, each
 * with its icon, on the board.
 */
function KidsTopCategory({ item }: { item: Item }) {
  const home = useApi((s) => api.item(KIDS_HOME_ID, s), []);
  const bg = useThemeImage(background(item));
  const cats = [...(home.data?.media ?? [])].filter((m) => m.group === "home").sort(bySequence);
  return (
    <div className="kids-home">
      <div className="kids-home-stage" style={{ backgroundImage: bg.url ? `url(${bg.url})` : undefined }}>
        <h1 className="kids-home-title">{item.title}</h1>
        <ul className="kids-cats">
          {cats.map((c, i) => (c.ref_id === item.ref_id ? null : <KidsCategory key={c.ref_id} c={c} slot={SLOTS[i]} sound={i + 1} />))}
        </ul>
        <ul className="kids-topics">
          {homeLinks(item).map((l) => (
            <li key={l.ref_id}>
              <Link to={homeLinkHref(l, true)} state={{ cat: item.ref_id } satisfies FromCategory}>
                <TopicIcon id={l.ref_id} />
                <span>{l.title}</span>
              </Link>
            </li>
          ))}
        </ul>
      </div>
    </div>
  );
}

/**
 * A subcategory: the top category's title in orange, its subcategories down the left (this one on
 * the blue arrow) and a scrolling grid of 64-px tiles with captions. A tile shows the "book" picture
 * until its thumbnail has loaded, and keeps it when there is none. Switching subcategories replaces
 * the history entry: Encarta switches them inside one page.
 */
function KidsSubcategory({ item }: { item: Item }) {
  const from = (useLocation().state as FromCategory | null)?.cat;
  const parents = (item.appears_in ?? []).filter((a) => a.group === "home" && a.ref_id !== KIDS_HOME_ID);
  const parentRef: AssocItem | undefined = parents.find((p) => p.ref_id === from) ?? parents[0];
  const parent = useApi(parentRef ? (s) => api.item(parentRef.ref_id, s) : null, [parentRef?.ref_id]);
  const bg = useThemeImage(background(item));
  const siblings = parent.data ? homeLinks(parent.data).filter((l) => l.group === "home") : [];
  return (
    <div className="kids-sub" style={{ backgroundImage: bg.url ? `url(${bg.url})` : undefined }}>
      <h1 className="kids-sub-title">{parentRef?.title ?? item.title}</h1>
      <h2 className="sr-only">{item.title}</h2>
      <ul className="kids-sub-list">
        {siblings.map((l) => (
          <li key={l.ref_id} className={l.ref_id === item.ref_id ? "current" : undefined}>
            <Link to={homeLinkHref(l, true)} replace state={{ cat: parentRef?.ref_id } satisfies FromCategory} aria-current={l.ref_id === item.ref_id ? "page" : undefined}>
              <TopicIcon id={l.ref_id} />
              <span>{l.title}</span>
            </Link>
          </li>
        ))}
      </ul>
      <div className="kids-grid-scroll">
        <ul className="kids-grid">
          {homeLinks(item).map((l) => (
            <li key={l.ref_id}>
              <Link to={homeLinkHref(l, true)}>
                <img
                  src={l.thumb ? fileUrl(l.thumb) : NO_PICTURE}
                  alt=""
                  width={64}
                  height={64}
                  loading="lazy"
                  onError={(e) => {
                    if (!e.currentTarget.src.endsWith(NO_PICTURE)) e.currentTarget.src = NO_PICTURE;
                  }}
                />
                <span>{l.title}</span>
              </Link>
            </li>
          ))}
        </ul>
      </div>
    </div>
  );
}
