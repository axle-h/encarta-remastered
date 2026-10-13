# Atlas output (`encarta-convert atlas`)

`encarta-convert atlas` reads the WLD atlas (`WLD_HD.MAD` + `WLD_CD.MAD`) and the atlas side files
(`P2MTBL.DAT`, `MSWORLD.SDB`, `D1–D4PLACES.TXT`, the TrueType fonts in `AREF/FONTS`) and writes `out/atlas/`
and `out/report/atlas.json`. Code: `crates/encarta-convert/src/atlas.rs` and `src/atlas/`. Format
knowledge: [mad.md](formats/mad.md), [mad-styling.md](formats/mad-styling.md), [p2mtbl.md](formats/p2mtbl.md),
[msworld.md](formats/msworld.md).

A full run takes about 4.5 minutes on 24 cores (the HYPSO raster pyramid is 4 of them) and writes 1.8 GB.
Peak memory is about 3 GB. Every file is rewritten on each run (temporary name, then rename), so re-running is
safe. `ENCARTA_ATLAS_ONLY=raster,vector,gazetteer,style,glyphs,tables,preview` runs only some stages, and
`ENCARTA_ATLAS_RASTERS=MOON,TECTONIC` only some rasters. The report keeps the entries of stages that did not run.

## Layout

```
out/atlas/
  raster/<raster>/<look>.pmtiles        Web Mercator XYZ, 256-px WebP tiles, one archive per LOOK a MapStyle uses
  raster/<raster>/index.pmtiles         the same pyramid as 8-bit grey PNG raster values (CMT indices)
  raster/<raster>/<look>.equirect.webp  whole-globe equirectangular texture (includes the poles)
  raster/<raster>/index.equirect.png
  raster/palettes.json                  every LOOK of every raster: colour, shade, pixel class per value; RCT legend
  vector/earth.pmtiles                  MVT, GeoGem bands 0–7, z0–8
  vector/earth-detail.pmtiles           MVT, GeoGem bands 8–9, z9–12
  vector/moon.pmtiles                   MVT, the lunar classes, z0–2
  gazetteer.sqlite                      places, names (FTS5), links, SDB countries, quiz pools
  styles/<map style>.json               MapLibre style per MapStyle record; styles/index.json
  sprites/sprite{,@2x}.{png,json}       the 149 Bitmap symbols as "bm<id>"
  glyphs/<face>/<start>-<end>.pbf       SDF glyphs from the disc fonts
  miller/<raster>/index-<level>.png, <look>-<level>.webp, miller/index.json    the flat (Miller) maps
  mad-tables.json                       the small MAD tables decoded (pens, labels, rules, ramps, collation …)
out/report/atlas.json                   per-stage reports, completeness of every MAD section, checks
out/report/atlas-preview/*.png          mosaics read back from the archives (validation)
```

Style, sprite and glyph URLs in the styles are root-relative (`pmtiles:///atlas/vector/earth.pmtiles`,
`/atlas/glyphs/{fontstack}/{range}.pbf`, `/atlas/sprites/sprite`), for a server that serves `out/` at `/`.
MapLibre needs absolute `sprite` and `glyphs` URLs, so the server (or the client's `transformStyle`) must
prefix the origin. The server must also clamp Range requests to the file size: PMTiles readers ask for the
first 16 KiB even of smaller archives.

## Projection

All tiles are **Web Mercator XYZ** (EPSG:3857). MapLibre GL JS 5 renders the same tiles on its globe
(`"projection": {"type": "globe"}` is set in every style), so no second tile set is needed. Web Mercator stops at
±85.05°, and MapLibre's globe fills the polar caps from the edge tiles. For a true polar view (or a three.js
globe) each raster also gets an equirectangular texture of the whole sphere (`*.equirect.*`, up to 8192 × 4096,
the WebP size limit). The Moon is a separate body. Its raster and its vector classes go to their own archives,
and the "Atlas Moon" style uses only those.

## Rasters

Each output pixel is mapped to latitude/longitude, then into the octahedral image (mad.md §6), and takes the
**nearest** raster value. Colour tiles apply the LOOK's CMT to that value. Every output row has one latitude, so
each row picks the coarsest pyramid level whose pixels are not coarser than the output pixels. Encarta's own
lower levels are therefore its generalisation for small zooms, not a resampling of ours. Source tiles are
decoded on demand into a small per-thread cache. No level mosaic is held in memory (HYPSO level 8 would be
65024² pixels).

