/**
 * Response shapes of the content server (docs/api.md). Fields are optional where the server omits
 * NULL columns. Body/inline node shapes are content-schema.md §8 (see `content/model.ts`).
 */
import type { ArticleLayout, Block, Inline } from "../content/model";

export type Edition = "deluxe" | "kids";

export interface MediaVariant {
  url: string;
  mime?: string;
  width?: number;
  height?: number;
  bytes?: number;
  duration_ms?: number;
}

export interface MediaInfo {
  sha256: string;
  kind: string;
  format?: string;
  mime?: string;
  size?: number;
  variants: Record<string, MediaVariant>;
}

export interface FileRef {
  file_name: string;
  url: string;
  width?: number;
  height?: number;
}

export interface ItemFile extends FileRef {
  role: string;
  media?: MediaInfo | null;
}

export interface ItemChild {
  kind: string;
  ref_id: number;
  sequence?: number | null;
  title?: string;
  item_kind?: string;
}

export interface AssocItem {
  ref_id: number;
  group: string;
  sec?: number | null;
  sequence?: number | null;
  appears?: number | null;
  par?: number | null;
  kind?: string;
  media_type?: string;
  title?: string;
  title_html?: string;
  caption_html?: string;
  thumb?: FileRef | null;
  /** The gallery-size picture (DATA `picon`). */
  picon?: FileRef | null;
  /** The full-size picture: the item's `image`, else (audio sets, composites) its first child's. */
  image?: FileRef | null;
  /** DATA `NoSquish="1"` (maps, charts, art that keeps its size). */
  no_squish?: boolean;
  /** Sidebar rows only: the sidebar's preview (HTML) and category ("GREAT WORKS OF LITERATURE"), for its teaser. */
  preview_html?: string;
  sidebar_category?: string;
}

export interface DataTable {
  ref_id?: number;
  kind: "facts" | "table" | string;
  title?: string;
  json: unknown;
  html: string;
}

export interface TimelineDate {
  year: number;
  month?: number | null;
  day?: number | null;
  circa?: boolean | number | null;
}

export interface TimelineEvent {
  seq: number;
  ref_id?: number | null;
  kind?: number;
  source_table?: string;
  title: string;
  title_html?: string;
  start: TimelineDate;
  end?: TimelineDate | null;
  span?: number;
  importance?: number;
  category_mask?: number;
  region_mask?: number;
  categories?: string[];
  regions?: string[];
  /** The event item has a body (`/api/article/{ref_id}`). */
  has_article?: boolean;
  /** Plain text of the body (only with `text=1`). */
  text?: string;
  /** How the Timeline draws it (timeline.md §3.2): a square and title, an orange bar, or a striped essay bar. */
  display?: "point" | "period" | "essay";
  /** The boxed upper-case bar label of periods and essays ("HISTORICAL ESSAY: …"). */
  label?: string;
  /** The pop-up date line ("4? BC - AD 29?"), and its two ends. */
  date_text?: string;
  start_text?: string;
  end_text?: string;
  /** Title size rank: 5 = largest (importance 1), 1 = smallest, 0 unranked. */
  prominence?: number;
}

export interface Item {
  ref_id: number;
  kind: string;
  source_group?: string;
  title?: string;
  title_html?: string;
  sort_title?: string;
  display_title_html?: string;
  full_title_html?: string;
  alt_title_html?: string;
  media_type?: string;
  subgroup?: string;
  subtype?: string;
  geocode?: number;
  caption_html?: string;
  credit?: string;
  credit_html?: string;
  preview_html?: string;
  editions?: Edition[];
  meta?: Record<string, unknown>;
  has_article?: boolean;
  files?: ItemFile[];
  children?: ItemChild[];
  media?: AssocItem[];
  appears_in?: AssocItem[];
  data_table?: DataTable | null;
  timeline?: TimelineEvent | null;
}

export interface SectionInfo {
  id: number;
  parent_id?: number | null;
  level: number;
  number?: string;
  title?: string;
  type?: number;
}

