// @vitest-environment jsdom
import { cleanup, render, screen } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { Item } from "../api/types";
import { TimelineStage } from "./MediaViewer";

vi.mock("../api/client", () => ({
  api: { timeline: vi.fn(async () => ({ total: 0, offset: 0, events: [] })), item: vi.fn(), tours: vi.fn() },
}));

afterEach(cleanup);

describe("timeline topic whose row has no start date", () => {
  it("renders without crashing and still offers the Timeline", () => {
    const item = { ref_id: 1, kind: "timeline-topic", title: "French Revolution Topic Timeline", timeline: { seq: 1, title: "x", start: null } } as unknown as Item;
    render(
      <MemoryRouter>
        <TimelineStage item={item} kids={false} />
      </MemoryRouter>,
    );
    expect(screen.getByText("Open in Timeline")).toBeTruthy();
  });
});
