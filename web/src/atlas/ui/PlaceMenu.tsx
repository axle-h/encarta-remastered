/**
 * Encarta's pop-up menu on a click (V11 shot): every place under the cursor with its symbol; each
 * opens a submenu with its articles and media (P2MTBL links) and "go to".
 */
import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { getPlaceLinks, linkLabel, sortLinks, type Endpoints, type PlaceLink } from "../data";
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
    // Places whose links are already here keep them (a re-click on the same place).
    setLinks((m) => {
      const missing = hits.filter((h) => !Array.isArray(m[h.entity]));
      if (!missing.length) return m;
      const next = { ...m };
      for (const h of missing) next[h.entity] = "loading";
      return next;
    });
    for (const h of hits) {
      getPlaceLinks(h.entity, ep, ac.signal)
        .then((l) => setLinks((m) => ({ ...m, [h.entity]: sortLinks(l) })))
        .catch(() => !ac.signal.aborted && setLinks((m) => ({ ...m, [h.entity]: "error" })));
    }
    return () => ac.abort();
  }, [hits, ep]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [onClose]);

  // Keep the menu inside the map: open upwards / leftwards near the bottom / right edge. The box is
  // placed on the element after every render, not through state: a state update from this layout
  // effect re-renders at sync priority, and while a links update of lower priority is pending React
  // re-applies the later ones on each such render, so `links` is new every time and the effect never
  // settled ("Maximum update depth exceeded" on a click whose places' links were cached).
  const el = useRef<HTMLDivElement>(null);
  useLayoutEffect(() => {
    const box = el.current;
    const parent = box?.parentElement;
    if (!box || !parent) return;
    const W = parent.clientWidth;
    const H = parent.clientHeight;
    const w = box.offsetWidth;
    const h = box.offsetHeight;
    box.style.left = `${x + w > W ? Math.max(0, x - w) : x}px`;
    box.style.top = `${y + h > H ? Math.max(0, Math.min(y - h, H - h)) : y}px`;
  });

  return (
    <div className="enc-atlas-menu" ref={el} role="menu" onMouseLeave={() => onHover(null)}>
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
                <button type="button" role="menuitem" onClick={() => onGoTo(h)}>
                  Map
                </button>
                {list.map((link) => (
                  <button key={`${link.kind}-${link.ref_id}`} type="button" role="menuitem" onClick={() => onOpen(link, h)}>
                    {linkLabel(link)}
                  </button>
                ))}
              </div>
            )}
          </div>
        );
      })}
    </div>
  );
}
