import { useEffect, useState, type ReactNode } from "react";
import { Link, useLocation, useNavigate } from "react-router-dom";
import { Icon } from "../components/Icons";
import { ErrorBoundary } from "../components/ErrorBoundary";
import { MenuButton, type MenuEntry } from "../components/Menu";
import { OptionsDialog } from "./OptionsDialog";
import { COPY_CHOICES, PRINT_CHOICES, ScopeDialog } from "./ScopeDialog";
import { FeaturesMenu } from "./FeaturesMenu";
import { useShortcuts } from "./shortcuts";
import { SearchBox } from "../components/SearchBox";
import { COPY_EVENT, OPTIONS_EVENT, PRINT_EVENT, copyScope, copySelection, openFind, printPage, printWith, selectedText, type ArticleScope } from "./commands";
import { favorites, pageTitle } from "./favorites";
import { useHistoryButtons } from "./history";

/** How many favorites the menu lists (Organize Favorites shows them all). */
export const FAVORITES_IN_MENU = 20;

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
  const [dialog, setDialog] = useState<null | "options" | "print" | "copy">(null);
  const isArticle = () => location.pathname.startsWith("/item/") && !!document.querySelector(".art-text");
  const print = () => (isArticle() ? setDialog("print") : printPage());
  const copy = () => {
    if (isArticle()) setDialog("copy");
    else void copySelection(pageTitle(document.title)).then((ok) => flash(ok ? "Copied, with its source." : "Nothing was copied."));
  };
  useEffect(() => {
    const on = (name: string, fn: () => void) => {
      window.addEventListener(name, fn);
      return () => window.removeEventListener(name, fn);
    };
    const offs = [on(OPTIONS_EVENT, () => setDialog("options")), on(PRINT_EVENT, print), on(COPY_EVENT, copy)];
    return () => offs.forEach((f) => f());
  });
  const choose = (kind: "print" | "copy", scope: ArticleScope) => {
    setDialog(null);
    const title = pageTitle(document.title);
    if (kind === "print") window.setTimeout(() => printWith(scope, title), 50);
    else void copyScope(scope, title).then((ok) => flash(ok ? "Copied, with its source." : "Nothing was copied."));
  };
  const file = (): MenuEntry[] => [
    { heading: true, label: "Go to", key: "goto" },
    { label: "Home", to: "/", shortcut: "Alt+Home", key: "g-home" },
    { label: "World Atlas", to: "/atlas", shortcut: "Ctrl+M", key: "g-atlas" },
    { label: "Dictionary", to: "/dictionary", shortcut: "Ctrl+D", key: "g-dict" },
    { label: "Timeline", to: "/timeline", shortcut: "Ctrl+T", key: "g-tl" },
    { label: "Geography Quiz", to: "/games/geoquiz", shortcut: "Ctrl+Q", key: "g-quiz" },
    { label: "Games & Fun Stuff", to: "/games", key: "g-games" },
    { separator: true, key: "sep1" },
    { label: "Print…", shortcut: "Ctrl+P", onSelect: print },
    { label: "Options…", shortcut: "Ctrl+O", onSelect: () => setDialog("options") },
  ];
  const edit = (): MenuEntry[] => [
    {
      label: "Copy",
      disabled: !selectedText().trim() && !location.pathname.startsWith("/item/"),
      onSelect: copy,
    },
    { label: "Read Aloud", disabled: true, key: "read-aloud" },
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
    { label: "Help", onSelect: () => navigate("/help"), key: "help" },
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
      {dialog === "options" && <OptionsDialog onClose={() => setDialog(null)} />}
      {dialog === "print" && (
        <ScopeDialog title="Print Options" action="Print" choices={PRINT_CHOICES} hasSelection={!!selectedText().trim()} onChoose={(sc) => choose("print", sc)} onCancel={() => setDialog(null)} />
      )}
      {dialog === "copy" && (
        <ScopeDialog title="Copy Options" action="Copy" choices={COPY_CHOICES} hasSelection={!!selectedText().trim()} onChoose={(sc) => choose("copy", sc)} onCancel={() => setDialog(null)} />
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
  useShortcuts();
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
