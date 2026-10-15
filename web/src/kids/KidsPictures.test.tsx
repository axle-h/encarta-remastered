// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import { afterEach, describe, expect, it, vi } from "vitest";
import { KidsPicturesPage } from "./KidsPictures";

const pic = (n: number) => ({ file_name: `p${n}.jpg`, url: `/media/file/p${n}.jpg`, width: 200, height: 100 });
const media = [
  { ref_id: 11, group: "media", sec: 1, sequence: 1, kind: "media", media_type: "image", title: "First", title_html: "First", picon: pic(1), image: pic(1) },
  { ref_id: 12, group: "media", sec: 1, sequence: 2, kind: "media", media_type: "image", title: "Second", title_html: "Second", picon: pic(2), image: pic(2) },
];
const ITEMS: Record<number, object> = {
  11: { ref_id: 11, kind: "media", media_type: "image", title: "First", caption_html: "First caption", credit: "Credit one", files: [{ role: "image", file_name: "p1.jpg", url: "/media/file/p1.jpg", width: 200, height: 100 }] },
  12: { ref_id: 12, kind: "media", media_type: "image", title: "Second", caption_html: "Second caption", credit: "Credit two", files: [{ role: "image", file_name: "p2.jpg", url: "/media/file/p2.jpg", width: 200, height: 100 }] },
};
const ARTICLE = { ref_id: 5, kind: "article", layout: "kids", title: "Beasts", title_html: "Beasts", body: [], media };

function stubFetch() {
  vi.stubGlobal(
    "fetch",
    vi.fn((input: RequestInfo | URL) => {
      const url = String(input);
      const it = /\/api\/item\/(\d+)/.exec(url);
      const body = /\/api\/article\/5/.test(url) ? ARTICLE : it ? ITEMS[Number(it[1])] : undefined;
      return Promise.resolve(body ? new Response(JSON.stringify(body), { headers: { "content-type": "application/json" } }) : new Response("{}", { status: 404 }));
    }),
  );
}

function renderAt(path: string) {
  stubFetch();
  return render(
    <MemoryRouter initialEntries={[path]}>
      <Routes>
        <Route path="/kids/item/:id/media" element={<KidsPicturesPage />} />
      </Routes>
    </MemoryRouter>,
  );
}

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

describe("KidsPicturesPage", () => {
  it("selects the first item by default", async () => {
    renderAt("/kids/item/5/media");
    expect(await screen.findByText("First caption")).toBeTruthy();
  });

  it("starts on the item named by ?max=", async () => {
    renderAt("/kids/item/5/media?max=12");
    expect(await screen.findByText("Second caption")).toBeTruthy();
    const tiles = document.querySelectorAll(".kp-tile");
    expect(tiles[1]!.getAttribute("aria-current")).toBe("true");
  });

  it("shows another item when its tile is clicked", async () => {
    renderAt("/kids/item/5/media");
    await screen.findByText("First caption");
    fireEvent.click(document.querySelectorAll(".kp-tile")[1]!);
    expect(await screen.findByText("Second caption")).toBeTruthy();
    await waitFor(() => expect(document.querySelectorAll(".kp-tile")[1]!.getAttribute("aria-current")).toBe("true"));
  });

  it("links Read Article back to the article", async () => {
    renderAt("/kids/item/5/media");
    const link = await screen.findByLabelText("Read Article");
    expect(link.getAttribute("href")).toBe("/kids/item/5");
  });
});
