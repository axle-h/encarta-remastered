# Atlas output (`encarta-convert atlas`)

`encarta-convert atlas` reads the WLD atlas (`WLD_HD.MAD` + `WLD_CD.MAD`) and the atlas side files
(`P2MTBL.DAT`, `MSWORLD.SDB`, `D1–D4PLACES.TXT`, the TrueType fonts in `AREF/FONTS`) and writes `out/atlas/`
and `out/report/atlas.json`. Code: `crates/encarta-convert/src/atlas.rs` and `src/atlas/`. Format
knowledge: [mad.md](formats/mad.md), [mad-styling.md](formats/mad-styling.md), [p2mtbl.md](formats/p2mtbl.md),
[msworld.md](formats/msworld.md).

A full run takes about 5 minutes on 24 cores (the HYPSO raster pyramid is 4 of them, the vector tiles 21 s)
and writes 1.9 GB. Peak memory is about 4.5 GB (the vector stage). Every file is rewritten on each run
(temporary name, then rename), so re-running is safe. `ENCARTA_ATLAS_ONLY=raster,vector,gazetteer,style,glyphs,tables,preview` runs only some stages, and
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
  vector/earth-detail.pmtiles           MVT, GeoGem bands 8–9, z10–12
  vector/moon.pmtiles                   MVT, the lunar classes, z0–3
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
(mad-styling.md §2.2). `P/m = 3135` was measured in the reference VM for Encarta's maximised 812 × 518 map
pane (`geo::VIEW_P_OVER_M`; docs/vm-findings.md V1/V2; it grows with the pane height, `P/m ≈ 6.05 × height`).
Everything is compiled for that pane at latitude 0; the client moves the zoom ranges for its own pane and the
latitude ([atlas-client.md](atlas-client.md)). Tile zoom `z` takes the band whose range contains the altitude
at `z`, the highest altitude shown with that tile. So a tile is never denser than Encarta anywhere in its zoom
range.

| Band | Altitude (km) | Zoom range | Cells/octant | Tile zooms |
|---|---|---|---|---|
| 0 | 50,000–40,000 | 0.97–1.30 | 1 | 0, 1 |
| 1 | 39,999–20,000 | 1.30–2.30 | 1 | 2 |
| 2 | 19,999–14,000 | 2.30–2.81 | 1 | none (z3 is already in band 3) |
| 3 | 13,999–8,000 | 2.81–3.62 | 2 | 3 |
| 4 | 7,999–4,000 | 3.62–4.62 | 4 | 4 |
| 5 | 3,999–2,000 | 4.62–5.62 | 8 | 5 |
| 6 | 1,999–800 | 5.62–6.94 | 16 | 6 |
| 7 | 799–150 | 6.94–9.35 | 64 | 7, 8 (`earth` maxzoom; z9 is overzoomed) |
| 8 | 149–30 | 9.35–11.68 | 256 | 10, 11 (`earth-detail`) |
| 9 | 29–1 | 11.68–16.58 | 1024 | 12 (`earth-detail` maxzoom) |

