// @vitest-environment jsdom
/**
 * Component tests on synthetic data: the inline renderer, the article body (sections, outline,
 * media column, inline tables) and the search box's autocomplete. `fetch` is mocked.
 */
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { MemoryRouter, Route, Routes, useLocation } from "react-router-dom";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { AssocItem, DataTable } from "./api/types";
import { ArticleBody, type BodyContext } from "./article/ArticleBody";
import { Inlines } from "./content/Inlines";
import type { Block, Inline } from "./content/model";
import { SearchBox } from "./components/SearchBox";
import { MediaViewerProvider, useMediaViewer } from "./media/MediaViewer";

function json(body: unknown) {
  return Promise.resolve(new Response(JSON.stringify(body), { status: 200, headers: { "Content-Type": "application/json" } }));
}

/** Answers `/api/item/{id}` with a synthetic image item, `/api/suggest` with two suggestions. */
function mockFetch() {
  const f = vi.fn((input: RequestInfo | URL) => {
    const url = String(input);
    const item = /\/api\/item\/(\d+)/.exec(url);
    if (item) {
      const id = Number(item[1]);
      return json({
        ref_id: id,
        kind: "media",
        media_type: "image",
        title: `Picture ${id}`,
        caption_html: `Caption of <em>${id}</em>`,
        credit: "Synthetic credit",
        files: [{ role: "picon", file_name: `p${id}.jsm`, url: `/media/file/p${id}.jsm`, width: 192, height: 128 }],
        appears_in: [],
      });
    }
    if (url.startsWith("/api/suggest")) {
      return json({
        query: "zeb",
        suggestions: [
          { ref_id: 1, display: "Zebra", url: "/item/1", category: "article" },
          { ref_id: 2, display: "zebra", url: "/dictionary/DICTA/2", category: "dictionary" },
        ],
      });
    }
    return Promise.resolve(new Response("{}", { status: 404 }));
  });
  vi.stubGlobal("fetch", f);
  return f;
}

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

function Where() {
  const l = useLocation();
  return <output data-testid="where">{l.pathname + l.search + l.hash}</output>;
}

describe("Inlines", () => {
  it("renders links with prefixes, styles, fractions and lists", () => {
    const c: Inline[] = [
      { t: "title-ref", c: ["Zebra"] },
      ", see ",
      { t: "link", kind: "qv", ref: 5, c: ["horse"] },
      " ",
      { t: "link", kind: "see-also", ref: 6, para: 3, c: [{ t: "em", c: ["Ass"] }] },
      " ",
      { t: "link", kind: "paren", ref: 7, c: ["x"] },
      { t: "frac", num: "1", den: "2" },
      { t: "smallcaps", c: ["bc"] },
      { t: "link", kind: "web", url: "www.example.org", c: ["site"] },
      { t: "list", style: "numbered", items: [["one"], ["two"]] },
      { t: "img", id: "00ABCDEF", placement: "right" },
    ];
    const { container } = render(
      <MemoryRouter>
        <Inlines c={c} />
      </MemoryRouter>,
    );
    expect(container.querySelector(".enc-title-ref")?.textContent).toBe("Zebra");
    expect(container.querySelector("a.enc-qv")?.getAttribute("href")).toBe("/item/5");
    expect(container.querySelector("a.enc-see-also")?.getAttribute("href")).toBe("/item/6#p3");
    expect(container.querySelector(".enc-see")?.textContent).toBe("See also ");
    expect(container.textContent).toContain("(x)");
    expect(container.querySelector(".enc-frac")?.textContent).toBe("1⁄2");
    expect(container.querySelector(".enc-sc")?.textContent).toBe("bc");
    expect(container.querySelector("a.enc-web")?.getAttribute("href")).toBe("http://www.example.org");
    expect(container.querySelectorAll("ol.enc-list-numbered li")).toHaveLength(2);
    expect(container.querySelector("img.enc-place-right")?.getAttribute("src")).toBe("/media/file/00abcdef.gif");
  });

  it("bolds the title and the alternative title, not the joining \"or\"", () => {
    const c: Inline[] = [{ t: "title-ref", title: ["Testland"], alt: ["Probaria"], c: ["Testland"] }, ", a made-up country."];
    const { container } = render(
      <MemoryRouter>
        <Inlines c={c} />
      </MemoryRouter>,
    );
    expect(Array.from(container.querySelectorAll(".enc-title-ref")).map((e) => e.textContent)).toEqual(["Testland", "Probaria"]);
    expect(container.querySelector(".enc-alt-title")?.textContent).toBe("Probaria");
    expect(container.textContent).toBe("Testland or Probaria, a made-up country.");
    // Without an alternative title it is one bold span and no "or".
    const one = render(
      <MemoryRouter>
        <Inlines c={[{ t: "title-ref", title: ["Zebra"], alt: [], c: ["Zebra"] }, ", member"]} />
      </MemoryRouter>,
    );
    expect(one.container.querySelectorAll(".enc-title-ref")).toHaveLength(1);
    expect(one.container.textContent).toBe("Zebra, member");
  });

  it("keeps links inside Kids mode", () => {
    const { container } = render(
      <MemoryRouter>
        <Inlines c={[{ t: "link", kind: "qv", ref: 9, c: ["x"] }]} opts={{ kids: true }} />
      </MemoryRouter>,
    );
    expect(container.querySelector("a")?.getAttribute("href")).toBe("/kids/item/9");
  });
});

