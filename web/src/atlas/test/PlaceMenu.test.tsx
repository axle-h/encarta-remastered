// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen } from "@testing-library/react";
import { StrictMode, useEffect, useRef, useState } from "react";
import { defaultEndpoints } from "../data";
import type { HitPlace } from "../hit";
import { PlaceMenu } from "../ui/PlaceMenu";

const ep = defaultEndpoints({ origin: "http://test" });

const place = (entity: number, name: string): HitPlace => ({ entity, name, className: "Town", kind: "point" }) as HitPlace;

/** Opens the menu from a native click listener, the way MapLibre's click handler does. */
function Host({ onError }: { onError: (e: unknown) => void }) {
  const [menu, setMenu] = useState<{ x: number; hits: HitPlace[] } | null>(null);
  const pane = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const el = pane.current!;
    let n = 0;
    const onClick = () => {
      n++;
      try {
        setMenu({ x: 10 * n, hits: n % 2 ? [place(1, "One"), place(2, "Two"), place(3, "Three")] : [place(2, "Two"), place(3, "Three")] });
      } catch (e) {
        onError(e);
      }
    };
    el.addEventListener("click", onClick);
    return () => el.removeEventListener("click", onClick);
  }, [onError]);
  return (
    <div>
      <div data-testid="pane" ref={pane} />
      {menu && <PlaceMenu ep={ep} x={menu.x} y={20} hits={menu.hits} onHover={() => {}} onGoTo={() => {}} onOpen={() => {}} onClose={() => setMenu(null)} />}
    </div>
  );
}

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

describe("PlaceMenu", () => {
  it("re-opens on places whose links are cached without looping (Maximum update depth)", async () => {
    // The second click's places are in the links cache from the first, so their links resolve at once.
    vi.stubGlobal("fetch", vi.fn(async () => new Response(JSON.stringify({ links: [] }), { status: 200, headers: { "content-type": "application/json" } })));
    const errors: unknown[] = [];
    const onWindowError = (e: ErrorEvent) => {
      errors.push(e.error ?? e.message);
      e.preventDefault();
    };
    window.addEventListener("error", onWindowError);
    const spy = vi.spyOn(console, "error").mockImplementation((...a: unknown[]) => errors.push(a[0]));
    render(
      <StrictMode>
        <Host onError={(e) => errors.push(e)} />
      </StrictMode>,
    );
    const pane = screen.getByTestId("pane");
    // Outside act, as a real browser event: React gives the click a discrete (sync) priority. A browser
    // runs the microtasks queued by a listener while the event is still being dispatched, so
    // `window.event` is still the click when the cached links resolve; jsdom has already reset it.
    const prev = (globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT;
    (globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = false;
    const click = new MouseEvent("click");
    Object.defineProperty(window, "event", { configurable: true, get: () => click });
    try {
      for (let i = 0; i < 3; i++) {
        pane.dispatchEvent(new MouseEvent("click", { bubbles: true }));
        await new Promise((r) => setTimeout(r, 30));
      }
    } finally {
      (globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = prev;
      spy.mockRestore();
      delete (window as { event?: unknown }).event;
      window.removeEventListener("error", onWindowError);
    }
    await act(async () => {});
    expect(errors.map(String).filter((e) => /Maximum update depth/.test(e))).toEqual([]);
    expect(screen.getByRole("menu")).toBeTruthy();
    expect(screen.getAllByRole("menuitem").map((b) => b.textContent)).toEqual(["One", "Two", "Three"]);
  });
});
