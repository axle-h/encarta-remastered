# HTTP API (`encarta-server`)

Status: **implemented** in `crates/encarta-server` (search in `crates/encarta-search`). Shapes below are the
contract for the web client; fields may be added, not removed or renamed without updating this file.

The server is read-only. It serves the converter output (`out/`, see [content-schema.md](content-schema.md),
[media.md](media.md), [tour-scene.md](tour-scene.md)), the search index (`out/search/`, built by
`encarta-convert search`) and the built web client.

## Running

```
encarta-server [--out DIR] [--static DIR] [--extracted DIR] [--listen ADDR]
```

| flag | env | default | meaning |
|---|---|---|---|
| `--out` | `ENCARTA_OUT` | `out` | converter output: `content.sqlite`, `media/`, `search/`, `tours/`, `atlas/` |
| `--static` | `ENCARTA_STATIC` | `web/dist` | built SPA (`index.html` + `assets/`); skipped if missing |
| `--extracted` | `ENCARTA_EXTRACTED` | off | unpacked disc (`extracted/`); media fallback for names the manifest does not resolve (or while it is missing). Off unless set |
| `--listen` (alias `--bind`) | `ENCARTA_LISTEN` | `0.0.0.0:8080` | listen address |

Flags win over environment variables. Every data path is opened read-only and may be on a read-only
volume (SQLite databases that cannot be opened normally, e.g. a WAL database without its `-shm` file, are
reopened with `immutable=1`; the tantivy index is opened through a lock-free read-only directory).
Logs go to stdout: a start-up line listing which parts of the data directory were found, then one line per
request (`GET /api/search?q=zebra 200 2.1ms`; `/healthz` is not logged). `SIGTERM`/`SIGINT` stop accepting
connections and let in-flight requests finish.

Every part of the data directory is optional and nothing stops start-up; with an empty data directory the
server still answers `/healthz` and serves the SPA. Without `content.sqlite` the content API answers `503`;
without `search/` the search endpoints answer `503`; without `media/manifest.sqlite` (and no `--extracted`)
media is `404`; without `atlas/` or `tours/` those paths are `404` (and the gazetteer endpoints `503`).

The search index is built by `encarta-convert search` (→ `out/search/`, about 25 s, 0.66 GB).

## Conventions

- All API routes are under `/api/`; responses are JSON (`application/json; charset=utf-8`), gzip/brotli
  compressed when the client accepts it. Errors: `{"error": "message"}` with status 400, 404, 500 or 503.
- `{id}` is a decimal content id (`refId`, e.g. `761575094` Zebra).
- `editions` is a list of `"deluxe"` (Encarta Premium) and/or `"kids"` (Encarta Kids).
- Fields from the content database keep their column names; `NULL` columns are omitted. Fields ending in
  `_html` are pre-rendered HTML fragments (classes `enc-*`, content-schema §8); they link to `/item/{id}`,
  `/item/{id}#p{para}`, `/media/file/{name}` and `/atlas/place/{gid}`, which the client handles (the first
  and last are client routes; `/media/file/…` is served by this server).
- Client-side routes (`/item/…`, `/atlas/…`, `/dictionary/…` …) that are not files get the SPA's
  `index.html`. Unknown `/api/…`, `/media/…` and `/out/…` paths are `404`.

## Health and metadata

`GET /healthz` → `200 ok` (plain text), always (liveness/readiness; it does not depend on the data).

`GET /api/meta` → what is available:
```json
{ "server": "encarta-server 0.1.0", "content": {"schema_version": "2", ...meta table},
  "search": { ...out/search/meta.json } | null, "media_manifest": true, "extracted_fallback": false,
  "tours": true, "atlas": true, "soundfont": true, "fonts": ["/fonts/REFSAN.TTF", …], "static": true }
```
`content` is `null` without a content database. `soundfont` says whether `out/soundfont/index.json` exists. `fonts` lists the font files of `out/fonts/` (see "Fonts"); `[]`
when that folder is missing.

## Items and articles

