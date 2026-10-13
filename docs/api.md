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
{ "server": "encarta-server 0.1.0", "content": {"schema_version": "1", ...meta table},
  "search": { ...out/search/meta.json } | null, "media_manifest": true, "extracted_fallback": false,
  "tours": true, "atlas": true, "static": true }
```
`content` is `null` without a content database.

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
  "data_table": { "kind": "facts", "json": {…}, "html": "…" } | null,
  "timeline":   TimelineEvent | null }
```
`files[].role` is the DATA role (`thumb`, `picon`, `image`, `audio`, `video`, `cc`, `xml`, `ticon` …).

`AssocItem`:
```json
{ "ref_id": 461550745, "group": "media", "sec": 1, "sequence": 4, "appears": null, "par": null,
  "kind": "media", "media_type": "image", "title": "Grévy’s Zebra", "title_html": "…",
  "caption_html": "…", "thumb": { "file_name": "t790545a.jtn", "url": "/media/file/t790545a.jtn",
  "width": 64, "height": 64 } | null }
```

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
  "source": "content", "word_count": 2496, "paragraph_count": 32, "section_count": 12,
  "body": [ …blocks, content-schema §8… ], "html": "<article class=\"enc-article\" …>",
  "outline": [ …article.outline_json… ],
  "sections": [ { "id": 1, "parent_id": null, "level": 1, "number": "I", "title": "INTRODUCTION", "type": 4 } ],
  "related": [ …REL tree, content-schema §4… ] | null, "related_editions": "DLX,STC,STE",
  "media":  [ AssocItem… ],  // forward associations (placed media, sidebars, tables, further reading …)
  "tables": [ { "ref_id": 461577482, "kind": "facts", "title": "…", "json": {…}, "html": "…" } ] }
```
`related` is the tree whose editions include `DLX` (or `KDC` with `edition=kids`). `body=0`/`html=0` drop
those fields. `404` if the item has no body.

`GET /api/place/{gid}` → `{ "place_id": 12, "links": [ { "ordinal", "kind", "kind_label", "ref_id", "title", "item_kind" } ] }`
(P2MTBL place → content links from `content.sqlite`; `gid` as in `/atlas/place/{gid}` links; `404` when the place
has no links). The atlas gazetteer has the place itself: `GET /api/atlas/place/{gid}` below.

`GET /api/quiz/{quiz_id}[?edition=kids]` → `{ "quiz_id", "edition", "quiz_type", "lang", "quiz": {…} }`.
`GET /api/interactive/{file_name}[?edition=kids]` → `{ "file_name", "edition", "package", "control_type", "title", "definition": {…} }`.

## Dictionary, thesaurus, translation

`GET /api/dictionary` → `{ "dictionaries": [ { "name": "DICTA", "kind": "dictionary", "title": "English",
"entry_count": 105032, "uniq_tag", "lex_file", "guid", "parent_guid", "signature": {…} } ] }`. Kinds:
`dictionary` (`DICTA`), `thesaurus` (`THESA`), `bilingual` (`ENG_FRA`, `FRA_ENG`, `ENG_GER`, `GER_ENG`,
`ENG_ITA`, `ITA_ENG`, `ENG_SPA`, `SPA_ENG`); `DICT`, `THES`, `TRANSL` are the empty group records.

`GET /api/dictionary/lookup?q=zebra[&dict=DICTA][&kind=bilingual][&prefix=1][&limit=50]` → headword and
form lookup, case-insensitive. `prefix=1` is the word wheel (forms starting with `q`, sorted).
```json
{ "query": "zebra", "entries": [ { "dictionary": "DICTA", "kind": "dictionary", "ref_id": 1861707064,
  "headword": "zebra", "summary": "…", "form": "zebra", "form_kind": "headword",
  "url": "/api/dictionary/DICTA/1861707064" } ] }
```
`form_kind`: `headword`, `inflection`, `runon`, `phrase`, `variant`.

`GET /api/dictionary/{dict}/{id}` → `{ "dictionary", "kind", "ref_id", "headword", "variety", "summary",
"body": {…}, "html": "…", "forms": [ {"form", "kind"} ], "audio": [ { "file_name": "….wav",
"url": "/media/file/….wav", "media": MediaInfo | null } ] }`. Pronunciation audio is GSM WAV: use
`media.variants.opus`/`aac`.

## Timeline

`GET /api/timeline/meta` → `{ "categories": [ {"bit": 1, "name": "Science and Technology"} ], "regions": […],
"count": 16465, "min_year": -…, "max_year": … }`.

`GET /api/timeline?from=1700&to=1800[&category=Political History,4][&region=Europe][&q=revolution][&min_importance=3][&limit=500][&offset=0]`
→ events overlapping `[from, to]` (years; negative = BC), ordered by start date. `category`/`region` take names
or bits, comma-separated, matching any.
```json
{ "total": 120, "offset": 0, "events": [ TimelineEvent… ] }
```
`TimelineEvent`: `{ "seq", "ref_id", "kind", "source_table", "title", "title_html",
"start": {"year", "month", "day", "circa"}, "end": {…} | null, "span", "importance", "category_mask",
"region_mask", "categories": ["…"], "regions": ["…"] }`.

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
"value_text", "decimals", "year", "note"} ] }` (ordered by rank).
`GET /api/stats/country/{id}` → `{ "country": {"id", "name"}, "values": [ {"indicator_id", "indicator",
"rank", "value", "value_text", "decimals", "year", "note"} ] }`.

