/**
 * Dev page for the atlas (`/atlas.html`). Query: `?style=atlas-political&lat=50&lon=10&alt=3000`,
 * `?place=25262` (gazetteer entity), `?url=msencarta://view=MapView/cmd=map/…`, `?view=flat`,
 * `?quiz=1` (quiz mode), `?highlight=<entity>` (filled in yellow), `?placement=maplibre` (MapLibre's
 * label collision instead of Encarta's placement, for comparison).
 * Opened articles are listed in a side strip instead of navigating away.
 */
import { StrictMode, useState } from "react";
import { createRoot } from "react-dom/client";
import { Atlas, type AtlasLink, type AtlasPlace } from "../index";

const q = new URLSearchParams(location.search);
const num = (k: string) => (q.has(k) ? Number(q.get(k)) : undefined);

function initialPlace(): AtlasPlace | undefined {
  if (q.get("url")) return q.get("url")!;
  if (q.has("place")) return { entity: num("place") };
  if (q.has("lat") || q.has("lon")) return { lat: num("lat"), lon: num("lon"), altitudeKm: num("alt") };
  return undefined;
}

function Dev() {
  const [opened, setOpened] = useState<AtlasLink[]>([]);
  return (
    <div style={{ display: "flex", height: "100%" }}>
      <div style={{ flex: 1, minWidth: 0 }}>
        <Atlas
          initialPlace={initialPlace()}
          initialStyle={q.get("style") ?? undefined}
          initialProjection={q.get("view") === "flat" ? "flat" : "globe"}
          quizMode={q.get("quiz") === "1"}
          labelPlacement={q.get("placement") !== "maplibre"}
          highlightEntity={num("highlight")}
          onPlaceClick={(hits) => console.log("atlas click:", hits.map((h) => h.name).join(", "))}
          onOpenArticle={(_, link) => setOpened((o) => [link, ...o].slice(0, 8))}
          onViewChange={(v) => {
            const p = new URLSearchParams(location.search);
            p.set("lat", v.lat.toFixed(3));
            p.set("lon", v.lon.toFixed(3));
            p.set("alt", String(Math.round(v.altitudeKm)));
            p.set("style", v.style);
            p.delete("place");
            p.delete("url");
            history.replaceState(null, "", `?${p}`);
          }}
        />
      </div>
      {opened.length > 0 && (
        <aside id="opened" style={{ width: 220, background: "#111", color: "#ddd", font: "12px system-ui", padding: 8, overflow: "auto" }}>
          <b>onOpenArticle</b>
          {opened.map((l, i) => (
            <p key={i}>
              <a style={{ color: "#8cf" }} href={`/item/${l.refId}`}>
                {l.title ?? l.refId}
              </a>
              <br />
              {l.kindLabel} · {l.placeName}
            </p>
          ))}
        </aside>
      )}
    </div>
  );
}

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <Dev />
  </StrictMode>,
);