`GET /api/item/{id}[?edition=kids]` → one item (`edition` only picks which edition's copy of a media file
`files[].media` describes) (any kind: article, media, map, timeline event, quotation …):
```json
{ "ref_id": 461550745, "kind": "media", "source_group": "media", "title": "Grévy’s Zebra",
  "title_html": "…", "sort_title": "…", "media_type": "image", "caption_html": "…", "credit": "…",
  "editions": ["deluxe"], "meta": { …item.meta_json… }, "has_article": false,
  "files": [ { "role": "image", "file_name": "t790545a.jpg", "width": 556, "height": 368,
               "url": "/media/file/t790545a.jpg", "media": MediaInfo | null } ],
  "children": [ { "kind": "media", "ref_id": 1, "sequence": 1, "title": "…", "item_kind": "media" } ],
  "media":      [ AssocItem… ],   // forward associations: what this item shows
  "appears_in": [ AssocItem… ],   // reverse associations: where it appears
  "referenced_by": [ { "ref_id", "group", "kind", "title", "title_html", "media_type" } ],  // items whose forward associations name this one
  "parents": [ { "ref_id", "kind", "title", "item_kind" } ],   // items whose `children` list this one
  "data_table": { "kind": "facts", "json": {…}, "html": "…" } | null,
  "timeline":   TimelineEvent | null }
```
`files[].role` is the DATA role (`thumb`, `picon`, `image`, `audio`, `video`, `cc`, `xml`, `ticon` …).
`referenced_by` (up to 500, one row per item, by sort title) is the way back where DATA stores no reverse rows: an
author's quotations, a contributor's articles, the articles that list a web link. `parents` (up to 50) leads from a
tour node to its stop or scene and tour.

An id that is not a DATA item but is in `atlas_media` (MSWORLD.ITR photos `i`/`m` and sounds `w` that P2MTBL links to,
msworld.md §3) is answered in the same shape with `source_group: "atlas-media"`, `kind` `media` or `audio`, the
title and credit, and the files that resolve: `image` (`B`), `picon` (`P`) and, as fallback, the 64-px pushpin
`thumb` (`<stem>.dtn`); `meta.atlas_media` has `place_id`, `paired_ref_id`, `legacy_id`.

`AssocItem`:
```json
{ "ref_id": 461550745, "group": "media", "sec": 1, "sequence": 4, "appears": null, "par": null,
  "kind": "media", "media_type": "image", "title": "Grévy’s Zebra", "title_html": "…",
  "caption_html": "…", "no_squish": true,
  "preview_html": "<p class=\"enc-intro\">…</p>", "sidebar_category": "GREAT WORKS OF LITERATURE",   // sidebar rows only
  "year": "1938",                     // archive rows only
  "subtype": "2DVT",                  // virttour rows only (2DVT, 3DVT)
  "url": "http://www.britainusa.com/", // weblink rows only
  "thumb": { "file_name": "t790545a.jtn", "url": "/media/file/t790545a.jtn", "width": 64, "height": 64 } | null,
  "picon": { "file_name": "t790545a.jsm", "url": "/media/file/t790545a.jsm", "width": 192, "height": 128 } | null,
  "image": { "file_name": "t790545a.jpg", "url": "/media/file/t790545a.jpg", "width": 556, "height": 368 } | null }
```
`thumb`, `picon` (gallery picture) and `image` (full size) are the target's first DATA file of that role; `image`
falls back to the first child's `image` (audio sets, composites: the flag of a flag-and-anthem box). `no_squish` is
present (true) when the target's DATA has `NoSquish="1"` (maps, charts). `preview_html` (the item's preview) and
`sidebar_category` (`meta_json.sidebar_category`, content-schema §2) are set on `sidebar` rows only: what a sidebar
teaser shows. `year` (DATA `year`), `subtype` (DATA `type`) and `url` (the stored address) are set on `archive`,
`virttour`/`vtour` and `weblink` rows only: what the right-column modules print (content-schema §2 "Right column"). With these a client lays out an article's
media without one `/api/item` request per box. `appears` is set on reverse rows only: content-schema.md §3.1.

`MediaInfo` (present once the media manifest exists):
```json
{ "sha256": "11b2…", "kind": "image", "format": "jpeg", "mime": "image/jpeg", "size": 37951,
  "variants": { "original": { "url": "/media/11b2…/original", "mime": "image/jpeg", "width": 556,
                              "height": 368, "bytes": 37951 },
                "webp": {…}, "avif": {…} } }
```
Variant names are the manifest's `derivative.format` values: `original`, `webp`, `avif`, `png`, `opus`,
`aac` (`.m4a`), `mp3`, `mp4`, `webm`, `poster` (WebP still), `vtt` (captions), … Audio/video items need
these: the originals are WMA/WMV/GSM WAV, which browsers cannot play.

