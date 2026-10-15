// @vitest-environment jsdom
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { afterEach, describe, expect, it, vi } from "vitest";
import { cardKind, KidsSearch, matchesWholeWord, snippetText } from "./KidsSearch";

const hit = (ref_id: number, title: string, extra: object = {}) => ({ ref_id, category: "media", kind: "media", title, url: `/item/${ref_id}`, ...extra });

function stub(hits: object[]) {
  vi.stubGlobal(
    "fetch",
    vi.fn(() => Promise.resolve(new Response(JSON.stringify({ total: hits.length, hits }), { headers: { "content-type": "application/json" } }))),
  );
}

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

describe("cardKind", () => {
  it("maps categories and media classes to the disc's card pictures", () => {
    expect(cardKind({ category: "article", kind: "article" })).toBe("article");
    expect(cardKind({ category: "media", kind: "media", media_type: "image" })).toBe("picture");
    expect(cardKind({ category: "media", kind: "audio-set", media_type: "audio" })).toBe("sound");
    expect(cardKind({ category: "media", kind: "media", media_type: "video" })).toBe("video");
    expect(cardKind({ category: "media", kind: "table", media_type: "table" })).toBe("table");
    expect(cardKind({ category: "media", kind: "interactive", media_type: "game" })).toBe("activity");
    expect(cardKind({ category: "map", kind: "map", media_type: "map" })).toBe("map");
    expect(cardKind({ category: "media", kind: "composite-media" })).toBe("pictures");
  });
});

describe("helpers", () => {
  it("matches whole words only", () => {
    expect(matchesWholeWord(hit(1, "Zebras") as never, "zebra")).toBe(true);
    expect(matchesWholeWord(hit(1, "Zebras") as never, "zeb")).toBe(false);
  });
  it("cuts the snippet at a sentence with ' ...'", () => {
    const long = "A short one. " + "Another sentence goes on and on. ".repeat(10);
    const s = snippetText(long);
    expect(s.endsWith(" ...")).toBe(true);
    expect(s.length).toBeLessThan(200);
  });
});

describe("KidsSearch", () => {
  it("renders one card per hit, linking into Kids", async () => {
    stub([
      hit(1, "Zebras", { category: "article", kind: "article", snippet_html: "A <b>zebra</b> is a horse." }),
      hit(2, "Zebra", { media_type: "image" }),
    ]);
    const { container } = render(
      <MemoryRouter>
        <KidsSearch q="zebra" />
      </MemoryRouter>,
    );
    await waitFor(() => expect(container.querySelectorAll(".ks-card").length).toBe(2));
    const cards = [...container.querySelectorAll<HTMLAnchorElement>(".ks-card")];
    expect(cards.map((c) => c.dataset.kind)).toEqual(["article", "picture"]);
    expect(cards.map((c) => c.getAttribute("href"))).toEqual(["/kids/item/1", "/kids/item/2"]);
    expect(screen.getByText("A zebra is a horse. ...")).toBeTruthy();
    expect(screen.getByText("Search Results")).toBeTruthy();
  });

  it("shows the Sorry card with links when nothing matches", async () => {
    stub([hit(3, "Zebras")]);
    render(
      <MemoryRouter>
        <KidsSearch q="zeb" />
      </MemoryRouter>,
    );
    await waitFor(() => expect(screen.getByText("Sorry!")).toBeTruthy());
    expect(screen.getByText(/We could not find "zeb\."/)).toBeTruthy();
    expect(screen.getByText("home page").getAttribute("href")).toBe("/kids");
    expect(screen.getByText("click here").getAttribute("href")).toBe("/search?q=zeb");
  });
});
