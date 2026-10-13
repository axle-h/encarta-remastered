import { Link } from "react-router-dom";
import { api } from "../api/client";
import type { AssocItem } from "../api/types";
import { useApi } from "../api/useApi";
import { useDocumentTitle } from "../components/useDocumentTitle";
import { background, bySequence, KIDS_HOME_ID } from "../pages/home";
import { useThemeImage } from "../pages/HomePage";

/** The Encarta Kids home screen: its category buttons over the Kids background. */
export function KidsHome() {
  useDocumentTitle("Encarta Kids");
  const res = useApi((s) => api.item(KIDS_HOME_ID, s), []);
  const item = res.data;
  const bg = useThemeImage(item ? background(item) : undefined);
  const cats = [...(item?.media ?? [])].filter((m) => m.group === "home").sort(bySequence);
  return (
    <div className="kids-home" style={{ backgroundImage: bg.url ? `url(${bg.url})` : undefined }}>
      <h1 className="kids-home-title">What do you want to learn about?</h1>
      {res.error && <p className="enc-error">The content server is not reachable ({res.error.message}).</p>}
      <ul className="kids-cats">
        {cats.map((c) => (
          <KidsCategory key={c.ref_id} c={c} />
        ))}
      </ul>
    </div>
  );
}

/** A category button: the category page's own theme picture (`hthm` "… (top category)"). */
function KidsCategory({ c }: { c: AssocItem }) {
  const page = useApi((s) => api.item(c.ref_id, s), [c.ref_id]);
  const theme = page.data?.media?.find((m) => m.group === "hthm");
  const img = useThemeImage(theme);
  return (
    <li>
      <Link to={`/kids/browse/${c.ref_id}`} className="kids-cat">
        {img.url ? <img src={img.url} alt="" width={img.width} height={img.height} /> : <span className="kids-cat-blank" />}
        <span className="kids-cat-name">{c.title}</span>
      </Link>
    </li>
  );
}
