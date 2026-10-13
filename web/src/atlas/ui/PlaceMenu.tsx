/**
 * Encarta's pop-up menu on a click (V11 shot): every place under the cursor with its symbol; each
 * opens a submenu with its articles and media (P2MTBL links) and "go to".
 */
import { useEffect, useState } from "react";
import { getPlaceLinks, linkLabel, type Endpoints, type PlaceLink } from "../data";
import type { HitPlace } from "../hit";
import { SpriteIcon } from "./Sprite";

interface Props {
  ep: Endpoints;
  x: number;
  y: number;
  hits: HitPlace[];
  onHover: (h: HitPlace | null) => void;
  onGoTo: (h: HitPlace) => void;
  onOpen: (link: PlaceLink, h: HitPlace) => void;
  onClose: () => void;
}

function KindIcon({ h }: { h: HitPlace }) {
  if (h.icon) return <SpriteIcon name={h.icon} />;
  return <span className={`enc-atlas-kind enc-atlas-kind--${h.kind}`} aria-hidden="true" />;
}

export function PlaceMenu({ ep, x, y, hits, onHover, onGoTo, onOpen, onClose }: Props) {
  const [openEntity, setOpenEntity] = useState<number | null>(null);
  const [links, setLinks] = useState<Record<number, PlaceLink[] | "loading" | "error">>({});

  useEffect(() => {
    const ac = new AbortController();
    for (const h of hits) {
      setLinks((m) => ({ ...m, [h.entity]: "loading" }));
      getPlaceLinks(h.entity, ep, ac.signal)
        .then((l) => setLinks((m) => ({ ...m, [h.entity]: l })))
        .catch(() => !ac.signal.aborted && setLinks((m) => ({ ...m, [h.entity]: "error" })));
    }
    return () => ac.abort();
  }, [hits, ep]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [onClose]);

  return (
    <div className="enc-atlas-menu" style={{ left: x, top: y }} role="menu" onMouseLeave={() => onHover(null)}>
      {hits.map((h) => {
        const l = links[h.entity];
        const list = Array.isArray(l) ? l : [];
        const isOpen = openEntity === h.entity;
        return (
          <div key={h.entity} className={`enc-atlas-menu__item${isOpen ? " is-open" : ""}`} onMouseEnter={() => onHover(h)}>
            <button
              type="button"
              role="menuitem"
              aria-haspopup="true"
              aria-expanded={isOpen}
              title={h.className}
              onClick={() => setOpenEntity(isOpen ? null : h.entity)}
            >
              <KindIcon h={h} />
              <span className="enc-atlas-menu__name">{h.name}</span>
              {list.length > 0 && <span className="enc-atlas-menu__arrow">▸</span>}
            </button>
            {isOpen && (
              <div className="enc-atlas-menu__sub" role="menu">
                {l === "loading" && <div className="enc-atlas-menu__note">Loading…</div>}
                {list.map((link) => (
                  <button key={`${link.kind}-${link.ref_id}`} type="button" role="menuitem" onClick={() => onOpen(link, h)}>
                    {linkLabel(link)}
                  </button>
                ))}
                <button type="button" role="menuitem" onClick={() => onGoTo(h)}>
                  Go to {h.name}
                </button>
              </div>
            )}
          </div>
        );
      })}
    </div>
  );
}
