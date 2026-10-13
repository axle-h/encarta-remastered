import { useEffect, useId, useRef, useState } from "react";
import { useLocation, useNavigate } from "react-router-dom";
import { api } from "../api/client";
import type { Suggestion } from "../api/types";
import { Html } from "./Html";
import { Icon } from "./Icons";

/** Kids mode keeps item and dictionary links inside `/kids`. */
export function kidsUrl(url: string, kids: boolean): string {
  if (!kids) return url;
  if (url.startsWith("/item/") || url.startsWith("/dictionary") || url.startsWith("/search")) return `/kids${url}`;
  return url;
}

const CATEGORY_LABEL: Record<string, string> = {
  article: "Article",
  media: "Media",
  dictionary: "Dictionary",
  timeline: "Timeline",
  map: "Map",
  quotation: "Quotation",
  sidebar: "Sidebar",
  web: "Web link",
};

/**
 * The search box with Encarta's word-wheel autocomplete (`/api/suggest`). Enter on a highlighted
 * suggestion opens it; otherwise Enter runs a full search.
 */
export function SearchBox({ kids = false, placeholder = "Type a search term", autoFocus = false }: { kids?: boolean; placeholder?: string; autoFocus?: boolean }) {
  const navigate = useNavigate();
  const location = useLocation();
  const [q, setQ] = useState("");
  const [items, setItems] = useState<Suggestion[]>([]);
  const [open, setOpen] = useState(false);
  const [active, setActive] = useState(-1);
  const box = useRef<HTMLDivElement>(null);
  const listId = useId();

  // Keep the box in step with the search page's query.
  useEffect(() => {
    if (location.pathname.endsWith("/search")) setQ(new URLSearchParams(location.search).get("q") ?? "");
    setOpen(false);
  }, [location.pathname, location.search]);

  useEffect(() => {
    const term = q.trim();
    if (!open || term.length < 1) {
      setItems([]);
      return;
    }
    const ctl = new AbortController();
    const t = window.setTimeout(() => {
      api.suggest({ q: term, limit: 10, edition: kids ? "kids" : undefined }, ctl.signal).then(
        (r) => {
          setItems(r.suggestions);
          setActive(-1);
        },
        () => undefined,
      );
    }, 120);
    return () => {
      window.clearTimeout(t);
      ctl.abort();
    };
  }, [q, open, kids]);

  useEffect(() => {
    if (!open) return;
    const close = (e: MouseEvent) => {
      if (!box.current?.contains(e.target as Node)) setOpen(false);
    };
    document.addEventListener("mousedown", close);
    return () => document.removeEventListener("mousedown", close);
  }, [open]);

  const go = (s?: Suggestion) => {
    setOpen(false);
    if (s) {
      navigate(kidsUrl(s.url, kids));
      return;
    }
    const term = q.trim();
    if (term) navigate(`${kids ? "/kids" : ""}/search?q=${encodeURIComponent(term)}`);
  };

  return (
    <div className={`searchbox${kids ? " kids" : ""}`} ref={box}>
      <form
        role="search"
        onSubmit={(e) => {
          e.preventDefault();
          go(active >= 0 ? items[active] : undefined);
        }}
      >
        <span className="searchbox-field">
          {!kids && <Icon name="search" size={16} className="searchbox-lens" />}
          <input
            type="search"
            value={q}
            placeholder={placeholder}
            aria-label="Search Encarta"
            role="combobox"
            aria-expanded={open && items.length > 0}
            aria-controls={listId}
            aria-autocomplete="list"
            aria-activedescendant={active >= 0 ? `${listId}-${active}` : undefined}
            autoComplete="off"
            spellCheck={false}
            autoFocus={autoFocus}
            onFocus={() => setOpen(true)}
            onChange={(e) => {
              setQ(e.target.value);
              setOpen(true);
            }}
            onKeyDown={(e) => {
              if (e.key === "ArrowDown") {
                e.preventDefault();
                setOpen(true);
                setActive((a) => Math.min(items.length - 1, a + 1));
              } else if (e.key === "ArrowUp") {
                e.preventDefault();
                setActive((a) => Math.max(-1, a - 1));
              } else if (e.key === "Escape") {
                setOpen(false);
              }
            }}
          />
        </span>
        <button type="submit" className="searchbox-go" aria-label="Search">
          <Icon name="go" size={18} />
        </button>
      </form>
      {open && items.length > 0 && (
        <ul className="suggest" id={listId} role="listbox">
          {items.map((s, i) => (
            <li
              key={`${s.ref_id}-${i}`}
              id={`${listId}-${i}`}
              role="option"
              aria-selected={i === active}
              className={i === active ? "active" : ""}
              onMouseEnter={() => setActive(i)}
              onMouseDown={(e) => {
                e.preventDefault();
                go(s);
              }}
            >
              <span className="suggest-text">
                <Html as="span" html={s.display_html ?? s.display} />
                {s.title && s.title !== s.display && <span className="suggest-title"> — {s.title}</span>}
              </span>
              {s.category && s.category !== "article" && <span className="suggest-cat">{CATEGORY_LABEL[s.category] ?? s.category}</span>}
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