Bands 8–9 hold only what Encarta adds below 150 km (city insets, city limits, urban areas, street-level roads
for the VE styles and the MinRect boxes), not a full map. So `earth-detail` is an extra source drawn over the
overzoomed `earth` source (the styles switch to it at z10, `vector::detail_from_zoom`). Extent 4096 at the
maximum zooms keeps every GeoGem coordinate exact (band 7 at z8: 0.0014° cells / 1023 vs 0.00034° per tile
unit, which is why `earth` stops at z8 although band 7 reaches z9.35; band 9 at z12 likewise). The Moon's data
stops at band 2, so `moon` runs z0–3 with z3 holding band 2. Each style's vector sources state these
`minzoom`/`maxzoom` (the PMTiles header gives MapLibre the same; the client's flat view needs them).

### Layers and properties

| Layer | Geometry | From |
|---|---|---|
| `areas` | (multi)polygons | area classes |
| `lines` | (multi)linestrings | line classes |
| `points` | (multi)points | point and radius-point classes (a radius-point part is a set of points) |
| `labels` | points | one anchor per named area feature and band: the GeoObj "go to" point when it lies in the feature, else an interior point of its largest ring |
| `outlines` | (multi)linestrings | the rings of area classes that a rule strokes (outline pens w8…, stroke highlights b16 = 1/2), without the GeoGem cell edges |

Every feature has the MVT id = entity id (so `feature-state` hover works per entity) and the properties
`class` (PDCAttribute row), `class_name`, `entity`, `name` (display name: the first U+0004 alternative without
its U+0002 abbreviation), `abbr`, `alt_names` ("; "-joined), `name_0009` (the LCID 0x0009 name where it
differs), `anonymous` (`{RESERVED}` placeholder names, which are dropped), and on points/labels `priority`
(PDCAttribute b13, the `symbol-sort-key`; line-name and shield layers, whose features carry none, use the class's b13 as a constant key) and on labels `goto_km`; `width_z10` on the labels of areas with an
abbreviation (the area's east–west extent in px at zoom 10, for the abbreviation fallback); `water: true` on
the line runs and polygon parts over water (draw-over masks, below); `geoobj` on named features, the object's
GeoObj `(record key, entry)` as `key · 65536 + entry` (the click menu's last tie-break, descending:
mad-styling.md §4.5). Within a tile, the parts of one object,
class and side (water/land) are merged into one feature.

**Polygons.** GeoGem area rings carry no meaningful orientation (both windings occur). Inside one object, class
and cell, a ring inside an odd number of other rings is a hole (Encarta fills with GDI's even-odd rule). The
converter nests the rings that way and writes exteriors with positive area (MVT spec), holes negative. Rings are
clipped per tile (Sutherland–Hodgman, 64-unit buffer), lines likewise (Liang–Barsky). Areas are also clipped at
GeoGem cell edges in the source data, so fills abut cell by cell. The styles set `fill-antialias: false` to avoid
hairline seams. A ring edge whose two ends lie on the same cell border (x or y = 0 or 1023) is such a cut; the
`outlines` layer is each stroked area's rings with those edges dropped (`vector::ring_outline`; 86,396 edges in
band 7), and outline pens and stroke highlights draw it instead of the polygons, so they no longer show the cell
seams. Borders and coasts are separate line classes, and (in bands 3–8 over Europe) the land/sea edge is the
boundary between `Ocean`/bathymetric and hypsometric areas.

**Draw-over masks** (mad-styling.md §5.2). Lines of the classes that some rule draws with a restricted pen
(`over` ≠ all; boundaries, graticules, time zones, reefs, urban inset boundaries) are split where they cross
between water and land of the HYPSO raster (CMT `kind` 0 = water), at the HYPSO level of the band's finest tile
zoom, with crossings found by bisection (`pixel::split_line`). The styles then draw each pen only on the runs
its mask allows for the style's raster (below). Simplification: the class under a line comes from the raster
alone, not from the area fills drawn before it (for HYPSO the same classes: water areas use `b14 = 1`, land
fills `b14 = 0`).

Polygons of the classes that some rule fills with a restricted pen or pattern brush
(`ClassUse::masked_fill`, 262 classes: Admin1/CountryRegion/Dependent colours, International organization,
time zones, LANG, REL, ECO, CLI, parks, reserves, urban areas, ice pack, intermittent lakes …; every such mask
lacks bit 2, i.e. not over water) are split the same way, along pixel edges. Per GeoGem cell, the HYPSO water
mask is sampled on the Web Mercator grid that resolves the lines' HYPSO level at the equator (2^17 pixels
around the world for band 7, the raster's z9 grid; 16-px blocks inside one all-water or all-land source tile
are filled without lookups, the others projected exactly at their corners and bilinearly between). Its water
pixels are merged into rectangles, and each polygon is intersected with them and differenced
(`pixel::WaterWindow`, `i_overlay`, even-odd, integer fixed-point coordinates). Polygons on one side keep their
geometry; mixed ones become a water part (`water: true`) and a land part with the same id and properties.
Beyond ±85.05° the edge row's class continues. The `outlines` layer stays whole (no outline pen has a
restricted mask). The report's `area_masks` gives per class the polygons and the share of their Mercator area
over water, summed over the bands: 0.6–1.0 for the "… - Small" dependency and country colours, 0.15–0.46 for
most Admin1/CountryRegion colours, 0.2 for International organization, 0.07–0.3 for time zones, 0.01–0.05 for
parks and urban areas. In band 7, 88,072 of 390,040 masked polygons are cut and 31,192 lie entirely over water.
Water parts are dropped for classes that no style fills over its water pixel class (`ClassUse::fill_water`)
and that no style makes interactive; the rest are kept for drawing and hit-testing.

**Hit sides** (mad-styling.md §4.4). The click menu lists a water class (rule `b4` high nibble 2: oceans, seas,
water labels, reefs) only where the point is over water, and leaves out land classes (nibble 1: countries,
divisions, continents, islands …) when a water class is listed; other classes are listed anywhere (VM: off
Brittany only "Bay of Biscay"; on Belle-Île, inside the Bay polygon, France and Brittany but not the Bay; off
Jersey, with no water polygon, "Jersey (U.K.)" over the sea; parks, World and tectonic plates over the sea). So
the area classes with nibble 2 in some Earth style (`Classes::water_hits`: Ocean, Inland sea, the water labels,
Lake - no draw, Reef …) are split along the HYPSO water mask as above, and each style's `encarta:interactive`
carries the nibble per zoom range (style.json below). The client lists a nibble-2 polygon only on its water
part and then drops the nibble-1 hits (atlas-client.md). Land classes are not split for hit-testing. Off
Brittany (47.596, −3.368) the Bay of Biscay is a water part; on Belle-Île (47.356, −3.226) it is a land part.
In band 7, 95,665 of 423,161 split polygons are cut and 56,016 lie entirely over water. Sizes: `earth.pmtiles`
404.9 MB (400.3 MB with the earlier "no land area over the sea" split; the water parts of interactive masked
fills are now kept), `earth-detail` 18.5 MB, the vector stage 26 s.

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
passes w7…w13 layer by layer by b12, hover highlights, point symbols and labels (by b13 descending, so the lowest b13 is on top: MapLibre places symbols from the top
layer down, and in Encarta the lowest b13 wins; points, area and line labels interleave). Rule → layer
follows mad-styling.md §14.1:

| Implemented | |
|---|---|
| rule altitude range → layer `minzoom`/`maxzoom`, rule selection by stored order | §4.2 |
| tri-state 1 → visible, 2 → `visibility: none` (an optional label on a shown symbol starts with `text-opacity: 0`), 0 → no layer | §4.3 |
| area fill: pen colour, pattern bitmap (`fill-pattern: bm<id>`) | §5 |
| line pens: width, dashes (`line-dasharray` from the bit mask), offset, edge-only pens (`passes < width` → two 1-px lines) | §5.1 |
| point symbols: `icon-image` = bitmap of pen w7, symbol and label together (`icon-optional: false`); radius points as circles | §7.4, §8 |
| labels: GeoLabel font/size/colour, bold/italic → face, rule halo colour, 1-px halo; line labels along lines, area labels on `labels` | §7 |
| alternative name (`w6` set, not on lines): the label's `text-field` is a `format`: main name in the `w5` style plus `"\n"` and the tile's `alt_names` as a last section with the `w6` face (`text-font`), `font-scale` = w6 size / w5 size and `text-color`; in brackets when the main name starts with U+3040…U+9FFF (kana/CJK); `w6` none → no alternative. Upper case (GeoLabel byte 26) is `upcase` per section. The 1-px rule between the lines (V15) comes from rule faces (Glyphs, below): the main section uses `"<w5 face> Rule …"`, the alternative `"<w6 face> Rule …"`. Such labels do not wrap (`text-max-width` 1000 when `alt_names` is present). Point labels have `text-justify: auto`, so the lines align on the symbol's side | §4.1, §7.2, §7.3 |
| road shields: a line class whose `w5` GeoLabel has a shape (byte 12 ≠ 0) gets a `symbol` layer on `lines` (filter `has name`, order after the labels) instead of a line label: `symbol-placement: line`, `symbol-spacing` 400, the name upright (`text-rotation-alignment`/`icon-rotation-alignment: viewport`), `icon-image` = sprite `shield<shape>-<outline>-<width>-<fill>-<fill2>`, `icon-text-fit: both` with padding, no overlap | §8.1 |
| `metadata["encarta:menu_excluded_classes"]`: the rows of the classes of PDCAttribute pen group 134 (time zones, DST areas, "Shading - gray"), which the click menu never lists (rows like the tiles' `class`; before 2026-10-15 the converter wrote class ids here, which left the time zones in the menu and took the Mexico/US shield roads out) | §16 (V11 result) |
| the click menu's sort data for the classes in `encarta:interactive`: `metadata["encarta:class_scale"] = {row: PDCAttribute scale / 1000}` and `metadata["encarta:routable_classes"] = [rows with PDCAttribute byte 6 set]` | §4.5 |
| point labels: `text-variable-anchor` in MOBB's candidate order above right, below right, above left, below left (`bottom-left`, `top-left`, `bottom-right`, `top-right`) with `text-radial-offset` 0.3; a point label without a drawn symbol tries `center` first, offset 0 | §7.4 |
| highlight pens (b15/b16/w17) as `feature-state: hover` layers (the client uses them for the selected place; Encarta highlights nothing on hover, V11) | §4.1 |
| draw-over masks of line pens and of fill pens (and pattern brushes): filter `["all", <class test>, ["==", ["get","water"], true]]` (or `!=`), pen dropped where it may draw nowhere | §5.2 |
| tint colours (fills, lines, highlights): the ramp entry `tint_index(L)` for the mean land luminance `L` of the style's raster LOOK (`encarta:surface`); highlight fills at 60 % | §6 |
| area labels: abbreviation (U+0002) where `length(name) · size · 0.6 em` exceeds the area's width at the current zoom (`width_z10 · 2^(z−10)`), tested every half zoom in a `step` | §7.3 |
| per class, the zoom ranges where the rules make it interactive (`b4`): `metadata["encarta:interactive"] = {class: [[minzoom, maxzoom, tri-state, hit side]…]}`; hit side = `b4` high nibble (0 anywhere, 1 land, 2 water; 0 in Moon styles) | §4.1, §4.4 |
| MinRect boxes and the style's altitude limits as `metadata` (`encarta:min_rects`, `encarta:minzoom/maxzoom`) | §3, §13.2 |
| `encarta:view_p_over_m` 3135, `encarta:view_p_over_m_per_height_px` 6.05, `encarta:reference_pane_height_px` 518 | §2 |

`encarta:surface` gives the pixel classes that the style's raster puts under water and land and the mean land
luminance, measured at 4,096 points spread over the sphere and classified by HYPSO (`pixel::calibrate`): HYPSO
LOOKs water class 1 / land class 4, the climate rasters class 0 on both, the Moon 4/4. Luminance is Rec. 601
luma (the spec says only "luminance"; an assumption).

Placeholders (layer `metadata["encarta:placeholder"]`): GeoTrans tint ramps over the hill shading are drawn as a
50 % fill of one ramp entry (§6: exact needs baking into the raster, per pixel); highlight fills with a ramp
likewise at 60 % (the "relative" highlight variant's per-colour offsets are not specified); GeoLabel letter spacing
(byte 10) is a hypothesis (upper case is byte 26, specified). The abbreviation test's 0.6 em per character is a
hypothesis (the spec says only "when the full name does not fit"); it gives "NETH." and "LUX." over Europe at
3,000 km, as V3/V11 show. The alternative-name rule sits at the main line's descender and the alternative 1.2 em
below the main line, about 2 px lower than in V15 (rule 3 px under the baseline, 2 px over the alternative's
capitals). Its thickness is 2 glyph units (1.2 px at 14 px, 0.9 px at 11 px; ×1.25 with Large labels), its ends
overhang by half the letter spacing, and a name with an alternative stays on one line (V15 wraps "Caldaro sulla
strada del vino"). Not modelled: the choice among Encarta's three label layouts by overlap cost (§7.3: the
alternative name is always drawn when the rule has `w6` and the feature has one, even when it makes the label
collide), the one-same-text-shield-per-sixth-of-the-pane rule and the shield's two-digit minimum width (§8.1:
`symbol-spacing` stands in for the first; MapLibre has no text-width minimum), the shield candidate cost, and the
GeoLabel bytes still open in §7.2. Layers with identical
paint, filter and zoom range are merged with a class filter (1,153 layers for Atlas Political, 13,390 in all).

**Sprites**: one sprite per Bitmap record, `bm<id>` (id = key − 0x10000), palette from BitmapPalette with its
transparent entry, at 1x and 2x (pixel-doubled). Plus one **shield sprite** per (shape, outline colour and
width, fill, second fill) that a rule's `w5` label uses (GeoLabel bytes 12–22; 5 in the shipped styles), named
`shield<shape>-…` (`shield.rs`): a 24-unit box drawn by us (rectangle, rounded rectangle, ellipse, each with or
without an inset outline, and our own simple Interstate shield, US-highway badge, maple leaf and Mexican shield;
no outline data from the binary), rendered at the ratio, with `stretchX`/`stretchY` (the middle row and column)
and `content` (the text box) for `icon-text-fit: both`.

**Glyphs**: SDF PBFs (24 px/em, 3-px buffer, radius 8, cutoff 0.25) for every face the styles use, from the
disc's own TTFs (`AREF/FONTS`). The face name is `<Label Font name> <Regular|Bold|Italic|Bold Italic>`. The
styles use 16 faces of Arial, Arial Narrow, Times New Roman, MS Reference Sans Serif and MS Reference Serif. All
are on the disc except Arial Bold Italic, which uses Arial Bold. The other Label Font faces (Tahoma, Georgia Ref,
Verdana Ref, Trebuchet MS) are referenced by no GeoLabel that a rule uses. A missing style falls back within the
family; a missing family falls back to MS Reference Sans Serif/Serif. Characters missing from a face come from
the other disc fonts (`glyphs.missing_chars` in the report: none). The ranges cover every character of the
GeoObj names (5 ranges; no kana/CJK, so the Japanese faces MS Mincho/MS PGothic are not needed). These are
Microsoft fonts: the output stays in `out/`.

**Rule faces** (the rule between a name and its alternative, mad-styling.md §7.3). MapLibre has no text
decoration, so the rule is built into the glyphs. A face `"<face> Rule <top> <height> <overhang>"` is `<face>`
with a bar in every glyph, in rows `top…top + height` of the glyph frame (24 units per em, 0 at the line top; the
generator scales each font so that ascender + descender = 24), from `overhang` before the pen position to
`overhang` past the advance. Main-name glyphs carry the bar at 24…26, the descender line. Alternative glyphs
carry it at (24 − 28.8)/s, with height 2/s and overhang/s for its `font-scale` s, because MapLibre starts the
second line 1.2 em (28.8 units) of the main scale lower. The two bars meet, and as both lines share the
justification, their union spans the wider line. The overhang is half the letter spacing, so the bar has no
gaps. Rule faces hold only the characters of names with an alternative (both cases, brackets, "; "). There are
108 of them (one per face, line role, scale and letter spacing of the rules with `w6`), 8 MB of the 12.9 MB of
glyphs. MapLibre fetches one only when a label with an alternative in that style is on screen (about 70 KB).

## Other outputs

- `miller/`: the 11 flat-map rasters (MillerTerrainGrid) assembled per level (512 × 384 … 3328 × 2295), as
  index PNG and LOOK WebP, with the fitted Miller projection in `miller/index.json` (mad-styling.md §12).
- `mad-tables.json`: GeoFaceScale with zoom ranges, PDCAttribute, the class-use summary, GeoPenDef, GeoLabel,
  Label Font (`{font, charset, name}`), ShieldId, BitmapPalette/Color/DimDir, GeoTrans, MinRect, Unicode
  Attributes, Font Conv, LegendGrid, MapStyle (raw), PDCAltitudeGrid (all rules), LangId. All of these are parsed
  by `encarta_formats::mad::tables` (the converter has no table parsers of its own).
- MapStyle names are char[32] (mad-styling.md §3): style 28 is "Atlas Physical Features-No HS"
  (`styles/atlas-physical-features-no-hs.json`); the 26-byte read before cut it to "…-No".

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
  protocol, waits for the map's `idle` event, saves a PNG; renders kept in `out/report/atlas-preview/maplibre/`,
  the previous round's in `maplibre-before/`). The converted styles render as atlas pages: Atlas Default,
  Political and Satellite Day over Europe (10° E 50° N z4.5), Time Zone (90° W 40° N z2.7), the Moon, the globe,
  Atlas Default over Paris (z8.6) and Trondheim (z7, draw-over masks over the fjord), VE Road over Paris at z11.
  Against the previous renders: at the same zoom the rules are those of a 1.31× higher altitude (P/m 3135, not
  2400), so slightly sparser; country names fall back to their abbreviations where they do not fit ("Neth.",
  "Lux.", "Bosnia & Herz."); boundaries crossing water draw their light water pen instead of the orange land
  ribbon; graticules are blue over water and grey over land.
- Unit tests: PMTiles writer/reader round trip (with leaf directories, runs, dedup), Hilbert ids, MVT encode/decode
  of synthetic geometry (holes, orientation, clipping), clipping, even-odd nesting, seam-free outlines, zoom bands
  (earth stops at z8, Moon to band 2), projection interpolation, zoom/level choice, altitude→zoom, dash arrays,
  pixel classes and pen masks, water/land line splitting, polygon splitting on synthetic masks (coast, lake
  hole, staircase, beyond the world edge), SDF, name conventions. Disc tests (skipped without `extracted/`):
  HYPSO ocean value at 0° N 140° W through a Mercator tile and a PMTiles round trip; band-7
  Denmark tile with Copenhagen, exterior-first polygons, dropped seam edges, water runs, a masked fill as water
  and land features of one id, and uniform-block mask sampling against point lookups.
