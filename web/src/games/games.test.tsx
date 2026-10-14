// @vitest-environment jsdom
/** Game components on synthetic data, with `fetch` mocked. */
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { ClimateData, Quiz } from "./api";
import { ClimateChart } from "./charts/CityClimate";
import { MatchBoard } from "./match/MatchGame";
import { parseMatch } from "./match/match";
import { QuizPlayer } from "./quiz/Quizzes";

function json(body: unknown) {
  return Promise.resolve(new Response(JSON.stringify(body), { status: 200, headers: { "Content-Type": "application/json" } }));
}

const part = (html: string) => ({ html, image: null, audio: null });
const QUIZZES: Quiz[] = [
  { id: 1, edition: "deluxe", type: "multichoice", question: part("Which is a bird?"), answers: [{ id: "1", ...part("Owl") }, { id: "2", ...part("Bat") }], correct: "1", accept: [], reward: part("Owls are birds."), credits: "", reward_credits: "" },
  { id: 2, edition: "deluxe", type: "textAnswer", question: part("Spell cat"), answers: [], correct: null, accept: ["cat"], reward: part("C-A-T"), credits: "", reward_credits: "Photo: Nobody" },
];

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

describe("QuizPlayer", () => {
  it("plays both quiz types and keeps score", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn((u: RequestInfo | URL) => {
        const ids = new URL(String(u), "http://x").searchParams.get("ids")!.split(",").map(Number);
        return json({ quizzes: ids.map((id) => QUIZZES.find((q) => q.id === id)) });
      }),
    );
    render(
      <MemoryRouter>
        <QuizPlayer title="Test Quiz" ids={[1, 2]} seed={1} />
      </MemoryRouter>,
    );
    // The seeded order is fixed; answer whichever comes first.
    for (let n = 0; n < 2; n++) {
      await screen.findByText(`Question ${n + 1} of 2`);
      if (screen.queryByText("Which is a bird?")) fireEvent.click(screen.getByLabelText("Owl"));
      else fireEvent.change(screen.getByLabelText(/Your answer/), { target: { value: "  The CAT " } });
      fireEvent.click(screen.getByRole("button", { name: "check answer" }));
      expect(await screen.findByText("Correct!")).toBeTruthy();
      fireEvent.click(screen.getByRole("button", { name: "continue" }));
    }
    expect(await screen.findByText("Well done!")).toBeTruthy();
    expect(screen.getByText("SCORE 010")).toBeTruthy();
  });

  it("shows the answer after a miss", async () => {
    vi.stubGlobal("fetch", vi.fn(() => json({ quizzes: [QUIZZES[0]] })));
    render(
      <MemoryRouter>
        <QuizPlayer title="Q" ids={[1]} />
      </MemoryRouter>,
    );
    fireEvent.click(await screen.findByLabelText("Bat"));
    fireEvent.click(screen.getByRole("button", { name: "check answer" }));
    expect(await screen.findByText("Not quite.")).toBeTruthy();
    expect(screen.getByText("Owl")).toBeTruthy();
  });
});

const CLIMATE: ClimateData = {
  id: "c",
  title: "City Climate",
  unit_systems: ["imperial", "metric"],
  default_units: "imperial",
  max_selection: 3,
  strings: { add: "< add a city >", remove: "< remove a city >", not_available: "na = Data not available" },
  categories: [{ id: "t", label: "Temperature" }],
  measures: [
    { id: "t.a", category: "t", label: "Annual Average", units: { imperial: { name: "F", abbr: "ºF", axis_min: 20, axis_max: 90 }, metric: { name: "C", abbr: "ºC", axis_min: -10, axis_max: 35 } } },
  ],
  cities: [
    { name: "Aville", country: "Ruritania", label: "Aville, Ruritania", values: { imperial: { "t.a": 53.5 }, metric: { "t.a": 11.9 } } },
    { name: "Btown", country: "Ruritania", label: "Btown, Ruritania", values: { imperial: { "t.a": null }, metric: { "t.a": null } } },
  ],
};

