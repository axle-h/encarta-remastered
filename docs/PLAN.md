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
- [ ] **Windows XP reference VM** (QEMU/KVM, i386, XP Pro SP3 from the MSDN ISO on the NAS).
  - Install Encarta Premium 2009 from `vendor/`. Snapshot a clean install.
  - VM disks live in `vm/` (gitignored). The licence key is entered at install and never recorded here.
  - Tools inside the VM: a debugger (for confirming decoder behaviour dynamically), and a way to move
    files in and out (shared folder or ISO).
  - Uses: ground-truth screenshots, behaviour reference for tours, atlas and games, and capture of
    **private fixtures** (rendered articles, map views) into `fixtures/` (gitignored).
  - Exit: Encarta launches in the VM, and a repeatable procedure exists to capture an article by id.

## Step 1 — Decoders (`crates/encarta-formats`)
For each format: write the spec in `docs/formats/`, implement it, run it over the whole disc,
compare with VM fixtures, and add a dump command to the CLI.

| # | Format | Delivers | Effort |
|---|---|---|---|
| 1.1 | ITOLITLS + LZX | Media, sidebars, dictionaries, interactives, templates | done |
| 1.2 | Catalog `ECN`/`ECS` + `guid.lst` | Resolve any content id to package + record | S |
| 1.3 | **AKC** (`CONT*/DATA*/DATAF*/REL*`, incl. Kids) | **Article bodies**, article metadata, related articles. Critical path. | L |
| 1.4 | Search exports (`PFX`, `IdXE`, `IdXEW`, ECT) | Titles, aliases, section titles (search itself is rebuilt) | S |
| 1.5 | Small formats: `TIMELINE.DAT`, `MINDMAZE.*`, `P2MTBL.DAT`, `MSWORLD.SDB`, `D*.TXT`, `$WW$CTCATALOG`, `*.LEX` word lists | Timeline, MindMaze, article↔map links, statistics, Geo Quiz pools, photo credits, spelling data | M |
| 1.6 | **MAD atlas**: TOC (done) → MOBB codec → `INDX` → `RMAP` rasters → every grid table (geometry, features, gazetteer, styles, symbols, labels) | Full atlas data | XL |
| 1.7 | **3D tours**: `.x`, `.3sc` scripts, `.3wm` walk maps, `.3lt` lights, `.3cl` point clouds, `.32d` HUD, `.3vp`; plus ACTX3DX3 runtime behaviour (camera, movement, collision, gravity, hotspots) | Full tour data and behaviour spec | L |
| 1.8 | Shockwave/Director (`.dcr`, `.cct`, `.swa`) | Identify what they drive; decide on decode vs. player port | M |

Exit: every file in every container and every catalog id has a decoder. No "unknown" bytes are left
that carry content.

## Step 2 — Converter (`crates/encarta-convert`)
One command: `encarta-convert --iso <path> --out out/`. It is idempotent and resumable.

- **Database** (SQLite): articles in our own normalised schema (structured JSON plus pre-rendered
  HTML fragments), outlines, related articles, media metadata (captions, credits, sizes),
  dictionary/thesaurus/bilingual dictionaries, timeline, quizzes, statistics, gazetteer.
- **Media** (content-addressed under `out/media/`):
  - images keep their original bytes, plus WebP/AVIF derivatives;
  - WAV/WMA → Opus (+ AAC fallback); WMV → MP4/H.264 + WebM; SAMI captions → WebVTT;
  - MIDI is kept and played with a soundfont synth in the browser;
  - SWF is kept for Ruffle.
- **Atlas**: raster and vector tiles in PMTiles, a MapLibre style, glyphs from the disc fonts, a symbol sprite sheet.
- **3D tours**: glTF scenes plus a JSON scene description (scripts, hotspots, lights, walk maps, HUD).
- **Search**: a tantivy index carrying Encarta's aliases and section titles.
- **Completeness report**: every container entry and catalog id is listed as converted, or as
  intentionally omitted with a reason. The target is zero unexplained omissions.

## Step 3 — Server (`crates/encarta-server`)
axum, read-only, one binary pointed at `out/`. Content API (articles, media, dictionary, timeline,
atlas metadata), search and autocomplete, media with HTTP Range, PMTiles tile serving.

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

## Step 7 — Games and remaining extras
MindMaze, quizzes (`SWQUIZ`), Geography Quiz, any Director-driven pieces from 1.8.

## Step 8 — Parity audit
A feature-by-feature checklist against the VM; fix gaps; performance and accessibility pass.

---

## Risks
- **AKC codec** (1.3): it's the only source of article text, so it gates Steps 2–4. Mitigation: do it first, use VM fixtures.
- **MAD vector tables** (1.6): about 30 undocumented table types. Mitigation: start with rasters to prove the codec; decode tables one at a time against VM screenshots.
- **3D runtime parity** (1.7/6): engine behaviour, not just data. Mitigation: dynamic observation in the VM.
- **Shockwave** (1.8): no mature web player. Mitigation: identify scope early; port or reimplement only what's used.
