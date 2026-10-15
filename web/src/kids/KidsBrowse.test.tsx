// @vitest-environment jsdom
import { cleanup, render, screen, within } from "@testing-library/react";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import { afterEach, describe, expect, it, vi } from "vitest";
import { KIDS_HOME_ID } from "../pages/home";
import { KidsBrowsePage } from "./KidsBrowse";

const link = (ref_id: number, group: string, title: string, sequence: number, extra = {}) => ({ ref_id, group, kind: group === "home" ? "home-page" : "article", title, sequence, ...extra });
const thumb = { file_name: "t31.jtn", url: "/media/file/t31.jtn", width: 64, height: 64 };

/** Synthetic Kids data: home → category 10 (subcategories 11, 12) and 20; 11 is in both. */
const ITEMS: Record<number, object> = {
  [KIDS_HOME_ID]: { ref_id: KIDS_HOME_ID, kind: "home-page", title: "Kids home", appears_in: [], media: [link(10, "home", "Cat", 1), link(20, "home", "Other", 2)] },
  10: {
    ref_id: 10,
    kind: "home-page",
    title: "Cat",
    appears_in: [{ ref_id: KIDS_HOME_ID, group: "home", title: "Kids home" }],
    media: [link(90, "hthm", "Cat (top category)", 3), link(91, "hsbg", "Cat (top category)", 4), link(11, "home", "Sub A", 2), link(12, "home", "Sub B", 1)],
  },
  20: { ref_id: 20, kind: "home-page", title: "Other", appears_in: [{ ref_id: KIDS_HOME_ID, group: "home" }], media: [link(11, "home", "Sub A", 1)] },
  11: {
    ref_id: 11,
    kind: "home-page",
    title: "Sub A",
    appears_in: [{ ref_id: 20, group: "home", title: "Other" }, { ref_id: 10, group: "home", title: "Cat" }],
    media: [link(93, "hthm", "Cat: Sub A", 3), link(32, "media", "No thumb", 2), link(31, "kids", "With thumb", 1, { thumb })],
  },
  12: { ref_id: 12, kind: "home-page", title: "Sub B", appears_in: [{ ref_id: 10, group: "home" }], media: [] },
  93: { ref_id: 93, kind: "home-theme", title: "Cat: Sub A", files: [{ role: "default", file_name: "i93.gsm", url: "/media/file/i93.gsm", width: 44, height: 42 }] },
};

function stubFetch() {
  vi.stubGlobal(
    "fetch",
    vi.fn((input: RequestInfo | URL) => {
      const m = /\/api\/item\/(\d+)/.exec(String(input));
      const it = m ? ITEMS[Number(m[1])] : undefined;
      return Promise.resolve(it ? new Response(JSON.stringify(it), { headers: { "content-type": "application/json" } }) : new Response("{}", { status: 404 }));
    }),
  );
}

function show(path: string, state?: object) {
  stubFetch();
  return render(
    <MemoryRouter initialEntries={[{ pathname: path, state }]}>
      <Routes>
        <Route path="/kids/browse/:id" element={<KidsBrowsePage />} />
      </Routes>
    </MemoryRouter>,
  );
}

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

describe("Kids category screens", () => {
  it("lists a top category's subcategories in sequence order, without its theme pictures", async () => {
    show("/kids/browse/10");
    const board = (await screen.findByText("Sub B")).closest("ul")!;
    expect(within(board).getAllByRole("link").map((a) => [a.textContent, a.getAttribute("href")])).toEqual([
      ["Sub B", "/kids/browse/12"],
      ["Sub A", "/kids/browse/11"],
    ]);
    // The subcategory's own icon (its `hthm` picture).
    await vi.waitFor(() => expect(board.querySelector('img[src="/media/file/i93.gsm"]')).not.toBeNull());
    expect(screen.queryByText(/top category/)).toBeNull();
  });

  it("shows a subcategory under the category it was opened from, with the book for missing thumbnails", async () => {
    show("/kids/browse/11", { cat: 10 });
    expect((await screen.findByRole("heading", { level: 1 })).textContent).toBe("Cat");
    const current = await screen.findByRole("link", { current: "page" });
    expect(current.textContent).toBe("Sub A");
    const tiles = screen.getByText("With thumb").closest("ul")!;
    expect([...tiles.querySelectorAll("img")].map((i) => i.getAttribute("src"))).toEqual(["/media/file/t31.jtn", "/ui/kids-no-picture.jpg"]);
  });

  it("falls back to the first parent without a remembered category", async () => {
    show("/kids/browse/11");
    await vi.waitFor(() => expect(screen.getByRole("heading", { level: 1 }).textContent).toBe("Other"));
  });
});
