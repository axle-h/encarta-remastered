import { describe, expect, it, vi } from "vitest";
import { stopFlashPlayer } from "./FlashPlayer";
import type { RufflePlayerElement } from "./ruffle";

describe("stopFlashPlayer", () => {
  it("pauses the movie before removing the element, so no sound starts on the closed AudioContext", () => {
    const order: string[] = [];
    const player = { volume: 1, pause: () => order.push("pause"), remove: () => order.push("remove") } as unknown as RufflePlayerElement;
    stopFlashPlayer(player);
    expect(order).toEqual(["pause", "remove"]);
    expect(player.volume).toBe(0);
  });

  it("still removes a player whose pause throws, and accepts none", () => {
    const remove = vi.fn();
    stopFlashPlayer({ pause: () => { throw new Error("gone"); }, remove } as unknown as RufflePlayerElement);
    expect(remove).toHaveBeenCalled();
    expect(() => stopFlashPlayer(null)).not.toThrow();
  });
});
