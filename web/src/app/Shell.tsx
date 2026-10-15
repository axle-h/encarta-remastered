import { useEffect, useId, useState, type ReactNode } from "react";
import { Link, useLocation, useNavigate } from "react-router-dom";
import { Icon } from "../components/Icons";
import { ErrorBoundary } from "../components/ErrorBoundary";
import { ArrowTip } from "../components/ArrowTip";
import { MenuButton, type MenuEntry } from "../components/Menu";
import { OptionsDialog } from "./OptionsDialog";
import { COPY_CHOICES, PRINT_CHOICES, ScopeDialog } from "./ScopeDialog";
import { FeaturesMenu } from "./FeaturesMenu";
import { useShortcuts } from "./shortcuts";
import { useDoubleClickLookup } from "./wordLookup";
import { SearchBox } from "../components/SearchBox";
import { COPY_EVENT, OPTIONS_EVENT, PRINT_EVENT, copyScope, copySelection, installCopySourceLine, openFind, printPage, printWith, selectedText, type ArticleScope } from "./commands";
import { canReadAloud, isReading, readAloud, readAloudSubmenu, stopReading } from "./readAloud";
import { AddFavoriteDialog, defaultFavoriteName } from "./AddFavoriteDialog";
import { favorites, pageTitle } from "./favorites";
import { useHistoryButtons } from "./history";
import { SearchPanel, type PanelQuery } from "./SearchPanel";
import { QuickSearchDialog } from "./QuickSearchDialog";
import { QUICK_SEARCH_EVENT } from "./searchPanel";
import { useSettings } from "../settings/settings";
import type { QuickKind } from "./featuresData";

/** How many favorites the menu lists (Organize Favorites shows them all). */
export const FAVORITES_IN_MENU = 20;

/** The Favorites menu's entries (adult and Kids): Add…, Organize…, then the saved pages or "<empty>". */
export function favoritesMenu(onAdd: () => void): MenuEntry[] {
  const list = favorites.list();
  const folders = favorites.folders();
  const top = list.filter((f) => !f.folder);
  return [
    { label: "Add to Favorites…", onSelect: onAdd, key: "fav-add" },
    { label: "Organize Favorites…", to: "/favorites", key: "fav-org" },
    { separator: true, key: "fav-sep" },
    ...(list.length === 0 && folders.length === 0 ? [{ label: "<empty>", disabled: true, key: "fav-empty" }] : []),
    ...folders.map((name) => {
      const inside = list.filter((f) => f.folder === name);
      return {
        label: name,
        key: `folder:${name}`,
        submenu: inside.length
          ? inside.slice(0, FAVORITES_IN_MENU).map((f) => ({ label: f.title, to: f.path, key: f.path }))
          : [{ label: "<empty>", disabled: true, key: `empty:${name}` }],
      };
    }),
    ...top.slice(0, FAVORITES_IN_MENU).map((f) => ({ label: f.title, to: f.path, key: f.path })),
    ...(top.length > FAVORITES_IN_MENU ? [{ label: `All ${list.length} favorites…`, to: "/favorites", key: "all" }] : []),
  ];
}

/**
 * Encarta's menu bar as captured: File (Print…, Go to ▸, Options…), Edit (Copy…, Read Aloud ▸,
 * Search Encarta, Find in this Article…), Favorites, Help. File › Exit is left out (a web page
 * does not close its window).
 */
