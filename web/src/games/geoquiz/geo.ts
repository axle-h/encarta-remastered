/**
 * Geography Quiz rules, as observed in the VM (docs/vm-findings.md, docs/games.md): a game is
 * `questions` (20) places drawn by the server from the level's pools. The map shows the place;
 * the player picks its name from five buttons. Each wrong pick greys that button and lowers the
 * points on offer along the level's `points` list (5, 4, 3, 2, 0 for Navigator).
 */
import type { GeoPlace } from "../api";

/** Points on offer after `wrong` wrong picks. */
export function pointsFor(points: number[], wrong: number): number {
  if (!points.length) return 0;
  return points[Math.min(wrong, points.length - 1)] ?? 0;
}

/** "Which country is this?", "Which city is this?" … */
export function questionText(p: Pick<GeoPlace, "kind" | "feature_type" | "class_name">): string {
  return `Which ${placeNoun(p)} is this?`;
}

export function placeNoun(p: Pick<GeoPlace, "kind" | "feature_type" | "class_name">): string {
  if (p.kind === 0) return "country";
  if (p.kind === 2) return "continent";
  const t = `${p.feature_type ?? ""} ${p.class_name ?? ""}`;
  if (/city|town|capital|PPL/i.test(t)) return "city";
  return "place";
}

export interface QuestionState {
  wrong: number[];
  done: boolean;
  points: number;
}

/** Applies a pick: a wrong entity is greyed; the right one ends the question with the points on offer. */
export function pick(state: QuestionState, choice: number, answer: number, points: number[]): QuestionState {
  if (state.done || state.wrong.includes(choice)) return state;
  if (choice === answer) return { ...state, done: true, points: pointsFor(points, state.wrong.length) };
  return { ...state, wrong: [...state.wrong, choice] };
}

export interface HighScore {
  name: string;
  score: number;
  level: number;
  date: number;
}

const KEY = "encarta.geoquiz.scores.v1";

export function readScores(): HighScore[] {
  try {
    const s = JSON.parse(localStorage.getItem(KEY) ?? "[]") as HighScore[];
    return Array.isArray(s) ? s : [];
  } catch {
    return [];
  }
}

/** Adds a score and keeps the ten best per level; returns the level's table. */
export function addScore(entry: HighScore, existing = readScores()): HighScore[] {
  const all = [...existing, entry];
  const keep: HighScore[] = [];
  for (const lv of [1, 2, 3, 4]) {
    keep.push(...all.filter((s) => s.level === lv).sort((a, b) => b.score - a.score || a.date - b.date).slice(0, 10));
  }
  try {
    localStorage.setItem(KEY, JSON.stringify(keep));
  } catch {
    /* unavailable */
  }
  return keep.filter((s) => s.level === entry.level);
}

export function lastName(): string {
  try {
    return localStorage.getItem("encarta.geoquiz.name") ?? "";
  } catch {
    return "";
  }
}

export function rememberName(n: string) {
  try {
    localStorage.setItem("encarta.geoquiz.name", n);
  } catch {
    /* unavailable */
  }
}
