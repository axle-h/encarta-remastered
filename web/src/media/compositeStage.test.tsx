// @vitest-environment jsdom
import { cleanup, render } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { Item } from "../api/types";
import { MediaStage, compositeParts } from "./MediaViewer";

vi.mock("../api/client", () => ({ api: { tours: vi.fn(), item: vi.fn(), timeline: vi.fn() } }));
afterEach(cleanup);

const file = (n: string) => ({ file_name: n, url: `/media/file/${n}` });
const composite = {
  ref_id: 701612476,
  kind: "composite-media",
  title: "Landscapes of France",
  files: [{ role: "picon", file_name: "genatlas.jsm", url: "/media/file/genatlas.jsm" }],
  children: [
    { kind: "media", ref_id: 1, title: "Seaside on Corsica", image: file("a.jpg") },
    { kind: "media", ref_id: 2, title: "No picture" },
    { kind: "media", ref_id: 3, title: "Vineyard in Burgundy", picon: file("b.jsm") },
  ],
} as unknown as Item;

describe("composite media", () => {
  it("lists the children that have a picture", () => {
    expect(compositeParts(composite).map((p) => p.ref_id)).toEqual([1, 3]);
    expect(compositeParts({ ...composite, kind: "media" } as Item)).toEqual([]);
  });

  it("shows the photographs instead of the no-picture placeholder", () => {
    const { container } = render(
      <MemoryRouter>
        <MediaStage item={composite} kids={false} />
      </MemoryRouter>,
    );
    expect(container.querySelectorAll(".mv-composite img").length).toBe(2);
    expect(container.querySelector(".mv-placeholder")).toBeNull();
  });
});
