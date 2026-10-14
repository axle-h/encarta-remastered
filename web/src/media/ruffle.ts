/**
 * Ruffle, the Flash Player emulator, self-hosted from the pinned `@ruffle-rs/ruffle` package
 * (`vite.config.ts` serves it at `/ruffle/`). Loaded on first use only: its wasm core is ~15 MB.
 *
 * Encarta's interactives (`.iax`) are run by the disc's own IAF player: `base.swf` reads the
 * `.iax` named by its `filePath` variable, then loads the control (`SlideShowControl.swf`,
 * `MatchGame.swf` …), skins and media by URLs relative to the `dswMedia/` tree, which the server
 * mirrors at `/media/disc/{edition}/dswMedia/…` (api.md "Media").
 */

export interface RufflePlayerElement extends HTMLElement {
  ruffle(): {
    load(options: {
      url: string;
      parameters?: Record<string, string>;
      base?: string;
      fontSources?: string[];
      defaultFonts?: Record<string, string[]>;
    }): Promise<void>;
  };
}

interface RuffleGlobal {
  config?: Record<string, unknown>;
  newest(): { createPlayer(): RufflePlayerElement };
}

declare global {
  interface Window {
    RufflePlayer?: RuffleGlobal;
  }
}

export const RUFFLE_PATH = "/ruffle/";

let loading: Promise<RuffleGlobal> | null = null;

export function loadRuffle(): Promise<RuffleGlobal> {
  if (loading) return loading;
  loading = new Promise<RuffleGlobal>((resolve, reject) => {
    window.RufflePlayer = window.RufflePlayer ?? ({} as RuffleGlobal);
    window.RufflePlayer.config = {
      publicPath: RUFFLE_PATH,
      autoplay: "on",
      unmuteOverlay: "hidden",
      splashScreen: false,
      letterbox: "on",
      contextMenu: "rightClickOnly",
      showSwfDownload: false,
      // The IAF players are AVM1 (SWF 6) movies; keep scripts from being cut off on slow machines.
      maxExecutionDuration: 30,
    };
    const s = document.createElement("script");
    s.src = `${RUFFLE_PATH}ruffle.js`;
    s.async = true;
    s.onload = () => (window.RufflePlayer?.newest ? resolve(window.RufflePlayer) : reject(new Error("Ruffle did not start")));
    s.onerror = () => {
      loading = null;
      reject(new Error("Ruffle could not be loaded"));
    };
    document.head.appendChild(s);
  });
  return loading;
}

/** URL of the IAF player and the `filePath` it is given for an `.iax` (all IAF folders are `a`, English). */
export function iafPlayer(iaxFile: string, edition: "deluxe" | "kids"): { url: string; filePath: string } {
  const id = iaxFile.replace(/\.iax$/i, "").toLowerCase();
  const root = `/media/disc/${edition}/dswMedia`;
  return { url: `${root}/shared/players/IAFPlayer/base.swf`, filePath: `${root}/IAF/a/${id}/${id}.iax` };
}
