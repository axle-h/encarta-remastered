# Plan

**Goal:** every piece of content on the Encarta Premium 2009 DVD, usable in a modern web browser and
served by a Rust server, with **no content lost and no feature dropped for lack of effort**.
That includes the full interactive 3D tours, the complete atlas (Encarta's own map data, not open-data
substitutes), the games and Encarta Kids.

**Architecture:** an offline, one-time **converter** reads the disc and writes modern formats into
`out/`. A **server** serves `out/` over HTTP, and a **web client** renders it. Work follows the
[clean-room policy](CLEAN_ROOM.md). Format knowledge lives in [`docs/formats/`](formats/inventory.md).

Effort key: S ≈ days, M ≈ 1–2 weeks, L ≈ 2–4 weeks, XL ≈ 1–3 months (part-time scale).

---

## Step 0 — Foundations
- [x] Repository, workspace, clean-room policy, format inventory.
- [x] ITOLITLS/LZX container reader (`eit` CLI): all 80 containers, 437,049 files.
- [x] **Windows XP reference VM** (QEMU/KVM, i386, XP Pro SP3 from the MSDN ISO on the NAS). See [vm.md](vm.md).
  - Install Encarta Premium 2009 from `vendor/`. Snapshot a clean install.
  - VM disks live in `vm/` (gitignored). The licence key is entered at install and never recorded here.
  - Tools inside the VM: a debugger (for confirming decoder behaviour dynamically), and a way to move
    files in and out (shared folder or ISO).
  - Uses: ground-truth screenshots, behaviour reference for tours, atlas and games, and capture of
    **private fixtures** (rendered articles, map views) into `fixtures/` (gitignored).
  - Exit: Encarta launches in the VM, and a repeatable procedure exists to capture an article by id. **Done**
    (`vm/capture-article.sh <id>`). Open: sound in the guest, a debugger workflow.

## Step 1 — Decoders (`crates/encarta-formats`)
For each format: write the spec in `docs/formats/`, implement it, run it over the whole disc,
compare with VM fixtures, and add a dump command to the CLI.

| # | Format | Delivers | Effort |
|---|---|---|---|
| 1.1 | ITOLITLS + LZX | Media, sidebars, dictionaries, interactives, templates | done |
| 1.2 | Catalog `ECN`/`ECS` + `guid.lst` | Resolve any content id to package + record | done (`catalog`) |
| 1.3 | **AKC** (`CONT*/DATA*/DATAF*/REL*`, incl. Kids) | **Article bodies**, article metadata, related articles. Critical path. | done (`akc check --all`: 634,248 records, all spec hashes) |
| 1.4 | Search exports (`PFX`, `IdXE`, `IdXEW`, ECT) | Titles, aliases, section titles (search itself is rebuilt) | done (`findidx`) |
| 1.5 | Small formats: `TIMELINE.DAT`, `MINDMAZE.*`, `P2MTBL.DAT`, `MSWORLD.SDB`, `D*.TXT`, `$WW$CTCATALOG`, `*.LEX` word lists | Timeline, MindMaze, place↔media links, statistics, Geo Quiz pools, photo credits, spelling data | done (`smallfmt`; a few field meanings open) |
| 1.6 | **MAD atlas**: TOC (done) → MOBB codec → `INDX` → `RMAP` rasters → every grid table (geometry, features, gazetteer, styles, symbols, labels) | Full atlas data | decoders done (`mad check --all`: 445,176 tiles, 237,814 geometry records, 1.81 M places); styling spec in [mad-styling.md](formats/mad-styling.md); converted by `encarta-convert atlas` |
| 1.7 | **3D tours**: `.x`, `.3sc` scripts, `.3wm` walk maps, `.3lt` lights, `.3cl` point clouds, `.32d` HUD, `.3vp`; plus ACTX3DX3 runtime behaviour (camera, movement, collision, gravity, hotspots) | Full tour data and behaviour spec | done (`tour`); VM checks open |
| 1.8 | Shockwave/Director (`.dcr`, `.cct`, `.swa`) | Afterburner reader + property lists → JSON for City Climate and Population Chart; SWA → Opus; orphans listed as omitted | done (`director`: both datasets, SWA → MP3); client charts in Step 7 |

Exit: every file in every container and every catalog id has a decoder. No "unknown" bytes are left
that carry content.

## Step 2 — Converter (`crates/encarta-convert`)
One command: `encarta-convert --iso <path> --out out/`. It is idempotent and resumable.

