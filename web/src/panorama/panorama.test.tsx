// @vitest-environment jsdom
/** The panoramic tour page on a synthetic tour: hotspots, site map, and the yaw kept in the URL for Back. */
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { MemoryRouter, Route, Routes, useLocation, useNavigate } from "react-router-dom";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { PanoramaTour } from "../pages/PanoramaTour";
import { MediaStage } from "../media/MediaViewer";
import { imageSwf } from "./syntheticSwf";
import type { Item } from "../api/types";

function json(body: unknown) {
  return Promise.resolve(new Response(JSON.stringify(body), { status: 200, headers: { "Content-Type": "application/json" } }));
}

const attrs = (a: Record<string, string>) => ({ source_attrs: { group: "virttour", ...a } });
const surround = { role: "surround", file_name: "s.svj", url: "/m/s.svj", media: { variants: { original: { width: 1440, height: 308, url: "/m/s" } } } };

const items: Record<number, Partial<Item>> = {
  10: {
    ref_id: 10,
    kind: "virtual-tour-node",
    title: "Nave",
    files: [surround] as Item["files"],
    meta: { ...attrs({}), panorama: { panangle: 0, width: 1440 } },
    children: [
      { kind: "label", ref_id: 102 },
      { kind: "hotspot", ref_id: 100 },
      { kind: "hotspot", ref_id: 101 },
      { kind: "site", ref_id: 103 },
      { kind: "media", ref_id: 500, title: "Pulpit" },
    ] as Item["children"],
  },
  11: { ref_id: 11, kind: "virtual-tour-node", title: "Choir", files: [surround] as Item["files"], meta: { ...attrs({}), panorama: { panangle: 30 } }, children: [] },
  100: { ref_id: 100, title: "To the Choir", meta: { ...attrs({ destmedia: "11" }), shed: [[100, 100], [160, 140]], quadrant: [[175, 100], [165, 100], [175, 124], [165, 124]] } },
  101: { ref_id: 101, title: "Oaken pulpit", meta: { ...attrs({ destmedia: "500" }), shed: [[200, 120], [240, 160]] } },
  102: { ref_id: 102, title: "Choir screen", meta: attrs({ x: "130", y: "90" }) },
  103: { ref_id: 103, title: "", files: [{ role: "image", file_name: "m.dib", url: "/m/m", width: 200, height: 80 }] as Item["files"], children: [{ kind: "hotspot", ref_id: 104 }, { kind: "hotspot", ref_id: 105 }] as Item["children"] },
  104: { ref_id: 104, title: "TBD", meta: attrs({ destmedia: "10", x: "20", y: "30" }) },
  105: { ref_id: 105, title: "TBD", meta: attrs({ destmedia: "11", x: "60", y: "30" }) },
};

const tour = {
  ref_id: 1,
  kind: "virtual-tour-node",
  title: "Test Abbey",
  meta: attrs({ type: "2DVT" }),
  children: [
    { kind: "media", ref_id: 10, title: "Nave" },
    { kind: "media", ref_id: 11, title: "Choir" },
  ],
} as unknown as Item;

class FakeImage {
  naturalWidth = 1440;
  naturalHeight = 308;
  decoding = "";
  src = "";
  decode() {
    return Promise.resolve();
  }
}

