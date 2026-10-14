// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, act } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { afterEach, describe, expect, it } from "vitest";
import { COPYRIGHT, citation } from "./commands";
import { BROWSE_ALL, FEATURE_GROUPS, featureHref } from "./featuresData";
import { FeaturesMenu } from "./FeaturesMenu";
import { OptionsDialog } from "./OptionsDialog";
import { PRINT_CHOICES, ScopeDialog } from "./ScopeDialog";
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
    const base = { title: "Print Options", action: "Print", choices: PRINT_CHOICES, onChoose: () => {}, onCancel: () => {} };
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
  it("is part of every citation", () => {
    expect(citation("Zebra", "u")).toContain(COPYRIGHT);
  });
});