`GET /api/article/{id}[?edition=kids][&body=0][&html=0]` → an article body and everything around it:
```json
{ "ref_id": 761575094, "title": "Zebra", "title_html": "Zebra", "kind": "article", "editions": ["deluxe"],
  "source": "content", "layout": "encyclopedia", "word_count": 2496, "paragraph_count": 32, "section_count": 12,
  "body": [ …blocks, content-schema §8… ], "html": "<article class=\"enc-article\" …>",
  "outline": [ …article.outline_json… ],
  "sections": [ { "id": 1, "parent_id": null, "level": 1, "number": "I", "title": "INTRODUCTION", "type": 4 } ],
  "related": [ …REL tree, content-schema §4… ] | null, "related_editions": "DLX,STC,STE",
  "alt_title_html": "Timor-Leste",   // the item's alternative title, when it has one (the body's title-ref carries it too)
  "media":  [ AssocItem… ],  // forward associations (placed media, sidebars, tables, further reading …)
  "tables": [ { "ref_id": 461577482, "kind": "facts", "title": "…", "json": {…}, "html": "…" } ] }
```
`related` is the tree whose editions include `DLX` (or `KDC` with `edition=kids`). `body=0`/`html=0` drop
those fields. `404` if the item has no body. `layout` is `article.layout` (`encyclopedia`, `kids`, `sidebar`: which page
layout to use, content-schema §2/§8). `body` is schema 2 (content-schema §8): `media` blocks carry `col` (`text` |
`margin`), `cluster` blocks group margin items into one box, a `kids` body ends with a `media-grid`, and `title-ref`
nodes carry `title` and `alt` parts.

`GET /api/place/{gid}` → `{ "place_id": 12, "links": [ { "ordinal", "kind", "kind_label", "ref_id", "title", "item_kind" } ] }`
(P2MTBL place → content links from `content.sqlite`; `gid` as in `/atlas/place/{gid}` links; `404` when the place
has no links). Only targets on this disc are listed: DATA items and `atlas_media` rows (titled from there, `item_kind`
`media` or `audio`); P2MTBL is shared with other editions and 2,656 of its targets are on none of this disc's. `kind_label` (p2mtbl.md): `article`, `flag-and-anthem`, `facts-and-figures`, `photo:<theme>`, `music`,
`maptrek-stop` (13), `flag` (14), `anthem` (15), `about-map` (16). The atlas gazetteer has the place itself: `GET /api/atlas/place/{gid}` below.

`GET /api/quiz/{quiz_id}[?edition=kids]` → `{ "quiz_id", "edition", "quiz_type", "lang", "quiz": {…} }`.
`GET /api/interactive/{file_name}[?edition=kids]` → `{ "file_name", "edition", "package", "control_type", "title", "definition": {…},
"refs": [ { "element": "image", "ref": "T627478A.swf", "resolution": "iaf-folder", "sha256": "…", "kind": "flash",
"url": "/media/disc/deluxe/dswMedia/IAF/a/0018888f/T627478A.swf" } ] }`. `refs` are the files the `.iax` names
(manifest `iax_ref`, media.md §2.7; omitted without a manifest); `url` is under `/media/disc/…` when the file lives in
`dswMedia/`, else `/media/{sha256}/original`, `null` when unresolved.

## Dictionary, thesaurus, translation

`GET /api/dictionary` → `{ "dictionaries": [ { "name": "DICTA", "kind": "dictionary", "title": "English",
"entry_count": 105032, "uniq_tag", "lex_file", "guid", "parent_guid", "signature": {…} } ] }`. Kinds:
`dictionary` (`DICTA`), `thesaurus` (`THESA`), `bilingual` (`ENG_FRA`, `FRA_ENG`, `ENG_GER`, `GER_ENG`,
`ENG_ITA`, `ITA_ENG`, `ENG_SPA`, `SPA_ENG`); `DICT`, `THES`, `TRANSL` are the empty group records.

`GET /api/dictionary/lookup?q=zebra[&dict=DICTA][&kind=bilingual][&prefix=1][&around=8][&limit=50]` → headword and
form lookup, case-insensitive. `prefix=1`: forms starting with `q`, sorted. `around=N` is the word wheel as the
reader shows it: **headwords only**, in wheel order (case and accents folded, hyphens and apostrophes ignored:
"parietal lobe", "pari-mutuel", "paring"), `entries` = up to `limit` headwords from the position of `q` on (whether or
not they start with it) and `before` = the `N` headwords before that position (in wheel order, nearest last).
Homographs are separate entries with the same headword (the reader labels them "Paris (1)", "Paris (2)").
```json
{ "query": "zebra", "entries": [ { "dictionary": "DICTA", "kind": "dictionary", "ref_id": 1861707064,
  "headword": "zebra", "summary": "…", "form": "zebra", "form_kind": "headword",
  "url": "/api/dictionary/DICTA/1861707064" } ] }
```
`form_kind`: `headword`, `inflection`, `runon`, `phrase`, `variant`.

`GET /api/dictionary/entry/{id}` → the same entry without naming the dictionary (ref ids are unique across all 13).

`GET /api/dictionary/{dict}/{id}` → `{ "dictionary", "kind", "ref_id", "headword", "variety", "summary",
"body": {…}, "html": "…", "forms": [ {"form", "kind"} ], "audio": [ { "file_name": "….wav",
"url": "/media/file/….wav", "media": MediaInfo | null } ] }`. Pronunciation audio is GSM WAV: use
`media.variants.opus`/`aac`.

