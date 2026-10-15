// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { CHARS_PER_PAGE, estimatePrintPages } from "./commands";
import { PrintDialog, printChoices, printKindOf } from "./PrintDialog";

afterEach(() => {
  cleanup();
  document.body.innerHTML = "";
});

const longArticle = (pages: number) => {
  document.getElementById("fx")?.remove();
  const fx = document.createElement("div");
  fx.id = "fx";
  document.body.appendChild(fx);
  fx.innerHTML = `<main><h1>Zebra</h1><div class="art-cols"><div class="art-text">${"x".repeat(Math.round(CHARS_PER_PAGE * pages))}</div></div></main>`;
};

describe("Print dialog", () => {
  it("offers Encarta's radios and three buttons for each kind", () => {
    expect(printChoices("article", false).map((c) => c.label)).toEqual(["Whole Article Text", "Whole Article With Media"]);
    expect(printChoices("article", true).map((c) => c.label)).toContain("Selected Text");
    expect(printChoices("picture", false).map((c) => c.label)).toEqual(["Picture", "Picture plus caption"]);
    expect(printChoices("timeline", false).map((c) => c.label)).toEqual(["All events on this page", "All events in current timeline"]);
    expect(printChoices("kids", false).map((c) => c.label)).toEqual(["Whole Article Text"]);
    render(<PrintDialog kind="article" hasSelection={false} onPrint={() => {}} onCancel={() => {}} />);
    expect(screen.getAllByRole("button").map((b) => b.textContent)).toEqual(["×", "Print Setup", "Print", "Cancel"]);
  });

  it("picks the dialog from what is on screen", () => {
    expect(printKindOf("/", false)).toBeNull();
    expect(printKindOf("/timeline", false)).toBe("timeline");
    longArticle(1);
    expect(printKindOf("/item/1", false)).toBe("article");
    expect(printKindOf("/kids/item/1", true)).toBe("kids");
    document.getElementById("fx")!.insertAdjacentHTML("beforeend", `<div class="mv-stage"></div>`);
    expect(printKindOf("/item/1", false)).toBe("picture");
  });

  it("prints a short article straight away, from Print and from Print Setup", () => {
    longArticle(2);
    const onPrint = vi.fn();
    render(<PrintDialog kind="article" hasSelection={false} onPrint={onPrint} onCancel={() => {}} />);
    fireEvent.click(screen.getByRole("button", { name: "Print Setup" }));
    fireEvent.click(screen.getByRole("button", { name: "Print" }));
    expect(onPrint).toHaveBeenCalledTimes(2);
    expect(onPrint).toHaveBeenCalledWith("text");
  });

  it("asks before printing an article longer than 5 pages", () => {
    longArticle(6);
    expect(estimatePrintPages("text")).toBeGreaterThan(5);
    const onPrint = vi.fn();
    const onCancel = vi.fn();
    const dlg = render(<PrintDialog kind="article" hasSelection={false} onPrint={onPrint} onCancel={onCancel} />);
    fireEvent.click(screen.getByRole("button", { name: "Print" }));
    expect(onPrint).not.toHaveBeenCalled();
    expect(screen.getByText(/longer than 5 pages/)).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "No" }));
    expect(onCancel).toHaveBeenCalled();
    expect(onPrint).not.toHaveBeenCalled();
    dlg.unmount();
    render(<PrintDialog kind="article" hasSelection={false} onPrint={onPrint} onCancel={onCancel} />);
    fireEvent.click(screen.getByRole("button", { name: "Print Setup" }));
    fireEvent.click(screen.getByRole("button", { name: "Yes" }));
    expect(onPrint).toHaveBeenCalledWith("text");
  });

  it("asks nothing for a picture or at exactly 5 pages", () => {
    longArticle(5);
    const onPrint = vi.fn();
    const first = render(<PrintDialog kind="article" hasSelection={false} onPrint={onPrint} onCancel={() => {}} />);
    fireEvent.click(screen.getByRole("button", { name: "Print" }));
    expect(onPrint).toHaveBeenCalledTimes(1);
    longArticle(9);
    first.unmount();
    render(<PrintDialog kind="picture" hasSelection={false} onPrint={onPrint} onCancel={() => {}} />);
    fireEvent.click(screen.getByRole("button", { name: "Print" }));
    expect(onPrint).toHaveBeenLastCalledWith("picture");
  });
});
