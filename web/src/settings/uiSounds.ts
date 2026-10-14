/**
 * Interface sounds (Tools > Options > Sounds): the startup sound, a click for buttons and menu
 * items, and the Kids home category sounds. The converter writes them from the disc's DLL
 * resources to `out/sounds/`; the server serves `/sounds/{name}.wav`. Browsers refuse sound before
 * the first user gesture, so a startup sound that is refused plays on the first click or key.
 */
export type UiSound = "startup" | "click" | `kids-${number}`;

const flags = { startup: true, menu: true };
const players = new Map<string, HTMLAudioElement>();
let startupDone = false;

/** Called by the settings provider whenever the two Options check boxes change. */
export function configureUiSounds(startup: boolean, menu: boolean) {
  flags.startup = startup;
  flags.menu = menu;
}

/** The control a click sound belongs to: a button, menu item or tab that is not disabled. */
export function isClickable(target: EventTarget | null): boolean {
  const el = (target as Element | null)?.closest?.('button, [role="menuitem"], [role="tab"], [role="option"]');
  return !!el && !el.hasAttribute("disabled") && el.getAttribute("aria-disabled") !== "true";
}

function play(name: UiSound, volume: number): Promise<void> {
  if (typeof Audio === "undefined") return Promise.resolve();
  try {
    let a = players.get(name);
    if (!a) {
      a = new Audio(`/sounds/${name}.wav`);
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
export function playMenuSound(name: UiSound = "click") {
  if (flags.menu) void play(name, name === "click" ? 0.7 : 0.8).catch(() => {});
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