## Timeline

`GET /api/timeline/meta` → `{ "categories": [ {"bit": 1, "name": "Science and Technology"} ], "regions": […],
"count": 16465, "min_year": -…, "max_year": … }`.

`GET /api/timeline?from=1700&to=1800[&category=Political History,4][&region=Europe][&q=revolution][&term=Aztec Empire][&min_importance=3][&order=importance][&text=1][&limit=500][&offset=0]`
→ events overlapping `[from, to]` (years; negative = BC; both optional), ordered by start date. `term` keeps the events
a TIMELINE.DAT term lists (timeline.md §5; case-insensitive exact term text: topic timelines such as "Aztec Empire",
centuries, search words); when no term has that text, an event with exactly that title. `category`/`region` take names
or bits, comma-separated, matching any. `order=importance` sorts the most prominent first (`prominence` descending, i.e. importance 1 first; then by
date): with `limit` it is the band of a timeline view, the window's N most important events in one request (`order=date` is
the default; anything else is `400`). `text=1` adds each event's body text.
```json
{ "total": 120, "offset": 0, "events": [ TimelineEvent… ] }
```
`TimelineEvent`: `{ "seq", "ref_id", "kind", "source_table", "title", "title_html",
"start": {"year", "month", "day", "circa"}, "end": {…} | null, "span", "importance", "category_mask",
"region_mask", "categories": ["…"], "regions": ["…"], "has_article": true, "text": "…",
"display": "period", "label": "WORLD WAR II", "date_text": "September 1, 1939 - September 2, 1945",
"start_text": "September 1, 1939", "end_text": "September 2, 1945", "prominence": 5 }`. The display columns
(timeline.md §3.2) say how the Timeline draws the event: `display` `point` (square and title), `period` (orange bar)
or `essay` (striped bar); `label` the boxed bar label (periods and essays only; "HISTORICAL ESSAY: …"); `date_text`
the pop-up date line ("4? BC - AD 29?"); `prominence` the title size rank (5 = largest, 0 unranked). `has_article`: the event
item has a body (`/api/article/{ref_id}`; the 13,061 timeline topics have none). `text` (only with `text=1`): the
plain text of that body, paragraphs joined by `\n`. `picture` (essays, only with `text=1`): `{ "url", "width", "height",
"title" }`, the thumbnail of the essay's first media child, drawn on its bar.

## MindMaze

`GET /api/mindmaze` → `{ "total": 8020, "categories": [ {"category": 0, "label": "…", "questions": n} ],
"levels": [ {"level": 1, "questions": n} ] }`.

`GET /api/mindmaze/questions?[category=0][&level=1][&limit=10][&random=1][&seed=42]` →
`{ "questions": [ Question… ] }`; `GET /api/mindmaze/question/{id}` → `Question`:
`{ "id", "category", "category_label", "level", "clue", "clue_html", "location",
"answers": [ {"ordinal", "correct", "text", "text_html", "article_ref_id", "article_exists"} ] }`.

## Statistics (MSWORLD.SDB)

`GET /api/stats` → `{ "categories": [ {"bit", "name"} ], "indicators": [ {"id", "name", "categories": [...],
"category_mask", "unit": {"id", "unit", "alt_unit", "factor", "offset"} | null, "content_ref_id", "source"} ],
"countries": [ {"id", "name"} ] }`.

