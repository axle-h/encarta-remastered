import { useEffect, useState, type ReactNode } from "react";
import { Link, useLocation, useNavigate } from "react-router-dom";
import { ErrorBoundary } from "../components/ErrorBoundary";
import { MenuButton, type MenuEntry } from "../components/Menu";
import { SearchBox } from "../components/SearchBox";
import { NavButtons } from "../app/Shell";
import { useHistoryButtons } from "../app/history";
import { articleText, copySelection, openFind, printPage, selectedText } from "../app/commands";
import { favorites, pageTitle } from "../app/favorites";
import { useSettings } from "../settings/settings";

function synthesis(): SpeechSynthesis | undefined {
  return typeof window !== "undefined" ? window.speechSynthesis : undefined;
}

/** Reads the whole article, or only the selection, with the browser's speech synthesis. */
export function readAloud(what: "article" | "selection"): boolean {
  const synth = synthesis();
  if (!synth) return false;
  const text = what === "selection" ? selectedText().trim() : articleText();
  if (!text) return false;
  synth.cancel();
  synth.speak(new SpeechSynthesisUtterance(text));
  return true;
}

export function stopReading() {
  synthesis()?.cancel();
}

/** Pauses reading, or resumes it when paused. */
export function togglePause() {
  const synth = synthesis();
  if (!synth) return;
  if (synth.paused) synth.resume();
  else synth.pause();
}

export const isReading = () => !!synthesis()?.speaking;

const tick = (on: boolean, label: string) => `${on ? "✓ " : " "}${label}`;

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
  const copy = () => void copySelection(pageTitle(document.title)).then((ok) => flash(ok ? "Copied, with its source." : "Select some text first."));
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
        read();
      } else if (e.key === "F9") {
        e.preventDefault();
        stopReading();
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
    { label: "Print", shortcut: "Ctrl+P", onSelect: printPage },
    { label: "Exit", to: "/" },
  ];
  const edit = (): MenuEntry[] => [
    { label: "Copy", onSelect: copy },
    { label: "Find in this Article…", shortcut: "Ctrl+F", disabled: !isArticle, onSelect: openFind },
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
  const favs = (): MenuEntry[] => [
    {
      label: "Add",
      onSelect: () => {
        favorites.add(here, pageTitle(document.title));
        flash(`“${pageTitle(document.title)}” is in your Favorites.`);
      },
    },
    { label: "Organize", to: "/favorites" },
  ];
  const onOff = (on: boolean, set: (v: boolean) => void, key: string): MenuEntry[] => [
    { label: tick(on, "On"), onSelect: () => set(true), key: `${key}-on` },
    { label: tick(!on, "Off"), onSelect: () => set(false), key: `${key}-off` },
  ];
  const tools = (): MenuEntry[] => {
    const speech = isArticle && !!synthesis();
    const reading = isReading();
    return [
      { label: "Find in this Article...", shortcut: "Ctrl+F", disabled: !isArticle, onSelect: openFind, key: "find" },
      { separator: true, key: "s1" },
      {
        label: "Read Aloud",
        shortcut: "F8",
        disabled: !speech,
        key: "ra",
        submenu: [
          { label: "Whole Article", onSelect: () => read("article"), key: "ra-all" },
          { label: "Selected Text", disabled: !selectedText().trim(), onSelect: () => read("selection"), key: "ra-sel" },
          { separator: true, key: "ra-s" },
          { label: "Stop Reading", shortcut: "F9", disabled: !reading, onSelect: stopReading, key: "ra-stop" },
          { label: synthesis()?.paused ? "Resume" : "Pause", disabled: !reading, onSelect: togglePause, key: "ra-pause" },
        ],
      },
      { separator: true, key: "s2" },
      { label: "Menu and Button Sounds", key: "ms", submenu: onOff(settings.menuSounds, (v) => update({ menuSounds: v }), "ms") },
      { label: "Startup Sound", key: "ss", submenu: onOff(settings.startupSound, (v) => update({ startupSound: v }), "ss") },
    ];
  };
  const help = (): MenuEntry[] => [
    { label: "Microsoft Encarta Help", shortcut: "F1", to: "/help", key: "help" },
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
  const btn = "menubar-btn";
  return (
    <div className="kids-menu menubar" role="menubar" aria-label="Menu bar">
      <MenuButton label={<><u>F</u>ile</>} ariaLabel="File" entries={file} buttonClassName={btn} menuClassName="kids-popup" />
      <MenuButton label={<><u>E</u>dit</>} ariaLabel="Edit" entries={edit} buttonClassName={btn} menuClassName="kids-popup" />
      <MenuButton label={<><u>V</u>iew</>} ariaLabel="View" entries={view} buttonClassName={btn} menuClassName="kids-popup" />
      <MenuButton label={<>F<u>a</u>vorites</>} ariaLabel="Favorites" entries={favs} buttonClassName={btn} menuClassName="kids-popup" />
      <MenuButton label={<><u>T</u>ools</>} ariaLabel="Tools" entries={tools} buttonClassName={btn} menuClassName="kids-popup" />
      <MenuButton label={<><u>H</u>elp</>} ariaLabel="Help" entries={help} buttonClassName={btn} menuClassName="kids-popup" />
      <span className="menubar-note" role="status" aria-live="polite">
        {note}
      </span>
    </div>
  );
}

/** Encarta Kids: its own bright chrome, search limited to the Kids edition, links stay in /kids. */
export function KidsShell({ children }: { children: ReactNode }) {
  const location = useLocation();
  const [note, setNote] = useState("");
  const copy = () => void copySelection(pageTitle(document.title)).then((ok) => setNote(ok ? "Copied, with its source." : "Select some text first."));
  return (
    <div className="shell kids-shell">
      <header className="kids-bar" title="Microsoft Encarta Kids 2009">
        <NavButtons homeTo="/kids" />
        <button type="button" className="orb kids-tool" aria-label="Copy" title="Copy" onClick={copy}>
          <span aria-hidden="true">⎘</span>
        </button>
        <button type="button" className="orb kids-tool" aria-label="Print" title="Print" onClick={printPage}>
          <span aria-hidden="true">⎙</span>
        </button>
        <KidsMenuBar />
        <div className="kids-search">
          <span className="kids-search-label">search</span>
          <SearchBox kids placeholder="" />
        </div>
        <Link to="/kids" className="kids-logo" aria-label="Encarta Kids home">
          <span className="kids-logo-small">Encarta</span>
          <span className="kids-logo-big">kids</span>
        </Link>
        <span className="sr-only" role="status" aria-live="polite">{note}</span>
      </header>
      <main className="page kids-page">
        <ErrorBoundary key={location.pathname}>{children}</ErrorBoundary>
      </main>
    </div>
  );
}
