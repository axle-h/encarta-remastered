import { useEffect, useState, type ReactNode } from "react";
import { Link, useLocation, useNavigate } from "react-router-dom";
import { ErrorBoundary } from "../components/ErrorBoundary";
import { MenuButton, type MenuEntry } from "../components/Menu";
import { SearchBox } from "../components/SearchBox";
import { favoritesMenu } from "../app/Shell";
import { ArrowTip } from "../components/ArrowTip";
import { PrintHost } from "../app/PrintDialog";
import { AddFavoriteDialog, defaultFavoriteName } from "../app/AddFavoriteDialog";
import { canReadAloud, isReading, readAloud, readAloudSubmenu, stopReading } from "../app/readAloud";
import { useHistoryButtons } from "../app/history";
import { copySelectionOrArticle, installCopySourceLine, openFind, requestPrint } from "../app/commands";
import { favorites, pageTitle } from "../app/favorites";
import { useSettings } from "../settings/settings";

const tick = (on: boolean, label: string) => `${on ? "✓ " : " "}${label}`;

/** The underlined access key of each Kids menu label (VM captures `fixtures/ui/kids/menu-*`, `sub-*`). */
const ACCESS_KEYS: Record<string, string> = {
  "Print...": "P",
  Exit: "x",
  Copy: "C",
  "Find in this Article...": "F",
  "Go to": "G",
  "Closed Captioning": "C",
  Back: "B",
  Forward: "F",
  "Home Page": "m",
  On: "n",
  Off: "O",
  "Add to Favorites...": "F",
  "Organize Favorites...": "O",
  "Read Aloud": "a",
  "Menu and Button Sounds": "M",
  "Whole Article": "W",
  "Selected Text": "T",
  "Stop Reading": "S",
  Pause: "P",
  "Microsoft Encarta Help": "H",
  Troubleshooting: "T",
  "Diagnosing Problems": "D",
  "Contacting Technical Support": "S",
};

const SHORTCUTS: Record<string, string> = { "Alt+Left": "Alt + Left Arrow", "Alt+Right": "Alt + Right Arrow", "Alt+Home": "Alt + Home" };

/**
 * Kids menus as Encarta Kids draws them: "..." for an ellipsis, the access key underlined, shortcuts
 * spaced ("Ctrl + P", "Alt + Left Arrow") and the tick in the left margin.
 */
function kidsEntries(entries: MenuEntry[]): MenuEntry[] {
  return entries.map((m) => {
    const submenu = m.submenu && kidsEntries(m.submenu);
    if (m.separator || typeof m.label !== "string") return { ...m, submenu };
    const checked = m.label.startsWith("✓ ");
    const text = m.label.replace(/^(✓ |\u2003)/, "").replace(/…/g, "...");
    const key = ACCESS_KEYS[text];
    const at = key ? text.indexOf(key) : -1;
    const label = (
      <>
        {checked && <span className="menu-check">✓</span>}
        {at < 0 ? text : <>{text.slice(0, at)}<u>{text[at]}</u>{text.slice(at + 1)}</>}
      </>
    );
    const shortcut = m.shortcut && (SHORTCUTS[m.shortcut] ?? m.shortcut.replace(/\+/g, " + "));
    return { ...m, label, ariaLabel: shortcut ? `${text} ${shortcut}` : text, shortcut, submenu };
  });
}

