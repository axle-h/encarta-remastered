import type { ReactNode } from "react";
import { Link, useLocation } from "react-router-dom";
import { ErrorBoundary } from "../components/ErrorBoundary";
import { NavButtons } from "../app/Shell";
import { SearchBox } from "../components/SearchBox";

/** Encarta Kids: its own bright chrome, search limited to the Kids edition, links stay in /kids. */
export function KidsShell({ children }: { children: ReactNode }) {
  const location = useLocation();
  return (
    <div className="shell kids-shell">
      <header className="kids-bar">
        <NavButtons homeTo="/kids" />
        <nav className="kids-menu" aria-label="Encarta Kids">
          <Link to="/kids/dictionary">Dictionary</Link>
          <Link to="/">Encarta Premium</Link>
        </nav>
        <div className="kids-search">
          <span className="kids-search-label">search</span>
          <SearchBox kids placeholder="" />
        </div>
        <Link to="/kids" className="kids-logo" aria-label="Encarta Kids home">
          <span className="kids-logo-small">Encarta</span>
          <span className="kids-logo-big">kids</span>
        </Link>
      </header>
      <main className="page kids-page">
        <ErrorBoundary key={location.pathname}>{children}</ErrorBoundary>
      </main>
    </div>
  );
}