const body: Block[] = [
  {
    t: "section",
    id: 1,
    kind: "introduction",
    level: 1,
    number: ["I"],
    title: ["INTRODUCTION"],
    blocks: [
      { t: "cluster", col: "margin", items: [{ ref: 100, group: "media" }, { ref: 101, group: "media" }] },
      { t: "p", id: 1, c: ["Intro text."] },
      { t: "media", ref: 102, group: "table", col: "margin" },
      { t: "media", ref: 103, group: "tableinline", col: "margin" },
    ],
  },
  {
    t: "section",
    id: 2,
    kind: "major",
    level: 1,
    number: ["II"],
    title: ["HABITAT"],
    blocks: [
      { t: "p", id: 2, c: ["Habitat text."] },
      { t: "section", id: 3, kind: "minor", level: 2, number: ["II", "A"], title: ["Plains"], blocks: [{ t: "p", id: 3, c: ["Plains text."] }] },
    ],
  },
];

function ctx(collapsed: number[] = [], toggle = () => undefined): BodyContext {
  const media: AssocItem[] = [
    { ref_id: 100, group: "media", kind: "media", media_type: "image", title: "First picture", thumb: { file_name: "a.jtn", url: "/media/file/a.jtn" },
      picon: { file_name: "p100.jsm", url: "/media/file/p100.jsm", width: 192, height: 128 }, image: { file_name: "p100.jpg", url: "/media/file/p100.jpg", width: 556, height: 370 } },
    { ref_id: 101, group: "media", kind: "media", media_type: "image", title: "Second picture", thumb: { file_name: "b.jtn", url: "/media/file/b.jtn" },
      picon: { file_name: "p101.jsm", url: "/media/file/p101.jsm", width: 192, height: 128 } },
    { ref_id: 104, group: "audionested", kind: "audio-set", media_type: "audio", title: "Flag and Anthem",
      image: { file_name: "f.jpg", url: "/media/file/f.jpg", width: 365, height: 180 } },
    { ref_id: 105, group: "media", kind: "media", media_type: "map", no_squish: true, title: "A Map",
      picon: { file_name: "m.jsm", url: "/media/file/m.jsm", width: 192, height: 192 }, image: { file_name: "m.jpg", url: "/media/file/m.jpg", width: 556, height: 414 } },
    { ref_id: 106, group: "sidebar", kind: "sidebar", title: "Ten Who Changed Things", sidebar_category: "HISTORIC DOCUMENTS",
      preview_html: "<p class='enc-intro'>A synthetic preview.</p>" },
  ];
  const tables = new Map<number, DataTable>([
    [102, { ref_id: 102, kind: "facts", json: { entries: [] }, html: "" }],
    [103, { ref_id: 103, kind: "table", title: "Inline Table", json: {}, html: "<table class='enc-table'><tr><td>cell</td></tr></table>" }],
  ]);
  return {
    assoc: new Map(media.map((m) => [m.ref_id, m])),
    tables,
    opts: {},
    collapsed: new Set(collapsed),
    toggle,
    previewSection: 1,
    mediaColumn: true,
    sequence: media,
  };
}

