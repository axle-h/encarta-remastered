/**
 * The style carousel on the right of the map (V4 panel shots): a thumbnail and caption per map style.
 * Thumbnails are the Mediterranean cut out of the style raster's Miller level-0 image
 * (`miller/<raster>/<look>-0.webp`, 512 × 384 for the whole world).
 */
import { useState } from "react";
import { dataFile, type Endpoints } from "../data";
import { orderedStyles, styleLabel, type StyleIndexEntry } from "../catalog";

interface Props {
  ep: Endpoints;
  index: StyleIndexEntry[];
  current: StyleIndexEntry | undefined;
  onPick: (e: StyleIndexEntry) => void;
}

const MILLER_LOOKS: Record<string, string> = { LANDWATER: "hypso", LBU_BORDERS: "hypso" };

export function thumbUrl(e: StyleIndexEntry, ep: Endpoints): string {
  const raster = e.raster.toLowerCase() === "locator" ? "hypso" : e.raster.toLowerCase();
  const look = MILLER_LOOKS[e.look.toUpperCase()] ?? e.look.toLowerCase();
  return dataFile(`miller/${raster}/${look}-0.webp`, ep);
}

function Thumb({ e, ep, active, onPick }: { e: StyleIndexEntry; ep: Endpoints; active: boolean; onPick: () => void }) {
  const moon = e.raster.toUpperCase() === "MOON";
  return (
    <button type="button" className={`enc-atlas-style${active ? " is-active" : ""}`} aria-pressed={active} onClick={onPick} title={e.name}>
      <span
        className="enc-atlas-style__img"
        style={{
          backgroundImage: `url("${thumbUrl(e, ep)}")`,
          // Mediterranean on the Earth (about 30–48° N, 8° W–20° E), Mare Imbrium on the Moon.
          backgroundSize: moon ? "300% auto" : "720% auto",
          backgroundPosition: moon ? "45% 30%" : "50.5% 22%",
        }}
      />
      <span className="enc-atlas-style__label">{styleLabel(e)}</span>
    </button>
  );
}

export function StylePicker({ ep, index, current, onPick }: Props) {
  const { menu, more } = orderedStyles(index);
  const [showMore, setShowMore] = useState(() => !!current && more.some((m) => m.id === current.id));
  return (
    <nav className="enc-atlas-styles" aria-label="Map styles">
      <div className="enc-atlas-styles__list">
        {menu.map((e) => (
          <Thumb key={e.id} e={e} ep={ep} active={current?.id === e.id} onPick={() => onPick(e)} />
        ))}
        {more.length > 0 && (
          <button type="button" className="enc-atlas-styles__more" aria-expanded={showMore} onClick={() => setShowMore((s) => !s)}>
            {showMore ? "Fewer styles" : `More styles (${more.length})`}
          </button>
        )}
        {showMore &&
          more.map((e) => (
            <button key={e.id} type="button" className={`enc-atlas-style-row${current?.id === e.id ? " is-active" : ""}`} aria-pressed={current?.id === e.id} onClick={() => onPick(e)}>
              {e.name}
            </button>
          ))}
      </div>
    </nav>
  );
}
