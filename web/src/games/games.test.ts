/** Game logic on synthetic data: MindMaze castle, quiz answers, Geography Quiz rules, charts, match games. */
import { describe, expect, it } from "vitest";
import { applyAnswer, answerOrder, buildFloor, distances, door, doorKey, sideDir, trapped, turn, unbarIfTrapped, walk, type Dir } from "./mindmaze/castle";
import { isKey, keyOut } from "./mindmaze/art";
import { groupLabel } from "./quiz/Quizzes";
import { isRight, normaliseAnswer, textAnswerMatches } from "./quiz/answer";
import { pick, placeNoun, pointsFor, questionText } from "./geoquiz/geo";
import { rewriteStyle, viaOut, viewFor } from "./geoquiz/view";
import { axisLabel, measureLabel, niceTicks, pickScale, seriesMax } from "./charts/chart";
import { mediaUrl, parseMatch, rewardIndex } from "./match/match";
import type { GeoPlace, Quiz } from "./api";

describe("MindMaze castle", () => {
  it("builds a connected floor whose exit is reachable", () => {
    for (let level = 1; level <= 4; level++) {
      const f = buildFloor(level, 1234 + level, [2, 5]);
      const reach = distances(f, f.start.x, f.start.y, () => true);
      expect(reach.size).toBe(f.w * f.h);
      expect(f.exit).not.toEqual(f.start);
      expect(f.rooms).toHaveLength(f.w * f.h);
      expect(Object.values(f.doors).every((d) => [2, 5].includes(d.category))).toBe(true);
    }
  });

  it("is deterministic per seed", () => {
    expect(buildFloor(2, 99, [])).toEqual(buildFloor(2, 99, []));
  });

  it("shares one door between two rooms", () => {
    expect(doorKey(1, 1, 1)).toBe(doorKey(2, 1, 3));
    expect(doorKey(1, 1, 2)).toBe(doorKey(1, 2, 0));
  });

  it("walks only through open doors and turns", () => {
    const f = buildFloor(1, 7, []);
    const p = { ...f.start, facing: 0 as Dir };
    const d = ([0, 1, 2, 3] as Dir[]).find((x) => door(f, p.x, p.y, x))!;
    expect(walk(f, p, d)).toBeNull();
    door(f, p.x, p.y, d)!.state = "open";
    const next = walk(f, p, d)!;
    expect(next.facing).toBe(d);
    expect(Math.abs(next.x - p.x) + Math.abs(next.y - p.y)).toBe(1);
    expect(turn(p, -1).facing).toBe(3);
    expect(turn(p, 2).facing).toBe(2);
    expect(sideDir(1, "left")).toBe(0);
    expect(sideDir(1, "right")).toBe(2);
  });

  it("unbars doors when the player is cut off from the exit", () => {
    const f = buildFloor(1, 3, []);
    for (const d of Object.values(f.doors)) d.state = "barred";
    expect(trapped(f, f.start.x, f.start.y)).toBe(true);
    expect(unbarIfTrapped(f, f.start.x, f.start.y)).toBe(Object.keys(f.doors).length);
    expect(trapped(f, f.start.x, f.start.y)).toBe(false);
  });

  it("scores per level and never below zero", () => {
    expect(applyAnswer(0, 3, true)).toBe(30);
    expect(applyAnswer(10, 4, false)).toBe(0);
    expect(applyAnswer(50, 2, false)).toBe(40);
  });

  it("shuffles answers stably per question", () => {
    const a = [1, 2, 3, 4];
    expect(answerOrder(a, 5, 1)).toEqual(answerOrder(a, 5, 1));
    expect([...answerOrder(a, 5, 1)].sort()).toEqual(a);
  });

  it("colour-keys cyan", () => {
    const px = new Uint8ClampedArray([0, 255, 255, 255, 10, 250, 250, 255, 1, 2, 3, 255]);
    expect(keyOut(px)).toBe(1);
    expect(px[3]).toBe(0);
    expect(px[7]).toBe(255);
    expect(isKey(4, 252, 249)).toBe(true);
  });
});

