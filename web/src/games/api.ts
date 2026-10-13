/**
 * Client for the games endpoints (docs/api.md "Games") plus the media helpers the games share.
 */
import { getJson, query } from "../api/client";
import type { MediaInfo } from "../api/types";

export interface MediaRef {
  file_name: string;
  url: string;
  width?: number | null;
  height?: number | null;
  media?: MediaInfo | null;
}

export interface LaunchItem {
  ref_id: number;
  title?: string;
  caption_html?: string;
  thumb?: string;
  picon?: string;
  in_deluxe?: boolean;
  in_kids?: boolean;
}

// --- MindMaze -------------------------------------------------------------------------------

export interface MazeAnswer {
  ordinal: number;
  correct: boolean;
  text: string;
  text_html: string;
  article_ref_id: number;
  article_exists: boolean;
  article_title?: string;
  /** Paragraph of the answer article that answers the clue (server-side anchor). */
  para?: number;
}

export interface MazeQuestion {
  id: number;
  category: number;
  category_label: string;
  level: number;
  clue: string;
  clue_html: string;
  location: number;
  answers: MazeAnswer[];
}

export interface MazeGame {
  level: number;
  seed: number;
  questions: MazeQuestion[];
}

export function getMazeQuestions(
  p: { level: number; categories: number[]; count?: number; seed?: number; exclude?: number[] },
  signal?: AbortSignal,
): Promise<MazeGame> {
  const q = query({
    level: p.level,
    categories: p.categories.length ? p.categories.join(",") : undefined,
    count: p.count,
    seed: p.seed,
    exclude: p.exclude?.length ? p.exclude.slice(-400).join(",") : undefined,
  });
  return getJson(`/api/games/mindmaze/game${q}`, signal, false);
}

export function getMazeAssets(signal?: AbortSignal): Promise<{ assets: Record<string, MediaInfo> }> {
  return getJson("/api/games/mindmaze/assets", signal);
}

// --- quizzes --------------------------------------------------------------------------------

export interface QuizGroup {
  id: string;
  name: string;
  questions: number[];
}

export interface QuizGame {
  file_name: string;
  edition: string;
  title: string;
  iax_title: string;
  groups: QuizGroup[];
  items: LaunchItem[];
}

export interface QuizGamesResponse {
  total: number;
  types: { type: string; quizzes: number }[];
  games: QuizGame[];
  ungrouped: number[];
}

export interface QuizPart {
  html: string;
  image: MediaRef | null;
  audio: MediaRef | null;
}

export interface Quiz {
  id: number;
  edition: string;
  type: "multichoice" | "textAnswer" | string;
  question: QuizPart;
  answers: (QuizPart & { id: string })[];
  correct: string | null;
  accept: string[];
  reward: QuizPart;
  credits: string;
  reward_credits: string;
}

export function getQuizGames(signal?: AbortSignal): Promise<QuizGamesResponse> {
  return getJson("/api/games/quizzes", signal);
}

export async function getQuizzes(ids: number[], signal?: AbortSignal): Promise<Quiz[]> {
  const out: Quiz[] = [];
  for (let i = 0; i < ids.length; i += 200) {
    const r = await getJson<{ quizzes: Quiz[] }>(`/api/games/quiz?ids=${ids.slice(i, i + 200).join(",")}`, signal);
    out.push(...r.quizzes);
  }
  return out;
}

// --- Geography Quiz -------------------------------------------------------------------------

export interface GeoLevel {
  level: number;
  name: string;
  difficulty: string;
  questions: number;
  points: number[];
  per_type: number[];
  type_mix: number[];
  pool?: number[];
}

export interface GeoPlace {
  entity: number;
  name: string;
  kind: 0 | 1 | 2;
  kind_label: "country" | "place" | "continent";
  region: number;
  region_name?: string;
  feature_type?: string;
  context?: string;
  class_name?: string;
  lat: number;
  lon: number;
  west?: number;
  south?: number;
  east?: number;
  north?: number;
  goto_km?: number;
}

export interface GeoGame {
  level: number;
  seed: number;
  settings: GeoLevel | null;
  places: GeoPlace[];
}

export function getGeoLevels(signal?: AbortSignal): Promise<{ levels: GeoLevel[] }> {
  return getJson("/api/games/geoquiz", signal);
}