## Search

`GET /api/search?q=french revolution[&kind=article][&edition=deluxe][&limit=20][&offset=0][&snippets=0]`

`kind` is a category — `article`, `media`, `dictionary`, `timeline`, `map`, `quotation`, `sidebar`, `web`,
`other` — or an exact item kind (`video`, `yearbook-article` …). All words must match; when nothing does the
search falls back to any word (`mode: "any"`).
```json
{ "query": "zebra", "mode": "all", "total": 157, "offset": 0,
  "facets": { "kind": {"article": 88, "media": 24, …}, "edition": {"deluxe": 145, "kids": 13} },
  "did_you_mean": "zebra",   // only when there are few hits and a correction exists
  "hits": [ { "ref_id": 761575094, "category": "article", "kind": "article", "editions": ["deluxe"],
              "title": "Zebra", "title_html": "Zebra", "score": 12.3,
              "url": "/item/761575094#p12",   // or "/dictionary/DICTA/1861707064"
              "para": { "kind": "p", "id": 12, "section_id": 4, "section_title": "Stripes", "anchor": "#p12" },
              "snippet_html": "… black and white <b>stripes</b> …" } ] }
```
`para` (deep link to the best-matching paragraph) is omitted when the title itself matches the query.
Facets ignore the `kind`/`edition` filters so they can label tabs.

How the index works (`crates/encarta-search`): one tantivy index with an *item* document per searchable item
(articles, media, maps, quotations, timeline events, sidebars, web links, every dictionary/thesaurus/bilingual
entry; not index terms or home-screen pages) and a *paragraph* document per article paragraph. Items are ranked
by BM25 over title (×4), PFX aliases (×2.5), section headings (×1.5) and text, plus a constant bonus when the
whole query equals a title or whole-title alias, a phrase bonus, and a static prior (category weight × inbound
cross-references; Kids-only items ×0.6 so Premium wins unless `edition=kids`). The best paragraph per hit is a
second query restricted to the hits, earlier paragraphs winning ties. Text is lower-cased, accent-folded and
English-stemmed ("Zebras" finds "zebra").

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

`GET /api/atlas/search?q=paris[&limit=20]` → place-name search with the gazetteer's own query
(`meta.search_sql`: exact name first, then importance, then FTS5 rank; atlas-output.md). Every word of `q` is a
prefix term, all must match (`"par" "fr"`).
```json
{ "query": "paris", "places": [ { "entity": 25262, "name": "Paris", "feature_type": "Sovereign Capital Million+ City",
  "context": "France", "lat": 48.86, "lon": 2.34 } ] }
```

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

  Folders in `name` are ignored (`/media/file/loose_media/dsv/t027282a.wmv` = `t027282a.wmv`); case does not
  matter. With `--extracted`, names the manifest does not know are read from the unpacked disc through the
  catalogs (`baggage.ecs`/`generic.ecs`; pronunciation WAVs are not in those catalogs and need the manifest).

Both support `Range` (single ranges; an end past the file is clamped, so `bytes=0-16383` of a 13 KB file is
`206 bytes 0-13068/13069`; only a start past the end is `416`), `HEAD`, `ETag`/`If-None-Match`,
`Last-Modified`/`If-Modified-Since`, and send the real `Content-Type` (the manifest's sniffed type, not the disc
extension: `.jtn`/`.jsm` are JPEG, `.gtn` GIF, `.dib` BMP …), `X-Content-Type-Options: nosniff` and
`Cache-Control: public, max-age=31536000, immutable` (the bytes behind a URL never change).

## Static files

- `/atlas/…` → `out/atlas/` (layout in [atlas-output.md](atlas-output.md)): PMTiles archives, sprites, glyphs,
  palettes, `miller/` … with the same `Range` rules as media. `GET /atlas/styles/<style>.json` is rewritten on the
  fly: root-relative `sprite`, `glyphs`, source `url`s and `tiles` become absolute for the request's origin
  (`X-Forwarded-Proto` + `X-Forwarded-Host`/`Host`, so `https://…` behind Traefik): `"/atlas/sprites/sprite"` →
  `"https://host/atlas/sprites/sprite"`, `"pmtiles:///atlas/vector/earth.pmtiles"` →
  `"pmtiles://https://host/atlas/vector/earth.pmtiles"`. A path under `/atlas/` that is not a file (e.g. the
  client route `/atlas/place/{gid}`) gets the SPA.
- `/out/atlas/…` (same files and rewriting, `404` when missing) and `/out/tours/…` → `out/tours/` (tour
  `index.json`, `<id>/scene.json`, `scene.glb`, `pvs.bin`, `images/`, `cursors/`): the same paths as the Vite dev
  server's `/out` middleware. Nothing else under `out/` is served (no databases).
- Atlas and tour files: `Cache-Control: public, max-age=3600` plus `ETag`/`Last-Modified` revalidation.
- `/assets/…` → `web/dist/assets/` with `Cache-Control: public, max-age=31536000, immutable` (Vite's hashed
  names); a missing asset is `404`, not the SPA. Other files of `web/dist` and `index.html` get `no-cache`.
- Anything else (`GET`/`HEAD`) that is not a file → `web/dist/index.html` (SPA fallback). Unknown `/api/…` is a
  JSON `404`; unknown `/media/…` and `/out/…` are plain `404`s.
- JSON, HTML, CSS, JS, SVG, XML and VTT responses are gzip/brotli-compressed when the client accepts it;
  ranges and binary media never are.
