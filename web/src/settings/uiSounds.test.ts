// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { configureUiSounds, installUiSoundDelegates, playMenuSound, playUiSound, playStartupSound } from "./uiSounds";

const played: string[] = [];
let refuse = false;

beforeEach(() => {
  played.length = 0;
  refuse = false;
  vi.stubGlobal(
    "Audio",
    class {
      src: string;
      volume = 1;
      currentTime = 0;
      preload = "";
      constructor(src: string) {
        this.src = src;
      }
      play() {
        if (refuse) return Promise.reject(new Error("NotAllowedError"));
        played.push(this.src);
        return Promise.resolve();
      }
    },
  );
});
afterEach(() => vi.unstubAllGlobals());

describe("interface sounds", () => {
  it("plays menu sounds only when the option is on", () => {
    configureUiSounds(true, false);
    playMenuSound();
    expect(played).toEqual([]);
    configureUiSounds(true, true);
    playMenuSound();
    playMenuSound("kids-3");
    expect(played).toEqual(["/sounds/click.wav", "/sounds/kids-3.wav"]);
  });

  it("plays the startup sound once, after the first gesture when refused", async () => {
    configureUiSounds(true, true);
    refuse = true;
    playStartupSound();
    await new Promise((r) => setTimeout(r, 0));
    expect(played).toEqual([]);
    refuse = false;
    document.dispatchEvent(new Event("pointerdown"));
    await new Promise((r) => setTimeout(r, 0));
    expect(played).toEqual(["/sounds/startup.wav"]);
    playStartupSound();
    expect(played).toHaveLength(1);
  });

  it("maps controls to the measured sounds", () => {
    configureUiSounds(true, true);
    const off = installUiSoundDelegates();
    document.body.innerHTML = `<button id=orb class=orb></button><button id=dis class=orb disabled></button>
      <div class=kids-shell><button id=kid class=orb></button></div>
      <div role=menu><div role=menuitem id=mi></div></div><a id=link href="#x"></a><div role=tablist><button role=tab id=tab></button></div>
      <div data-snd=timeline><button id=tl></button></div><div class=menu><button id=title></button></div>`;
    const fire = (id: string, type: string) => document.getElementById(id)!.dispatchEvent(new Event(type, { bubbles: true }));
    const take = () => played.splice(0).map((s) => s.replace("/sounds/", "").replace(".wav", ""));
    fire("orb", "pointerover");
    fire("orb", "pointerdown");
    fire("orb", "pointerup");
    fire("orb", "click");
    expect(take()).toEqual(["tick", "click", "click"]);
    for (const t of ["pointerover", "pointerdown", "pointerup", "click"]) fire("dis", t);
    fire("kid", "pointerover");
    fire("mi", "pointerover");
    fire("link", "pointerover");
    fire("link", "click");
    fire("tab", "click");
    expect(take()).toEqual(["kids-hover", "tick"]);
    fire("tl", "click");
    fire("title", "click");
    expect(take()).toEqual(["timeline", "click"]);
    off();
    configureUiSounds(true, false);
    playUiSound("tick");
    expect(take()).toEqual([]);
  });

  it("plays the dialog button and tour site-map sounds", () => {
    configureUiSounds(true, true);
    const off = installUiSoundDelegates();
    document.body.innerHTML = `<div class=dlg-backdrop><button id=ok></button><button role=tab id=page></button></div>
      <div class=pano-site><button id=pt class=pano-site-pt></button><button id=here class="pano-site-pt current"></button></div>`;
    const fire = (id: string, type: string) => document.getElementById(id)!.dispatchEvent(new Event(type, { bubbles: true }));
    const take = () => played.splice(0).map((s) => s.replace("/sounds/", "").replace(".wav", ""));
    // VM: a dialog's push buttons tick on rollover and click once; its page tabs are silent.
    fire("ok", "pointerover");
    fire("ok", "click");
    fire("page", "pointerover");
    fire("page", "click");
    expect(take()).toEqual(["tick", "click"]);
    // VM (ENCARTAR.DLL 578, 579): entering and clicking a site-map stop; the you-are-here point is not a target.
    fire("pt", "pointerover");
    fire("pt", "click");
    fire("here", "pointerover");
    fire("here", "click");
    expect(take()).toEqual(["tour-hover", "tour-pick"]);
    playUiSound("tourPick");
    expect(take()).toEqual(["tour-pick"]);
    off();
  });
});