describe("quiz answers", () => {
  it("folds case, accents, punctuation, articles and separators", () => {
    expect(normaliseAnswer("  The Côte-d’Ivoire! ")).toBe("cote divoire");
    expect(textAnswerMatches("a Sidewinder", ["sidewinder", "sidewinder snake"])).toBe(true);
    expect(textAnswerMatches("10000", ["10,000"])).toBe(true);
    expect(textAnswerMatches("3.5", ["3.5"])).toBe(true);
    expect(textAnswerMatches("cat", ["dog"])).toBe(false);
    expect(textAnswerMatches("", [""])).toBe(false);
  });

  it("checks multiple choice by answer id", () => {
    const q = { type: "multichoice", correct: "2", accept: [] } as unknown as Quiz;
    expect(isRight(q, "2")).toBe(true);
    expect(isRight(q, "1")).toBe(false);
  });

  it("spells out the boards' coded group names", () => {
    expect(groupLabel("2_NoAm_Countries")).toBe("North America: Countries (round 2)");
    expect(groupLabel("3_Austr&Pacific_CitiesPeople")).toBe("Australia & Pacific: Cities & People (round 3)");
    expect(groupLabel("Pets - Dogs")).toBe("Pets - Dogs");
  });
});

describe("Geography Quiz", () => {
  const points = [25, 20, 10, 5, 0];
  it("lowers the points per wrong pick and ends on the right one", () => {
    let s = { wrong: [] as number[], done: false, points: 0 };
    s = pick(s, 1, 9, points);
    expect(s.wrong).toEqual([1]);
    expect(pointsFor(points, s.wrong.length)).toBe(20);
    expect(pick(s, 1, 9, points)).toBe(s);
    s = pick(s, 2, 9, points);
    s = pick(s, 9, 9, points);
    expect(s).toEqual({ wrong: [1, 2], done: true, points: 10 });
    expect(pointsFor(points, 7)).toBe(0);
  });

  it("words the question by place type", () => {
    expect(questionText({ kind: 0 })).toBe("Which country is this?");
    expect(questionText({ kind: 1, feature_type: "Sovereign Capital City" })).toBe("Which city is this?");
    expect(placeNoun({ kind: 1, feature_type: "mountain" })).toBe("place");
    expect(placeNoun({ kind: 2 })).toBe("continent");
  });

  it("fits areas by their box, across the antimeridian too, and cities by a regional view", () => {
    const base = { entity: 1, name: "X", region: 1, choices: [], lat: 0, lon: 0 } as unknown as GeoPlace;
    expect(viewFor({ ...base, kind: 0, west: 170, east: -170, south: -10, north: 10 }).bounds).toEqual([
      [170, -10],
      [190, 10],
    ]);
    expect(viewFor({ ...base, kind: 1, lat: 5, lon: 6 })).toEqual({ center: [6, 5], zoom: 3.6 });
  });

  it("points style URLs at /out/atlas", () => {
    expect(viaOut("http://h:1/atlas/sprites/sprite")).toBe("http://h:1/out/atlas/sprites/sprite");
    expect(viaOut("pmtiles://http://h/out/atlas/vector/earth.pmtiles")).toBe("pmtiles://http://h/out/atlas/vector/earth.pmtiles");
    const s = rewriteStyle({ version: 8, sprite: "/atlas/s", glyphs: "/atlas/g/{fontstack}/{range}.pbf", sources: { a: { type: "vector", url: "pmtiles:///atlas/v.pmtiles" } }, layers: [] });
    expect(s.sprite).toBe("/out/atlas/s");
    expect((s.sources.a as { url: string }).url).toBe("pmtiles:///out/atlas/v.pmtiles");
  });
});

