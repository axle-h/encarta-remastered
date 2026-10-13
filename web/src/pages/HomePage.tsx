import { Link } from "react-router-dom";
import { api } from "../api/client";
import type { AssocItem } from "../api/types";
import { useApi } from "../api/useApi";
import { Icon } from "../components/Icons";
import { useDocumentTitle } from "../components/useDocumentTitle";
import { fileByRole, fileUrl } from "../media/pick";
import { background, homeLinkHref, homeSections, HOME_ID, themeIcons } from "./home";

/** URL of a theme or background item's picture (`default`/`background` file). */
export function useThemeImage(m: AssocItem | undefined): { url?: string; width?: number; height?: number; color?: string } {
  const it = useApi(m ? (s) => api.item(m.ref_id, s) : null, [m?.ref_id]);
  const f = fileByRole(it.data, "background", "default", "image", "picon");
  const sa = it.data?.meta?.source_attrs as { colorvalue?: string } | undefined;
  return { url: fileUrl(f), width: f?.width, height: f?.height, color: sa?.colorvalue ? `#${sa.colorvalue}` : undefined };
}

export function HomePage() {
  useDocumentTitle(null);
  const home = useApi((s) => api.item(HOME_ID, s), []);
  const item = home.data;
  const icons = item ? themeIcons(item) : new Map<string, AssocItem>();
  const bg = useThemeImage(item ? background(item) : undefined);
  const logo = useThemeImage(icons.get("logo"));
  const deco = useThemeImage(icons.get("decoration"));
  const sections = item ? homeSections(item) : [];

  // Three columns, as on the disc's "Home 3col" screen. Editions without a Dictionary feature
  // get one in the last column, before Encarta Kids.
  const cols = sections.map((s) => s.links);
  const hasDict = cols.some((c) => c.some((l) => (l.title ?? "").toLowerCase() === "dictionary"));
  if (cols.length > 0 && !hasDict) {
    const last = cols[cols.length - 1]!;
    const kidsAt = last.findIndex((l) => (l.title ?? "").toLowerCase().includes("kids"));
    last.splice(kidsAt >= 0 ? kidsAt : last.length, 0, { ref_id: 0, group: "feat", title: "Dictionary" });
  }

  return (
    <div
      className="home"
      style={{
        backgroundImage: bg.url ? `url(${bg.url})` : undefined,
        backgroundColor: bg.color,
      }}
    >
      <div className="home-inner">
        <div className="home-logo">
          {logo.url ? (
            <img src={logo.url} alt="Microsoft Encarta Premium" width={logo.width} height={logo.height} />
          ) : (
            <div className="home-logo-text">
              <span className="home-logo-ms">Microsoft®</span>
              <span className="home-logo-name">Encarta</span>
              <span className="home-logo-ed">Premium</span>
            </div>
          )}
        </div>
        <nav className="home-panel" aria-label="Encarta categories">
          {deco.url && <img className="home-deco" src={deco.url} alt="" width={deco.width} height={deco.height} />}
          {home.error && <p className="enc-error">The content server is not reachable ({home.error.message}).</p>}
          {cols.map((links, i) => (
            <ul key={i} className="home-col">
              {links.map((l) => (
                <li key={`${l.ref_id}-${l.title}`}>
                  <Link to={l.ref_id === 0 ? "/dictionary" : homeLinkHref(l)}>
                    <Icon name="arrow" size={18} />
                    {l.title}
                  </Link>
                </li>
              ))}
            </ul>
          ))}
        </nav>
      </div>
    </div>
  );
}
