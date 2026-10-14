import { Link } from "react-router-dom";
import { api } from "../api/client";
import type { AssocItem } from "../api/types";
import { useApi } from "../api/useApi";
import { useDocumentTitle } from "../components/useDocumentTitle";
import { background, bySequence, KIDS_HOME_ID } from "../pages/home";
import { useThemeImage } from "../pages/HomePage";
import { playMenuSound } from "../settings/uiSounds";
import { fileByRole, fileUrl } from "../media/pick";

/** The Encarta Kids home screen: its category buttons over the Kids background. */
export function KidsHome() {
  useDocumentTitle("Encarta Kids");
  const res = useApi((s) => api.item(KIDS_HOME_ID, s), []);
  const item = res.data;
  const bg = useThemeImage(item ? background(item) : undefined);
  const cats = [...(item?.media ?? [])].filter((m) => m.group === "home").sort(bySequence);
  return (
    <div className="kids-home">
      <div className="kids-home-stage" style={{ backgroundImage: bg.url ? `url(${bg.url})` : undefined }}>
        <h1 className="kids-home-title">Encarta Kids: what do you want to learn about?</h1>
        {res.error && <p className="enc-error">The content server is not reachable ({res.error.message}).</p>}
        <ul className="kids-cats">
          {cats.map((c, i) => (
            <KidsCategory key={c.ref_id} c={c} slot={SLOTS[i]} sound={i + 1} />
          ))}
        </ul>
      </div>
    </div>
  );
}

/**
 * Where the ten category buttons sit on the 1024×600 home background: two columns of five panels
 * (measured from the background picture).
 */
const SLOTS: [number, number][] = [
  [8, 8],
  [8, 97],
  [8, 186],
  [8, 272],
  [8, 352],
  [585, 8],
  [585, 97],
  [585, 186],
  [585, 272],
  [585, 352],
];

/**
 * A category button: the category page's own theme picture (`hthm` "… (top category)"), with its
 * mouse-over picture on hover. The picture carries the label; the text is for screen readers.
 */
function KidsCategory({ c, slot, sound }: { c: AssocItem; slot?: [number, number]; sound: number }) {
  const page = useApi((s) => api.item(c.ref_id, s), [c.ref_id]);
  const theme = page.data?.media?.find((m) => m.group === "hthm");
  const themeItem = useApi(theme ? (s) => api.item(theme.ref_id, s) : null, [theme?.ref_id]);
  const def = fileByRole(themeItem.data, "default");
  const over = fileByRole(themeItem.data, "mouseover");
  const style = slot ? { left: slot[0], top: slot[1] } : undefined;
  return (
    <li className={slot ? "kids-cat-slot" : undefined} style={style}>
      <Link
        to={`/kids/browse/${c.ref_id}`}
        onMouseEnter={() => playMenuSound(`kids-${sound}`)}
        onClick={() => playMenuSound("click")}
        className={`kids-cat${def ? " has-art" : ""}`} aria-label={c.title}>
        {def ? (
          <>
            <img className="kids-cat-art" src={fileUrl(def)} alt="" width={def.width} height={def.height} />
            {over && <img className="kids-cat-over" src={fileUrl(over)} alt="" width={over.width} height={over.height} />}
          </>
        ) : (
          <span className="kids-cat-name">{c.title}</span>
        )}
      </Link>
    </li>
  );
}
