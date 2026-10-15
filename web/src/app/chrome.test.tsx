// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, act } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { afterEach, describe, expect, it } from "vitest";
import { COPYRIGHT, SOURCE_LINE, SOURCE_LINE_MD, addSourceToCopy } from "./commands";
import { BROWSE_ALL, FEATURE_GROUPS, featureHref } from "./featuresData";
import { FeaturesMenu } from "./FeaturesMenu";
import { OptionsDialog } from "./OptionsDialog";
import { COPY_CHOICES, ScopeDialog } from "./ScopeDialog";
import { shortcutFor } from "./shortcuts";
import { SETTINGS_KEY, SettingsProvider } from "../settings/settings";

const store = new Map<string, string>();
Object.defineProperty(window, "localStorage", {
  configurable: true,
  value: { getItem: (k: string) => store.get(k) ?? null, setItem: (k: string, v: string) => void store.set(k, v), removeItem: (k: string) => void store.delete(k) },
});

afterEach(() => {
  cleanup();
  store.clear();
  delete document.documentElement.dataset.textSize;
});

const key = (k: string, o: Partial<Parameters<typeof shortcutFor>[0]> = {}) => shortcutFor({ key: k, ctrlKey: false, metaKey: false, altKey: false, shiftKey: false, ...o });

describe("Features menu", () => {
  it("lists the three groups and Browse all with /item routes", () => {
    expect(FEATURE_GROUPS.map((g) => g.title)).toEqual(["Dictionary & Foreign Language Tools", "Geography Tools", "Explore"]);
    expect(featureHref(BROWSE_ALL)).toBe("/item/701766702");
    render(
      <MemoryRouter>
        <FeaturesMenu />
      </MemoryRouter>,
    );
    fireEvent.click(screen.getByRole("button", { name: "Features" }));
    expect(screen.getByRole("menuitem", { name: "Dictionary" }).getAttribute("href")).toBe("/item/701712323");
    expect(screen.getByRole("menuitem", { name: "Browse all" })).toBeTruthy();
    expect(screen.queryByRole("menuitem", { name: "Flags of the World" })).toBeNull();
  });
  it("opens a More flyout after the hover delay", async () => {
    render(
      <MemoryRouter>
        <FeaturesMenu />
      </MemoryRouter>,
    );
    fireEvent.click(screen.getByRole("button", { name: "Features" }));
    const more = screen.getAllByRole("menuitem", { name: /More/ })[0]!;
    fireEvent.mouseEnter(more.parentElement!);
    await act(async () => {
      await new Promise((r) => setTimeout(r, 150));
    });
    expect(screen.getByRole("menuitem", { name: "Flags of the World" }).getAttribute("href")).toBe("/item/701713732");
  });
});

describe("shortcuts", () => {
  it("maps Encarta's keys and leaves Ctrl+F and Ctrl+C alone", () => {
    expect(key("Home", { altKey: true })).toEqual({ go: "/" });
    expect(key("ArrowLeft", { altKey: true })).toEqual({ back: -1 });
    expect(key("m", { ctrlKey: true })).toEqual({ go: "/atlas" });
    expect(key("q", { ctrlKey: true })).toEqual({ go: "/games/geoquiz" });
    expect(key("t", { ctrlKey: true })).toEqual({ go: "/timeline" });
    expect(key("s", { ctrlKey: true })).toEqual({ focusSearch: true });
    expect(key("f", { ctrlKey: true })).toBeNull();
    expect(key("c", { ctrlKey: true })).toBeNull();
  });
});

describe("dialogs", () => {
  it("enables Selected Text only with a selection", () => {
    const base = { title: "Copy Options", action: "Copy", choices: COPY_CHOICES, onChoose: () => {}, onCancel: () => {} };
    const { rerender } = render(<ScopeDialog {...base} hasSelection={false} />);
    expect((screen.getByLabelText("Selected Text") as HTMLInputElement).disabled).toBe(true);
    rerender(<ScopeDialog {...base} hasSelection />);
    expect((screen.getByLabelText("Selected Text") as HTMLInputElement).disabled).toBe(false);
  });
  it("stores the options on OK and applies the text size", () => {
    render(
      <SettingsProvider>
        <OptionsDialog onClose={() => {}} />
      </SettingsProvider>,
    );
    expect(screen.getAllByRole("tab")).toHaveLength(5);
    fireEvent.click(screen.getByLabelText("Large"));
    fireEvent.click(screen.getByRole("button", { name: "OK" }));
    expect(JSON.parse(window.localStorage.getItem(SETTINGS_KEY)!).textSize).toBe("large");
    expect(document.documentElement.dataset.textSize).toBe("large");
  });
});

describe("copyright", () => {
  it("is part of the source line", () => {
    expect(SOURCE_LINE).toContain(COPYRIGHT);
  });
});

describe("Ctrl+C in an article", () => {
  it("adds Encarta's source line to the copied text and HTML", () => {
    const art = document.createElement("div");
    art.className = "art-text";
    art.innerHTML = "<p>Zebras are <i>striped</i>.</p>";
    document.body.appendChild(art);
    const range = document.createRange();
    range.selectNodeContents(art.querySelector("p")!);
    window.getSelection()!.removeAllRanges();
    window.getSelection()!.addRange(range);
    const data = new Map<string, string>();
    const e = Object.assign(new Event("copy", { cancelable: true }), { clipboardData: { setData: (t: string, v: string) => void data.set(t, v) } }) as unknown as ClipboardEvent;
    expect(addSourceToCopy(e)).toBe(true);
    expect(data.get("text/plain")).toBe(`Zebras are *striped*.\n\n${SOURCE_LINE_MD}`);
    expect(data.get("text/html")).toContain("<b>Microsoft® Encarta® 2009.</b>");
    art.remove();
  });
});
