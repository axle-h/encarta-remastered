// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { defaultEndpoints } from "../data";
import { SearchBox } from "../ui/SearchBox";

const ep = defaultEndpoints({ origin: "http://test" });

function mockFetch(handler: (url: string) => { status: number; body: unknown }) {
  const calls: string[] = [];
  vi.stubGlobal(
    "fetch",
    vi.fn(async (url: string) => {
      calls.push(url);
      const r = handler(url);
      return new Response(JSON.stringify(r.body), { status: r.status, headers: { "content-type": "application/json" } });
    }),
  );
  return calls;
}

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

describe("SearchBox", () => {
  it("queries the gazetteer and picks a place with Enter", async () => {
    const calls = mockFetch(() => ({
      status: 200,
      body: { query: "test", places: [{ entity: 7, name: "Testville", feature_type: "Town", context: "Testland", lat: 1, lon: 2 }] },
    }));
    const onPick = vi.fn();
    render(<SearchBox ep={ep} body="earth" onPick={onPick} delay={0} />);
    const input = screen.getByRole("searchbox");
    await act(async () => {
      fireEvent.change(input, { target: { value: "testv" } });
      await new Promise((r) => setTimeout(r, 10));
    });
    expect(calls[0]).toBe("http://test/api/atlas/search?q=testv&body=earth&limit=20");
    expect(await screen.findByText("Testville")).toBeTruthy();
    expect(screen.getByText("Town, Testland")).toBeTruthy();
    fireEvent.keyDown(input, { key: "Enter" });
    expect(onPick).toHaveBeenCalledWith(expect.objectContaining({ entity: 7 }));
    expect(screen.queryByRole("listbox")).toBeNull();
  });

  it("says so when the server has no place search", async () => {
    mockFetch(() => ({ status: 404, body: { error: "not found" } }));
    render(<SearchBox ep={ep} body="moon" onPick={() => undefined} delay={0} />);
    await act(async () => {
      fireEvent.change(screen.getByRole("searchbox"), { target: { value: "mare" } });
      await new Promise((r) => setTimeout(r, 10));
    });
    expect(await screen.findByText("Place search is not available on this server.")).toBeTruthy();
  });
});
