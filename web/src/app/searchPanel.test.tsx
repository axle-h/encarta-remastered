// @vitest-environment jsdom
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { MemoryRouter, useLocation } from "react-router-dom";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { FEATURE_GROUPS } from "./featuresData";
import { FeaturesMenu } from "./FeaturesMenu";
import { SearchArea } from "./Shell";
import { SettingsProvider, parseSettings } from "../settings/settings";

const json = (body: unknown) => Promise.resolve(new Response(JSON.stringify(body), { status: 200, headers: { "Content-Type": "application/json" } }));
const hit = (id: number, title: string) => ({ ref_id: id, category: "article", kind: "article", title, url: `/item/${id}`, snippet_html: "snippet" });
const calls: string[] = [];

function Where() {
  const l = useLocation();
  return <output data-testid="where">{`${l.pathname}${l.search}`}</output>;
}

function app() {
  return render(
    <MemoryRouter>
      <SettingsProvider>
        <FeaturesMenu />
        <SearchArea />
        <Where />
        <div data-testid="page">page</div>
      </SettingsProvider>
    </MemoryRouter>,
  );
}

beforeEach(() => {
  const m = new Map<string, string>();
  Object.defineProperty(window, "localStorage", {
    configurable: true,
    value: { getItem: (k: string) => m.get(k) ?? null, setItem: (k: string, v: string) => void m.set(k, v), removeItem: (k: string) => void m.delete(k) },
  });
  calls.length = 0;
  vi.stubGlobal("fetch", (url: string) => {
    calls.push(url);
    if (url.includes("/api/suggest")) return json({ query: "", suggestions: [] });
    const kind = new URL(url, "http://x").searchParams.get("kind");
    if (kind === "map") return json({ query: "zebra", total: 0, offset: 0, hits: [] });
    return json({ query: "zebra", total: 2, offset: 0, hits: [hit(1, "Zebra"), hit(2, "Quagga")] });
  });
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

const type = (term: string, name = "Search Encarta") => {
  const box = screen.getByRole("combobox", { name });
  fireEvent.change(box, { target: { value: term } });
  fireEvent.submit(box.closest("form")!);
};

describe("search panel", () => {
  it("opens on Enter with category bars, one open at a time, and the tab toggles it", async () => {
    app();
    const tab = screen.getByRole("button", { name: "Search results" });
    expect(tab.textContent).toBe("▾");
    type("zebra");
    expect(await screen.findByRole("button", { name: /Best Matches \(2\)/ })).toBeTruthy();
    expect(tab.textContent).toBe("▴");
    expect(screen.getByRole("button", { name: /Best Matches/ }).getAttribute("aria-expanded")).toBe("true");
    const maps = await screen.findByRole("button", { name: /Maps \(0\)/ });
    fireEvent.click(maps);
    expect(maps.getAttribute("aria-expanded")).toBe("true");
    expect(screen.getByRole("button", { name: /Best Matches/ }).getAttribute("aria-expanded")).toBe("false");
    expect(await screen.findByRole("link", { name: "Search all maps" })).toBeTruthy();
    expect(screen.getByRole("link", { name: "Search all maps" }).getAttribute("href")).toBe("/search?q=zebra&kind=map");
    fireEvent.click(tab);
    expect(screen.queryByRole("complementary", { name: "Search results" })).toBeNull();
  });

  it("hides on a page click unless keep open is checked", async () => {
    app();
    type("zebra");
    await screen.findByRole("button", { name: /Best Matches/ });
    fireEvent.mouseDown(screen.getByTestId("page"));
    expect(screen.queryByRole("complementary", { name: "Search results" })).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Search results" }));
    fireEvent.click(screen.getByRole("checkbox", { name: /Keep search results list open/ }));
    fireEvent.mouseDown(screen.getByTestId("page"));
    expect(screen.getByRole("complementary", { name: "Search results" })).toBeTruthy();
    expect(parseSettings(window.localStorage.getItem("encarta.settings.v1")).keepSearchOpen).toBe(true);
  });

  it("auto-displays the best match when asked", async () => {
    app();
    type("zebra");
    fireEvent.click(await screen.findByRole("checkbox", { name: /Automatically display best match/ }));
    type("quagga");
    await waitFor(() => expect(screen.getByTestId("where").textContent).toBe("/item/1"));
  });
});

describe("Quick Search", () => {
  it("is a modal dialog; Enter closes it and opens the panel with only that category", async () => {
    expect(FEATURE_GROUPS.flatMap((g) => [...g.entries, ...g.more]).filter((e) => e.quick).map((e) => e.quick)).toEqual(["map", "media"]);
    app();
    fireEvent.click(screen.getByRole("button", { name: "Features" }));
    fireEvent.click(screen.getByRole("menuitem", { name: "Quick Search: Maps" }));
    const dlg = screen.getByRole("dialog", { name: "Quick Search" });
    expect(dlg.getAttribute("aria-modal")).toBe("true");
    type("zebra", "Quick Search: Maps");
    await act(async () => undefined);
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(screen.getByRole("button", { name: /Maps/ })).toBeTruthy();
    expect(screen.queryByRole("button", { name: /Articles/ })).toBeNull();
    expect(screen.queryByRole("button", { name: /Best Matches/ })).toBeNull();
  });
});
