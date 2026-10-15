// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from "vitest";
import { FANFARE_SECONDS, playFanfare } from "./fanfare";

afterEach(() => vi.unstubAllGlobals());

describe("fanfare", () => {
  it("is about two seconds", () => {
    expect(FANFARE_SECONDS).toBeGreaterThan(1.8);
    expect(FANFARE_SECONDS).toBeLessThan(2.2);
  });

  it("starts one oscillator per note and stays silent without Web Audio", () => {
    const started: number[] = [];
    const param = () => ({ value: 0, setValueAtTime() {}, linearRampToValueAtTime() {} });
    const node = () => ({ connect: (n: unknown) => n });
    vi.stubGlobal(
      "AudioContext",
      class {
        currentTime = 0;
        destination = {};
        resume() {}
        createGain() {
          return { ...node(), gain: param() };
        }
        createOscillator() {
          return { ...node(), frequency: param(), type: "", start: (t: number) => started.push(t), stop() {} };
        }
      },
    );
    playFanfare();
    expect(started).toHaveLength(9);
    vi.stubGlobal("AudioContext", undefined);
    expect(() => playFanfare()).not.toThrow();
  });
});
