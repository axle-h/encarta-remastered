// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { MemoryRouter, Route, Routes, useLocation } from "react-router-dom";
import { afterEach, describe, expect, it, vi } from "vitest";
import { SettingsProvider } from "../settings/settings";
import { KidsMenuBar, KidsShell } from "./KidsShell";

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

function Where() {
  return <span data-testid="where">{useLocation().pathname}</span>;
}

function setup(path = "/kids") {
  return render(
    <SettingsProvider>
      <MemoryRouter initialEntries={[path]}>
        <KidsMenuBar />
        <Where />
      </MemoryRouter>
    </SettingsProvider>,
  );
}

const items = (menu: string) => {
  fireEvent.click(screen.getByRole("button", { name: menu }));
  return screen.getAllByRole("menuitem").map((e) => e.textContent?.replace(/\s+/g, " ").trim());
};

describe("Kids menu bar", () => {
  it("has the six captured menus", () => {
    setup();
    const names = screen.getAllByRole("button").map((b) => b.getAttribute("aria-label"));
    expect(names).toEqual(["File", "Edit", "View", "Favorites", "Tools", "Help"]);
  });

  it("lists the captured items", () => {
    setup();
    expect(items("File")).toEqual(["PrintCtrl+P", "Exit"]);
    cleanup();
    setup();
    expect(items("Favorites")).toEqual(["Add", "Organize"]);
    cleanup();
    setup();
    expect(items("Help")[0]).toBe("Microsoft Encarta HelpF1");
  });

  it("Exit goes to the adult home", () => {
    setup("/kids/dictionary");
    fireEvent.click(screen.getByRole("button", { name: "File" }));
    fireEvent.click(screen.getByRole("menuitem", { name: "Exit" }));
    expect(screen.getByTestId("where").textContent).toBe("/");
  });

  it("opens with the keyboard", () => {
    setup();
    fireEvent.keyDown(screen.getByRole("button", { name: "Tools" }), { key: "ArrowDown" });
    expect(screen.getByRole("menu")).toBeTruthy();
  });

  it("Read Aloud submenu speaks the selection, with Stop and Pause", () => {
    const speak = vi.fn();
    const cancel = vi.fn();
    const pause = vi.fn();
    vi.stubGlobal("speechSynthesis", { speaking: true, paused: false, speak, cancel, pause, resume: vi.fn() });
    vi.stubGlobal("SpeechSynthesisUtterance", class { constructor(public text: string) {} });
    vi.spyOn(window, "getSelection").mockReturnValue({ toString: () => "hello" } as Selection);
    setup("/kids/item/1");
    fireEvent.click(screen.getByRole("button", { name: "Tools" }));
    fireEvent.click(screen.getByRole("menuitem", { name: /Read Aloud/ }));
    expect(screen.getAllByRole("menuitem").map((e) => e.textContent)).toEqual(expect.arrayContaining(["Whole Article", "Selected Text", "Stop ReadingF9", "Pause"]));
    fireEvent.click(screen.getByRole("menuitem", { name: "Selected Text" }));
    expect(speak).toHaveBeenCalledOnce();
    fireEvent.click(screen.getByRole("button", { name: "Tools" }));
    fireEvent.click(screen.getByRole("menuitem", { name: /Read Aloud/ }));
    fireEvent.click(screen.getByRole("menuitem", { name: /Stop Reading/ }));
    expect(cancel).toHaveBeenCalled();
    vi.unstubAllGlobals();
  });

  it("Read Aloud and Find are disabled on the Kids home", () => {
    setup();
    fireEvent.click(screen.getByRole("button", { name: "Tools" }));
    expect((screen.getByRole("menuitem", { name: /Read Aloud/ }) as HTMLButtonElement).disabled).toBe(true);
    expect((screen.getByRole("menuitem", { name: /Find in this Article/ }) as HTMLButtonElement).disabled).toBe(true);
  });

  it("cascades with the keyboard: Right opens, Left closes, Enter activates", () => {
    setup("/kids/dictionary");
    fireEvent.keyDown(screen.getByRole("button", { name: "View" }), { key: "ArrowDown" });
    const parent = screen.getByRole("menuitem", { name: /Closed Captioning/ });
    expect(parent.getAttribute("aria-haspopup")).toBe("menu");
    expect(document.activeElement).toBe(screen.getByRole("menuitem", { name: /Go to/ }));
    fireEvent.keyDown(document.activeElement!, { key: "ArrowDown" });
    expect(document.activeElement).toBe(parent);
    fireEvent.keyDown(parent, { key: "ArrowRight" });
    const on = screen.getByRole("menuitem", { name: /On/ });
    expect(document.activeElement).toBe(on);
    fireEvent.keyDown(on, { key: "ArrowLeft" });
    expect(screen.queryByRole("menuitem", { name: /On/ })).toBeNull();
    expect(document.activeElement).toBe(parent);
    fireEvent.keyDown(parent, { key: "ArrowRight" });
    fireEvent.click(screen.getByRole("menuitem", { name: /On/ }));
    expect(screen.queryByRole("menu")).toBeNull();
  });

  it("lists the captured Go to, Help and sounds submenus", () => {
    setup("/kids/dictionary");
    fireEvent.click(screen.getByRole("button", { name: "View" }));
    fireEvent.click(screen.getByRole("menuitem", { name: /Go to/ }));
    const sub = screen.getAllByRole("menu")[1]!;
    expect(Array.from(sub.querySelectorAll('[role="menuitem"]')).map((e) => e.textContent)).toEqual(["BackAlt+Left", "ForwardAlt+Right", "Home PageAlt+Home"]);
    cleanup();
    setup();
    fireEvent.click(screen.getByRole("button", { name: "Help" }));
    fireEvent.click(screen.getByRole("menuitem", { name: /Troubleshooting/ }));
    expect(screen.getByRole("menuitem", { name: "Diagnosing Problems" })).toBeTruthy();
    expect(screen.getByRole("menuitem", { name: "Contacting Technical Support" })).toBeTruthy();
    cleanup();
    setup();
    fireEvent.click(screen.getByRole("button", { name: "Tools" }));
    fireEvent.click(screen.getByRole("menuitem", { name: /Startup Sound/ }));
    expect(screen.getByRole("menuitem", { name: /On/ })).toBeTruthy();
  });

  it("Ctrl+L toggles captions and F9 stops reading", () => {
    const cancel = vi.fn();
    vi.stubGlobal("speechSynthesis", { speaking: true, cancel });
    setup("/kids/item/1");
    fireEvent.keyDown(window, { key: "F9" });
    expect(cancel).toHaveBeenCalled();
    fireEvent.keyDown(window, { key: "l", ctrlKey: true });
    fireEvent.click(screen.getByRole("button", { name: "View" }));
    fireEvent.click(screen.getByRole("menuitem", { name: /Closed Captioning/ }));
    expect(screen.getByRole("menuitem", { name: /On/ }).textContent).toContain("✓");
    vi.unstubAllGlobals();
  });
});

describe("KidsShell", () => {
  it("shows the toolbar buttons", () => {
    render(
      <MemoryRouter initialEntries={["/kids"]}>
        <Routes>
          <Route path="*" element={<KidsShell>x</KidsShell>} />
        </Routes>
      </MemoryRouter>,
    );
    for (const n of ["Home", "Back", "Forward", "Copy", "Print"]) expect(screen.getAllByRole(n === "Home" ? "link" : "button", { name: n }).length).toBeGreaterThan(0);
  });
});
