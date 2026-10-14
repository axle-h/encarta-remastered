import { useEffect, useRef, useState } from "react";
import { Link, useLocation } from "react-router-dom";
import { Icon } from "../components/Icons";
import { FEATURES_EVENT } from "./commands";
import { BROWSE_ALL, FEATURE_GROUPS, featureHref } from "./featuresData";

/** Hover delay before a "More" flyout opens (measured: 100 ms). */
export const FLYOUT_DELAY_MS = 100;

/**
 * Features button and its flyout: three groups with a heading and entries, each ending in "More"
 * (a second flyout), then "Browse all". Alt+T opens it; Escape or a click outside closes it.
 */
export function FeaturesMenu() {
  const [open, setOpen] = useState(false);
  const [flyout, setFlyout] = useState<number | null>(null);
  const wrap = useRef<HTMLDivElement>(null);
  const button = useRef<HTMLButtonElement>(null);
  const timer = useRef<number | undefined>(undefined);
  const location = useLocation();

  const close = (refocus = false) => {
    setOpen(false);
    setFlyout(null);
    if (refocus) button.current?.focus();
  };
  useEffect(() => close(), [location.pathname, location.search]); // eslint-disable-line react-hooks/exhaustive-deps
  useEffect(() => () => window.clearTimeout(timer.current), []);
  useEffect(() => {
    const onOpen = () => {
      setOpen(true);
      button.current?.focus();
    };
    window.addEventListener(FEATURES_EVENT, onOpen);
    return () => window.removeEventListener(FEATURES_EVENT, onOpen);
  }, []);
  useEffect(() => {
    if (!open) return;
    const outside = (e: MouseEvent) => {
      if (!wrap.current?.contains(e.target as Node)) close();
    };
    const key = (e: KeyboardEvent) => {
      if (e.key === "Escape") close(true);
    };
    document.addEventListener("mousedown", outside);
    document.addEventListener("keydown", key);
    return () => {
      document.removeEventListener("mousedown", outside);
      document.removeEventListener("keydown", key);
    };
  }, [open]);

  const hover = (i: number | null) => {
    window.clearTimeout(timer.current);
    timer.current = window.setTimeout(() => setFlyout(i), i === null ? 0 : FLYOUT_DELAY_MS);
  };

  return (
    <div className="menu features" ref={wrap}>
      <button
        type="button"
        ref={button}
        className="features-btn"
        aria-haspopup="menu"
        aria-expanded={open}
        aria-label="Features"
        onClick={() => (open ? close() : setOpen(true))}
      >
        <span>
          Fea<u>t</u>ures
        </span>{" "}
        <Icon name="chevron" size={12} />
      </button>
      {open && (
        <div className="menu-popup features-flyout" role="menu" aria-label="Features">
          {FEATURE_GROUPS.map((g, i) => (
            <div className="ff-group" key={g.title} role="group" aria-label={g.title}>
              <div className="ff-heading">{g.title}</div>
              <ul>
                {g.entries.map((e) => (
                  <li key={e.id} role="none" onMouseEnter={() => hover(null)}>
                    <Link role="menuitem" to={featureHref(e)}>
                      {e.label}
                    </Link>
                  </li>
                ))}
                {g.more.length > 0 && (
                  <li role="none" className="ff-more" onMouseEnter={() => hover(i)}>
                    <button
                      type="button"
                      role="menuitem"
                      aria-haspopup="menu"
                      aria-expanded={flyout === i}
                      onClick={() => setFlyout(flyout === i ? null : i)}
                      onKeyDown={(e) => {
                        if (e.key === "ArrowRight") setFlyout(i);
                        if (e.key === "ArrowLeft") setFlyout(null);
                      }}
                    >
                      <span>More</span>
                      <span aria-hidden="true">▸</span>
                    </button>
                    {flyout === i && (
                      <ul className="ff-sub" role="menu" aria-label={`More ${g.title}`}>
                        {g.more.map((e) => (
                          <li key={e.id} role="none">
                            <Link role="menuitem" to={featureHref(e)}>
                              {e.label}
                            </Link>
                          </li>
                        ))}
                      </ul>
                    )}
                  </li>
                )}
              </ul>
            </div>
          ))}
          <div className="ff-browse" onMouseEnter={() => hover(null)}>
            <Link role="menuitem" to={featureHref(BROWSE_ALL)}>
              {BROWSE_ALL.label}
            </Link>
          </div>
        </div>
      )}
    </div>
  );
}
