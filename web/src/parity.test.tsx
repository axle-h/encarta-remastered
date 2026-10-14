// @vitest-environment jsdom
/** Component tests for the parity-audit additions: menus, Favorites, item kinds, Web Sites. */
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { MemoryRouter, Route, Routes, useLocation } from "react-router-dom";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { AssocItem } from "./api/types";
import { MenuBar } from "./app/Shell";
import { FAVORITES_KEY, favorites } from "./app/favorites";
import { LinksPanel } from "./article/SidePanels";
import { MenuButton } from "./components/Menu";
import { ItemPage } from "./pages/ItemPage";

function json(body: unknown) {
  return Promise.resolve(new Response(JSON.stringify(body), { status: 200, headers: { "Content-Type": "application/json" } }));
}

function Where() {
  const l = useLocation();
  return <output data-testid="where">{`${l.pathname}${l.search}`}</output>;
}

/** A Storage stand-in: Node's own `localStorage` global shadows jsdom's when it is not configured. */
function memoryStorage(): Storage {
  const m = new Map<string, string>();
  return {
    get length() {
      return m.size;
    },
    clear: () => m.clear(),
    getItem: (k) => m.get(k) ?? null,
    key: (i) => [...m.keys()][i] ?? null,
    removeItem: (k) => void m.delete(k),
    setItem: (k, v) => void m.set(k, String(v)),
  };
}