/** Kids menu bar: File, Edit, View, Favorites, Tools, Help (items and submenus as captured). */
export function KidsMenuBar() {
  const location = useLocation();
  const navigate = useNavigate();
  const { settings, update } = useSettings();
  const { canBack, canForward } = useHistoryButtons();
  const [note, setNote] = useState("");
  const flash = (msg: string) => {
    setNote(msg);
    window.setTimeout(() => setNote(""), 2500);
  };
  const here = `${location.pathname}${location.search}${location.hash}`;
  const isArticle = location.pathname.startsWith("/kids/item/");
  const [adding, setAdding] = useState(false);
  const copy = () => void copySelectionOrArticle().then((ok) => flash(ok ? "Copied." : "Nothing was copied."));
  const read = (what: "article" | "selection" = "article") => {
    if (!readAloud(what)) flash("Nothing to read aloud here.");
  };
  const atHome = location.pathname === "/kids";
  useEffect(() => {
    const key = (e: KeyboardEvent) => {
      const mod = e.ctrlKey || e.metaKey;
      if (e.key === "F1") {
        e.preventDefault();
        navigate("/help");
      } else if (e.key === "F8" && isArticle) {
        e.preventDefault();
        if (!isReading()) read();
      } else if (e.key === "F9") {
        e.preventDefault();
        stopReading();
      } else if (mod && e.key.toLowerCase() === "p") {
        e.preventDefault();
        requestPrint();
      } else if (mod && e.key.toLowerCase() === "l") {
        e.preventDefault();
        update({ captions: !settings.captions });
      } else if (mod && e.key.toLowerCase() === "f" && isArticle) {
        e.preventDefault();
        openFind();
      } else if (e.altKey && e.key === "ArrowLeft") {
        e.preventDefault();
        if (canBack) navigate(-1);
      } else if (e.altKey && e.key === "ArrowRight") {
        e.preventDefault();
        if (canForward) navigate(1);
      } else if (e.altKey && e.key === "Home") {
        e.preventDefault();
        navigate("/kids");
      }
    };
    window.addEventListener("keydown", key);
    return () => window.removeEventListener("keydown", key);
  });
  const file = (): MenuEntry[] => [
    { label: "Print...", shortcut: "Ctrl+P", disabled: !isArticle, onSelect: requestPrint, key: "print" },
    { separator: true, key: "s1" },
    { label: "Exit", to: "/", key: "exit" },
  ];
  const edit = (): MenuEntry[] => [
    { label: "Copy", shortcut: "Ctrl+C", disabled: !isArticle, onSelect: copy, key: "copy" },
    { separator: true, key: "s1" },
    { label: "Find in this Article...", shortcut: "Ctrl+F", disabled: !isArticle, onSelect: openFind, key: "find" },
  ];
  const view = (): MenuEntry[] => [
    {
      label: "Go to",
      key: "goto",
      submenu: [
        { label: "Back", shortcut: "Alt+Left", disabled: !canBack, onSelect: () => navigate(-1), key: "back" },
        { label: "Forward", shortcut: "Alt+Right", disabled: !canForward, onSelect: () => navigate(1), key: "fwd" },
        { separator: true, key: "s1" },
        { label: "Home Page", shortcut: "Alt+Home", disabled: atHome, to: "/kids", key: "home" },
      ],
    },
    { separator: true, key: "s1" },
    {
      label: "Closed Captioning",
      shortcut: "Ctrl+L",
      key: "cc",
      submenu: [
        { label: tick(settings.captions, "On"), onSelect: () => update({ captions: true }), key: "cc-on" },
        { label: tick(!settings.captions, "Off"), onSelect: () => update({ captions: false }), key: "cc-off" },
      ],
    },
  ];
  const favs = () => favoritesMenu(() => setAdding(true));
  const onOff = (on: boolean, set: (v: boolean) => void, key: string): MenuEntry[] => [
    { label: tick(on, "On"), onSelect: () => set(true), key: `${key}-on` },
    { label: tick(!on, "Off"), onSelect: () => set(false), key: `${key}-off` },
  ];
  const tools = (): MenuEntry[] => {
    const speech = isArticle && canReadAloud();
    return [
      { label: "Find in this Article...", shortcut: "Ctrl+F", disabled: !isArticle, onSelect: openFind, key: "find" },
      { separator: true, key: "s1" },
      {
        label: "Read Aloud",
        shortcut: "F8",
        disabled: !speech,
        key: "ra",
        submenu: readAloudSubmenu(read),
      },
      { separator: true, key: "s2" },
      { label: "Menu and Button Sounds", key: "ms", submenu: onOff(settings.kidsMenuSounds, (v) => update({ kidsMenuSounds: v }), "ms") },
    ];
  };
  const help = (): MenuEntry[] => [
    { label: "Microsoft Encarta Help", shortcut: "F1", to: "/help", key: "help" },
    { separator: true, key: "s0" },
    {
      label: "Troubleshooting",
      key: "ts",
      submenu: [
        { label: "Diagnosing Problems", to: "/help", key: "diag" },
        { label: "Contacting Technical Support", to: "/help", key: "support" },
      ],
    },
    { separator: true, key: "s1" },
    { label: "About Encarta", to: "/help", key: "about" },
  ];
  // The article's right-click menu (VM capture dict-context-menu).
  const context = (): MenuEntry[] => [
    { label: "Find in this Article...", shortcut: "Ctrl+F", onSelect: openFind, key: "find" },
    { separator: true, key: "s1" },
    { label: "Copy", shortcut: "Ctrl+C", onSelect: copy, key: "copy" },
    { label: "Print...", shortcut: "Ctrl+P", onSelect: requestPrint, key: "print" },
    { label: "Add to Favorites...", onSelect: () => setAdding(true), key: "fav" },
    { separator: true, key: "s2" },
    { label: "Read Aloud", shortcut: "F8", disabled: !canReadAloud(), key: "ra", submenu: readAloudSubmenu(read) },
  ];
  const btn = "menubar-btn";
  return (
    <div className="kids-menu menubar" role="menubar" aria-label="Menu bar">
      <MenuButton label={<><u>F</u>ile</>} ariaLabel="File" entries={() => kidsEntries(file())} buttonClassName={btn} menuClassName="kids-popup" />
      <MenuButton label={<><u>E</u>dit</>} ariaLabel="Edit" entries={() => kidsEntries(edit())} buttonClassName={btn} menuClassName="kids-popup" />
      <MenuButton label={<><u>V</u>iew</>} ariaLabel="View" entries={() => kidsEntries(view())} buttonClassName={btn} menuClassName="kids-popup" />
      <MenuButton label={<>F<u>a</u>vorites</>} ariaLabel="Favorites" entries={() => kidsEntries(favs())} buttonClassName={btn} menuClassName="kids-popup" />
      <MenuButton label={<><u>T</u>ools</>} ariaLabel="Tools" entries={() => kidsEntries(tools())} buttonClassName={btn} menuClassName="kids-popup" />
      <MenuButton label={<><u>H</u>elp</>} ariaLabel="Help" entries={() => kidsEntries(help())} buttonClassName={btn} menuClassName="kids-popup" />
      <MenuButton contextFor=".kids-art" ariaLabel="Article" entries={() => kidsEntries(context())} menuClassName="kids-popup" />
      <span className="menubar-note" role="status" aria-live="polite">
        {note}
      </span>
      <PrintHost kids />
      {adding && (
        <AddFavoriteDialog
          name={defaultFavoriteName(pageTitle(document.title), isArticle)}
          onCancel={() => setAdding(false)}
          onAdd={(name, folder) => {
            favorites.add(here, name, folder);
            setAdding(false);
            flash(`“${name}” is in your Favorites.`);
          }}
        />
      )}
    </div>
  );
}

