# MAD atlas styling, labels and the remaining MAD tables (round 2)

Status: **round 2 of the MAD specification.** It covers the open points of [mad.md §9](mad.md#9-open-points-and-next-steps):
styling (MapStyle, PDCAltitudeGrid, GeoPenDef, GeoTrans, GeoLabel, Label Font, Font Conv, bitmaps, ShieldId,
LegendGrid, MinRect), the GeoObj fields `c/e/f/g`, the Find-key letter count, `Unicode Attributes`, the
flat (Miller) map tiles, and some smaller fields. It reads [mad.md](mad.md) as the base spec. Where it corrects
mad.md, the change is listed in [§15 Amendments to mad.md](#15-amendments-to-madmd).

How sure each statement is, in this document:

- **validated**: checked against the whole disc, or against every record of the table, by the private prototype;
- **code**: read from how MOBB200.DLL uses the field (addresses in §17), not yet seen in the original app;
- **hypothesis**: fits the data, but neither of the above. Each one has a VM check in §16.

A private prototype renderer (`analysis/decompiled/mad-proto/r2/render_style.py`) draws a map from these rules
with Encarta's own data. Its output for Europe in three styles looks like an Encarta atlas page (§14). The renders
are not yet compared with the original app, so the VM screenshots in §16 are the next step.

Contents: [1 Units: altitude](#1-units-everything-is-altitude) · [2 Viewpoint and zoom](#2-viewpoint-altitude-and-the-maplibre-zoom) ·
[3 MapStyle](#3-mapstyle) · [4 PDCAltitudeGrid rules](#4-pdcaltitudegrid-display-rules) · [5 GeoPenDef](#5-geopendef-pens-and-brushes) ·
[6 GeoTrans](#6-geotrans-tint-ramps) · [7 Labels](#7-labels-geolabel-label-font-name-conventions) ·
[8 Symbols](#8-symbols-bitmaps-shields-legend-icons) · [9 Draw order](#9-draw-order-and-the-drawing-passes) ·
[10 GeoObj c/e/f/g](#10-geoobj-fields-c-e-f-g) · [11 Search tables](#11-font-conv-find-keys-unicode-attributes) ·
[12 Miller flat map](#12-miller-flat-map-millerstyles-millerterraingrid) · [13 Small items](#13-smaller-items) ·
[14 MapLibre recipe and renders](#14-maplibre-recipe-and-prototype-renders) · [15 Amendments](#15-amendments-to-madmd) ·
[16 VM checklist](#16-vm-screenshot-checklist) · [17 Open points and provenance](#17-open-points-and-provenance)

---

## 1. Units: everything is altitude

The "scales" in the MAD tables are **camera altitudes above the ground**, not map-scale denominators. The file
stores them in metres. MOBB divides them by 1000 and works in **kilometres** (code). This applies to:

| Field | File value | Meaning |
|---|---|---|
| `GeoFaceScale.max/min` (mad.md §8.2) | 50,000,000 … 1,000 | altitude band of each GeoGem grid level: 50,000 km … 1 km |
| `PDCAltitudeGrid.smax/smin` (§4) | e.g. 3,999,999 … 150,000 | altitude range in which a display rule applies (4,000 km … 150 km) |
| `PDCAttribute.scale` (mad.md §8.1) | 200,000 for cities | default "go to" altitude for features of that class: 200 km |
| `MapStyle` min/max altitude (§3) | 150,000 … 50,000,000 | the zoom limits of the style |
| `MinRect.altitude` (§13.2) | 5,000 | a lower zoom limit, 5 km, inside a rectangle |
| `GeoObj.c` (§10) | 200 | "go to" altitude of one feature, already in km |
| RAST entry `+0x34` (mad.md §5.3) | 150.0 (HYPSO), 2000.0 (SATELLITE) | float, km: the lowest altitude at which this raster is shown. For 11 of the 12 rasters it equals the minimum altitude of the raster's own MapStyle (Atlas Default 150, Satellite Day 2000, Night 4000, Tectonic 8000, Moon and the climate maps 14000, Population 8000). LOCATOR has 60,000 against Atlas Locator's 14,000. The HYPSO pair `(150.0, 8)` means "at 150 km and below use level 8". |

The table name `PDCAltitudeGrid` and the viewpoint code (§2) confirm this reading. Every check in this document
uses it.

## 2. Viewpoint, altitude and the MapLibre zoom

### 2.1 The MOBB viewpoint (code)

A view has a camera direction (a unit vector towards the point below the camera) and an **altitude `a` in km**
above a sphere of radius **R = 6378.2 km**. MOBB keeps `floor(a)` as an integer, and that integer is the value
it compares with the rule ranges (§4.2) and with the band table.

- **Globe view** (FUN_6358bb0f): the globe is drawn with a scale of `f = P·R / (2.5·m·a)` pixels per earth
  radius. Near the nadir this is **`P / (2.5·m·a)` pixels per km**. `P` (an integer) and `m` (a float) are two
  fields of the view description that ENCARTA.EXE passes in. They are probably a reference viewport size in
  pixels and a zoom/print factor. The globe counts as "whole globe visible" when `f < 0.65·√(w² + h²)` or
  `a ≥ 14,000 km`.
- **Flat view** (Miller viewpoint, vtable at 0x635e5278, method FUN_63588cdd): the same parameters give
  `P·R / (5·m·a)` pixels per radian of longitude, so `P / (5·m·a)` pixels per km along the equator.
- ENCARTA.EXE gets "pixels per earth radius" back from MOBB and draws its scale bar with
  `metres per pixel = 6378200 / f` (FUN_00594600).

`P` and `m` are not yet known as numbers. They must be measured in the VM (§16, shots V1–V3). As a working
value, the prototype uses **`P/m ≈ 4 × viewport height`**. With it, the label density of each altitude band looks
like an atlas page (§14). This value is a **hypothesis**.

### 2.2 Altitude → Web Mercator zoom

For a MapLibre map with 512-px tiles, take the ground resolution at the view centre (latitude φ):

```
km_per_px(a) = 2.5 · m · a / P                       (globe; nadir)
zoom(a, φ)   = log2( 40075.017 · cos φ / (512 · km_per_px(a)) )
altitude(z)  = 40075.017 · cos φ · P / (512 · 2^z · 2.5 · m)
```

The rule tables then become zoom ranges: a rule with altitude range `[smin, smax]` km becomes
`minzoom = zoom(smax)` and `maxzoom = zoom(smin)`. Use φ = 0 to build one style for the whole world. The
cos φ term makes Encarta's per-altitude rules switch at slightly different Mercator zooms at other latitudes. A
style compiled at φ = 0 is accurate near the equator and switches about `log2(1/cos φ)` zoom levels later at
latitude φ (0.5 zoom at 45°). The converter can use `zoom(a, 45°)` instead if mid-latitudes matter more.

Worked example (with the working value `P/m = 2400`, a 600-px-high view): `a = 150 km` (the Atlas Default
minimum) gives 0.156 km/px and zoom 8.97 at the equator. `a = 4000 km` gives 4.17 km/px and zoom 4.23.
`a = 50,000 km` gives zoom 0.58.

### 2.3 Map URLs (for VM captures)

ENCARTA.EXE opens a map view from a `msencarta://` URL (string table at 0x6463cc…0x646450, builder
FUN_00413e40, parser FUN_00419940):

```
msencarta://view=MapView/cmd=map/lat=<deg>/long=<deg>/alt=<number>/style=<n>[/map_flags=<n>]
msencarta://view=MapView/cmd=map/eid=<entity id>/style=<n>
```

`lat/long/alt` are printed with two decimals. The parser treats `style=19` as style 0, and for `style=20` it divides
`alt` by a constant (0x6e6c3c in .data). The unit of `alt` (km is likely) is the first thing to confirm (V1). The
`style` number is probably the MapStyle id of §3.

## 3. MapStyle

33 records × 115 bytes (validated: WLD_HD and SWLD_HD, identical except Atlas Default's minimum altitude, 150 km
in WLD and 1300 km in SWLD).

| Off | Type | Field |
|---|---|---|
| 0x00 | char[32] | style name |
| 0x20 | char[32] | RAST raster name (mad.md §5.3) |
| 0x40 | char[32] | LOOK style name of that raster (mad.md §5.4) |
| 0x60 | u16 | style id = PDCAltitudeGrid key (0…19, 21…33) |
| 0x62 | u8 r, g, b | colour 1: black in 30 styles, white in "Atlas Default Print", `30 47 6D` in "Atlas Locator". **Hypothesis**: the background outside the globe (space). |
| 0x65 | u8 r, g, b | colour 2: `EB EA E1` (Atlas Default), `7B 73 4A` (Satellite Day), `10 21 42` (Night), `33 33 33` (Moon), `CC CC CC` (thematic). **Hypothesis**: the globe colour where no raster pixel is drawn yet (loading), close to the raster's average colour. |
| 0x68 | u8 | 1 = default style (only "Atlas Default"). MOBB stores the index of the last such record. |
| 0x69 | u32 | minimum altitude (m): 150,000 Atlas Default; 2,000,000 Satellite; 1,000 for the VE styles |
| 0x6D | u32 | maximum altitude (m): 50,000,000; 39,999,000 for VE Road/Overlay; 500,000,000 for Locator |
| 0x71 | u8 | flag: 1 in all styles except "Atlas Locator" (meaning open) |
| 0x72 | u8 | flag: 0 in all styles (meaning open) |

Colours are stored R, G, B (MOBB builds a Windows COLORREF from them, red in the low byte).
`MinRect` (§13.2) can lower the minimum altitude inside some rectangles.

ENCARTA.EXE's style menu uses its own resource names (`mapstyle_comprehensive`, `mapstyle_physical_features`,
…, string table at 0x646078). Matching them to the ids is a VM task (V4).

## 4. PDCAltitudeGrid: display rules

Record layout: mad.md §7.9. One record per MapStyle id. Rules are sorted by class row, then by **descending**
altitude range. A class has 1 to 11 rules per style. Together they cover 50,000 km down to 1 km without gaps:
each `smin` is the next rule's `smax` + 1 m (or + 1000 m) (validated: 33 records, 33,086 rules, 19,998
(style, class) pairs).

### 4.1 Field table

`*` = field run through the special-value map: `0xFEFA` → none (−1), `0xFEF9` → special pen −2 (an internal
pen; it is not used in the shipped data). "Tri-state" is explained in §4.3.

| Field | Width | Meaning | Status |
|---|---|---|---|
| `cls` | W | PDCAttribute row | validated |
| `b2` low nibble | B | **geometry** visible: tri-state | code |
| `b2` high nibble | | a level, 1 or 3 (MOBB turns 0 into 1). 49 = `0x31` in 11 rules. Meaning open | — |
| `b3` | B | **label** visible: tri-state | code |
| `b4` low nibble | B | **interactive** (the feature goes into the hit-test list used for tooltips and clicks): tri-state | code |
| `b4` high nibble | | 0/1/2, mostly on areas (17 = `0x11`, 33 = `0x21`). Meaning open (probably the same tri-state for a 4th aspect) | — |
| `w5` | W | **GeoLabel index** of the label style | code; validated range (all < 422) |
| `w6` * | W | second GeoLabel index (alternate label style; its fonts and colours are loaded with w5). When it is used is open | code |
| `w7 … w13` * | W | **pen layers 0…6** (GeoPenDef index). Lines: up to 5 stacked strokes in the shipped data (w12, w13 always none). Areas: w7 is the fill pen. Points: w7 is the **symbol pen** (its bitmap is the symbol) | code + renders |
| `b14` | B | the **pixel class** of this rule's pen colours: 0 → class 4, 1 → class 2, 2 → class 3, 3 → class 6 (§5.4). Lines mostly 3, areas 0 or 1 | code |
| `b15` | B | 1 = the feature is **highlighted** when the mouse is over it or it is selected | code |
| `b16` | B | highlight mode: 1 or 2 → highlight drawn as a stroke with pen `w17`; 3 → highlight drawn as a fill with pen `w17` | code |
| `w17` * | W | **highlight pen** (pen 0 = 7-px yellow line for rivers/lakes; pen 76 = GeoTrans tint ramp 2 for country regions) | code + data |
| `b18, b19, b20` | B | **halo colour** R, G, B of this rule's labels (255,255,255 in 32,825 rules; 74,74,74; 102,102,102; 51,51,51) | code (colour passed to the label drawer) |
| `w21, w22` * | W | always none | validated |
| `w23` * | W | **Bitmap id** of the icon used for this class in lists and the legend (8, 11, 120 for the three million+ city classes) | code |
| `w24` * | W | second icon Bitmap id (106 in 33 rules of radius-point classes) | code |
| `smax, smin` | D | altitude range in metres (§1) | validated |

MOBB also computes a "half size" for each rule. It uses it to pad the search box when it hit-tests features
(FUN_6356c2ba):
- point class: half of the larger side of the bitmap of pen `w7`;
- radius-point class: half of the width of pen `w7`;
- line class: `(max width over w7…w13 + 1) / 2`;
- area class: 0.

### 4.2 Which rule applies (code: FUN_6356c1fd)

```
rule(cls, a):     # a = floor(altitude in km)
    rules = the rules of class cls, in stored order (largest ranges first)
    return the first rule with smin/1000 <= a, or "none"
```

`smax` is not tested, because the ranges are contiguous and every class's last rule reaches 1 km. So a class
that has rules in a style always gets one; a class without rules in that style is not drawn. A second lookup (FUN_6356c261) returns the **last** rule of the class
whose geometry is visible. MOBB uses it for the legend and similar lists.

Example (style 0, class 0 "PPL - non-capital million+ city", validated): at 3000 km the rule with range
3,999,999–150,000 applies: `b2=0x11, b3=1, b4=1, w5=106, w6=36, w7=107, w23=11`. GeoLabel 106 is font 9 ("MS
Reference Sans Serif"), 14 px, black. Pen 107 carries Bitmap 10 (key 0x1000A), the city symbol.

### 4.3 Tri-state visibility (code: FUN_6356cba2, FUN_6356cb70, FUN_6356cbd4)

The rule's value `v` (0, 1 or 2) is combined with a per-class user override `u` (MOBB keeps four byte arrays per
class, one per aspect):

| `u` | shown when |
|---|---|
| 1 | never (the user turned the class off) |
| 2 | `v ≠ 0` (the user turned the class on) |
| other (default 0) | `v = 1` |

So **`v = 1` means shown by default, `v = 2` means hidden by default but optional, and `v = 0` means never shown**
at that altitude. For example, airports far out have `b2 = 2` and are shown only when the user asks for them.

Counts over all styles: `b2` low nibble 0 = 22,508, 1 = 8,978, 2 = 1,600 rules. For a MapLibre style, compile
`v = 1` as a visible layer. Compile `v = 2` as a layer with `visibility: none`, and let the UI's "show more"
toggles turn it on.

## 5. GeoPenDef: pens and brushes

1231 records × 38 bytes. In the shipped data every record is its own chain head (`head = own index`,
`byte 3 = 1`) (validated). The chain mechanism exists for pens at other resolutions (see `res` below).

| Off | Type | Field | Meaning |
|---|---|---|---|
| 0 | u16 | `head` | index of the chain head (= own index in all records) |
| 2 | u8 | — | 0 |
| 3 | u8 | `res` | resolution factor of this variant (1 everywhere). When MOBB draws at a factor `k` (print), it picks the chain member whose `res` divides `k` and multiplies `width` and `passes` by `k/res` (FUN_63556852) |
| 4 | u32 | `pattern` | dash bit mask, see §5.1 |
| 8 | u8 | `plen` | dash period in pixels (1…32) |
| 9 | u8 | `width` | line width in pixels (0 = not a line pen) |
| 10 | u8 | `passes` | how many of the outermost pixel rows of the line are drawn, see §5.1 (= width in 1,210 pens) |
| 11 | u8 | `tint` | GeoTrans ramp index (§6), or 0xFF = plain colour |
| 12 | u16 | `bitmap` | Bitmap id (§8) or 0xFFFF. For point rules: the symbol. For area rules: a pattern brush |
| 14 | u8 | — | 0, 3, 4 or 25 in 7 pens (open) |
| 15 | u8 r, g, b | `colour` | |
| 18…22 | 5 × u8 | `over[0…4]` | may this pen draw over pixels of a given class: bits 1, 2, 4, 8, 16 of a mask (§5.4) |
| 23 | u8 | — | MOBB stores `byte == 0` (64 pens have 1; open) |
| 24 | u8 | `offset` | signed perpendicular offset = byte − 0x80. 0 in 1216 pens. 15 area pens have byte 0 (−128), which looks like a "not applicable" value |
| 25…29 | 5 × u8 | — | read by MOBB, 0 in all pens |
| 30…37 | 8 bytes | — | skipped by MOBB (`00 00 00 00 01 00 00 00` in every pen) |

### 5.1 Line rasterisation (code: FUN_635770d0 sets up, FUN_63576710 draws)

A line of width `w` is drawn as `w` one-pixel Bresenham passes, offset perpendicular to the segment by
`0, −1, +1, −2, +2, …`: pass `k` has offset `(k+1)/2` when `k+1` is even and `−⌊(k+1)/2⌋` when it is odd. Which
side is "+" depends on the segment's direction octant. The passes drawn are
`k = w−passes … w−1`, the outermost ones:

- `passes = width`: a solid line `width` px wide;
- `passes < width`: only the edges. For example pen 434 (width 7, passes 2) is two thin parallel lines 6 px
  apart, and pen 18 (width 4, passes 1) is one thin line offset by 2 px to one side.

**Dashes.** MOBB keeps a running distance `d` along the line, adding 1 per straight step and √2 per diagonal
step (the √2 constant was not read from the binary). A pixel is set when bit `round(d) mod plen` of `pattern`
is 1, counting from the least significant bit. `pattern = 0xFFFFFFFF` (with any `plen`) is solid. Data check:
the dashed pens are used by tunnels, disputed and undefined boundaries, intermittent rivers and graticules, and
by no solid-looking class (validated by listing every class that uses each pattern).

Examples:
- pen 5 = `pattern 0x1, plen 24, width 5`, grey `66 66 66`: a 1-px tick of width 5 every 24 px. Drawn over pen 3
  (solid grey width 1) this is the railway symbol (rule "Railroad - minor": `w7 = 3, w8 = 5`).
- pen 6 = `0x7, plen 5, width 1`, blue `4B 64 F7`: dashes of 3 on and 2 off (Ferry).
- pen 68 = `0x3F, plen 10, width 4`, `D7 60 40`: 6 on, 4 off, under pen 62 (solid yellow `FF FF 99`, width 2).
  This is "Road - text label - major" at 150–30 km.

**MapLibre.** `line-width = width`. `line-dasharray` = the run lengths of the bit pattern, divided by the width
(MapLibre measures dashes in line widths). Rotate the pattern so that it starts with a run of 1 bits. A pen with
`passes < width` becomes two lines with `line-offset = ±(width−1)/2` and `line-width = 1` (or one, for
`passes = 1`). Draw the layers of a rule in order w7, w8, … (§9).

**Areas.** A pen with `width = 0` used as `w7` of an area rule is a brush. It fills with `colour`, unless it has
a tint ramp (`tint ≠ 0xFF`: §6) or a pattern bitmap (`bitmap ≠ 0xFFFF`: tile the bitmap, transparent where its
palette entry is transparent). Pen 150 (bitmap 75) is used by many label-only areas (Region, Physiographic
region) and is never visible in their rules.

### 5.2 Pixel classes and "draw over" masks (code: FUN_635760a0, data at 0x635e3b18)

MOBB renders into an 8-bit palette image. Each palette entry carries a **pixel class**:

| class | given to | mask bit |
|---|---|---|
| 0 | unassigned | 16 |
| 1 | raster colours with CMT `kind` 0 | 2 |
| 2 | raster CMT `kind` 1; colours of rules with `b14 = 1` | 2 |
| 3 | colours of rules with `b14 = 2` | 2 |
| 4 | raster CMT `kind` 2; colours of rules with `b14 = 0`; one style-level colour (which MapStyle colour is open) | 1 |
| 5 | raster CMT `kind` 3 | 0 (never overdrawn) |
| 6 | label colours; rules with `b14 = 3` | 8 |
| 7 | — | 16 |
| (extra) | palette entries with a second per-entry flag set (its source is open) | 4 |
| 8 | a second style-level colour | 0 |

(The table `class → bit` is the 8 bytes `10 02 02 02 01 00 08 10` at 0x635e3b18.)

A pen sets a pixel only if `over` has the bit of that pixel's current class. With the HYPSO CMT, land has
`kind` 0 and sea/ice `kind` 2 (mad.md §5.4), so:

- pen 358 "Sovereign - boundary" (`over = 0,1,0,0,0` → bit 2) draws over land and over lakes (`b14 = 1`), not
  over the sea;
- pen 370 (graticule, `over = 0,1,0,1,0`) draws over land and label-class pixels. Read literally, that means the
  graticule is hidden over water. This is surprising, so VM shot V7 checks it.

This answers the CMT `kind` question of mad.md §9.7: it is the pixel class of the raster colour.

**MapLibre.** Most of this follows from the draw order. Where a pen must not cover the sea (boundaries) or must
cover only the sea, clip the layer with the land/water polygons. The converter has them: GeoGem coastline and
ocean classes. Or bake those lines into the raster tiles.

## 6. GeoTrans: tint ramps

7515 bytes = **167 ramps × 15 colours × (r, g, b)** (validated: `7515 = 167 · 45`; the largest `tint` used by
any pen is 166).

A pen with `tint = k` does not paint its own colour. Each pixel it covers becomes
`ramp[k][i]`, where `i` comes from the brightness of the pixel underneath (FUN_6355e990):

```
L  = luminance of the underlying pixel × 100   (0 … 25500)
i  = 15 − L·15/25500, clamped to 0…14      (bright → low i, dark → high i)
```

There is also a "relative" variant used for highlights. It takes the mean brightness of the area as its reference
and shifts the index by a per-colour offset. So a tinted area keeps the hill shading of the terrain under it.
Ramp 2 is a grey ramp from `E1 E1 E1` to `96 96 96` (the country highlight, pen 76). Ramp 163 runs from light
to dark red, and ramp 166 from bright to dark green (route highlights).

**MapLibre.** MapLibre has no multiply blend. Choose one of two:
- **exact (recommended):** pre-render the tinted areas into the raster tiles, applying the ramp to the decoded
  HYPSO pixels;
- **approximate:** a `fill` layer with `ramp[k][7]` at about 50 % opacity over the hill-shaded raster.

## 7. Labels: GeoLabel, Label Font, name conventions

### 7.1 Label Font

Records `{u8 font id, u8 charset, u8 n, UTF-16 name[n]}` until the end of the section. Charset is 0x00
(Windows ANSI) or 0x80 (Shift-JIS). WLD_HD:

| id | ANSI face | Japanese face |
|---|---|---|
| 1 | Times New Roman | MS Mincho |
| 2 | Arial | MS PGothic |
| 3 | Arial Narrow | — |
| 4 | Tahoma | MS PGothic |
| 5 | Georgia Ref | MS Mincho |
| 6 | Trebuchet MS | — |
| 7 | Verdana Ref | MS PGothic |
| 8 | MS Reference Serif | MS Mincho |
| 9 | MS Reference Sans Serif | MS PGothic |

MOBB creates the font as: height `−size` px, weight 700 if bold else 400, italic flag, and the face of
`(font id, charset)`. Text containing kana or CJK ideographs (U+3040–30FF, U+3300–9FAF) uses the Shift-JIS face.
Its size is clamped to 10…13 px and then increased by a quarter (×1.25). Bold becomes weight 600 and italic is
dropped. For a vertical label the face name gets an `@` prefix.

### 7.2 GeoLabel (422 records × 27 bytes)

| Off | Field | Meaning | Status |
|---|---|---|---|
| 0 | font | Label Font id (1…9; 2 = 187×, 9 = 175×, 8 = 40×) | code |
| 1–3 | colour | text colour R, G, B (`18 45 80` blue for water names, black for places) | code |
| 4 | size1 | size in px (8…18). MOBB scales it by a user "label size" percentage | code |
| 5 | size2 | second size (equal to size1 in 270 labels; up to 3 px smaller otherwise). **Hypothesis**: the smallest size an area/line label may shrink to | — |
| 6 | — | 1 (417×) or 2 (5×) | open |
| 7 | — | 0/1/2 | open |
| 8, 9 | — | mostly 4, 4 | open |
| 10 | spacing | 0…5. **Hypothesis**: extra letter spacing in px (3 in large sea names) | — |
| 11 | two nibbles | low 0/1, high 0/1/2. **Hypothesis**: low nibble = upper case (1 in place labels such as GeoLabel 106) | — |
| 12 | alt | 0 in 356 labels. When ≠ 0, a name that contains U+0005 is labelled with the part after it, and the three extra colours below are used | code (partly) |
| 13–15 | colour2 | extra colour (when byte 12 ≠ 0) | code |
| 16 | — | 1 (379×). **Hypothesis**: halo on | — |
| 17–19 | colour3 | extra colour | code |
| 20–22 | colour4 | extra colour | code |
| 23 | bold | 1 = bold | code (FUN_6353d379) |
| 24 | italic | 1 = italic (56 labels, e.g. GeoLabel 7, sea names) | code |
| 25 | — | flag tested before placing a label (FUN_635351a0) | open |
| 26 | — | flag | open |

**Halo** (code: FUN_6353b940, FUN_635426ff). MOBB first draws the text with the halo colour at every offset in
`[−1, 1]²` except the centre. Then it draws the text in its own colour at the centre. The halo colour comes from
the rule (`b18…b20`, §4.1). MapLibre: `text-halo-width: 1`, `text-halo-blur: 0`.

### 7.3 Name conventions in GeoObj

Validated over all 11,808 GeoObjGrid0409 records. These are the only control characters in names:
- **U+0002** separates a name from its **abbreviation**: `"United Kingdom\u0002U.K."`, `"Switzerland\u0002Switz."`
  (837 names). Use the abbreviation when the full name does not fit;
- **U+0004** separates **alternative names**: `"Danube\u0004Donau"` (353 names). Show the first, and offer the
  others to search or show them in brackets (VM V9);
- U+0005 (GeoLabel byte 12) does not occur in the shipped names.

### 7.4 Placement and priority

What is established (code and renders):
- Labels and point symbols go through one collision test. A point feature whose label cannot be placed is not
  drawn at all. Without this rule, every town of band 5 appears as a black square at 3000 km (render §14).
- Priority: `PDCAttribute.b13` (mad.md §8.1, 1…255) orders the candidates. Lower values win: million+ cities
  have 23, sovereign capital towns 35, towns 80, villages 100. (**Hypothesis**, consistent with the renders.)
- Point labels are placed beside the symbol. MOBB tries several positions (FUN_63535290 loops over candidate
  boxes and keeps the one with the fewest overlaps). Line labels follow the line, and MOBB rotates the font
  (`lfEscapement` from the segment angle).

MapLibre: `symbol-sort-key = b13`. Use `text-variable-anchor` [right, left, top, bottom] for points,
`symbol-placement: line` for line classes, and `text-allow-overlap: false` with `icon-optional: false` so that
symbol and label appear together.

## 8. Symbols: bitmaps, shields, legend icons

- **Bitmap** records (mad.md §7.2) are indexed by `Bitmap id = key − 0x10000` (0…148). Pens refer to them by id
  (validated: all 115 distinct pen bitmap ids are < 149). Pixel `p` has the colour `BitmapPalette[p]`: 60 ×
  `{u8 index, u8 r, g, b, u8 transparent}`, index 0 = `(0,255,0)` transparent (validated as R, G, B: the city
  symbols come out red and white, and the park and mine icons look right).
- **Sprite sheet:** one sprite per Bitmap id, `sprite name = "bm<id>"`, transparent where `transparent = 1`.
  Point layers use `icon-image = "bm" + bitmap of pen w7`. Area pattern brushes use `fill-pattern` with the same
  sprites.
- **w23 / w24** (§4.1) are Bitmap ids for list and legend icons (code: FUN_63519a2d/FUN_63519add build Windows
  icons and image lists from them). They do not appear on the map.
- **ShieldId** (81 bytes = 27 × `{u8 shield type, u16 PDCAttribute row}`): maps a road-shield type code (1…27) to
  a **shield point class** (validated: every value is one of rows 456–467, "Shield - Interstate" … "Shield -
  US"). Type 1 → 463 "Shield - Motorway", 7 → 456 Interstate, 8 → 467 US, 9/12/15/18 → 465 State, 10/13/20 →
  459 County, 14 → 462 Mexico Federal, 16 → 460 European, 17 → 458 Brazil, 19 → 457 Australia, 21–27 → 461 Hong
  Kong. The shield class's rule gives the shield bitmap (pen w7) and the number's label style (w5). **Open:**
  where a road's shield type and number come from. GeoObj `cls` is 0 everywhere and no name contains a shield
  code. The GeoStreet* tables in MOBB's table list (§17) are not on this disc. Shields may therefore not occur in
  the shipped world data (VM V8).

## 9. Draw order and the drawing passes

What is established (code: FUN_6351da30, FUN_6351cda0, FUN_6351fc90):

1. Each GeoGem record's parts are split into runs of consecutive parts with the same geometry kind and the same
   `PDCAttribute.b12`. **Line** runs (kind 2) go into one list, **area and radius-point** runs (kinds 3 and 1)
   into another.
2. Each list is sorted by `b12`, ascending. The sort is a C runtime sort, so the order of equal values is not
   defined; keep the record order. **`b12` is the z-order** (0 for most classes, 1 rivers, 2 basins and country
   regions, 5 admin regions, 60 admin-1 borders, 100 parks and graticules, 120 lakes and sea floor, 125 arterial
   roads, 136 toll highways, 150 sovereign boundaries, 205/255 route highlights).
3. Lines are drawn **layer by layer**: the pass for layer `L` draws pen `w(7+L)` of every visible line feature.
   So all casings are drawn before all centre lines.
4. Then point symbols and labels (§7.4).

Not read in the code: whether all areas come before all lines. The renders assume areas, then lines, then
symbols and labels (V5 checks it).

MapLibre layer order:
1. `background` (MapStyle colour 1);
2. the raster;
3. fills by `b12`;
4. for `L = 0…4`: line layer `L` of every line class, by `b12`;
5. highlight layers (`w17`, filtered on feature-state `hover`);
6. symbols by `b13`.

## 10. GeoObj fields c, e, f, g

Validated, and code (FUN_6351bb2f, FUN_6351bcf6): for an entry with **`c ≠ 0`**, the feature has its own **"go
to" point and altitude**:

- `e` is a **GeoGem cell key** (mad.md §7.8, any band). It is not the key of the record that holds the entry;
- `(f, g)` = **`(x, y)` in that cell**. Note the order: `f` is the x (longitude) coordinate and `g` the y;
- `c` is the **"go to" altitude in km** (200 for capitals and big cities, 995 Volta, 1055 Lake Victoria).

For `c = 0` (11 of 389 entries in record 0), MOBB uses the class altitude `PDCAttribute.scale / 1000` and the
geometry centre. `cls` (field 5) is 0 everywhere.

Self-check (WLD, GeoObjGrid0409; position = cell frame of `e` with x = f, y = g):

| Entity | record | c | e (band) | f | g | lat, lon |
|---|---|---|---|---|---|---|
| 25262 Paris | 16 | 200 | 4921 (7) | 682 | 764 | 48.863, 2.344 |
| 7398111 London | 18 | 200 | 13240 (7) | 74 | 650 | 51.519, −0.102 |
| 3702 Berlin | 16 | 200 | 5121 (7) | 530 | 353 | 52.516, 13.385 |
| 32380 Sydney | 21 | 200 | 24780 (7) | 484 | 86 | −33.868, 151.210 |
| 8540 Dallas | 19 | 200 | 16563 (7) | 171 | 315 | 32.777, −96.796 |
| 35124 Lake Victoria | 0 | 1055 | 1725 (6) | 928 | 236 | −1.298, 33.228 |
| 35473 Volta | 0 | 995 | 1224 (6) | 81 | 329 | 7.434, −0.445 |

All are within about 2 km of the usual city-centre coordinates. This point is the best anchor for point labels, and for
"centre on this place" links from articles.

## 11. Font Conv, Find keys, Unicode Attributes

### 11.1 Font Conv (4608 bytes) is a character-class and collation table

Despite its name, it is **3 × 256 records × 6 bytes**: one 256-entry table for each G1G2 code page map 0, 1, 2
(mad.md §8.3). Record `{u16 base, u16 weight, u8 letter, u8 separator}` for each byte value:

- `weight`: a case- and accent-insensitive sort weight. Digits are 1120, 1130, … 1210; letters a = 1230,
  b = 1250, c = 1270 … in steps of 20. Å and a both have weight 1230. The apostrophe has 65, and `"` has 40
  (treated as character 0x27);
- `letter = 1` for characters that count as letters (514 of 768 entries: alphanumerics and the
  transliteration marks);
- `separator = 1` for word separators: the space and the bytes 8 and 9 (4 entries).

**Find Grid `nlet` (mad.md §7.6), validated 100 %:** `nlet` = the number of key bytes **before the first `[`**
whose Font Conv entry, in the current G1G2 shift map, has `letter = 1`. Shift codes 1…5 are not counted, and
bytes 8/9 reset the map to 0 as in G1G2 decoding. This holds for all 166,461 full keys and all 230,851
continuation keys on every 7th page of Find Grid0409. The continuation keys were built with mad.md's
first-word-plus-fragment rule, which is therefore also confirmed. The earlier misses (the rotated keys) came only
from counting by character type and from cutting the bracket at `" ["`. Rotated keys have the bracket after a
0x08 (`An\x08Yuang\x08[Yong’an]` → 7 letters).

**Find Key `text` (mad.md §8.4), validated on 34 pages (every 40th):** it is the list of Font Conv weights of
the page's first key, **up to the first separator byte** (the first word). Page 0: key `"Jelaniya Cape [Mys
Zhelaniya]"` with a leading `0x94` → `40 1410 1310 1450 1230 1490 1390 1710 1230`. The weights give the order of
the Find pages, so a search reimplementation can binary-search a page with them.

### 11.2 Unicode Attributes (2926 bytes = 418 × 7)

Records `{u16 code point, u16 upper-case form, u16 base letter, u8 ANSI fallback}`. Examples: `ä` → upper `Ä`,
base `a`, fallback 0xE4. `ā` → `Ā`, base `a`, fallback `a`. `‘` (U+2018) → base `'`, fallback 0x27. MOBB loads
them into a lookup by code point (FUN_63583434). Probably used for case-insensitive and accent-insensitive
matching of typed search text. A rebuilt search can use Unicode case folding and NFD stripping instead; this table
is only needed for exact parity.

## 12. Miller flat map: MillerStyles, MillerTerrainGrid

### 12.1 MillerStyles (344 bytes, validated)

```
u16 version = 0x0100            (MOBB rejects other values: "bad Miller style version")
u16 tile_w = 256, u16 tile_h = 255
u8  nlevels = 5
nlevels × { u32 width_px, u32 height_px }:  512×384, 896×640, 1792×1275, 3328×2295, 6656×4845
u8  nrasters = 11
nrasters × { char raster[] NUL, { char look[] NUL } … NUL, u32 min_altitude_km, u8 last_level }
```

| raster | looks | min alt (km) | levels |
|---|---|---|---|
| HYPSO | GENERAL, GENERALFLAT, STATISTICAL, POLITICAL, HYPSOFLAT, HYPSO | 250 | 0–3 |
| SATELLITE | satellite | 2000 | 0–3 |
| NIGHT | night | 4000 | 0–3 |
| TECTONIC | TECTONIC | 8000 | 0–3 |
| RAINJAN, RAINJULY, RAINAVG, TEMPJAN, TEMPJULY | (same name) | 14000 | 0–2 |
| POPULATION | POPULATION | 8000 | 0–3 |
| MOON | MOON | 8000 | 0–3 |

(Level 4, 6656 × 4845, is described but no raster uses it.) The `min altitude` values equal the RAST `+0x34`
values for SATELLITE, NIGHT, TECTONIC, POPULATION and the climate rasters. They differ for HYPSO (250 km flat,
150 km globe) and MOON (8000 km flat, 14000 km globe).

### 12.2 MillerTerrainGrid keys (validated: all 1263 records)

A level of size `W × H` has `ceil(W/256) × ceil(H/255)` tiles: 4, 12, 35, 117. Records are stored raster by
raster in the order above, and within a raster level by level, so a raster with levels 0–3 has 168 records and
one with 0–2 has 51. `6 × 168 + 5 × 51 = 1263`.

Each record's third word `t` is the tile's **slot**: the Morton (Z-order) code of `(row, col)`, with the
column in the low bit of each pair: `col = bits 0, 2, 4, …` and `row = bits 1, 3, 5, …` of `t`. Edge tiles are
cropped: the last column is `W − 256·(cols−1)` wide and the last row `H − 255·(rows−1)` tall. Check: level 1
slots `0…9, 12, 13`, with `t = 5` → row 0, col 3, 128 px wide, and `t = 12` → row 2, col 2, 130 px tall.
Pixels are raster values, coloured by the raster's LOOK CMT (mad.md §5.4).

### 12.3 Projection (validated by fit and overlay)

The assembled tiles of a level form one whole-world **Miller cylindrical** map, 360° wide, centred on Greenwich
(render `png/r2/miller_HYPSO_1.png`). For a level of size `W × H`:

```
k   = W / 360                                   pixels per degree of longitude
x   = W/2 + k · lon
y   = y0 − k · (180/π) · 1.25 · ln tan(π/4 + 0.4 · lat)     (lat in radians inside tan)
y0  ≈ H/2
```

The fit maximised the agreement of land/water masks between the Miller tiles and the octahedral HYPSO raster
(`r2/millerfit2.py`, 20,000 sample pixels). Level 0 gives `k = 1.4267` (W/360 = 1.4222), `x0 = 256.0`,
`y0 = 192.2`. Level 1 gives `k = 2.4889` (= 896/360), `x0 = 448.0` (= W/2), `y0 = 322.4` (H/2 = 320). The fit
resolution is about 2 px. Drawing the GeoGem band-4 sovereign boundaries with these parameters onto level 1
(`png/r2/millercheck.png`) puts them on the raster's borders: the 49th parallel, the Alaska meridian, Africa and
Chile. So the vertical extent is about ±89° at level 1. Whether `y0` is exactly `H/2` must be checked at level 3
(open). The Miller viewpoint's constants (−0.625 = −1.25/2, 3.125 at 0x6360735c) fit the same formula.

## 13. Smaller items

### 13.1 LegendGrid triplets

LCID 0x409 = 0x9: `(0,1,0) "1,000,000+ people"`, `(1,1,2) "Populated Places"`, `(2,1,3) "National Capital"`.
**Hypothesis**: `(legend row, entry kind, Bitmap id)`. Bitmaps 0, 2 and 3 are small place dots. This is the
legend of the "Atlas Locator" style (the only one that uses these three classes). VM V10.

### 13.2 MinRect (14146 bytes = 643 × 22, validated)

`{u16 MapStyle id, u32 altitude (m), f32 lat1, f32 lon1, f32 lat2, f32 lon2}`. MOBB swaps the corners into
(min, max) order. All 643 rows have altitude 5000 m (5 km). Styles: 14 (147 rows), 0, 10, 27, 30 (124 each).
Each row is a box around a small island state or a city (Antigua, Bali, Barbados, Bermuda, Hawaii … Venice,
Vienna, Warsaw, Washington, Zürich). Meaning: inside the box the style may zoom down to 5 km, below its normal
minimum (150 km for Atlas Default). In MapLibre, allow a higher `maxzoom` inside these boxes (for example a
separate high-zoom source that covers only them).

### 13.3 CMT kind, RAST scale

See §5.2 (CMT `kind` = pixel class) and §1 (RAST `+0x34` = minimum altitude).

### 13.4 TESS triangle codes, Find Data detail tables

Not studied in this round. A converter needs neither: TESS can be regenerated (mad.md §5.2), and the Find Data
strings that matter are already reachable.

## 14. MapLibre recipe and prototype renders

### 14.1 Building one MapLibre style per MapStyle record

```
for each MapStyle s:
  background-color = colour1(s)
  raster source  = tiles of raster(s) coloured with CMT of look(s)
                   (+ pre-rendered GeoTrans tints, §6)
  minzoom/maxzoom of the map = zoom(max altitude), zoom(min altitude)          (§2.2)
  for each class row c, for each rule r of (s, c):
      z0 = zoom(r.smax/1000), z1 = zoom(r.smin/1000)
      kind 3 (area): if geometry tri-state(r.b2) != 0:
             fill layer: filter class == c, minzoom z0, maxzoom z1,
             fill-color = pen(r.w7).colour | fill-pattern = "bm"+pen(r.w7).bitmap,
             visibility = (tri == 1 ? visible : none)
      kind 2 (line): for L in 0..4 with r.w(7+L) != none:
             line layer (pass L): width, colour, dasharray, offset from pen (§5.1)
      kind 0/1 (point): symbol layer with icon "bm"+pen(r.w7).bitmap
      if tri-state(r.b3) != 0: symbol layer for the label with GeoLabel(r.w5):
             text-font = Label Font(font), text-size = size1, text-color = colour,
             text-halo-color = (b18,b19,b20), text-halo-width = 1, symbol-sort-key = PDCAttribute.b13
```

Merge consecutive rules with identical paint into one layer with a zoom-stepped `filter`, to keep the layer
count manageable (style 0 has 1254 rules). Layer order: §9.

Fonts: the faces are Windows fonts ("MS Reference Sans Serif", "Arial", …). The disc's `Fonts` folder (if it
ships these faces) or the user's Windows fonts are the source for the glyph PBFs. This is a converter
decision, not a format question.

### 14.2 Prototype renders (private, `analysis/decompiled/mad-proto/png/r2/`)

These are local equirectangular renders with `P/m = 4 × 600 = 2400`. Raster level is the finest level that is
not coarser than the screen. Simplifications in the prototype: labels are horizontal and placed at the point or at
the bounding-box centre; collision uses boxes only; upper case is applied when GeoLabel byte 11's low nibble is 1
(a hypothesis); highlights are not drawn.

| File | Style | Centre | Alt | What it shows |
|---|---|---|---|---|
| `eu_s0_a3000.png` | 0 Atlas Default | 50 N 10 E | 3000 km | hill-shaded HYPSO; graticule every 2°; national capitals (red-ringed dots), big cities in black capitals, regions in red; railways as ticked grey lines; rivers |
| `eu_s10_a3000.png` | 10 Atlas Political | same | 3000 km | the POLITICAL look plus solid country fills from the four `CountryRegion` colour classes (yellow `E7 CC A7`, green `BE C9 B6`, purple `D4 C1 D6`, red `E2 B7 B5`) |
| `eu_s7_a3000.png` | 7 Satellite Day | same | 3000 km | satellite raster; country names in yellow, cities in white with black halo, black boundaries |
| `de_s10_a1000.png` | 10 Atlas Political | 50 N 10 E | 1000 km | band 6: first-level divisions tinted (German states in shades of orange), international boundary drawn as a dotted black line over an orange ribbon (rule `w7 = 358`, `w8 = 442`), town density about right |
| `paris_s0_a400.png` | 0 | 48.9 N 2.35 E | 400 km | band 7: very dense village labels. Either `P` is larger at this altitude, or Encarta thins labels more than the prototype does (V2, V6) |

## 15. Amendments to mad.md

1. **§8.2, §7.9, §8.1 and §5.3: "scale" means altitude** (§1 here). GeoFaceScale bands, PDCAltitudeGrid ranges,
   `PDCAttribute.scale` and the MapStyle limits are camera altitudes in metres (km after MOBB's /1000). The phrase
   "map-scale denominators (1:50,000,000 …)" in §8.2 is wrong. RAST `+0x34` is a minimum altitude in km.
2. **§7.7 GeoObj `c, e, f, g`**: `e` is a GeoGem cell key, `(f, g)` = `(x, y)` in that cell (f = x), and `c` =
   the "go to" altitude in km (§10 here). Round 1's test of `f, g` against the record's own cell had the wrong
   cell and the axes swapped.
3. **§7.6 Find Grid `nlet`**: count with Font Conv's `letter` flag, and cut at the first `[` (§11.1). Validated
   100 %, which also confirms the continuation rule.
4. **§8.4 Find Key `text`**: Font Conv weights of the first word of the page's first key (§11.1).
5. **§8.7 table entries:**
   - `MapStyle`: the three names are char[32] each, and the full layout is in §3.
   - `GeoPenDef`: field layout in §5. All records are their own chain heads, and byte 3 is a resolution factor,
     not an "is head" flag.
   - `GeoLabel`: §7.2. Byte 0 is the Label Font id and bytes 23/24 are bold/italic.
   - `Label Font`: the record has a **u8 id and a u8 charset**, not a u16 id.
   - `ShieldId`: values are PDCAttribute rows (§8).
   - `GeoTrans`: 167 ramps × 15 RGB (§6).
   - `Font Conv`: §11.1.
   - `Unicode Attributes`: §11.2.
   - `MinRect`: §13.2.
   - `MillerStyles`: §12.1. The "u16 pairs" in mad.md are really u32 level sizes.
6. **§7.3 MillerTerrainGrid**: `t` is a Morton slot of `(row, col)` with the column in the low bits (§12.2).
   The per-raster level counts come from MillerStyles.
7. **§7.9 PDCAltitudeGrid**: field meanings in §4.1. `w5` and `w6` are GeoLabel indices (not pens). `w23` and
   `w24` are Bitmap ids (not pens). Only `w7…w13` and `w17` are pens.
8. **§8.1 PDCAttribute**: byte 12 (`b12`) is the z-order of the draw lists (§9), and byte 13 is very probably the
   label priority (§7.4). Byte 6 (the "flag") is tested when a feature is added to the hit-test list
   (FUN_635461c0).

## 16. VM screenshot checklist

For the lead to capture. Use a fixed window size, recorded with each shot (for example the map pane at
800 × 600), and record Encarta's map options (label size, "show more places" or similar). Each shot can be
opened with the URL form of §2.3. If `alt` turns out not to be km, adjust the numbers and note the unit.

| # | URL (`msencarta://view=MapView/cmd=map/…`) | Purpose |
|---|---|---|
| V1 | `lat=0.00/long=0.00/alt=6378.00/style=0` and `…/alt=1000.00/style=0` | unit of `alt`; scale bar length → `P/m` (§2.1) |
| V2 | `lat=48.86/long=2.35/alt=400.00/style=0`, `…/alt=150.00/style=0` | measure pixels Paris–Orléans (111 km) → `P/m`; label density at band 7 |
| V3 | `lat=50.00/long=10.00/alt=3000.00/style=0` | compare with `eu_s0_a3000.png` (graticule, capitals, case of city names, halo) |
| V4 | same view with `style=1` … `style=33` (one shot per id, at least 0, 2, 7, 8, 9, 10, 13, 16, 30) | style id ↔ menu name; background colour 1/2 |
| V5 | `lat=50.00/long=10.00/alt=3000.00/style=10` | country fills, boundary colour, area/line order |
| V6 | `lat=51.51/long=-0.12/alt=50.00/style=0` and `…/alt=20.00/style=0` | road casings and dashes (pen 68 "major road" dashed casing?), railway ticks, label thinning |
| V7 | `lat=40.00/long=-30.00/alt=3000.00/style=0` | is the graticule drawn over the ocean? (§5.2) |
| V8 | `lat=40.75/long=-74.00/alt=30.00/style=0`, `lat=52.00/long=5.00/alt=30.00/style=0` | road shields present? (US interstate, European E-roads) |
| V9 | `lat=48.20/long=16.37/alt=800.00/style=0` | how "Danube/Donau" (U+0004) and abbreviations (U+0002) are shown |
| V10 | `style=17` (Atlas Locator) at `alt=20000.00` | legend content (LegendGrid triplets) |
| V11 | hover the mouse over France at V5, and over Lake Geneva at V3 | highlight pens (`b15/b16/w17`): country tint, 7-px yellow lake outline |
| V12 | `lat=0.00/long=0.00/alt=20000.00/style=0`, then switch Encarta to its flat-map view (if it has one) | Miller map extent, flat-map scale (§2.1, §12) |
| V13 | `lat=12.50/long=-69.95/alt=5.00/style=0` (Aruba is not in MinRect) and `lat=45.44/long=12.33/alt=5.00/style=0` (Venice is in MinRect) | the MinRect zoom limit |
| V14 | a map with `style=12` (Statistical) and a statistic chosen | how statistical classes `Z - (STAT) …` are coloured |

## 17. Open points and provenance

Open, in priority order:
1. `P` and `m` (§2.1). This gates exact zoom matching (V1, V2).
2. GeoLabel bytes 5–12, 16, 25, 26 (sizes, case, spacing, halo switch). Then PDCAltitude `b2` high nibble,
   `b4` high nibble, and when `w6` is used.
3. The road shield source (§8).
4. Whether areas are always drawn before lines (§9). The exact label candidate positions (§7.4).
5. The Miller `y0` at levels 2–3 (§12.3), and the flat-view scale relation (§2.1).
6. The meaning of MapStyle colour 1/2 and flags (§3); LegendGrid (§13.1).
7. TESS triangle codes, Find Data detail tables, GeoPenDef bytes 14, 23 and 25–29 (all 0 or nearly).

Provenance (MOBB200.DLL, same build as mad.md §10):

| Topic | Functions / data |
|---|---|
| MapStyle record | FUN_63555a66, FUN_6352258b |
| PDCAltitudeGrid reader, special pen values, half sizes | FUN_6356c2ba, FUN_6356c1c2 |
| Rule lookup (first rule with smin ≤ alt; last visible rule) | FUN_6356c1fd, FUN_6356c261; contexts FUN_6356cab2, FUN_6356cafc, FUN_63556497, FUN_6355650c |
| Tri-state checks (geometry, label, hit-test) | FUN_6356cba2, FUN_6356cb70, FUN_6356cbd4; hit-test list FUN_635461c0 |
| Highlight (b15/b16) | FUN_63561398, accessors FUN_6351fc50, FUN_6351fc6c |
| Palette preparation per style (which colours, which pixel class) | FUN_635560ee, FUN_635561f2, FUN_6355606e, FUN_63556020 |
| GeoPenDef record, chains, resolution variants | FUN_6357800b, FUN_635568cf, FUN_63556852, special pens FUN_635567c1 |
| Line set-up and rasteriser (passes, dash bits, distance) | FUN_635770d0, FUN_63576710; draw-over table FUN_635760a0, class→bit bytes at 0x635e3b18 |
| Pen accessors (offset, extra bytes) | FUN_63576f60, FUN_63576fa0, FUN_63577030, FUN_63577080 |
| GeoTrans ramps | parser FUN_6355e946, tint table FUN_6355e990, FUN_6356b734 |
| GeoLabel record | FUN_63555675, FUN_63522439; sizes FUN_63533560, FUN_63533810; font spec FUN_6353d379 |
| Fonts | Label Font parser FUN_6353d098; LOGFONT FUN_635429f0, FUN_63542b70, FUN_63543f1c; text and halo FUN_6353b940, FUN_63542752, FUN_635426ff |
| Label placement | FUN_63535290, colour accessor FUN_63535230 |
| Draw lists and z-order | FUN_6351da30 (runs by kind and b12), sort key FUN_6351cda0, line pass FUN_6351fc90, object sort FUN_6351ce2a |
| Icons from w23/w24 | FUN_63519a2d, FUN_63519add |
| GeoObj "go to" point | FUN_635226dc (reader), FUN_6351bb2f, FUN_6351bcf6 |
| ShieldId | FUN_6355780e |
| MinRect | FUN_6355f45e, FUN_6355f377 |
| Font Conv, Unicode Attributes | FUN_63518367 / FUN_63518302; FUN_63583434 |
| MillerStyles | FUN_63574650 (table 0x31), FUN_63573e50 |
| Viewpoints | base FUN_6358e440; globe FUN_6358bee6, FUN_6358bb0f, FUN_6358aed1; Miller vtable 0x635e5278, FUN_63588cdd |
| Table name list (62 names, includes GeoStreet*, GeoTag*, GridIndex*, GI* not on this disc) | 0x63606330 |
| ENCARTA.EXE map URL, scale bar | FUN_00413e40, FUN_00419940, FUN_00594600 |