describe("ArticleBody", () => {
  it("renders numbered sections, paragraphs with anchors, a cluster box and inline tables", async () => {
    const fetch = mockFetch();
    const { container } = render(
      <MemoryRouter>
        <MediaViewerProvider>
          <ArticleBody blocks={body} ctx={ctx()} />
        </MediaViewerProvider>
      </MemoryRouter>,
    );
    expect(container.querySelector("#s1 .enc-section-title")?.textContent).toBe("I. INTRODUCTION");
    expect(container.querySelector("#s3 .enc-section-title")?.textContent).toBe("A. Plains");
    expect(container.querySelector("#p2")?.textContent).toBe("Habitat text.");
    // A cluster is one box with two thumbnails; the lone facts table is not in the column.
    const boxes = container.querySelectorAll("figure.enc-mbox");
    expect(boxes).toHaveLength(1);
    expect(boxes[0]!.querySelectorAll(".enc-mbox-thumbs button")).toHaveLength(2);
    expect(container.querySelector(".enc-inline-table td")?.textContent).toBe("cell");
    // The gallery picture comes from the association row: no request per box.
    expect(container.querySelector(".enc-mbox-pic img")?.getAttribute("src")).toBe("/media/file/p100.jsm");
    fireEvent.click(boxes[0]!.querySelectorAll(".enc-mbox-thumbs button")[1]!);
    expect(container.querySelector(".enc-mbox-pic img")?.getAttribute("src")).toBe("/media/file/p101.jsm");
    expect(fetch.mock.calls.filter(([u]) => String(u).startsWith("/api/item/"))).toHaveLength(0);
  });

  it("draws text-column items wide and margin items as boxes, as stored", () => {
    mockFetch();
    const blocks: Block[] = [
      {
        t: "section", id: 1, kind: "introduction", level: 1, number: ["I"], title: ["INTRODUCTION"],
        blocks: [
          { t: "media", ref: 104, group: "audionested", col: "text" },
          { t: "p", id: 1, c: ["One."] },
          { t: "media", ref: 100, group: "media", col: "text" },
          { t: "p", id: 2, c: ["Two."] },
          { t: "media", ref: 105, group: "media", col: "margin" },
          { t: "p", id: 3, c: ["Three."] },
          { t: "cluster", col: "margin", items: [{ ref: 101, group: "media" }] },
        ],
      },
    ];
    const { container } = render(
      <MemoryRouter>
        <MediaViewerProvider>
          <ArticleBody blocks={blocks} ctx={ctx()} />
        </MediaViewerProvider>
      </MemoryRouter>,
    );
    const boxes = Array.from(container.querySelectorAll("figure.enc-mbox"));
    expect(boxes.map((b) => b.classList.contains("enc-mbox-wide"))).toEqual([true, true, false, false]);
    // A 556 px photo is scaled to the 432 px text column; the 365 px flag keeps its size.
    expect(boxes[1]!.querySelector("img")?.getAttribute("width")).toBe("432");
    expect(boxes[1]!.querySelector("img")?.getAttribute("src")).toBe("/media/file/p100.jpg");
    expect(boxes[0]!.querySelector("img")?.getAttribute("width")).toBe("365");
    expect(boxes[2]!.querySelector("img")?.getAttribute("src")).toBe("/media/file/m.jsm");
    // A one-item cluster is a margin box without thumbnails.
    expect(boxes[3]!.querySelector(".enc-mbox-thumbs")).toBeNull();
  });

  it("draws a sidebar teaser in the text column", () => {
    mockFetch();
    const blocks: Block[] = [{ t: "p", id: 1, c: ["Text."] }, { t: "media", ref: 106, group: "sidebar", col: "text" }];
    const { container } = render(
      <MemoryRouter>
        <MediaViewerProvider>
          <ArticleBody blocks={blocks} ctx={ctx()} />
        </MediaViewerProvider>
      </MemoryRouter>,
    );
    const t = container.querySelector("aside.enc-sidebar-teaser")!;
    expect(t.querySelector(".enc-sb-hdr")?.textContent).toBe("Sidebars");
    expect(t.querySelector(".enc-sb-cat")?.textContent).toBe("HISTORIC DOCUMENTS");
    expect(t.querySelector(".enc-sb-title")?.textContent).toBe("Ten Who Changed Things");
    expect(t.querySelector(".enc-sb-body")?.textContent).toBe("A synthetic preview.");
    expect(t.querySelector("a.enc-sb-open")?.getAttribute("href")).toBe("/item/106");
    expect(container.querySelector("figure.enc-mbox")).toBeNull();
  });

  it("prints no section numbers or toggles in the Kids layout, and leaves the grid to the page", () => {
    mockFetch();
    const blocks: Block[] = [...body, { t: "media-grid", col: "grid", items: [{ ref: 100, group: "media" }] }];
    const { container } = render(
      <MemoryRouter>
        <MediaViewerProvider>
          <ArticleBody blocks={blocks} ctx={{ ...ctx(), layout: "kids", mediaColumn: false }} />
        </MediaViewerProvider>
      </MemoryRouter>,
    );
    expect(container.querySelector("#s2 .enc-section-title")?.textContent?.trim()).toBe("HABITAT");
    expect(container.querySelector(".enc-secnum")).toBeNull();
    expect(container.querySelector(".enc-toggle")).toBeNull();
    expect(container.querySelector("figure.enc-mbox")).toBeNull();
    expect(container.querySelector(".enc-inline-table td")?.textContent).toBe("cell");
  });

  it("shows only headings and the introduction preview in the outline", () => {
    mockFetch();
    const toggle = vi.fn();
    const { container } = render(
      <MemoryRouter>
        <MediaViewerProvider>
          <ArticleBody blocks={body} ctx={ctx([1, 2, 3], toggle)} />
        </MediaViewerProvider>
      </MemoryRouter>,
    );
    expect(container.querySelector("#p2")).toBeNull();
    expect(container.querySelector(".enc-preview")?.textContent).toContain("Intro text.");
    expect(container.querySelector("#s3 .enc-section-title")?.textContent).toBe("A. Plains");
    fireEvent.click(container.querySelector("#s2 .enc-toggle")!);
    expect(toggle).toHaveBeenCalledWith(2);
  });
});