export function MenuBar() {
  const location = useLocation();
  const navigate = useNavigate();
  const { canBack, canForward } = useHistoryButtons();
  const [note, setNote] = useState("");
  const here = `${location.pathname}${location.search}${location.hash}`;
  const flash = (msg: string) => {
    setNote(msg);
    window.setTimeout(() => setNote(""), 2500);
  };
  const [dialog, setDialog] = useState<null | "options" | "print" | "copy" | "favorite">(null);
  const onItem = location.pathname.startsWith("/item/");
  const isArticle = () => onItem && !!document.querySelector(".art-text");
  const print = () => (isArticle() ? setDialog("print") : printPage());
  const copied = (ok: boolean) => flash(ok ? "Copied." : "Nothing was copied.");
  const copy = () => {
    if (isArticle()) setDialog("copy");
    else void copySelection().then(copied);
  };
  const read = (what: "article" | "selection") => {
    if (!readAloud(what)) flash("Nothing to read aloud here.");
  };
  const focusSearch = () => {
    const box = document.querySelector<HTMLInputElement>(".topbar-search input");
    box?.focus();
    box?.select();
  };
  useEffect(() => {
    const on = (name: string, fn: () => void) => {
      window.addEventListener(name, fn);
      return () => window.removeEventListener(name, fn);
    };
    const key = (e: KeyboardEvent) => {
      if (e.key === "F8" && isArticle()) {
        e.preventDefault();
        if (!isReading()) read("article");
      } else if (e.key === "F9") {
        e.preventDefault();
        stopReading();
      }
    };
    window.addEventListener("keydown", key);
    const offs = [on(OPTIONS_EVENT, () => setDialog("options")), on(PRINT_EVENT, print), on(COPY_EVENT, copy), () => window.removeEventListener("keydown", key)];
    return () => offs.forEach((f) => f());
  });
  useEffect(installCopySourceLine, []);
  const choose = (kind: "print" | "copy", scope: ArticleScope) => {
    setDialog(null);
    if (kind === "print") window.setTimeout(() => printWith(scope), 50);
    else void copyScope(scope).then(copied);
  };
  const file = (): MenuEntry[] => [
    { label: "Print…", shortcut: "Ctrl+P", onSelect: print, key: "print" },
    { separator: true, key: "s1" },
    {
      label: "Go to",
      key: "goto",
      submenu: [
        { label: "Back", shortcut: "Alt+Left Arrow", disabled: !canBack, onSelect: () => navigate(-1), key: "g-back" },
        { label: "Forward", shortcut: "Alt+Right Arrow", disabled: !canForward, onSelect: () => navigate(1), key: "g-fwd" },
        { separator: true, key: "g-s" },
        { label: "Home Page", shortcut: "Alt+Home", to: "/", key: "g-home" },
      ],
    },
    { separator: true, key: "s2" },
    { label: "Options…", shortcut: "Ctrl+O", onSelect: () => setDialog("options"), key: "options" },
  ];
  const edit = (): MenuEntry[] => [
    { label: "Copy…", shortcut: "Ctrl+C", disabled: !selectedText().trim() && !onItem, onSelect: copy, key: "copy" },
    { separator: true, key: "s1" },
    { label: "Read Aloud", shortcut: "F8", disabled: !isArticle() || !canReadAloud(), key: "read-aloud", submenu: readAloudSubmenu(read) },
    { separator: true, key: "s2" },
    { label: "Search Encarta", shortcut: "Ctrl+S", onSelect: () => window.setTimeout(focusSearch, 0), key: "search" },
    { separator: true, key: "s3" },
    { label: "Find in this Article…", shortcut: "Ctrl+F", disabled: !onItem, onSelect: openFind, key: "find" },
  ];
  const favs = () => favoritesMenu(() => setDialog("favorite"));
  const help = (): MenuEntry[] => [
    { label: "Help", onSelect: () => navigate("/help"), key: "help" },
    { label: "Keyboard Shortcuts", onSelect: () => navigate("/help#keys") },
    { label: "About This Encarta", onSelect: () => navigate("/help") },
  ];
  const title = pageTitle(document.title);
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
      {dialog === "favorite" && (
        <AddFavoriteDialog
          name={defaultFavoriteName(title, isArticle())}
          onCancel={() => setDialog(null)}
          onAdd={(name, folder) => {
            favorites.add(here, name, folder);
            setDialog(null);
            flash(`“${name}” is in your Favorites.`);
          }}
        />
      )}
    </div>
  );
}