/**
 * The Kids round buttons, drawn from the disc's button strips (`/ui/kids-*.png`: normal, mouse-over,
 * disabled, pressed; docs/vm-findings.md "Kids chrome"). Disabled ones get no tooltip, as in Encarta.
 */
function KidsButton({ name, label, tip, disabled = false, onClick, to }: { name: string; label: string; tip: string; disabled?: boolean; onClick?: () => void; to?: string }) {
  const cls = `orb kids-btn kids-btn-${name}`;
  const el = to ? (
    <Link to={to} className={cls} aria-label={label} />
  ) : (
    <button type="button" className={cls} aria-label={label} disabled={disabled} onClick={onClick} />
  );
  return disabled ? el : <ArrowTip text={tip}>{el}</ArrowTip>;
}

/** Encarta Kids: its own lime chrome, search limited to the Kids edition, links stay in /kids. */
export function KidsShell({ children }: { children: ReactNode }) {
  const location = useLocation();
  const navigate = useNavigate();
  const { canBack, canForward, backTitle, forwardTitle } = useHistoryButtons();
  const [note, setNote] = useState("");
  const copy = () => void copySelectionOrArticle().then((ok) => setNote(ok ? "Copied." : "Nothing was copied."));
  // Copy and Print are greyed out on the home and category screens (VM captures).
  const onItem = location.pathname.startsWith("/kids/item/");
  useEffect(installCopySourceLine, []);
  return (
    <div className="shell kids-shell">
      <header className="kids-bar">
        <span className="kids-bar-left" aria-hidden="true" />
        <KidsButton name="home" label="Home" tip="Open the Encarta Kids home page." to="/kids" />
        <KidsButton name="back" label="Back" tip={backTitle ? `Back to ${backTitle}` : "Back"} disabled={!canBack} onClick={() => navigate(-1)} />
        <KidsButton name="forward" label="Forward" tip={forwardTitle ? `Forward to ${forwardTitle}` : "Forward"} disabled={!canForward} onClick={() => navigate(1)} />
        <span className="kids-bar-joint" aria-hidden="true" />
        <KidsButton name="copy" label="Copy" tip="Copy the article or picture." disabled={!onItem} onClick={copy} />
        <span className="kids-bar-gap-copy" aria-hidden="true" />
        <KidsButton name="print" label="Print" tip="Print the article or picture." disabled={!onItem} onClick={requestPrint} />
        <span className="kids-bar-gap-print" aria-hidden="true" />
        <KidsMenuBar />
        <div className="kids-search">
          <span className="kids-search-label" aria-hidden="true" />
          <SearchBox kids placeholder="" />
        </div>
        <Link to="/kids" className="kids-logo" aria-label="Encarta Kids home" />
        <span className="sr-only" role="status" aria-live="polite">{note}</span>
      </header>
      <main className="page kids-page">
        <ErrorBoundary key={location.pathname}>{children}</ErrorBoundary>
      </main>
    </div>
  );
}
