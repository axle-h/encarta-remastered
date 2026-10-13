import { Link } from "react-router-dom";
import { api } from "../api/client";
import type { ToursIndex } from "../api/types";
import { useApi } from "../api/useApi";
import { useDocumentTitle } from "../components/useDocumentTitle";
import { fileUrl, galleryImage } from "../media/pick";

/** 3D Virtual Tours: the converted tours (out/tours/index.json) with their disc pictures. */
export function ToursPage() {
  useDocumentTitle("3D Virtual Tours");
  const index = useApi((s) => api.tours(s), []);
  return (
    <div className="tours">
      <h1 className="page-title">3D Virtual Tours</h1>
      <p className="enc-muted">Walk through reconstructions of ancient sites. Tours need WebGL and download 10–20 MB each.</p>
      {index.error && <p className="enc-error">No tours are available ({index.error.message}).</p>}
      <ul className="tours-grid">
        {index.data?.tours.map((t) => <TourCard key={t.id} t={t} />)}
      </ul>
    </div>
  );
}

function TourCard({ t }: { t: ToursIndex["tours"][number] }) {
  const item = useApi((s) => api.item(t.contentId, s), [t.contentId]);
  const pic = galleryImage(item.data);
  return (
    <li>
      <Link to={`/tours/${t.id}`} className="tours-card">
        {pic ? <img src={fileUrl(pic)} alt="" width={pic.width} height={pic.height} /> : <span className="tours-noimg" />}
        <span className="tours-name">{item.data?.title ?? t.name}</span>
        {t.totalBytes && <span className="tours-size">{(t.totalBytes / 1e6).toFixed(0)} MB</span>}
      </Link>
    </li>
  );
}
