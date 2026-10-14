/**
 * Checking quiz answers (docs/games.md "Quizzes"). A typed answer matches when it equals one of
 * the quiz's accepted answers after folding case, accents, punctuation, spacing, a leading
 * article and thousands separators.
 */
import type { Quiz } from "../api";

export function normaliseAnswer(s: string): string {
  return s
    .replace(/<[^>]*>/g, " ")
    .replace(/&nbsp;/g, " ")
    .normalize("NFKD")
    .replace(/[̀-ͯ]/g, "")
    .toLowerCase()
    .replace(/(\d),(?=\d{3}\b)/g, "$1")
    .replace(/[’'`]/g, "")
    .replace(/[^\p{L}\p{N}.]+/gu, " ")
    .replace(/\.(?!\d)/g, " ")
    .trim()
    .replace(/^(a|an|the)\s+/, "")
    .replace(/\s+/g, " ");
}

export function textAnswerMatches(typed: string, accepted: string[]): boolean {
  const t = normaliseAnswer(typed);
  if (!t) return false;
  return accepted.some((a) => normaliseAnswer(a) === t);
}

/** True when `choice` (a multichoice answer id, or typed text) answers `quiz`. */
export function isRight(quiz: Quiz, choice: string): boolean {
  if (quiz.type === "textAnswer") return textAnswerMatches(choice, quiz.accept);
  return quiz.correct != null && String(quiz.correct) === String(choice);
}

/** Points per right answer: the board games' "Score +5" event (docs/games.md). */
export const QUIZ_POINTS = 5;

/** Plain text of an HTML fragment (for the text answer shown after a miss). */
export function plain(html: string): string {
  return html.replace(/<br\s*\/?>/gi, " ").replace(/<[^>]*>/g, "").replace(/\s+/g, " ").trim();
}