describe("SearchBox", () => {
  it("suggests as you type and opens a suggestion", async () => {
    const f = mockFetch();
    render(
      <MemoryRouter>
        <SearchBox />
        <Where />
      </MemoryRouter>,
    );
    const input = screen.getByRole("combobox");
    fireEvent.focus(input);
    fireEvent.change(input, { target: { value: "zeb" } });
    await waitFor(() => expect(screen.getAllByRole("option")).toHaveLength(2));
    expect(String(f.mock.calls.at(-1)?.[0])).toContain("/api/suggest?q=zeb");
    fireEvent.keyDown(input, { key: "ArrowDown" });
    fireEvent.keyDown(input, { key: "ArrowDown" });
    fireEvent.submit(input.closest("form")!);
    expect(screen.getByTestId("where").textContent).toBe("/dictionary/DICTA/2");
  });

  it("runs a full search on Enter without a highlighted suggestion", () => {
    mockFetch();
    render(
      <MemoryRouter>
        <SearchBox kids />
        <Where />
      </MemoryRouter>,
    );
    const input = screen.getByRole("combobox");
    fireEvent.change(input, { target: { value: "big cats" } });
    fireEvent.submit(input.closest("form")!);
    expect(screen.getByTestId("where").textContent).toBe("/kids/search?q=big%20cats");
  });
});

function Opener({ list }: { list: AssocItem[] }) {
  const v = useMediaViewer();
  return (
    <button type="button" onClick={() => v.open(list[0]!.ref_id, list)}>
      open
    </button>
  );
}

describe("MediaViewer", () => {
  it("opens on the media URL parameter, steps with the arrow keys and closes with Escape", async () => {
    mockFetch();
    const list: AssocItem[] = [
      { ref_id: 7, group: "media", title: "Seven" },
      { ref_id: 8, group: "media", title: "Eight" },
    ];
    render(
      <MemoryRouter initialEntries={["/item/1"]}>
        <Routes>
          <Route
            path="*"
            element={
              <MediaViewerProvider>
                <Opener list={list} />
                <Where />
              </MediaViewerProvider>
            }
          />
        </Routes>
      </MemoryRouter>,
    );
    fireEvent.click(screen.getByText("open"));
    expect(screen.getByTestId("where").textContent).toBe("/item/1?media=7");
    await waitFor(() => expect(screen.getByRole("dialog").textContent).toContain("Picture 7"));
    expect(screen.getByText("1 of 2")).toBeTruthy();
    act(() => {
      fireEvent.keyDown(window, { key: "ArrowRight" });
    });
    expect(screen.getByTestId("where").textContent).toBe("/item/1?media=8");
    await waitFor(() => expect(screen.getByRole("dialog").textContent).toContain("Caption of 8"));
    act(() => {
      fireEvent.keyDown(window, { key: "Escape" });
    });
    expect(screen.getByTestId("where").textContent).toBe("/item/1");
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("traps Tab inside the dialog and returns focus to the opener on close", async () => {
    mockFetch();
    const list: AssocItem[] = [
      { ref_id: 7, group: "media", title: "Seven" },
      { ref_id: 8, group: "media", title: "Eight" },
    ];
    render(
      <MemoryRouter initialEntries={["/item/1"]}>
        <Routes>
          <Route path="*" element={<MediaViewerProvider><Opener list={list} /></MediaViewerProvider>} />
        </Routes>
      </MemoryRouter>,
    );
    const opener = screen.getByText("open");
    opener.focus();
    fireEvent.click(opener);
    const dlg = await screen.findByRole("dialog");
    await waitFor(() => expect(document.activeElement).toBe(dlg));
    const buttons = Array.from(dlg.querySelectorAll<HTMLElement>("button:not([disabled])"));
    const first = buttons[0]!;
    const last = buttons[buttons.length - 1]!;
    last.focus();
    fireEvent.keyDown(document.activeElement!, { key: "Tab" });
    expect(document.activeElement).toBe(first);
    fireEvent.keyDown(document.activeElement!, { key: "Tab", shiftKey: true });
    expect(document.activeElement).toBe(last);
    act(() => {
      fireEvent.keyDown(window, { key: "Escape" });
    });
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(document.activeElement).toBe(opener);
  });
});