export function getGeoGame(level: number, seed?: number, signal?: AbortSignal): Promise<GeoGame> {
  return getJson(`/api/games/geoquiz/${level}${query({ seed })}`, signal, false);
}

// --- charts ---------------------------------------------------------------------------------

export type Units = "imperial" | "metric";

export interface ClimateMeasure {
  id: string;
  category: string;
  label: string;
  units: Record<Units, { name: string; abbr: string; axis_min: number; axis_max: number }>;
}

export interface ClimateCity {
  name: string;
  country: string;
  label: string;
  values: Record<Units, Record<string, number | null>>;
}

export interface Narration {
  file: string;
  duration_secs?: number;
  media?: MediaInfo | null;
}

export interface ClimateData {
  id: string;
  title: string;
  dataset_title?: string;
  unit_systems: Units[];
  default_units: Units;
  max_selection: number;
  strings: { add: string; remove: string; not_available: string };
  categories: { id: string; label: string }[];
  measures: ClimateMeasure[];
  cities: ClimateCity[];
  narration?: Narration;
}

export interface PopMeasure {
  id: string;
  label: string;
  units: Record<Units, { name: string; scales: number[][] }>;
}

export interface PopDataset {
  id: string;
  country: string;
  footnote: string;
  default_units: Units;
  strings: { add: string; remove: string };
  category_label: string;
  years: number[];
  axis: { chart_start: number; chart_end: number; label_step: number };
  measures: PopMeasure[];
  regions: { name: string; values: Record<Units, Record<string, (number | null)[]>> }[];
}

export interface PopulationData {
  id: string;
  title: string;
  max_selection: number;
  unit_systems: Units[];
  datasets: PopDataset[];
  narration?: Narration;
}

export function getChart<T>(name: "city_climate" | "population", signal?: AbortSignal): Promise<T> {
  return getJson(`/api/games/chart/${name}`, signal);
}

// --- match games and interactives -------------------------------------------------------------

export interface MatchGameInfo {
  file_name: string;
  edition: string;
  title: string;
  iax_title: string;
  audience: "adult" | "kids";
  items: LaunchItem[];
}

export interface InteractiveResponse {
  file_name: string;
  edition: string;
  control_type: string;
  title: string;
  iax_title: string;
  definition: unknown;
  media: Record<string, MediaInfo>;
  items: LaunchItem[];
}

export function getMatchGames(signal?: AbortSignal): Promise<{ games: MatchGameInfo[] }> {
  return getJson("/api/games/match", signal);
}

export function getInteractive(file: string, edition?: string, signal?: AbortSignal): Promise<InteractiveResponse> {
  return getJson(`/api/games/interactive/${encodeURIComponent(file)}${query({ edition })}`, signal);
}

// --- media helpers --------------------------------------------------------------------------

/** A picture's URL: WebP, then the original, then the disc name. */
export function imageUrl(info: MediaInfo | null | undefined, fallbackName?: string): string | undefined {
  const v = info?.variants ?? {};
  return v.webp?.url ?? v.png?.url ?? v.original?.url ?? (fallbackName ? `/media/file/${fallbackName.toLowerCase()}` : undefined);
}

/** A lossless picture (PNG, then the original): for images that are colour-keyed. */
export function losslessUrl(info: MediaInfo | null | undefined): string | undefined {
  const v = info?.variants ?? {};
  return v.png?.url ?? v.original?.url;
}

/** Audio sources a browser can play, best first. */
export function audioUrls(info: MediaInfo | null | undefined): string[] {
  const v = info?.variants ?? {};
  const out: string[] = [];
  for (const k of ["opus", "aac", "mp3"]) if (v[k]) out.push(v[k].url);
  if (v.original && (info?.format === "mp3" || v.original.mime === "audio/mpeg")) out.push(v.original.url);
  return out;
}

/** Picks the first source the browser says it can play. */
export function playableAudio(info: MediaInfo | null | undefined): string | undefined {
  const urls = audioUrls(info);
  if (typeof document === "undefined") return urls[0];
  const probe = document.createElement("audio");
  const v = info?.variants ?? {};
  for (const k of ["opus", "aac", "mp3"]) {
    const x = v[k];
    if (x && probe.canPlayType?.(x.mime) !== "") return x.url;
  }
  return urls[urls.length - 1];
}

/** Every `?`-free MIDI original (MindMaze music). */
export function midiUrl(info: MediaInfo | null | undefined): string | undefined {
  return info?.variants?.original?.url;
}