beforeEach(() => {
  vi.stubGlobal(
    "fetch",
    vi.fn((u: string) => {
      const m = /\/api\/item\/(\d+)/.exec(String(u));
      const it = m ? items[Number(m[1])] : undefined;
      return it ? json({ children: [], files: [], meta: {}, ...it }) : Promise.resolve(new Response("{}", { status: 404 }));
    }),
  );
  vi.stubGlobal("Image", FakeImage);
  vi.spyOn(HTMLCanvasElement.prototype, "getContext").mockReturnValue(null);
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

function Where() {
  const l = useLocation();
  const nav = useNavigate();
  return (
    <>
      <output data-testid="where">{`${l.pathname}${l.search}`}</output>
      <button type="button" onClick={() => nav(-1)}>
        Back
      </button>
    </>
  );
}

function renderAt(url: string) {
  return render(
    <MemoryRouter initialEntries={[url]}>
      <Routes>
        <Route path="/item/1" element={<PanoramaTour tour={tour} />} />
        <Route path="*" element={<p>media page</p>} />
      </Routes>
      <Where />
    </MemoryRouter>,
  );
}

describe("panoramic tour page", () => {
  it("places hotspots and labels and marks the current scene on the site map", async () => {
    renderAt("/item/1?scene=10");
    expect(await screen.findByRole("button", { name: "To the Choir (go there)" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "Oaken pulpit" })).toBeTruthy();
    expect(screen.getByText("Choir screen")).toBeTruthy();
    const here = await screen.findByRole("button", { name: "Nave (you are here)" });
    expect(here.getAttribute("aria-current")).toBe("location");
    expect(screen.getByRole("button", { name: "Choir" }).className).toContain("pano-site-pt");
  });

  it("opens a media hotspot as a page and comes Back to the same scene and yaw", async () => {
    renderAt("/item/1?scene=10&yaw=33");
    const hot = await screen.findByRole("button", { name: "Oaken pulpit" });
    // The view exists once the picture is decoded.
    await waitFor(() => expect(screen.getByRole("application")).toBeTruthy());
    await new Promise((r) => setTimeout(r, 0));
    fireEvent.click(hot);
    await waitFor(() => expect(screen.getByTestId("where").textContent).toBe("/item/500"));
    fireEvent.click(screen.getByRole("button", { name: "Back" }));
    await waitFor(() => expect(screen.getByTestId("where").textContent).toBe("/item/1?scene=10&yaw=33.0"));
  });

  it("moves to another scene through a scene hotspot", async () => {
    renderAt("/item/1?scene=10");
    fireEvent.click(await screen.findByRole("button", { name: "To the Choir (go there)" }));
    await waitFor(() => expect(screen.getByTestId("where").textContent).toBe("/item/1?scene=11"));
    expect(await screen.findByRole("heading", { name: "Choir" })).toBeTruthy();
  });
});

describe("single panoramas in the media viewer", () => {
  const vrDef = {
    slides: { slide: { standardslide: { "@vrslide": "1", image: "P.swf", vrdata: { scene: { "@fov": "80", "@initangle": "180", "@displaywidth": "544", "@displayheight": "380" } } } } },
  };
  const panoItem = (subgroup: string, iax = "00aa.iax") =>
    ({
      ref_id: 7,
      kind: "interactive",
      title: "View from a Wall",
      meta: { source_attrs: { group: "flash", subgroup } },
      files: [{ role: "item", file_name: iax, url: `/m/${iax}`, width: 544, height: 401 }],
    }) as unknown as Item;

  function mockApi(definition: unknown) {
    const swf = imageSwf(2000, 500, 230);
    const f = vi.fn((u: string) => {
      if (String(u).startsWith("/api/interactive/")) return json({ file_name: "00aa.iax", edition: "deluxe", definition, refs: [{ element: "image", ref: "P.swf", url: "/media/disc/p.swf" }] });
      if (String(u) === "/media/disc/p.swf") return Promise.resolve(new Response(swf as BlobPart));
      return Promise.resolve(new Response("{}", { status: 404 }));
    });
    vi.stubGlobal("fetch", f);
    URL.createObjectURL = vi.fn(() => "blob:pano");
    URL.revokeObjectURL = vi.fn();
    return f;
  }

  it("draws subgroup 360 interactives natively from the image SWF", async () => {
    const f = mockApi(vrDef);
    render(
      <MemoryRouter>
        <MediaStage item={panoItem("360")} kids={false} />
      </MemoryRouter>,
    );
    expect(await screen.findByRole("application", { name: /View from a Wall/ })).toBeTruthy();
    expect(f.mock.calls.map(([u]) => String(u))).toContain("/media/disc/p.swf");
    expect(document.querySelector(".enc-flash")).toBeNull();
  });

  it("falls back to the Flash player when the definition has no vrslide", async () => {
    mockApi({ slides: { slide: { standardslide: { "@vrslide": "0", image: "a.jpg" } } } });
    render(
      <MemoryRouter>
        <MediaStage item={panoItem("360", "00bb.iax")} kids={false} />
      </MemoryRouter>,
    );
    // (the API client caches definitions by file name, hence another name)
    await waitFor(() => expect(document.querySelector(".enc-flash")).toBeTruthy());
  });
});