describe("charts", () => {
  it("picks the smallest preset scale that fits", () => {
    const scales = [
      [0, 2, 4],
      [0, 20, 40],
      [0, 200, 400],
    ];
    expect(pickScale(scales, 3)).toEqual([0, 2, 4]);
    expect(pickScale(scales, 4)).toEqual([0, 2, 4]);
    expect(pickScale(scales, 41)).toEqual([0, 200, 400]);
    expect(pickScale(scales, 9999)).toEqual([0, 200, 400]);
    expect(seriesMax([[null, 3], [7, null]])).toBe(7);
  });

  it("puts ticks on the axis ends", () => {
    expect(niceTicks(0, 90, 8)).toEqual([0, 10, 20, 30, 40, 50, 60, 70, 80, 90]);
    const t = niceTicks(-30, 90, 8);
    expect(t[0]).toBe(-30);
    expect(t[t.length - 1]).toBe(90);
    expect(niceTicks(0, 4000, 8).at(-1)).toBe(4000);
  });

  it("labels", () => {
    expect(axisLabel(12_000_000)).toBe("12M");
    expect(axisLabel(4000)).toBe("4,000");
    expect(measureLabel("Percantage Urban")).toBe("Percentage Urban");
  });
});

describe("match games", () => {
  const def = {
    iaigame: { strings: { currentleveltext: { value: "Level:" }, resetbuttontext: { value: "Again!" } } },
    slides: {
      slide: [
        {
          "@sequence": "2",
          "@itemsperbucket": "3",
          "@topbuckettype": "Text",
          "@bottombuckettype": "image",
          standardslide: { caption: { body: "Second" }, items: { item: { "@id": "9", topbucket: { caption: { title: "T" } }, bottombucket: { image: { value: "b.jpg" } }, solvedbucket: { caption: { title: "S" } } } }, rewards: "" },
        },
        {
          "@sequence": "1",
          "@itemsperbucket": "8",
          "@topbuckettype": "image",
          "@bottombuckettype": "Text",
          standardslide: {
            caption: { body: "First" },
            items: { item: [{ "@id": "0", topbucket: { image: { value: "a.jpg" }, caption: { body: "hint" } }, bottombucket: { caption: { title: "Colony" } }, solvedbucket: { caption: { title: "Ants" } } }] },
            rewards: { reward: [{ text: { value: "Perfect" } }, { text: { value: "Great" } }, { text: { value: "Good" } }] },
          },
        },
      ],
    },
  };

  it("reads slides as levels in sequence order", () => {
    const m = parseMatch(def);
    expect(m.levels.map((l) => l.caption)).toEqual(["First", "Second"]);
    expect(m.levels[0]!.topType).toBe("image");
    expect(m.levels[1]!.topType).toBe("text");
    expect(m.levels[1]!.bottomType).toBe("image");
    expect(m.levels[0]!.items[0]!.top).toMatchObject({ image: "a.jpg", body: "hint" });
    expect(m.levels[0]!.rewards).toEqual(["Perfect", "Great", "Good"]);
    expect(m.levels[1]!.items[0]!.bottom.image).toBe("b.jpg");
    expect(m.strings.reset).toBe("Again!");
    expect(m.strings.tries).toBe("tries");
  });

  it("chooses the reward by misses", () => {
    expect(rewardIndex(8, 8)).toBe(0);
    expect(rewardIndex(8, 12)).toBe(1);
    expect(rewardIndex(8, 20)).toBe(2);
  });

  it("resolves media names case-insensitively", () => {
    const media = { "T1.JPG": { sha256: "s", kind: "image", variants: { original: { url: "/media/s/original" } } } };
    expect(mediaUrl(media, "t1.jpg", "image")).toBe("/media/s/original");
    expect(mediaUrl(media, "zz.jpg", "image")).toBe("/media/file/zz.jpg");
    expect(mediaUrl(media, undefined, "audio")).toBeUndefined();
  });
});
