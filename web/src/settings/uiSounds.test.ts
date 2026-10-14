// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { configureUiSounds, isClickable, playMenuSound, playStartupSound } from "./uiSounds";

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

  it("finds enabled buttons, menu items and tabs", () => {
    document.body.innerHTML = `<button id=a><i id=i></i></button><button id=b disabled></button><div role=menuitem id=m></div><p id=p></p>`;
    const el = (id: string) => document.getElementById(id);
    expect(isClickable(el("i"))).toBe(true);
    expect(isClickable(el("b"))).toBe(false);
    expect(isClickable(el("m"))).toBe(true);
    expect(isClickable(el("p"))).toBe(false);
  });
});