`GET /api/stats/indicator/{id}` → `{ "indicator": {…}, "values": [ {"country_id", "country", "rank", "value",
"value_text", "decimals", "year", "year_end", "period", "note"} ] }` (ordered by rank).
`GET /api/stats/country/{id}` → `{ "country": {"id", "name"}, "values": [ {"indicator_id", "indicator",
"rank", "value", "value_text", "decimals", "year", "year_end", "period", "note"} ] }`.
`period` is the year span the value covers, as the atlas prints it in parentheses: "1990-2005", or "1995" for one year
(`year_end` = year + the SDB's C column; msworld.md).

## Search

`GET /api/search?q=french revolution[&kind=article][&edition=deluxe][&limit=20][&offset=0][&snippets=0]`

`kind` is a category — `article`, `media`, `dictionary`, `timeline`, `map`, `quotation`, `sidebar`, `web`,
`other` — or an exact item kind (`video`, `yearbook-article` …). All words must match; when nothing does the
search falls back to any word (`mode: "any"`). The **last word is a prefix** while it is being typed (unless the
query ends with a space or a quote, or the word is one letter): "french rev" matches "revolution", and the French
Revolution article comes first.
```json
{ "query": "zebra", "mode": "all", "total": 157, "offset": 0,
  "facets": { "kind": {"article": 88, "media": 24, …}, "edition": {"deluxe": 145, "kids": 13} },
  "did_you_mean": "zebra",   // only when there are few hits and a correction exists
  "hits": [ { "ref_id": 761575094, "category": "article", "kind": "article", "editions": ["deluxe"],
              "title": "Zebra", "title_html": "Zebra", "score": 12.3,
              "url": "/item/761575094#p12",   // or "/dictionary/DICTA/1861707064"
              "para": { "kind": "p", "id": 12, "section_id": 4, "section_title": "Stripes", "anchor": "#p12" },
              "snippet_html": "… black and white <b>stripes</b> …",
              "media_type": "image", "thumb": { "file_name", "url", "width", "height" } | null } ] }
```
`media_type` and `thumb` (the item's thumbnail) come from the content database (not for dictionary hits; omitted
without one).
`para` (deep link to the best-matching paragraph) is omitted when the title itself matches the query, or starts
with it when the last word is a prefix.
Facets ignore the `kind`/`edition` filters so they can label tabs.
Without `edition` Kids-only items are left out of `hits`/`total` (Encarta Premium's search has none; the Kids UI always sends
`edition=kids`); `edition=all` returns everything. Verified against the VM, vm-findings.md "Search ranking".

How the index works (`crates/encarta-search`): one tantivy index with an *item* document per searchable item
(articles, media, maps, quotations, timeline events, sidebars, web links, index terms, every
dictionary/thesaurus/bilingual entry; not home-screen pages) and a *paragraph* document per article paragraph. Items are ranked
by BM25 over title (×4), PFX aliases (×2.5), section headings (×1.5) and text, plus a constant bonus when the
whole query equals a title or whole-title alias (30; a title minus a trailing parenthetical counts, so "rome" is an exact match for "Rome (Italy)" and "Rome (Georgia)"), a smaller one (20) when a title starts with the query whose last
word is a prefix, phrase bonuses (a phrase-prefix one on titles), and a static prior (category weight × inbound
cross-references; Kids-only items ×0.6). The best paragraph per hit is a
second query restricted to the hits, earlier paragraphs winning ties. Text is lower-cased, accent-folded and
English-stemmed ("Zebras" finds "zebra"). A partly typed last word also matches the 24 most frequent indexed terms
of each field that start with it (unstemmed, folded), each scored by BM25 with the field's boost.

**Index terms** (DATAF, content-schema §4, §10.1) are hits of kind `index-term` (category `other`, low prior), with the
summary "Index: N entries — …". As in Encarta 2009 they are not pages: a client expands them in place with
`/api/index-term/{id}`. A term is also an alias of each item it points to when it has at most 30 targets
("Herpestidae" finds Mongoose), and a term with one target autocompletes to it. `out/report/search.json` counts them
(`index_terms`, `index_term_aliases`).

`GET /api/index-term/{id}[?edition=kids]` → an index term and its entries (`404` for other ids):
```json
{ "ref_id": 210000001, "title": "Herpestidae", "title_html": "Herpestidae", "editions": ["deluxe"],
  "tree_editions": "STE,STC,DLX,STD",
  "entries": [ { "title": ["mongoose"], "ref": 761574651, "group": "article", "para": 4, "pn": 1 },
               { "title": ["quotations"], "children": [ { "title": […], "ref": 1861504632, "group": "quotation" } ] } ] }
```
`entries` are the children of the tree's root (the root is the term itself): tree nodes as in content-schema §4 (`ref` →
`/item/{ref}#p{para}`; a node without `ref` is a subheading). The tree is the one whose editions include `DLX` (`KDC` with
`edition=kids`), else another.

`GET /api/suggest?q=zeb[&limit=10][&edition=kids][&kind=article]` → autocomplete over Encarta's word wheel:
```json
{ "query": "zeb", "suggestions": [ { "ref_id": 761575094, "display": "Zebra", "display_html": "Zebra",
  "title": "Zebra", "category": "article", "editions": ["deluxe"], "score": 4.1, "fuzzy": false,
  "url": "/item/761575094" } ] }
```
`display` is the word-wheel text (may be an alias such as "Equus zebra"), `title` the item it opens.
`fuzzy` marks typo-tolerant completions (used only when nothing starts with `q`).

