import { Link, useParams } from "react-router-dom";
import { api } from "../api/client";
import { useApi } from "../api/useApi";
import { useDocumentTitle } from "../components/useDocumentTitle";

/**
 * Stand-in for the World Atlas (plan Step 5, built separately in src/atlas/). `/atlas/place/{gid}`
 * links from articles still list the place's content links.
 */
export function AtlasPlaceholder() {
  useDocumentTitle("World Atlas");
  const rest = useParams()["*"] ?? "";
  const m = /^place\/(\d+)/.exec(rest);
  const gid = m ? Number(m[1]) : null;
  const place = useApi(gid !== null ? (s) => api.place(gid, s) : null, [gid]);
  return (
    <div className="atlas-ph">
      <h1 className="page-title">World Atlas</h1>
      <p>The interactive World Atlas is not available in this build yet.</p>
      {gid !== null && (
        <section>
          <h2>Place {gid}</h2>
          {(place.error || place.data?.links.length === 0) && <p className="enc-muted">No information about this place.</p>}
          <ul>
            {place.data?.links.map((l) => (
              <li key={`${l.ordinal}-${l.ref_id}`}>
                <Link to={`/item/${l.ref_id}`}>{l.title ?? l.ref_id}</Link>
                {l.kind_label && <span className="enc-muted"> ({l.kind_label})</span>}
              </li>
            ))}
          </ul>
        </section>
      )}
    </div>
  );
}
