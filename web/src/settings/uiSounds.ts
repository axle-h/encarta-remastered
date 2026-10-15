/**
 * Interface sounds (Tools > Options > Sounds). The converter writes them from the disc's DLL
 * resources to `out/sounds/`; the server serves `/sounds/{name}.wav`. Browsers refuse sound before
 * the first user gesture, so a startup sound that is refused plays on the first click or key.
 *
 * Which control plays what was measured in the XP reference VM (docs/media.md section 9):
 * `tick` is the rollover of toolbar buttons, menu-bar titles, atlas text buttons and menu highlights;
 * `click` the press of buttons (round toolbar buttons and menu-bar items: on press and again on
 * release; dialog buttons, radios and check boxes: once, on release, and never from the keyboard);
 * `kidsHover` the
 * rollover of the Kids round buttons; `timeline` the timeline controls; `tourHover` / `tourPick` the
 * pointer entering / clicking a stop on a tour's site map (both nearly silent). Plain links and tabs are
 * silent. Encarta Kids has its own "Menu and Button Sounds" switch, on by default (adult: off).
 * One document-level delegate ({@link installUiSoundDelegates}) maps controls to sounds;
 * `data-snd="timeline"` (or `"none"`) on an element or ancestor overrides the built-in rules.
 */
export type UiSound = "startup" | "click" | "tick" | "kidsHover" | "timeline" | "tourHover" | "tourPick" | `kids-${number}`;

const FILES: Record<string, string> = { kidsHover: "kids-hover", tourHover: "tour-hover", tourPick: "tour-pick" };

const flags = { startup: true, menu: false, kidsMenu: true };
const players = new Map<string, HTMLAudioElement>();
let startupDone = false;

/** Called by the settings provider whenever the sound settings change (`kidsMenu`: the Kids switch). */
export function configureUiSounds(startup: boolean, menu: boolean, kidsMenu = menu) {
  flags.startup = startup;
  flags.menu = menu;
  flags.kidsMenu = kidsMenu;
}

/** Encarta Kids keeps its own menu-sound switch; the Kids shell is on screen while it is in use. */
function menuSoundsOn(): boolean {
  const kids = typeof document !== "undefined" && document.querySelector(".kids-shell") !== null;
  return kids ? flags.kidsMenu : flags.menu;
}

function play(name: UiSound, volume: number): Promise<void> {
  if (typeof Audio === "undefined") return Promise.resolve();
  try {
    let a = players.get(name);
    if (!a) {
      a = new Audio(`/sounds/${FILES[name] ?? name}.wav`);
      a.preload = "auto";
      players.set(name, a);
    }
    a.volume = volume;
    a.currentTime = 0;
    return Promise.resolve(a.play()).catch(() => Promise.reject(new Error("blocked")));
  } catch {
    return Promise.resolve();
  }
}

/** A menu or button sound (the "Play menu and button sounds" option); silent when it is off. */
export function playUiSound(name: UiSound) {
  if (menuSoundsOn()) void play(name, name === "click" ? 0.7 : 0.8).catch(() => {});
}

/** Alias of {@link playUiSound} (default: the click). */
export function playMenuSound(name: UiSound = "click") {
  playUiSound(name);
}

interface Spec {
  over?: UiSound;
  down?: UiSound;
  up?: UiSound;
  click?: UiSound;
}