`GET /api/spell?q=zebr[&edition=kids][&lexicon=DICTA,THESA]` → spelling from the disc's lexicons
(`lexicon_word`). Default vocabulary: the search spellers `EESPLA01` + `EESPLA01_KIDS` (mostly names and rare words)
plus the `DICTA` and `THESA` headword lists; `edition=kids` uses `EESPLA01_KIDS` + `DICTA`; `lexicon=` picks any
of the 12 lists (`BILFA` etc. for the bilingual dictionaries). Candidates are ranked by edit distance
(Damerau–Levenshtein, ≤ 1 for words of up to 4 letters, else ≤ 2) and how often the word occurs in the articles:
```json
{ "query": "zebr", "corrected": "zebra",
  "words": [ { "word": "zebr", "known": false,
               "suggestions": [ { "word": "zebra", "distance": 1, "freq": 1234 } ] } ] }
```

## Atlas gazetteer (`out/atlas/gazetteer.sqlite`)

`GET /api/atlas/search?q=paris[&body=moon][&limit=20]` → place-name search with the gazetteer's own query
(`meta.search_sql`: exact name first, then importance, then FTS5 rank; atlas-output.md). Every word of `q` is a
prefix term, all must match (`"par" "fr"`). `body=earth|moon` keeps only places on that body (the ranking is the
same; the filter looks up to 5,000 places down the list).
```json
{ "query": "paris", "places": [ { "entity": 25262, "name": "Paris", "feature_type": "Sovereign Capital Million+ City",
  "context": "France", "lat": 48.86, "lon": 2.34, "goto_km": 200.0, "bbox": [2.237, 48.816, 2.469, 48.903],
  "body": "earth", "class_name": "PPL - sovereign capital million+ city" } ] }
```
`goto_km` is the "go to" altitude, `bbox` the extent `[west, south, east, north]` (`null` when the place has none),
`body` `earth` or `moon`, `class_name` the PDC class (`place_full`).

`GET /api/atlas/place/{entity}` → `{ "place": { …place_full row: entity, name, abbr, body, lat, lon, west, south,
east, north, goto_km, importance, feature_type, context, class_name, classes: […] … },
"names": [ {"id", "entity", "name", "kind"} ], "links": [ {"kind", "kind_label", "ref_id", "title", "item_kind"} ] }`
(`title`/`item_kind` come from `content.sqlite` when present).

## Media

- `GET /media/file/{name}[?edition=kids][&original=1]` — a disc file by its baggage/catalog name (lower-case,
  e.g. `t790545a.jtn`, `00045123.wmv`), as used in the pre-rendered HTML. Resolved through the media manifest
  (`catalog_key`, then `file.name`) to the content-addressed original. For images the server may answer with
  a smaller AVIF/WebP derivative when the request's `Accept` allows it (`Vary: Accept`); `original=1` disables
  this. Without a manifest, the original bytes are read from `--extracted`.