- **Database** (SQLite): articles in our own normalised schema (structured JSON plus pre-rendered
  HTML fragments), outlines, related articles, media metadata (captions, credits, sizes),
  dictionary/thesaurus/bilingual dictionaries, timeline, quizzes, statistics, gazetteer. **Done**: `encarta-convert content`
  → `out/content.sqlite` (schema in [content-schema.md](content-schema.md); 634,247 catalog ids, 0 unexplained).
- **Media** (content-addressed under `out/media/`):
  - images keep their original bytes, plus WebP/AVIF derivatives;
  - WAV/WMA → Opus (+ AAC fallback); WMV → MP4/H.264 + WebM; SAMI captions → WebVTT;
  - MIDI is kept and played with a soundfont synth in the browser;
  - SWF is kept for Ruffle; SWA → strip the header → Opus.
  - **Done**: `encarta-convert media` → `out/media/` + `manifest.sqlite` ([media.md](media.md)): 127,255 unique
    sources, 0 failed, 0 unexplained. AVIF for JPEG sources only, MP3 kept as is, the two Director charts as JSON.
- **Atlas**: raster and vector tiles in PMTiles, a MapLibre style, glyphs from the disc fonts, a symbol sprite sheet.
  **Done**: `encarta-convert atlas` → `out/atlas/` (PMTiles rasters and MVT, `gazetteer.sqlite`, 33 MapLibre
  styles, sprites, SDF glyphs; layout in [atlas-output.md](atlas-output.md); 0 unexplained MAD sections). Zooms use a
  working P/m = 2400 until measured in the VM.
- **3D tours**: glTF scenes plus a JSON scene description (scripts, hotspots, lights, walk maps, HUD). **Done**: `encarta-convert tours`
  → `out/tours/<id>/` (scene.glb, scene.json, pvs.bin; schema in [tour-scene.md](tour-scene.md)).
- **Search**: a tantivy index carrying Encarta's aliases and section titles. **Done**: `encarta-convert search`
  → `out/search/` (586,702 items, 663,085 paragraphs, autocomplete and spelling; see [api.md](api.md)).
- **Completeness report**: every container entry and catalog id is listed as converted, or as
  intentionally omitted with a reason. The target is zero unexplained omissions.

## Step 3 — Server (`crates/encarta-server`)
axum, read-only, one binary pointed at `out/`. Content API (articles, media, dictionary, timeline,
atlas metadata), search and autocomplete, media with HTTP Range, PMTiles tile serving.
**Done**: routes in [api.md](api.md); runs read-only from `out/`, serves `web/dist`, deployed per `k8s/README.md`.

## Step 4 — Web client: core reference
Home, article reader (our renderer for the content schema: quick-verify links, sidebars, tables,
pronunciations, media galleries, outlines), media browser, video with captions, dictionary,
thesaurus and translation, search with autocomplete, timeline, interactives (`.iax` + Ruffle),
Encarta Kids mode.

## Step 5 — Atlas
MapLibre globe and map: every raster look (terrain, satellite, night, moon, tectonic, climate,
population), the vector map with Encarta styling and labels, place search, links between places and
articles, statistics overlays, Geography Quiz.

## Step 6 — 3D virtual tours (full interactivity)
three.js runtime: scene loading, first-person walking with collision and gravity, flickering lights,
point-cloud statues, sky and water, HUD, hotspots that open articles and media, and the `.3sc`
script behaviours. Checked side by side with the VM.
**Done** (VM side-by-side still open): `web/src/tours/` (`mountTour`), dev page `web/tours.html`; all 7 tours
load and walk; design and parity choices in [tour-runtime.md](tour-runtime.md).

## Step 7 — Games and remaining extras
MindMaze, quizzes (`SWQUIZ`), Geography Quiz, City Climate and Population Chart (rebuilt from the 1.8 data).

## Step 8 — Parity audit
A feature-by-feature checklist against the VM; fix gaps; performance and accessibility pass.

---

## Risks
- ~~**AKC codec** (1.3)~~: resolved. Every record decodes and matches the catalog.
- **MAD vector tables** (1.6): about 30 undocumented table types. Mitigation: start with rasters to prove the codec; decode tables one at a time against VM screenshots.
- **3D runtime parity** (1.7/6): engine behaviour, not just data. Mitigation: dynamic observation in the VM.
- ~~**Shockwave** (1.8)~~: retired. Only two data-driven charts; reimplemented, no player needed.