const ROUND: Spec = { down: "click", up: "click" };
/** Items of the menu-bar menus: highlight tick, click on press and again on release (VM: 31973, 31974). */
const MENU_ITEM: Spec = { over: "tick", ...ROUND };
const RULES: [string, Spec][] = [
  [".kids-shell .orb, .kids-shell .searchbox-go", { over: "kidsHover", ...ROUND }],
  [".orb", { over: "tick", ...ROUND }],
  [".searchbox-go", { over: "tick", click: "click" }],
  // The frameless "longer than 5 pages" prompt: Yes/No click, no rollover tick (VM pass 5).
  [".print-close", {}], // the window frame's close box is Windows', not Encarta's
  [".print-prompt button", { click: "click" }],
  [".menu.features > button, .enc-atlas-btn, .dlg-backdrop button:not([role=tab])", { over: "tick", click: "click" }],
  [".pano-site-pt:not(.current)", { over: "tourHover", click: "tourPick" }],
  // The Features flyout's items are silent in Encarta (VM pass 2 listened to them; vm-findings.md).
  ['.features-flyout [role="menuitem"]', {}],
  ['[role="menuitem"]', MENU_ITEM],
  [".menu > button", { over: "tick", click: "click" }],
  [".enc-atlas button, input[type=checkbox], input[type=radio]", { click: "click" }],
];

function disabled(el: Element): boolean {
  return el.hasAttribute("disabled") || el.getAttribute("aria-disabled") === "true";
}

/** The sounds of the control under `target`, with the element they belong to; null for none. */
function specOf(target: EventTarget | null): { el: Element; spec: Spec } | null {
  const t = target as Element | null;
  if (!t?.closest) return null;
  const forced = t.closest("[data-snd]");
  if (forced) {
    const v = forced.getAttribute("data-snd");
    const b = t.closest("button");
    return v === "timeline" && b && !disabled(b) ? { el: b, spec: { click: "timeline" } } : null;
  }
  for (const [sel, spec] of RULES) {
    const el = t.closest(sel);
    if (el && !disabled(el)) return { el, spec };
  }
  return null;
}

/** Installs the delegate that plays the interface sounds; returns its remover. */
export function installUiSoundDelegates(): () => void {
  let keyboard = false;
  const on = <K extends keyof DocumentEventMap>(type: K, fn: (e: DocumentEventMap[K]) => void) => {
    document.addEventListener(type, fn as EventListener, true);
    return () => document.removeEventListener(type, fn as EventListener, true);
  };
  const offs = [
    on("pointerover", (e) => {
      if (e.pointerType === "touch") return;
      const hit = specOf(e.target);
      if (!hit?.spec.over || hit.el.contains(e.relatedTarget as Node | null)) return;
      playUiSound(hit.spec.over);
    }),
    on("pointerdown", (e) => {
      keyboard = false;
      const s = specOf(e.target)?.spec.down;
      if (s) playUiSound(s);
    }),
    on("pointerup", (e) => {
      const s = specOf(e.target)?.spec.up;
      if (s) playUiSound(s);
    }),
    on("click", (e) => {
      // Keyboard activation (Enter, Space, arrow keys on radios) is silent in Encarta's controls.
      if (keyboard && e.detail === 0) return;
      const s = specOf(e.target)?.spec.click;
      if (s) playUiSound(s);
    }),
    on("keydown", (e) => {
      keyboard = true;
      if (e.key === "Escape" && (e.target as Element | null)?.closest?.('[role="menu"]:not(.features-flyout)')) playUiSound("tick");
    }),
    // Arrow keys move focus between menu items: that is the menu's highlight changing.
    on("focusin", (e) => {
      if (keyboard && (e.target as Element | null)?.closest?.('[role="menuitem"]:not(.features-flyout *)')) playUiSound("tick");
    }),
  ];
  return () => offs.forEach((f) => f());
}

/** The startup sound, once per page load, when the option is on. */
export function playStartupSound() {
  if (startupDone || !flags.startup || typeof document === "undefined") return;
  startupDone = true;
  play("startup", 0.8).catch(() => {
    // Refused before any gesture: play at the first one.
    const again = () => {
      document.removeEventListener("pointerdown", again, true);
      document.removeEventListener("keydown", again, true);
      if (flags.startup) void play("startup", 0.8).catch(() => {});
    };
    document.addEventListener("pointerdown", again, true);
    document.addEventListener("keydown", again, true);
  });
}
