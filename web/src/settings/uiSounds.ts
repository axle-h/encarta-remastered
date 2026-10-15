/**
 * Interface sounds (Tools > Options > Sounds). The converter writes them from the disc's DLL
 * resources to `out/sounds/`; the server serves `/sounds/{name}.wav`. Browsers refuse sound before
 * the first user gesture, so a startup sound that is refused plays on the first click or key.
 *
 * Which control plays what was measured in the XP reference VM (docs/media.md section 9):
 * `tick` is the rollover of toolbar buttons, atlas text buttons and menu highlights; `click` the
 * press of buttons (round toolbar buttons: on press and again on release); `kidsHover` the
 * rollover of the Kids round buttons; `timeline` the timeline controls; `tourHover` / `tourPick` the
 * pointer entering / clicking a stop on a tour's site map (both nearly silent). Plain links and tabs are
 * silent. One document-level delegate ({@link installUiSoundDelegates}) maps controls to sounds;
 * `data-snd="timeline"` (or `"none"`) on an element or ancestor overrides the built-in rules.
 */
export type UiSound = "startup" | "click" | "tick" | "kidsHover" | "timeline" | "tourHover" | "tourPick" | `kids-${number}`;

const FILES: Record<string, string> = { kidsHover: "kids-hover", tourHover: "tour-hover", tourPick: "tour-pick" };

const flags = { startup: true, menu: false };
const players = new Map<string, HTMLAudioElement>();
let startupDone = false;

/** Called by the settings provider whenever the two Options check boxes change. */
export function configureUiSounds(startup: boolean, menu: boolean) {
  flags.startup = startup;
  flags.menu = menu;
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
  if (flags.menu) void play(name, name === "click" ? 0.7 : 0.8).catch(() => {});
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
const RULES: [string, Spec][] = [
  [".kids-shell .orb, .kids-shell .searchbox-go", { over: "kidsHover", ...ROUND }],
  [".orb", { over: "tick", ...ROUND }],
  [".searchbox-go", { over: "tick", click: "click" }],
  [".menu.features > button, .enc-atlas-btn, .dlg-backdrop button:not([role=tab])", { over: "tick", click: "click" }],
  [".pano-site-pt:not(.current)", { over: "tourHover", click: "tourPick" }],
  ['[role="menuitem"]', { over: "tick" }],
  [".menu > button, .enc-atlas button, input[type=checkbox], input[type=radio]", { click: "click" }],
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
      const s = specOf(e.target)?.spec.click;
      if (s) playUiSound(s);
    }),
    on("keydown", (e) => {
      keyboard = true;
      if (e.key === "Escape" && (e.target as Element | null)?.closest?.('[role="menu"]')) playUiSound("tick");
    }),
    // Arrow keys move focus between menu items: that is the menu's highlight changing.
    on("focusin", (e) => {
      if (keyboard && (e.target as Element | null)?.closest?.('[role="menuitem"]')) playUiSound("tick");
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
