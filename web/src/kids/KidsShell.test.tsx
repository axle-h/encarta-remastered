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
    expect(items("Favorites")).toEqual(["Add to Favorites…", "Organize Favorites…", "<empty>"]);
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

  it("Read Aloud submenu follows the VM: start while idle, Stop and Pause/Continue while reading", () => {
    const synth = { speaking: false, paused: false, speak: vi.fn(), cancel: vi.fn(), pause: vi.fn(), resume: vi.fn() };
    vi.stubGlobal("speechSynthesis", synth);
    vi.stubGlobal("SpeechSynthesisUtterance", class { constructor(public text: string) {} });
    vi.spyOn(window, "getSelection").mockReturnValue({ toString: () => "hello" } as Selection);
    setup("/kids/item/1");
    const sub = () => {
      fireEvent.click(screen.getByRole("button", { name: "Tools" }));
      fireEvent.click(screen.getByRole("menuitem", { name: /Read Aloud/ }));
      return (name: RegExp | string) => screen.getByRole("menuitem", { name }) as HTMLButtonElement;
    };
    let item = sub();
    expect(item("Whole Article").disabled).toBe(false);
    expect(item(/Stop Reading/).disabled).toBe(true);
    expect(item("Pause").disabled).toBe(true);
    fireEvent.click(item("Selected Text"));
    expect(synth.speak).toHaveBeenCalledOnce();
    synth.speaking = true;
    item = sub();
    expect(item("Whole Article").disabled).toBe(true);
    expect(item("Selected Text").disabled).toBe(true);
    fireEvent.click(item("Pause"));
    expect(synth.pause).toHaveBeenCalled();
    synth.paused = true;
    item = sub();
    fireEvent.click(item("Continue"));
    expect(synth.resume).toHaveBeenCalled();
    item = sub();
    fireEvent.click(item(/Stop Reading/));
    expect(synth.cancel).toHaveBeenCalled();
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

  it("shows the arrow tooltip on a tool button after a short hover", async () => {
    render(
      <MemoryRouter initialEntries={["/kids"]}>
        <Routes>
          <Route path="*" element={<KidsShell>x</KidsShell>} />
        </Routes>
      </MemoryRouter>,
    );
    fireEvent.mouseEnter(screen.getByRole("button", { name: "Copy" }));
    expect((await screen.findByRole("tooltip")).textContent).toContain("Copy the article or picture.");
    fireEvent.mouseLeave(screen.getByRole("button", { name: "Copy" }));
    expect(screen.queryByRole("tooltip")).toBeNull();
  });
});
