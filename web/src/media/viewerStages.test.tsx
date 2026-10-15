// @vitest-environment jsdom
import { cleanup, render, screen } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { Item } from "../api/types";
import { WebLinkStage, TimelineStage } from "./MediaViewer";
import { isGenericIcon, mediaClass } from "./pick";
import { timelineHref } from "../pages/itemRoute";

vi.mock("../api/client", () => ({
  api: {
    timeline: vi.fn(async () => ({ total: 2, offset: 0, events: [{ ref_id: 5, title: "Southern States Secede", date_text: "1861", start: { year: 1861 } }] })),
    item: vi.fn(),
    tours: vi.fn(),
  },
}));

afterEach(cleanup);

describe("media viewer: generic kinds", () => {
  it("classifies timeline and web link items", () => {
    expect(mediaClass({ kind: "timeline-topic" })).toBe("timeline");
    expect(mediaClass({ kind: "timeline-event", media_type: "timeline" })).toBe("timeline");
    expect(mediaClass({ kind: "weblink", media_type: "weblink" })).toBe("weblink");
    expect(mediaClass({ kind: "live-link", media_type: "weblink" })).toBe("weblink");
  });

  it("recognises shared baggage icons", () => {
    expect(isGenericIcon({ file_name: "timegp.gsm" })).toBe(true);
    expect(isGenericIcon({ file_name: "tablegp.jsm" })).toBe(true);
    expect(isGenericIcon({ file_name: "t027282a.jsm" })).toBe(false);
  });

  it("builds Timeline routes", () => {
    expect(timelineHref({ kind: "timeline-topic", ref_id: 1, title: "America Fights the Civil War" })).toBe("/timeline?term=America%20Fights%20the%20Civil%20War");
    expect(timelineHref({ kind: "timeline-topic", ref_id: 1, title: "Aztec Empire Topic Timeline" })).toBe("/timeline?term=Aztec%20Empire");
    expect(timelineHref({ kind: "timeline-event", ref_id: 7, title: "x", timeline: {} as never })).toBe("/timeline?event=7");
    expect(timelineHref({ kind: "article", ref_id: 7 })).toBeNull();
  });

  it("shows a topic's events with an Open in Timeline link, the icon only as a thumbnail", async () => {
    const item = {
      ref_id: 701769814,
      kind: "timeline-topic",
      title: "America Fights the Civil War",
      files: [{ role: "picon", file_name: "timegp.gsm" }],
    } as unknown as Item;
    const { container } = render(
      <MemoryRouter>
        <TimelineStage item={item} kids={false} />
      </MemoryRouter>,
    );
    expect(await screen.findByText("Southern States Secede")).toBeTruthy();
    expect(screen.getByText("Open in Timeline").getAttribute("href")).toBe("/timeline?term=America%20Fights%20the%20Civil%20War");
    expect(container.querySelector("img.mv-generic")).not.toBeNull();
  });

  it("shows an event's date and links to it", () => {
    const item = {
      ref_id: 461575820,
      kind: "timeline-event",
      title: "Southern States Secede",
      timeline: { ref_id: 461575820, title: "x", start: { year: 1861 }, date_text: "1860 - 1861" },
    } as unknown as Item;
    render(
      <MemoryRouter>
        <TimelineStage item={item} kids={false} />
      </MemoryRouter>,
    );
    expect(screen.getByText("1860 - 1861")).toBeTruthy();
    expect(screen.getByText("Open in Timeline").getAttribute("href")).toBe("/timeline?event=461575820");
  });

  it("shows a web link's address and the archive copy", () => {
    const item = { ref_id: 1, kind: "weblink", meta: { url: ["http://www.abdn.ac.uk/x"] } } as unknown as Item;
    render(<WebLinkStage item={item} />);
    expect(screen.getByText("Open the web site").getAttribute("href")).toBe("http://www.abdn.ac.uk/x");
    expect(screen.getByText(/archived in 2008/).getAttribute("href")).toContain("web.archive.org");
  });
});