beforeEach(() => {
  Object.defineProperty(window, "localStorage", { value: memoryStorage(), configurable: true });
  favorites.reset();
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

describe("MenuButton keyboard", () => {
  function Bar() {
    return (
      <div role="menubar">
        <MenuButton label="One" entries={() => [{ label: "A", onSelect: () => undefined }, { label: "B", disabled: true }, { label: "C", to: "/c" }]} />
        <MenuButton label="Two" entries={() => [{ label: "D", to: "/d" }]} />
      </div>
    );
  }

  it("opens on ArrowDown, moves with arrows/End, skips disabled items, closes on Escape and moves along the bar", async () => {
    render(
      <MemoryRouter>
        <Bar />
      </MemoryRouter>,
    );
    const one = screen.getByRole("button", { name: "One" });
    one.focus();
    fireEvent.keyDown(one, { key: "ArrowDown" });
    await waitFor(() => expect(document.activeElement?.textContent).toBe("A"));
    fireEvent.keyDown(document.activeElement!, { key: "ArrowDown" });
    expect(document.activeElement?.textContent).toBe("C");
    fireEvent.keyDown(document.activeElement!, { key: "ArrowDown" });
    expect(document.activeElement?.textContent).toBe("A");
    fireEvent.keyDown(document.activeElement!, { key: "End" });
    expect(document.activeElement?.textContent).toBe("C");
    fireEvent.keyDown(document.activeElement!, { key: "Escape" });
    expect(screen.queryByRole("menu")).toBeNull();
    expect(document.activeElement).toBe(one);
    expect(one.getAttribute("aria-expanded")).toBe("false");
    fireEvent.keyDown(one, { key: "ArrowUp" });
    await waitFor(() => expect(document.activeElement?.textContent).toBe("C"));
    fireEvent.keyDown(document.activeElement!, { key: "ArrowRight" });
    await waitFor(() => expect(document.activeElement?.textContent).toBe("D"));
    expect(screen.getByRole("button", { name: "Two" }).getAttribute("aria-expanded")).toBe("true");
  });

  it("closes a menu opened with the mouse on Escape", () => {
    render(
      <MemoryRouter>
        <Bar />
      </MemoryRouter>,
    );
    fireEvent.click(screen.getByRole("button", { name: "One" }));
    expect(screen.getByRole("menu")).toBeTruthy();
    fireEvent.keyDown(document.body, { key: "Escape" });
    expect(screen.queryByRole("menu")).toBeNull();
  });
});

describe("Favorites", () => {
  it("adds the current page from the menu bar and lists it", async () => {
    document.title = "Zebra - Encarta";
    render(
      <MemoryRouter initialEntries={["/item/761575094#p3"]}>
        <MenuBar />
      </MemoryRouter>,
    );
    fireEvent.click(screen.getByRole("button", { name: "Favorites" }));
    fireEvent.click(screen.getByRole("menuitem", { name: "Add to Favorites" }));
    const stored = JSON.parse(window.localStorage.getItem(FAVORITES_KEY) ?? "[]");
    expect(stored[0]).toMatchObject({ path: "/item/761575094#p3", title: "Zebra" });
    await screen.findByText(/is in your Favorites/);
    fireEvent.click(screen.getByRole("button", { name: "Favorites" }));
    expect(screen.getByRole("menuitem", { name: "Rename in Favorites" })).toBeTruthy();
    expect(screen.getByRole("menuitem", { name: "Zebra" }).getAttribute("href")).toBe("/item/761575094#p3");
  });

  it("still works when storage throws", () => {
    Object.defineProperty(window, "localStorage", {
      configurable: true,
      value: { getItem: () => { throw new Error("blocked"); }, setItem: () => { throw new Error("blocked"); } },
    });
    favorites.reset();
    act(() => favorites.add("/item/1", "One"));
    expect(favorites.list().map((f) => f.path)).toEqual(["/item/1"]);
  });
});

describe("item kinds", () => {
  function mock(items: Record<number, unknown>) {
    vi.stubGlobal(
      "fetch",
      vi.fn((input: RequestInfo | URL) => {
        const m = /\/api\/item\/(\d+)/.exec(String(input));
        const body = m ? items[Number(m[1])] : undefined;
        return body ? json(body) : Promise.resolve(new Response('{"error":"no"}', { status: 404 }));
      }),
    );
  }
  function at(path: string) {
    render(
      <MemoryRouter initialEntries={[path]}>
        <Routes>
          <Route path="/item/:id" element={<ItemPage />} />
          <Route path="*" element={<Where />} />
        </Routes>
      </MemoryRouter>,
    );
  }

  it("shows a web link's address, its archived copy and where it is listed", async () => {
    mock({
      41: {
        ref_id: 41, kind: "weblink", title: "Poisonous Creatures", caption_html: "A list of <em>toxic</em> animals.", editions: ["deluxe"],
        meta: { url: ["http://example.com/poison"] }, files: [], appears_in: [], referenced_by: [{ ref_id: 7, kind: "article", title: "Toad" }],
      },
    });
    at("/item/41");
    const link = await screen.findByRole("link", { name: "http://example.com/poison" });
    expect(link.getAttribute("rel")).toContain("noopener");
    expect(screen.getByRole("link", { name: /archived in 2008/ }).getAttribute("href")).toBe("https://web.archive.org/web/2008/http://example.com/poison");
    expect(screen.getByRole("link", { name: "Toad" }).getAttribute("href")).toBe("/item/7");
  });

  it("lists a quotation author's quotations and a contributor's articles", async () => {
    mock({
      51: {
        ref_id: 51, kind: "quotation-author", title: "Doe, Jane", editions: ["deluxe"], files: [], appears_in: [],
        meta: { name: ["Jane Doe"], born: ["1900"], died: ["1980"], nationality: ["Synthetic"] },
        referenced_by: [{ ref_id: 52, kind: "quotation", title: "Ambition: Onward." }],
      },
      61: {
        ref_id: 61, kind: "contributor", title: "John Roe", editions: ["deluxe"], files: [], appears_in: [],
        meta: { credentials: ["Ph.D."], info: ["Professor of ", { t: "em", c: ["Stripes"] }, "."] },
        referenced_by: [{ ref_id: 100, kind: "article", title: "Zebra" }],
      },
    });
    at("/item/51");
    expect(await screen.findByRole("heading", { name: "Jane Doe" })).toBeTruthy();
    expect(screen.getByText("1900–1980 · Synthetic")).toBeTruthy();
    expect(screen.getByRole("link", { name: "Ambition: Onward." }).getAttribute("href")).toBe("/item/52");
    cleanup();
    at("/item/61");
    expect(await screen.findByText("Stripes")).toBeTruthy();
    expect(screen.getByRole("link", { name: "Zebra" }).getAttribute("href")).toBe("/item/100");
  });

  it("sends feature records and topic timelines to their pages", async () => {
    mock({
      71: { ref_id: 71, kind: "feature", subtype: "BA2F", title: "Translate a Word", editions: ["deluxe"], files: [], appears_in: [] },
      72: { ref_id: 72, kind: "timeline-topic", title: "Aztec Empire Topic Timeline", editions: ["deluxe"], files: [], appears_in: [] },
      73: { ref_id: 73, kind: "feature", subtype: "EONL", title: "Encarta on the Web", editions: ["deluxe"], files: [], appears_in: [] },
    });
    at("/item/71");
    expect((await screen.findByTestId("where")).textContent).toBe("/translate?pair=ENG_FRA");
    cleanup();
    at("/item/72");
    expect((await screen.findByTestId("where")).textContent).toBe("/timeline?term=Aztec%20Empire");
    cleanup();
    at("/item/73");
    expect(await screen.findByText(/Microsoft web service in 2009/)).toBeTruthy();
  });
});

describe("Web Sites panel", () => {
  it("shows the first links and the rest on request", () => {
    const items: AssocItem[] = Array.from({ length: 10 }, (_, i) => ({ ref_id: i + 1, group: "weblink", title: `Site ${i + 1}` }));
    render(
      <MemoryRouter>
        <LinksPanel title="Web Sites" items={items} kids={false} limit={8} />
      </MemoryRouter>,
    );
    expect(screen.getAllByRole("link")).toHaveLength(8);
    fireEvent.click(screen.getByRole("button", { name: "Show all 10" }));
    expect(screen.getAllByRole("link")).toHaveLength(10);
  });
});
