import { Link, useSearchParams } from "react-router-dom";
import { api } from "../api/client";
import type { SearchHit } from "../api/types";
import { useApi } from "../api/useApi";
import { Html } from "../components/Html";
import { useDocumentTitle } from "../components/useDocumentTitle";
import { fileUrl, mediaClass } from "../media/pick";
import "../styles/kids-search.css";

export type KidsCardKind = "article" | "sound" | "video" | "table" | "activity" | "map" | "picture" | "pictures";

/** Which of the disc's result card pictures (`/ui/kids-search-*.jpg`) a hit gets. */
export function cardKind(h: Pick<SearchHit, "kind" | "media_type" | "category">): KidsCardKind {
  if (h.kind === "composite-media" || h.kind === "panorama") return "pictures";
  switch (mediaClass({ kind: h.kind, media_type: h.media_type })) {
    case "audio":
      return "sound";
    case "video":
      return "video";
    case "table":
      return "table";
    case "interactive":
      return "activity";
    case "map":
      return "map";
    case "image":
      return "picture";
    default:
      return "article";
  }
}

const textOf = (html: string | undefined) =>
  (html ?? "")
    .replace(/<[^>]*>/g, "")
    .replace(/&nbsp;/g, " ")
    .replace(/&amp;/g, "&")
    .replace(/&lt;/g, "<")
    .replace(/&gt;/g, ">")
    .replace(/\s+/g, " ")
    .trim();

/** Whole-word match (the original finds nothing for "zeb"), allowing a plural ending. */
export function matchesWholeWord(h: SearchHit, q: string): boolean {
  const words = q.toLowerCase().match(/[\p{L}\p{N}']+/gu) ?? [];
  if (words.length === 0) return true;
  const hay = `${textOf(h.title_html ?? h.title)} ${textOf(h.snippet_html)}`.toLowerCase();
  const tokens = new Set(hay.match(/[\p{L}\p{N}'’]+/gu) ?? []);
  return words.some((w) => tokens.has(w) || tokens.has(`${w}s`) || tokens.has(`${w}es`));
}

/** The article lead the card shows: whole sentences up to about three lines, then " ...". */
export function snippetText(html: string | undefined): string {
  const t = textOf(html);
  if (t.length <= 170) return t ? `${t} ...` : t;
  const sentences = t.match(/[^.!?]+[.!?]+["”’]?\s*/g) ?? [t];
  let out = "";
  for (const s of sentences) {
    if (out && out.length + s.length > 170) break;
    out += s;
  }
  if (out.length > 190) out = out.slice(0, 160).replace(/\s+\S*$/, "");
  return `${out.trim()} ...`;
}

function Card({ h }: { h: SearchHit }) {
  const kind = cardKind(h);
  const to = `/kids${h.url.startsWith("/") ? h.url : `/${h.url}`}`;
  const pic = fileUrl(h.picon ?? h.thumb);
  return (
    <li className="ks-item">
      <Link to={to} className={`ks-card ks-${kind}`} data-kind={kind}>
        <span className="ks-over" aria-hidden="true" />
        {pic && (
          <span className="ks-thumb">
            <img src={pic} alt="" />
          </span>
        )}
        <Html as="span" className="ks-title" html={h.title_html ?? h.title} />
        {kind === "article" && <span className="ks-snippet">{snippetText(h.snippet_html)}</span>}
      </Link>
    </li>
  );
}

/** `/kids/search?q=`: the Kids search results. */
export function KidsSearchPage() {
  const [params] = useSearchParams();
  return <KidsSearch q={(params.get("q") ?? "").trim()} />;
}

/** Encarta Kids search results, from fixtures/ui/kids2/search-results*.png. */
export function KidsSearch({ q }: { q: string }) {
  useDocumentTitle(q ? `Search: ${q}` : "Search");
  const res = useApi(q ? (s) => api.search({ q, edition: "kids", limit: 50 }, s) : null, [q]);
  const hits = (res.data?.hits ?? []).filter((h) => h.kind !== "index-term" && matchesWholeWord(h, q));
  const done = !!res.data;
  return (
    <div className="ks">
      <h1 className="ks-head">
        <span className="ks-head-big">Search Results</span>
        {q && (
          <span className="ks-head-small">
            {" "}
            for "<b>{q}</b>" in Encarta Kids
          </span>
        )}
      </h1>
      <div className="ks-scroll">
        {res.error && <p className="ks-note">Search is not available ({res.error.message}).</p>}
        {!done && !res.error && q && <div className="enc-spinner" />}
        {done && hits.length === 0 && (
          <div className="ks-none">
            <div className="ks-sorry">
              <span className="ks-sorry-head">Sorry!</span>
              <p>
                We could not find "{q}." Please try again with different words, or look through topics on the <Link to="/kids">home page</Link>.
              </p>
            </div>
            <p className="ks-full">
              To try your search in the full edition of Encarta, <Link to={`/search?q=${encodeURIComponent(q)}`}>click here</Link>.
            </p>
          </div>
        )}
        {hits.length > 0 && (
          <ol className="ks-list">
            {hits.map((h) => (
              <Card key={`${h.ref_id}-${h.url}`} h={h} />
            ))}
          </ol>
        )}
      </div>
    </div>
  );
}