/** Back, Forward and Home with Encarta's arrow tooltips ("Back to <page>"; none on a disabled button). */
export function NavButtons({ homeTo = "/", homeTip = "Home" }: { homeTo?: string; homeTip?: string }) {
  const navigate = useNavigate();
  const { canBack, canForward, backTitle, forwardTitle } = useHistoryButtons();
  const back = (
    <button type="button" className="orb orb-back" disabled={!canBack} onClick={() => navigate(-1)} aria-label="Back">
      <Icon name="back" size={22} />
    </button>
  );
  const fwd = (
    <button type="button" className="orb orb-fwd" disabled={!canForward} onClick={() => navigate(1)} aria-label="Forward">
      <Icon name="forward" size={16} />
    </button>
  );
  return (
    <div className="nav-buttons">
      <div className="nav-pair">
        {canBack ? <ArrowTip text={backTitle ? `Back to ${backTitle}` : "Back"}>{back}</ArrowTip> : back}
        {canForward ? <ArrowTip text={forwardTitle ? `Forward to ${forwardTitle}` : "Forward"}>{fwd}</ArrowTip> : fwd}
      </div>
      <ArrowTip text={homeTip}>
        <Link to={homeTo} className="orb orb-home" aria-label="Home">
          <Icon name="home" size={22} />
        </Link>
      </ArrowTip>
    </div>
  );
}

/**
 * The search box with the ▾ tab under it, the results panel at the right and the Quick Search
 * dialog. Enter opens the panel; the tab toggles it; with "keep open" off a click on the page hides it.
 */
export function SearchArea() {
  const { settings } = useSettings();
  const [query, setQuery] = useState<PanelQuery | null>(null);
  const [open, setOpen] = useState(false);
  const [quick, setQuick] = useState<QuickKind | null>(null);
  const panelId = useId();
  const [top, setTop] = useState<number>();
  useEffect(() => {
    if (open) setTop(document.querySelector(".chrome")?.getBoundingClientRect().bottom || undefined);
  }, [open]);
  const location = useLocation();
  useEffect(() => {
    const on = (e: Event) => setQuick((e as CustomEvent<QuickKind>).detail);
    window.addEventListener(QUICK_SEARCH_EVENT, on);
    return () => window.removeEventListener(QUICK_SEARCH_EVENT, on);
  }, []);
  useEffect(() => setQuick(null), [location.pathname, location.search]);
  useEffect(() => {
    if (!open || settings.keepSearchOpen) return;
    const down = (e: MouseEvent) => {
      const t = e.target as Element | null;
      if (t?.closest?.(".search-panel, .search-tab, .topbar-search, .dlg-backdrop")) return;
      setOpen(false);
    };
    document.addEventListener("mousedown", down);
    return () => document.removeEventListener("mousedown", down);
  }, [open, settings.keepSearchOpen]);
  const search = (q: string, only?: QuickKind) => {
    setQuick(null);
    setQuery({ q, only });
    setOpen(true);
  };
  return (
    <>
      <div className="topbar-search" role="search">
        <SearchBox onSearch={(q) => search(q)} />
      </div>
      <button type="button" className="search-tab" aria-expanded={open} aria-controls={panelId} aria-label="Search results" title="Search results" onClick={() => setOpen(!open)}>
        <span aria-hidden="true">{open ? "▴" : "▾"}</span>
      </button>
      {open && <SearchPanel id={panelId} top={top} query={query} onClose={() => setOpen(false)} />}
      {quick && <QuickSearchDialog kind={quick} onSearch={(q) => search(q, quick)} onCancel={() => setQuick(null)} />}
    </>
  );
}

export function Shell({ children }: { children: ReactNode }) {
  const location = useLocation();
  const home = location.pathname === "/";
  useShortcuts();
  useDoubleClickLookup();
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
          <SearchArea />
        </header>
      </div>
      <main className="page" id="content" tabIndex={-1}>
        <ErrorBoundary key={location.pathname}>{children}</ErrorBoundary>
      </main>
    </div>
  );
}