describe("ClimateChart", () => {
  it("draws chosen cities, na for missing values, and switches units", () => {
    render(<ClimateChart data={CLIMATE} initial={["Aville, Ruritania", "Btown, Ruritania"]} />);
    expect(screen.getByText("53.5")).toBeTruthy();
    expect(screen.getByText("na")).toBeTruthy();
    expect(screen.getByText(/Annual Average \(ºF\)/)).toBeTruthy();
    fireEvent.click(screen.getByLabelText("Metric"));
    expect(screen.getByText("11.9")).toBeTruthy();
    expect(screen.getByText(/Annual Average \(ºC\)/)).toBeTruthy();
    const slot3 = screen.getByLabelText("City 3") as HTMLSelectElement;
    expect(slot3.options[0]!.textContent).toBe("< add a city >");
    expect(screen.getByLabelText<HTMLSelectElement>("City 1").options[0]!.textContent).toBe("< remove a city >");
  });
});

describe("MatchBoard", () => {
  it("matches pairs and counts tries", async () => {
    vi.useFakeTimers();
    const def = parseMatch({
      slides: {
        slide: {
          "@itemsperbucket": "2",
          "@topbuckettype": "Text",
          "@bottombuckettype": "Text",
          standardslide: {
            caption: { body: "Match them" },
            items: {
              item: [
                { "@id": "a", topbucket: { caption: { title: "Kitten" } }, bottombucket: { caption: { title: "Cat" } }, solvedbucket: { caption: { title: "A young cat" } } },
                { "@id": "b", topbucket: { caption: { title: "Puppy" } }, bottombucket: { caption: { title: "Dog" } }, solvedbucket: { caption: { title: "A young dog" } } },
              ],
            },
            rewards: { reward: [{ text: { value: "Perfect!" } }, { text: { value: "Great" } }, { text: { value: "Good" } }] },
          },
        },
      },
    });
    render(
      <MemoryRouter>
        <MatchBoard title="Pets" def={def} media={{}} seed={3} />
      </MemoryRouter>,
    );
    fireEvent.click(screen.getByText("Kitten"));
    fireEvent.click(screen.getByText("Dog"));
    expect(screen.getByText(/0 matches in 1 tries/)).toBeTruthy();
    await act(async () => {
      vi.advanceTimersByTime(800);
    });
    fireEvent.click(screen.getByText("Kitten"));
    fireEvent.click(screen.getByText("Cat"));
    fireEvent.click(screen.getByText("Puppy"));
    fireEvent.click(screen.getByText("Dog"));
    expect(screen.getByText(/2 matches in 3 tries/)).toBeTruthy();
    expect(screen.getByText("A young cat")).toBeTruthy();
    expect(screen.getByText("Great")).toBeTruthy();
    vi.useRealTimers();
  });
});

describe("GeoQuiz start dialog", () => {
  it("lists the four levels by their Encarta names", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(() =>
        json({
          levels: [
            { level: 1, name: "Navigator", difficulty: "easy", questions: 20, points: [5, 4, 3, 2, 0], per_type: [0, 0, 0], type_mix: [0.7, 0.2, 0.1] },
            { level: 4, name: "World Class", difficulty: "most difficult", questions: 20, points: [25, 20, 10, 5, 0], per_type: [4, 4, 4], type_mix: [0.4, 0.6, 0] },
          ],
        }),
      ),
    );
    // MapLibre sets up its worker from a blob URL when imported.
    URL.createObjectURL ??= () => "blob:test";
    const { GeoQuiz } = await import("./geoquiz/GeoQuiz");
    render(
      <MemoryRouter>
        <GeoQuiz />
      </MemoryRouter>,
    );
    await waitFor(() => expect(screen.getByText("Navigator (easy)")).toBeTruthy());
    expect(screen.getByText("World Class (most difficult)")).toBeTruthy();
    expect(screen.getByText(/enter your name and choose your level of difficulty/)).toBeTruthy();
  });
});

describe("GeoQuiz scores dialog", () => {
  it("shows the result with Exit and Try Again", async () => {
    URL.createObjectURL ??= () => "blob:test";
    const { ScoresDialog } = await import("./geoquiz/GeoQuiz");
    const again = vi.fn();
    render(
      <ScoresDialog name="Ann" level={null} score={42} max={100} right={9} total={20} table={[{ name: "Ann", score: 42, level: 1, date: 0 }]} onAgain={again} onExit={() => undefined} />,
    );
    expect(screen.getByRole("dialog", { name: "Geography Quiz - Scores" })).toBeTruthy();
    expect(screen.getAllByText("42").length).toBeGreaterThan(0);
    screen.getByText("Try Again").click();
    expect(again).toHaveBeenCalled();
    expect(screen.getByText("Exit")).toBeTruthy();
    expect(screen.getByText("High Scores for Geography Quiz:")).toBeTruthy();
    expect(screen.getByText("Navigator")).toBeTruthy();
  });
});