- `GET /media/{sha256}/{variant}` — a content-addressed output (`variant` as in `MediaInfo.variants`).
- `GET|POST /media/disc/{edition}/dswMedia/{*path}` — a file of the disc's `dswMedia/` folders (Flash interactives,
  their players, skins and media; the SW\*, SWQUIZ and SWSHARED containers) **by path**, case-insensitive, preferring
  `edition`'s copy (`kids` = the `EE/KIDS/…` containers). Flash movies load their siblings by relative URL, so the
  IAF player runs from here: `…/dswMedia/shared/players/IAFPlayer/base.swf` with `filePath=…/dswMedia/IAF/a/<id>/<id>.iax`.
  An `.iax` that is not in the tree (they live in `baggage/`) is resolved by its name. `POST` is answered like `GET`
  (Flash's `loadMovie` with variables posts them). Original bytes, same headers as `/media/file`, except the IAF skins
  (`…/IAFPlayer/skins/*.swf`): a script that sets `onLoad` on a clip right after `loadMovie` has that `onLoad`
  renamed, since Ruffle runs the handler where Flash does not (games.md, picture answers); ETag `"{sha256}-ruffle1"`,
  no ranges.

  The atlas's MSWORLD.ITR catalog names that are on the disc under another name resolve through the manifest's
  `atlas_file` (media.md §7), after the catalog keys and file names.

  Folders in `name` are ignored (`/media/file/loose_media/dsv/t027282a.wmv` = `t027282a.wmv`); case does not
  matter. With `--extracted`, names the manifest does not know are read from the unpacked disc through the
  catalogs (`baggage.ecs`/`generic.ecs`; pronunciation WAVs are not in those catalogs and need the manifest).

Both support `Range` (single ranges; an end past the file is clamped, so `bytes=0-16383` of a 13 KB file is
`206 bytes 0-13068/13069`; only a start past the end is `416`), `HEAD`, `ETag`/`If-None-Match`,
`Last-Modified`/`If-Modified-Since`, and send the real `Content-Type` (the manifest's sniffed type, not the disc
extension: `.jtn`/`.jsm` are JPEG, `.gtn` GIF, `.dib` BMP …), `X-Content-Type-Options: nosniff` and
`Cache-Control: public, max-age=31536000, immutable` (the bytes behind a URL never change).

## Caching

Every successful `GET /api/…` answer without its own header gets `Cache-Control: public, max-age=3600` (the content
never changes under a running server). Errors, unknown routes, `/api/games/…` (random draws) and unseeded
`/api/mindmaze/questions` get `no-cache`.

## Interface sounds

`GET /sounds/{name}.wav` — one of `out/sounds/` (`startup`, `click`, `tick`, `kids-hover`, `timeline`, `kids-1` … `kids-10`; lower-case letters, digits and
`-` only, else 404); `Cache-Control: public, max-age=3600`. `encarta-convert content` extracts them from the disc's DLL
resources (media.md §9).

## Fonts

`GET /fonts/{file}` — a font of `out/fonts/` (`.ttf`, `.otf`, `.woff`, `.woff2`, name case-insensitive), listed in
`/api/meta` `fonts`; `Cache-Control: public, max-age=3600`. The web client declares the MS Reference faces it finds
there (`REFSER*.TTF`, `REFSAN*.TTF`) and hands those and the Tahoma, Times New Roman and Arial files to Ruffle as
device fonts (web-client.md). `encarta-convert content` copies these 17 TTFs of `AREF/FONTS` to `out/fonts/`
(content-schema §9) and writes a `.woff2` beside each `REFSER*`/`REFSAN*` face (null-transform WOFF2, Brotli; about
53 % smaller). Ruffle reads TrueType/OpenType only, so the TTFs stay; the `@font-face` rules list `local()`, then WOFF2, then TTF.

## Static files

- `/atlas/…` → `out/atlas/` (layout in [atlas-output.md](atlas-output.md)): PMTiles archives, sprites, glyphs,
  palettes, `miller/` … with the same `Range` rules as media. `GET /atlas/styles/<style>.json` is rewritten on the
  fly: root-relative `sprite`, `glyphs`, source `url`s and `tiles` become absolute for the request's origin and keep
  the path the style was fetched under (`/atlas/…` or `/out/atlas/…`: a style read through `/out/atlas/`, as the Vite
  dev proxy forwards it, points at `/out/atlas/…`)
  (`X-Forwarded-Proto` + `X-Forwarded-Host`/`Host`, so `https://…` behind Traefik): `"/atlas/sprites/sprite"` →
  `"https://host/atlas/sprites/sprite"`, `"pmtiles:///atlas/vector/earth.pmtiles"` →
  `"pmtiles://https://host/atlas/vector/earth.pmtiles"`. A path under `/atlas/` that is not a file (e.g. the
  client route `/atlas/place/{gid}`) gets the SPA.
- `/out/atlas/…` (same files and rewriting, `404` when missing) and `/out/tours/…` → `out/tours/` (tour
  `index.json`, `<id>/scene.json`, `scene.glb`, `pvs.bin`, `images/`, `cursors/`) and `/out/soundfont/…` →
  `out/soundfont/` (the MIDI synth's General MIDI soundfont: `index.json`, `presets/{bank}-{program}.json`,
  `samples/{id}.flac` as `audio/flac`; media.md §8): the same paths as the Vite dev server's `/out` middleware.
  Nothing else under `out/` is served (no databases).
- Atlas, tour and soundfont files: `Cache-Control: public, max-age=3600` plus `ETag`/`Last-Modified` revalidation.
- `/assets/…` → `web/dist/assets/` with `Cache-Control: public, max-age=31536000, immutable` (Vite's hashed
  names); a missing asset is `404`, not the SPA. Other files of `web/dist` and `index.html` get `no-cache`.
- Anything else (`GET`/`HEAD`) that is not a file → `web/dist/index.html` (SPA fallback). Unknown `/api/…` is a
  JSON `404`; unknown `/media/…` and `/out/…` are plain `404`s.
- JSON, HTML, CSS, JS, SVG, XML and VTT responses are gzip/brotli-compressed when the client accepts it, and so are
  the large binaries that shrink well: `application/wasm` (Ruffle), `font/ttf`/`otf`, `model/gltf-binary` and
  `application/octet-stream` (tour `pvs.bin`). Ranges, bodies under 512 bytes and media never are.
- Ruffle's content-hashed files under `/ruffle/` (`core.ruffle.<hash>.js`, `<hash>.wasm`) get
  `Cache-Control: public, max-age=31536000, immutable`; `ruffle.js` and the other SPA files `no-cache`.

## Games

Module `crates/encarta-server/src/games.rs`; game rules and data notes in [games.md](games.md). `seed` makes a deal
repeatable (any u64; the response echoes it); without it the server picks one.

`GET /api/games` → what the games page can offer:
```json
{ "mindmaze": { "total": 8020, "categories": [ {"category", "label", "questions"} ] } | null,
  "quiz": { "total": 1811, "types": [ {"type": "multichoice", "quizzes": 1113} ] } | null,
  "geoquiz": { "levels": [ GeoLevel… ] } | null,   // null without the gazetteer
  "charts": [ {"id": "city_climate", "title": "City Climate"}, {"id": "population", …} ],   // converted datasets only
  "match_games": 64 }
```

`GET /api/games/mindmaze/game?[level=1][&categories=0,2][&count=30][&seed=][&exclude=1,2,…]` → `{ "level", "seed",
"questions": [ Question… ] }`: `count` (1–200) random questions of one level (1–4) from the categories (all when
absent); ids in `exclude` come last. `Question` is the `/api/mindmaze/question/{id}` shape; the correct answer also has
`"para"` (the answer article's paragraph that answers the clue, games.md "Answer anchors"; absent when none matches)
and every answer `"article_title"`.

`GET /api/games/mindmaze/assets` → `{ "assets": { "titlescr.dib": MediaInfo, "knock.wav": MediaInfo, "intro.mid":
MediaInfo, … } }`: every `MINDMAZE.EIT` file by lower-case name (`503` without the media manifest).

`GET /api/games/quizzes[?edition=kids]` → the board games and their quiz groups:
```json
{ "total": 1811, "types": […], "ungrouped": [ 3, … ],
  "games": [ { "file_name": "00174d68.iax", "edition": "kids", "title": "Animals Quiz", "iax_title": "Kids: Animals Quiz",
               "groups": [ {"id": "1", "name": "Pets - General", "questions": [1188, …]} ],
               "items": [ LaunchItem… ] } ] }
```
`LaunchItem` (the `interactive` items whose DATA `item` file is the `.iax`): `{ "ref_id", "title", "caption_html",
"thumb": "/media/file/….gtn", "picon": "/media/file/….gsm", "in_deluxe", "in_kids", "subgroup" }`. Group `questions`
keep only ids in the `quiz` table.

`GET /api/games/quiz?ids=1,2,…[&edition=kids]` (1–200 ids; `400` otherwise) → `{ "quizzes": [ Quiz… ] }` in the order
asked (unknown ids skipped):
```json
{ "id": 1, "edition": "deluxe", "type": "multichoice" | "textAnswer",
  "question": QuizPart, "answers": [ QuizPart + {"id": "1"} ], "correct": "2" | null, "accept": ["sidewinder", …],
  "reward": QuizPart, "credits": "…", "reward_credits": "…" }
```
`QuizPart` = `{ "html": "…", "image": MediaRef | null, "audio": MediaRef | null }`; `MediaRef` = `{ "file_name",
"url", "width", "height", "media": MediaInfo | null }`, resolved in the quiz's own folder through the manifest's
`iax_ref`. Multiple-choice quizzes have `correct` (an answer `id`); typed ones have `accept`.

`GET /api/games/geoquiz` → `{ "levels": [ GeoLevel… ] }`; `GeoLevel` = `{ "level": 1, "name": "Navigator",
"difficulty": "easy", "questions": 20, "points": [5,4,3,2,0], "per_type": [0,0,0], "type_mix": [0.7,0.2,0.1],
"pool": [47, 20, 7] }` (`pool` = places per type). `503` without the gazetteer.

`GET /api/games/geoquiz/{level}[?seed=][&count=20]` → a game (`404` for a level outside 1–4):
```json
{ "level": 4, "seed": 2, "settings": GeoLevel,
  "places": [ { "entity": 143, "name": "Slovakia", "kind": 0, "kind_label": "country" | "place" | "continent",
                "region": 47609, "region_name": "Eastern Europe", "feature_type", "context", "class_name",
                "lat", "lon", "west", "south", "east", "north", "goto_km",
                "choices": [ {"entity": 143, "name": "Slovakia"}, … 5 in all, in display order ] } ] }
```

`GET /api/games/chart/{city_climate|population}` → the converted dataset (shockwave.md §7) with
`narration.media` = the narration's MediaInfo (MP3/Opus/AAC variants). `404` for other names or when not converted.

`GET /api/games/match[?edition=kids]` → `{ "games": [ { "file_name", "edition", "title", "iax_title", "audience":
"adult" | "kids", "items": [ LaunchItem… ] } ] }` (every `MatchGame` interactive, once per file).

`GET /api/games/interactive/{file}[?edition=kids]` → `{ "file_name", "edition", "package", "control_type", "title",
"iax_title", "definition": {…}, "media": { "<ref as written>": MediaInfo }, "items": [ LaunchItem… ] }`: like
`/api/interactive/{name}` plus every file the `.iax` references, resolved (`iax_ref`). `404` when unknown.