| Raster | Levels | Tile zooms | LOOKs written | Encoding |
|---|---|---|---|---|
| HYPSO | 0–8 | 0–9 | HYPSO, HYPSOFLAT, POLITICAL, STATISTICAL, GENERAL, GENERALFLAT | lossless WebP |
| SATELLITE | 0–5 | 0–6 | SATELLITE | lossy WebP q85 (lossless when smaller) |
| NIGHT | 0–4 | 0–5 | NIGHT | lossy |
| MOON | 0–3 | 0–4 | MOON | lossy |
| TECTONIC, POPULATION | 0–3 | 0–4 | own | lossless |
| RAINJAN, RAINJULY, RAINAVG, TEMPJAN, TEMPJULY | 0–2 | 0–3 | own | lossless |
| LOCATOR | 0 | 0–1 | LANDWATER | lossless |

The maximum zoom is the one where a 256-px Mercator pixel at the equator matches the finest octahedral pixel
(level `L` → zoom `L + 1`). MapLibre overzooms beyond it. HYPSO's LOOK `LBU_BORDERS` is used by no MapStyle, so
it is not baked. Its palette is in `palettes.json`, and the index tiles can be recoloured with it. Identical
tiles are stored once (only 127,310 of HYPSO's 349,525 index tiles are distinct).

**Recolouring.** The `index` archives hold the raw raster values. Styles reuse one HYPSO index grid. A client
can draw any LOOK, or a statistics overlay, by mapping values through `palettes.json` (for example in a
`maplibregl.addProtocol` handler). Each style also declares a `raster-index` source for this.

## Vector tiles

### Bands and zooms

GeoFaceScale bands are camera-altitude ranges (mad-styling.md §1). Altitude becomes zoom through one function,
`atlas::geo::zoom_for_altitude`, with `zoom = log2(40075.017 · (P/m) / (512 · 2.5 · altitude_km))`
(mad-styling.md §2.2). `P/m = 2400` is the spec's working value until the VM measures it
(`geo::VIEW_P_OVER_M`). Tile zoom `z` takes the band whose range contains the altitude at `z`, the highest
altitude shown with that tile. So a tile is never denser than Encarta anywhere in its zoom range.

| Band | Altitude (km) | Zoom range | Cells/octant | Tile zooms |
|---|---|---|---|---|
| 0 | 50,000–40,000 | 0.59–0.91 | 1 | 0 |
| 1 | 39,999–20,000 | 0.91–1.91 | 1 | 1 |
| 2 | 19,999–14,000 | 1.91–2.42 | 1 | 2 |
| 3 | 13,999–8,000 | 2.42–3.23 | 2 | 3 |
| 4 | 7,999–4,000 | 3.23–4.23 | 4 | 4 |
| 5 | 3,999–2,000 | 4.23–5.23 | 8 | 5 |
| 6 | 1,999–800 | 5.23–6.55 | 16 | 6 |
| 7 | 799–150 | 6.55–8.97 | 64 | 7, 8 (`earth` maxzoom) |
| 8 | 149–30 | 8.97–11.29 | 256 | 9, 10, 11 (`earth-detail`) |
| 9 | 29–1 | 11.29–16.2 | 1024 | 12 (`earth-detail` maxzoom) |

Bands 8–9 hold only what Encarta adds below 150 km (city insets, city limits, urban areas, street-level roads
for the VE styles and the MinRect boxes), not a full map. So `earth-detail` is an extra source drawn over the
overzoomed `earth` source. Extent 4096 at the maximum zooms keeps every GeoGem coordinate exact (band 7 at z8:
0.0014° cells / 1023 vs 0.00034° per tile unit; band 9 at z12 likewise).

### Layers and properties

| Layer | Geometry | From |
|---|---|---|
| `areas` | (multi)polygons | area classes |
| `lines` | (multi)linestrings | line classes |
| `points` | (multi)points | point and radius-point classes (a radius-point part is a set of points) |
| `labels` | points | one anchor per named area feature and band: the GeoObj "go to" point when it lies in the feature, else an interior point of its largest ring |

Every feature has the MVT id = entity id (so `feature-state` hover works per entity) and the properties
`class` (PDCAttribute row), `class_name`, `entity`, `name` (display name: the first U+0004 alternative without
its U+0002 abbreviation), `abbr`, `alt_names` ("; "-joined), `name_0009` (the LCID 0x0009 name where it
differs), `anonymous` (`{RESERVED}` placeholder names, which are dropped), and on points/labels `priority`
(PDCAttribute b13, the `symbol-sort-key`) and on labels `goto_km`. Within a tile, the parts of one object and
class are merged into one feature.

**Polygons.** GeoGem area rings carry no meaningful orientation (both windings occur). Inside one object, class
and cell, a ring inside an odd number of other rings is a hole (Encarta fills with GDI's even-odd rule). The
converter nests the rings that way and writes exteriors with positive area (MVT spec), holes negative. Rings are
clipped per tile (Sutherland–Hodgman, 64-unit buffer), lines likewise (Liang–Barsky). Areas are also clipped at
GeoGem cell edges in the source data, so fills abut cell by cell. The styles set `fill-antialias: false` to avoid
hairline seams. Outlines of areas show those cell edges: borders and coasts are separate line classes, and (in
bands 3–8 over Europe) the land/sea edge is the boundary between `Ocean`/bathymetric and hypsometric areas.
GeoGem cells never straddle ±180°. Features near the antimeridian are also written, shifted by ±360°, into the
buffers of the edge tiles. Latitudes beyond ±85.05° are clipped by the tile bounds.

**What goes where** (from the PDCAltitudeGrid rules of all 33 styles, `objects::ClassUse`):
- classes no rule draws, labels or hit-tests (`{no draw no label no hd}`, `River - centerline - anon`):
  not in tiles; their extents feed the gazetteer;
- `earth`/`moon`: geometry of every class that is drawn (by default or optionally) or hit-tested (hover
  tooltips), plus label points for labelled area classes;
- `earth-detail`: geometry only of classes drawn by default. Optional-only area classes there (Island,
  Physiographic region …) are whole-cell label squares, so they become label points. Keeping them would have
  meant 0.9 M mostly empty z12 tiles.

## Gazetteer (`gazetteer.sqlite`)

| Table | Content |
|---|---|
| `place` | one row per GeoObj entity (1,813,920): `entity`, `name`, `abbr`, `name_0009`, `anonymous`, `body` (earth/moon), `feature_type_id`, `context_id` (Find Grid), `class` (main PDCAttribute row: lowest b13) and `classes` (JSON), `lat`/`lon` with `position_source` (goto → entity_center → point → geometry_bbox), `west/south/east/north` (narrower of the −180…180 and 0…360 boxes; `west > east` crosses the antimeridian), `goto_km`, `min_band`/`max_band`, `geoobj_key`/`geoobj_entry`, `sdb_country`, `link_count`, `importance` |
| `place_full` (view) | `place` plus `feature_type`, `context`, `class_name` text |
| `place_name` + `place_fts` | every name form: `display`, `abbr`, `alternative`, `lcid_0009`, `search_key` (Find Grid keys, G1G2-decoded, rotations and bracketed canonical names split out). FTS5 external content, `unicode61 remove_diacritics 2`, prefix indexes 2 and 3 |
| `place_link` | P2MTBL: `entity`, `kind`, `kind_label` (p2mtbl.md), `content_id` (joins `out/content.sqlite`) |
| `sdb_country` | MSWORLD.SDB country ids (= entity ids) |
| `quiz_place` | Geography Quiz pools D1–D4 (`level`, `kind`, `region`, `entity`) |
| `feature_type`, `context`, `class`, `meta` | lookup tables; `meta.search_sql` |

Ranking: exact name match first, then `importance`, then FTS5 rank. `importance` = 4 (SDB country) + 2 (has
an article link) + up to 1 (link count) + log10(1 + go-to altitude km) + (255 − class priority)/127.5 + 0.5
(has a Find type), and −10 for anonymous features. The query (also in `meta.search_sql`):

```sql
WITH hits AS (SELECT entity, max(lower(name) = lower(:text)) AS exact, min(rank) AS r
              FROM place_fts WHERE place_fts MATCH :fts GROUP BY entity)
SELECT p.entity, p.name, p.feature_type, p.context, p.lat, p.lon
FROM hits h JOIN place_full p ON p.entity = h.entity
ORDER BY h.exact DESC, p.importance DESC, h.r LIMIT 20;
```
"london" → London (UK) then London (Ontario); "paris" → Paris (France); "donau" → Danube.

## Styles

One style per MapStyle record (33; `styles/index.json`). Each has a background (MapStyle colour 2), the
raster of its LOOK, then the rule layers in Encarta's draw order (mad-styling.md §9): fills by b12, line pen
passes w7…w13 layer by layer by b12, hover highlights, point symbols and labels (sorted by b13). Rule → layer
follows mad-styling.md §14.1:

| Implemented | |
|---|---|
| rule altitude range → layer `minzoom`/`maxzoom`, rule selection by stored order | §4.2 |
| tri-state 1 → visible, 2 → `visibility: none` (an optional label on a shown symbol starts with `text-opacity: 0`), 0 → no layer | §4.3 |
| area fill: pen colour, pattern bitmap (`fill-pattern: bm<id>`) | §5 |
| line pens: width, dashes (`line-dasharray` from the bit mask), offset, edge-only pens (`passes < width` → two 1-px lines) | §5.1 |
| point symbols: `icon-image` = bitmap of pen w7, symbol and label together (`icon-optional: false`); radius points as circles | §7.4, §8 |
| labels: GeoLabel font/size/colour, bold/italic → face, rule halo colour, 1-px halo; line labels along lines, area labels on `labels` | §7 |
| highlight pens (b15/b16/w17) as `feature-state: hover` layers | §4.1 |
| MinRect boxes and the style's altitude limits as `metadata` (`encarta:min_rects`, `encarta:minzoom/maxzoom`) | §3, §13.2 |

Placeholders (layer `metadata["encarta:placeholder"]`): GeoTrans tint ramps over the hill shading are drawn as a
50 % fill of the ramp's middle colour (§6: exact needs baking into the raster); GeoLabel upper case (byte 11) and
letter spacing (byte 10) are hypotheses; area outline pens show cell seams. Not yet modelled: the pixel-class
"draw over" masks (§5.2: e.g. boundaries not drawn over the sea), the alternate label style w6, label
abbreviation fallback (U+0002) and the GeoLabel bytes still open in §7.2. Layers with identical paint and zoom
range are merged with a class filter, which gives 1024 layers for Atlas Default.

**Sprites**: one sprite per Bitmap record, `bm<id>` (id = key − 0x10000), palette from BitmapPalette with its
transparent entry, at 1x and 2x (pixel-doubled).

**Glyphs**: SDF PBFs (24 px/em, 3-px buffer, radius 8, cutoff 0.25) for every face the styles use, from the
disc's own TTFs (`AREF/FONTS`). The face name is `<Label Font name> <Regular|Bold|Italic|Bold Italic>`. The
styles use 16 faces of Arial, Arial Narrow, Times New Roman, MS Reference Sans Serif and MS Reference Serif. All
are on the disc except Arial Bold Italic, which uses Arial Bold. The other Label Font faces (Tahoma, Georgia Ref,
Verdana Ref, Trebuchet MS) are referenced by no GeoLabel that a rule uses. A missing style falls back within the
family; a missing family falls back to MS Reference Sans Serif/Serif. Characters missing from a face come from
the other disc fonts (`glyphs.missing_chars` in the report: none). The ranges cover every character of the
GeoObj names (5 ranges; no kana/CJK, so the Japanese faces MS Mincho/MS PGothic are not needed). These are
Microsoft fonts: the output stays in `out/`.

## Other outputs

- `miller/`: the 11 flat-map rasters (MillerTerrainGrid) assembled per level (512 × 384 … 3328 × 2295), as
  index PNG and LOOK WebP, with the fitted Miller projection in `miller/index.json` (mad-styling.md §12).
- `mad-tables.json`: GeoFaceScale with zoom ranges, PDCAttribute, the class-use summary, GeoPenDef, GeoLabel,
  Label Font, ShieldId, BitmapPalette/Color/DimDir, GeoTrans, MinRect, Unicode Attributes, Font Conv, LegendGrid,
  MapStyle (raw), PDCAltitudeGrid (all rules), LangId.

## Completeness (`out/report/atlas.json` → `completeness`)

Every section of WLD_HD and WLD_CD is listed with status and destination; currently 0 unexplained. Omitted:
`FindGECContextGrid/Lookup` (duplicates the Find Grid type/context pairs), `Find Key` (MOBB's page index,
replaced by FTS5), the TESS globe mesh inside `Terrain` (regenerable). **SWLD** (`SWLD_HD/CD.MAD`) is omitted:
it is the reduced atlas ENCARTA.EXE falls back to without WLD_CD, with HYPSO levels 0–7 (a simplified rendering
without bathymetry), GeoGem bands 0–7 and 31,662 entities, all of which are in WLD.

## Validation

- `out/report/atlas-preview/*.png`: mosaics decoded back from the archives with the vector lines drawn on top
  (Europe, Alps z9, Denmark areas, the antimeridian, satellite, night, tectonic, Moon, Paris detail).
- `raster_check` in the report: max-zoom index tiles against direct lookups in the octahedral pyramid at 20,000
  random pixels per raster (HYPSO 100 %, the others 99.8–99.99 %; the rest are pixel-boundary cases of the
  8-px projection interpolation).
- `tools/atlas-maplibre/`: `server.js` (Range-capable static server for `out/`), `index.html` (MapLibre GL JS
  5.6 + pmtiles 4.3 viewer, absolutises sprite/glyph URLs) and `shoot.mjs` (headless Chromium via the DevTools
  protocol, waits for the map's `idle` event, saves a PNG; renders kept in `out/report/atlas-preview/maplibre/`).
  The converted styles render as atlas pages (Atlas
  Default and Political over Europe, Satellite Day, Time Zone, the Moon, VE Road over Paris at z11).
- Unit tests: PMTiles writer/reader round trip (with leaf directories, runs, dedup), Hilbert ids, MVT encode/decode
  of synthetic geometry (holes, orientation, clipping), clipping, even-odd nesting, projection interpolation,
  zoom/level choice, altitude→zoom, dash arrays, SDF, name conventions. Disc tests (skipped without
  `extracted/`): HYPSO ocean value at 0° N 140° W through a Mercator tile and a PMTiles round trip; band-7
  Denmark tile with Copenhagen and exterior-first polygons.