export interface OutlineNode {
  t: "p" | "section" | string;
  id: number;
  title?: string;
  kind?: string;
  type?: number;
  items?: OutlineNode[];
}

export interface TreeNode {
  title: Inline[];
  ref?: number;
  group?: string;
  para?: number;
  pn?: number;
  node?: number;
  list?: boolean;
  sort_top?: boolean;
  children?: TreeNode[];
}

export interface Article {
  ref_id: number;
  title: string;
  title_html?: string;
  kind: string;
  /** Page layout (content-schema §8); absent from servers older than schema 2. */
  layout?: ArticleLayout;
  editions?: Edition[];
  source?: string;
  word_count?: number;
  paragraph_count?: number;
  section_count?: number;
  body?: Block[];
  html?: string;
  outline?: OutlineNode[] | null;
  sections?: SectionInfo[];
  related?: TreeNode[] | null;
  related_editions?: string;
  media?: AssocItem[];
  tables?: DataTable[];
}

export interface SearchHit {
  ref_id: number;
  category: string;
  kind: string;
  editions?: Edition[];
  title: string;
  title_html?: string;
  score?: number;
  url: string;
  para?: { kind: string; id: number; section_id?: number; section_title?: string; anchor?: string } | null;
  snippet_html?: string;
  media_type?: string;
  thumb?: FileRef | null;
}

export interface SearchResponse {
  query: string;
  mode?: "all" | "any";
  total: number;
  offset: number;
  facets?: { kind?: Record<string, number>; edition?: Record<string, number> };
  did_you_mean?: string;
  hits: SearchHit[];
}

export interface Suggestion {
  ref_id: number;
  display: string;
  display_html?: string;
  title?: string;
  category?: string;
  editions?: Edition[];
  score?: number;
  fuzzy?: boolean;
  url: string;
}

export interface SuggestResponse {
  query: string;
  suggestions: Suggestion[];
}

export interface SpellResponse {
  query: string;
  corrected?: string | null;
  words: { word: string; known: boolean; suggestions: { word: string; distance: number; freq?: number }[] }[];
}

export interface DictionaryInfo {
  name: string;
  kind: "dictionary" | "thesaurus" | "bilingual" | "translation" | string;
  title: string;
  entry_count: number;
}

export interface DictLookupEntry {
  dictionary: string;
  kind: string;
  ref_id: number;
  headword: string;
  summary?: string;
  form?: string;
  form_kind?: string;
  url?: string;
}

export interface DictLookupResponse {
  query: string;
  entries: DictLookupEntry[];
  /** With `around=N`: the N word-wheel headwords before the query, in wheel order. */
  before?: DictLookupEntry[];
}

export interface DictEntry {
  dictionary: string;
  kind: string;
  ref_id: number;
  headword: string;
  variety?: string;
  summary?: string;
  body?: unknown;
  html: string;
  forms?: { form: string; kind: string }[];
  audio?: { file_name: string; url: string; media?: MediaInfo | null }[];
}

export interface TimelineMeta {
  categories: { bit: number; name: string }[];
  regions: { bit: number; name: string }[];
  count: number;
  min_year: number;
  max_year: number;
}

export interface TimelineResponse {
  total: number;
  offset: number;
  events: TimelineEvent[];
}

export interface InteractiveRef {
  element: string;
  ref: string;
  resolution?: string;
  sha256?: string;
  kind?: string;
  url?: string | null;
}

export interface Interactive {
  file_name: string;
  edition: Edition;
  package?: string;
  control_type?: string;
  title?: string;
  definition: Record<string, unknown>;
  refs?: InteractiveRef[];
}

export interface PlaceResponse {
  place_id: number;
  links: { ordinal: number; kind: number; kind_label?: string; ref_id: number; title?: string; item_kind?: string }[];
}

export interface ToursIndex {
  tours: { id: string; name: string; contentId: number; totalBytes?: number }[];
}
