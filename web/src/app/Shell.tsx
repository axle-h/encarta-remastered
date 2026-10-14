import { useState, type ReactNode } from "react";
import { Link, useLocation, useNavigate } from "react-router-dom";
import { Icon } from "../components/Icons";
import { ErrorBoundary } from "../components/ErrorBoundary";
import { MenuButton, type MenuEntry } from "../components/Menu";
import { SearchBox } from "../components/SearchBox";
import { copySelection, openFind, printPage, selectedText } from "./commands";
import { favorites, pageTitle } from "./favorites";
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
  { to: "/games/geoquiz", label: "Geography Quiz" },
  { to: "/games", label: "Games & Quizzes" },
];

/** Rebuilt from data on the disc that Encarta 2009 itself no longer opens (docs/games.md). */
export const EXTRAS: { to: string; label: string }[] = [
  { to: "/games/mindmaze", label: "MindMaze" },
  { to: "/games/city-climate", label: "City Climate" },
  { to: "/games/population", label: "Population Chart" },
];

/** How many favorites the menu lists (Organize Favorites shows them all). */
export const FAVORITES_IN_MENU = 20;

function FeaturesMenu() {
  const entries = (): MenuEntry[] => [
    ...FEATURES.map((f) => ({ label: f.label, to: f.to, key: f.to })),
    { heading: true, label: "Extras", key: "extras" },
    ...EXTRAS.map((f) => ({ label: f.label, to: f.to, key: f.to })),
  ];
  return (
    <MenuButton
      className="features"
      buttonClassName="features-btn"
      menuClassName="features-menu"
      ariaLabel="Features"
      label={
        <>
          <span>
            Feat<u>u</u>res
          </span>{" "}
          <Icon name="chevron" size={12} />
        </>
      }
      entries={entries}
    />
  );
}

/** Encarta's menu bar: File, Edit, Favorites, Help. */
export function MenuBar() {
  const location = useLocation();
  const navigate = useNavigate();
  const [note, setNote] = useState("");
  const here = `${location.pathname}${location.search}${location.hash}`;
  const flash = (msg: string) => {
    setNote(msg);
    window.setTimeout(() => setNote(""), 2500);
  };
  const file = (): MenuEntry[] => [{ label: "Print…", shortcut: "Ctrl+P", onSelect: printPage }];
  const edit = (): MenuEntry[] => [
    {
      label: "Copy",
      disabled: !selectedText().trim(),
      onSelect: () => void copySelection(pageTitle(document.title)).then((ok) => flash(ok ? "Copied, with its source." : "Nothing was copied.")),
    },
    { label: "Find in Article…", shortcut: "Ctrl+F", disabled: !location.pathname.startsWith("/item/"), onSelect: openFind },
  ];
  const favs = (): MenuEntry[] => {
    const list = favorites.list();
    const saved = list.some((f) => f.path === here);
    return [
      {
        label: saved ? "Rename in Favorites" : "Add to Favorites",
        onSelect: () => {
          favorites.add(here, pageTitle(document.title));
          flash(`“${pageTitle(document.title)}” is in your Favorites.`);
        },
      },
      { label: "Organize Favorites…", to: "/favorites" },
      ...(list.length > 0 ? [{ separator: true, key: "sep" }] : []),
      ...list.slice(0, FAVORITES_IN_MENU).map((f) => ({ label: f.title, to: f.path, key: f.path })),
      ...(list.length > FAVORITES_IN_MENU ? [{ label: `All ${list.length} favorites…`, to: "/favorites", key: "all" }] : []),
    ];
  };
  const help = (): MenuEntry[] => [
    { label: "Keyboard Shortcuts", onSelect: () => navigate("/help#keys") },
    { label: "About This Encarta", onSelect: () => navigate("/help") },
  ];
  return (
    <div className="menubar" role="menubar" aria-label="Menu bar">
      <MenuButton label={<><u>F</u>ile</>} ariaLabel="File" entries={file} buttonClassName="menubar-btn" />
      <MenuButton label={<><u>E</u>dit</>} ariaLabel="Edit" entries={edit} buttonClassName="menubar-btn" />
      <MenuButton label={<>F<u>a</u>vorites</>} ariaLabel="Favorites" entries={favs} buttonClassName="menubar-btn" />
      <MenuButton label={<><u>H</u>elp</>} ariaLabel="Help" entries={help} buttonClassName="menubar-btn" />
      <span className="menubar-note" role="status" aria-live="polite">
        {note}
      </span>
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
      <a className="skip-link" href="#content">
        Skip to content
      </a>
      <div className="chrome">
        <MenuBar />
        <header className="topbar">
          <NavButtons />
          <FeaturesMenu />
          <div className="topbar-search" role="search">
            <SearchBox />
          </div>
        </header>
      </div>
      <main className="page" id="content" tabIndex={-1}>
        <ErrorBoundary key={location.pathname}>{children}</ErrorBoundary>
      </main>
    </div>
  );
}
