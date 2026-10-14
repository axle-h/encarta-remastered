// @vitest-environment jsdom
/** Index terms as expandable search results (synthetic data; `fetch` is mocked). */
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import { afterEach, describe, expect, it, vi } from "vitest";
import { SearchPage } from "./pages/SearchPage";

function json(body: unknown) {
  return Promise.resolve(new Response(JSON.stringify(body), { status: 200, headers: { "Content-Type": "application/json" } }));
}

function mockFetch() {
  const f = vi.fn((input: RequestInfo | URL) => {
    const url = String(input);
    if (url.startsWith("/api/search")) {
      return json({
        query: "hippotigris",
        mode: "all",
        total: 2,
        offset: 0,
        facets: { kind: { other: 1, article: 1 } },
        hits: [
          { ref_id: 9001, category: "other", kind: "index-term", title: "Hippotigris", url: "/item/9001", snippet_html: "Index: 2 entries — zebra; Horse" },
          { ref_id: 100, category: "article", kind: "article", title: "Zebra", url: "/item/100" },
        ],
      });
    }
    if (url.startsWith("/api/index-term/9001")) {
      return json({
        ref_id: 9001,
        title: "Hippotigris",
        entries: [
          { title: ["zebra"], ref: 100, para: 2, group: "article" },
          { title: ["quotations"], children: [{ title: ["Stripes: ", { t: "em", c: ["Dazzle"] }], ref: 300, group: "quotation" }] },
        ],
      });
    }
    return json({});
  });
  vi.stubGlobal("fetch", f);
  return f;
}

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

describe("index-term search results", () => {
  it("expand in place to their entries, with links to the paragraph", async () => {
    const fetch = mockFetch();
    render(
      <MemoryRouter initialEntries={["/search?q=hippotigris"]}>
        <Routes>
          <Route path="/search" element={<SearchPage />} />
        </Routes>
      </MemoryRouter>,
    );
    const toggle = await screen.findByRole("button", { name: /Hippotigris/ });
    expect(toggle.getAttribute("aria-expanded")).toBe("false");
    expect(screen.getByText("Index")).toBeTruthy();
    expect(screen.getByRole("link", { name: "Zebra" }).getAttribute("href")).toBe("/item/100");
    expect(fetch.mock.calls.some(([u]) => String(u).startsWith("/api/index-term/"))).toBe(false);
    fireEvent.click(toggle);
    expect(toggle.getAttribute("aria-expanded")).toBe("true");
    const entry = await screen.findByRole("link", { name: "zebra" });
    expect(entry.getAttribute("href")).toBe("/item/100#p2");
    expect(screen.getByText("quotations")).toBeTruthy();
    await waitFor(() => expect(screen.getByRole("link", { name: /Stripes/ }).getAttribute("href")).toBe("/item/300"));
    fireEvent.click(toggle);
    expect(screen.queryByRole("link", { name: "zebra" })).toBeNull();
  });
});
