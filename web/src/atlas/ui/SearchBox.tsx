import { useEffect, useRef, useState } from "react";
import { AtlasApiError, searchPlaces, type Endpoints, type PlaceHit } from "../data";

interface Props {
  ep: Endpoints;
  body: "earth" | "moon";
  onPick: (hit: PlaceHit) => void;
  /** Debounce in ms (tests use 0). */
  delay?: number;
}

/** Place search over the gazetteer (`GET /api/atlas/search`), with a result list as the user types. */
export function SearchBox({ ep, body, onPick, delay = 180 }: Props) {
  const [q, setQ] = useState("");
  const [hits, setHits] = useState<PlaceHit[]>([]);
  const [open, setOpen] = useState(false);
  const [active, setActive] = useState(0);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const box = useRef<HTMLDivElement>(null);
  const picked = useRef<string | null>(null);

  useEffect(() => {
    const text = q.trim();
    if (text === picked.current) return; // the box shows the place just picked
    picked.current = null;
    if (text.length < 2) {
      setHits([]);
      setError(null);
      return;
    }
    const ac = new AbortController();
    const t = setTimeout(() => {
      setBusy(true);
      searchPlaces(text, ep, { body, limit: 20, signal: ac.signal })
        .then((r) => {
          const list = r.filter((h) => !h.body || h.body === body);
          setHits(list);
          setActive(0);
          setError(null);
          setOpen(true);
        })
        .catch((e: unknown) => {
          if (ac.signal.aborted) return;
          setHits([]);
          setOpen(true);
          if (e instanceof AtlasApiError && (e.status === 404 || e.status === 503)) setError("Place search is not available on this server.");
          else setError("Place search failed.");
        })
        .finally(() => setBusy(false));
    }, delay);
    return () => {
      clearTimeout(t);
      ac.abort();
    };
  }, [q, ep, body, delay]);

  useEffect(() => {
    const close = (ev: MouseEvent) => {
      if (box.current && !box.current.contains(ev.target as Node)) setOpen(false);
    };
    document.addEventListener("mousedown", close);
    return () => document.removeEventListener("mousedown", close);
  }, []);

  const pick = (h: PlaceHit | undefined) => {
    if (!h) return;
    setOpen(false);
    picked.current = h.name;
    setQ(h.name);
    onPick(h);
  };

  return (
    <div className="enc-atlas-search" ref={box}>
      <input
        type="search"
        value={q}
        placeholder={body === "moon" ? "Find a place on the Moon" : "Find a place"}
        aria-label="Find a place"
        aria-autocomplete="list"
        aria-expanded={open && hits.length > 0}
        onChange={(e) => setQ(e.target.value)}
        onFocus={() => hits.length && setOpen(true)}
        onKeyDown={(e) => {
          if (e.key === "ArrowDown") {
            e.preventDefault();
            setActive((a) => Math.min(hits.length - 1, a + 1));
          } else if (e.key === "ArrowUp") {
            e.preventDefault();
            setActive((a) => Math.max(0, a - 1));
          } else if (e.key === "Enter") {
            e.preventDefault();
            pick(hits[active] ?? hits[0]);
          } else if (e.key === "Escape") {
            setOpen(false);
          }
        }}
      />
      {busy && <span className="enc-atlas-search__busy" aria-hidden="true" />}
      {open && (hits.length > 0 || error) && (
        <ul className="enc-atlas-search__list" role="listbox">
          {error && <li className="enc-atlas-search__error">{error}</li>}
          {hits.map((h, i) => (
            <li
              key={h.entity}
              role="option"
              aria-selected={i === active}
              className={i === active ? "is-active" : undefined}
              onMouseEnter={() => setActive(i)}
              onMouseDown={(e) => {
                e.preventDefault();
                pick(h);
              }}
            >
              <span className="enc-atlas-search__name">{h.name}</span>
              <span className="enc-atlas-search__meta">{[h.feature_type, h.context].filter(Boolean).join(", ")}</span>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
