import { useEffect, useRef, useState, type ReactNode } from "react";
import { Link, useLocation, useNavigate } from "react-router-dom";
import { Icon } from "../components/Icons";
import { ErrorBoundary } from "../components/ErrorBoundary";
import { SearchBox } from "../components/SearchBox";
import { useHistoryButtons } from "./history";

export const FEATURES: { to: string; label: string }[] = [
  { to: "/kids", label: "Encarta Kids" },
  { to: "/dictionary", label: "Dictionary" },
  { to: "/thesaurus", label: "Thesaurus" },
  { to: "/translate", label: "Translations" },
  { to: "/atlas", label: "World Atlas" },
  { to: "/timeline", label: "Timeline" },
  { to: "/media", label: "Media Gallery" },
  { to: "/tours", label: "3D Virtual Tours" },
  { to: "/search", label: "Search" },
];

function FeaturesMenu() {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);
  const location = useLocation();
  useEffect(() => {
    setOpen(false);
  }, [location.pathname]);
  useEffect(() => {
    if (!open) return;
    const close = (e: MouseEvent) => {
      if (!ref.current?.contains(e.target as Node)) setOpen(false);
    };
    const esc = (e: KeyboardEvent) => e.key === "Escape" && setOpen(false);
    document.addEventListener("mousedown", close);
    document.addEventListener("keydown", esc);
    return () => {
      document.removeEventListener("mousedown", close);
      document.removeEventListener("keydown", esc);
    };
  }, [open]);
  return (
    <div className="features" ref={ref}>
      <button type="button" className="features-btn" aria-haspopup="menu" aria-expanded={open} onClick={() => setOpen(!open)}>
        <span>Feat<u>u</u>res</span> <Icon name="chevron" size={12} />
      </button>
      {open && (
        <ul className="features-menu" role="menu">
          {FEATURES.map((f) => (
            <li key={f.to} role="none">
              <Link role="menuitem" to={f.to}>
                {f.label}
              </Link>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

export function NavButtons({ homeTo = "/" }: { homeTo?: string }) {
  const navigate = useNavigate();
  const { canBack, canForward } = useHistoryButtons();
  return (
    <div className="nav-buttons">
      <div className="nav-pair">
        <button type="button" className="orb orb-back" disabled={!canBack} onClick={() => navigate(-1)} aria-label="Back" title="Back">
          <Icon name="back" size={22} />
        </button>
        <button type="button" className="orb orb-fwd" disabled={!canForward} onClick={() => navigate(1)} aria-label="Forward" title="Forward">
          <Icon name="forward" size={16} />
        </button>
      </div>
      <Link to={homeTo} className="orb orb-home" aria-label="Home" title="Home">
        <Icon name="home" size={22} />
      </Link>
    </div>
  );
}

export function Shell({ children }: { children: ReactNode }) {
  const location = useLocation();
  const home = location.pathname === "/";
  return (
    <div className={`shell${home ? " shell-home" : ""}`}>
      <header className="topbar">
        <NavButtons />
        <FeaturesMenu />
        <div className="topbar-search">
          <SearchBox />
        </div>
      </header>
      <main className="page">
        <ErrorBoundary key={location.pathname}>{children}</ErrorBoundary>
      </main>
    </div>
  );
}
