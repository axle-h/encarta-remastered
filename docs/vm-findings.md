# Findings from the XP reference VM

Ground-truth observations of Encarta Premium 2009 running in the Windows XP reference VM (see [vm.md](vm.md)).
Each item gives the observation, how it was observed (the private fixture under `fixtures/` that backs it), and the
conclusion for the spec. The specs themselves are not edited here; the lead folds these in.
Fixtures are Microsoft-derived and stay local (`fixtures/` is gitignored). Index files: `fixtures/articles/INDEX.md`,
`fixtures/tours/INDEX.md`, `fixtures/atlas/INDEX.md`.

Setup common to everything below: 1024x768x16 screen (Cirrus), Encarta window maximised, software rendering
(no Direct3D hardware device exists on the Cirrus adapter). Guest clock pinned to 2009-03-01.

How tour numbers were measured: `tourhost` (our own C# host, `vm/guest/tourhost.cs`, driven by `vm/tour.py`) hosts
the same `ACTX3DX3.OCX` control at Encarta's size (616x420). It calls the control's `Do()` and receives its
`Event3DX`, so `camera.pos?` can be sampled every 50-100 ms while a key is held through QMP. Its render of the
Acropolis start pose matches Encarta's own control to within 219 of 258,720 pixels (scattered rasteriser noise), so the
engine behaves the same. Host-side behaviour (site map, hotspot navigation, menus) was observed in Encarta itself.
The harness loads tours from the extracted files in disk mode (`script.load=C:\tours\<nnnnn>\load.3sc`, the same
path as Encarta's `_3DVTContent` development override), see "Unresolved" for ITS mode.

---

## tours-3d.md

### §2 / §3 Command interface
- **`Do()` results.** `SCENE.HALFREZDRIVING=0` returns `Failed` (confirms §2.2/§6.10: the host's spelling is not
  recognised). `SCENE.DISPLAYFPS=1` returns `Failed` but does switch the overlay on. `script.load=<path>\load.3sc`
  returns `Failed` even when the load then runs normally and `WORLDLOADED` arrives 1.6-2.3 s later. Conclusion: the
  reply of these commands is not a reliable success signal; a port should not depend on it.
  Backed by: tourhost session logs summarised in `fixtures/tours/*/viewpoints/index.json` (`load_seconds`).
- **Spaces in arguments.** `script.itsfileurl=C:\Program Files\...\04257.eit` returns `-2` (not found); the same file
  by its 8.3 path returns `1`. Conclusion: the statement text loses its spaces (or is cut at the first space) before the
  path is used. Encarta must pass a path without spaces. Add to §3.2 that spaces in values are not preserved.
- **Event text case.** Events arrive upper-cased: `HOTSPOTCLICKED:HOTSPOTID=701504869`,
  `USERLEVELCHANGE:QTSREFID=701505293`, `WORLDLOADED:XOFFSET=-1.00,ZOFFSET=0.65,XEXTENT=10.00,ZEXTENT=18.30,FIELDOFVIEW=80,SM_RECTTOP=7,...`.
  This confirms the §3.2 case-folding rule applies to `SCRIPT.EVENT(...)` text (no `^` in these scripts).
  The first event after creation is `FinishedInitializing` (dispid 1), as in §2.2.
- **`USERLOCUPDATE`** fires every 1.00-1.02 s while a tour is loaded (13 consecutive events). Confirms §2.2.
- **Camera NaN quirk (new).** `CAMERA.POS`/`CAMERA.MOVETO` to the position the camera is already at (same X and Z)
  makes `CAMERA.POS?` and `CAMERA.PITCH?` report `-1.#IND00` (NaN) afterwards; once in that state further MOVETOs
  keep the NaN until `CAMERA.HOMEPOV` (which also resets `HEIGHT` to 0.05). Observed three times in tourhost; a fresh
  load followed by MOVETOs to other positions never showed it. This is probably why the stop commands of five tours
  begin with a dummy `CAMERA.MOVETO=160,...` or `350,...`. A port does not need the NaN, but should note that a move to
  the current position is degenerate for the look-vector rule (§6.10 "keeps the look vector").

### §2.3 / §6.9 Tick rate, time factor, speeds (checklist §10 "Tick rate and speeds", §6.13 items 1-2)
- **Frame/tick rate.** `SCENE.DISPLAYFPS=1` shows `SW 3 65.832245` standing still and `SW 3 47.080460` while walking
  (Acropolis start, 616x420). `fixtures/tours/701504257/fps-overlay-walking.png`. So about 64 ticks/s idle (the 15.6 ms
  system timer) and fewer while the software rasteriser is busy. "SW" confirms software mode; "3" is unexplained.
- **Walk speed = 100 x `SPEED.FLY` units per second, independent of the tick rate.** Acropolis (`SPEED.FLY=.002`):
  0.6007 units in 2.955 s = **0.203 units/s** horizontally (per-sample values 0.13-0.26, mean 0.20) while the frame rate
  was ~47/s. Roman Forum (`.0035`): **0.347 units/s** on the flat. `fixtures/tours/701504257/walk-up-3s.txt`,
  `fixtures/tours/701505285-stairs-walk.txt`. Confirms `k = clamp(100·dt, 1, 5)` with `move = k·SPEED.FLY` (speeds are
  per 1/100 s); the k>=1 floor never mattered here because dt was always > 10 ms.
- **Turn speed = 200 x `SPEED.TURN` degrees per second.** Acropolis (`.5`): right arrow held 3 s turned +303.9° over
  2.95 s of change = **101-103°/s** (left arrow: -101°/s). Right arrow increases yaw. Position does not drift while
  turning. `fixtures/tours/701504257/turn-right-3s.txt`. Confirms §6.9 (100°/s).
- **Pitch.** PgUp/PgDn change pitch by exactly `PITCHSPEED` (6° at Acropolis) per tick, not time-scaled: holding PgUp
  from -40 gave -34, -28, ... , 38, 44 at 10-30 ms intervals, then stopped. Steps that would reach |pitch| >= 45 are
  refused (from -40 it stops at 44; from 45 down it stops at -39). Starting at 21 one tap reached 44.998962 (= 21 + 4x6
  minus float error), so the test is on the value derived from the look vector. **Home** sets pitch to 0. **Positive
  pitch looks up**: `fixtures/tours/701504257/pitch+30.png` shows sky, `pitch-30.png` shows the ground. Confirms §6.1,
  §6.9, §10 "Pitch".

### §6.1 / §6.10 Field of view (§6.13 last item, §11 FOV unknown)
- Rotating the camera by exactly 5° of yaw at pitch 0 shifts the image centre by 32 px. f = 32/tan 5° = 366 px, so with
  the 616 px wide view the **horizontal FOV is 80.2°** (vertical 59.7°). `CAMERA.FOV=80` is the **horizontal** field of
  view at Encarta's 616x420 control. Measured in tourhost (Acropolis Propylaea, yaw 20 → 25).

### §6.7 Collision (checklist "Collision")
- Walking at yaw 93.4 into the Propylaea side wall: X increased at 0.20/s until x = -1.384349, then stayed constant to
  6 decimals while Z kept changing at 0.0124 units/s = 0.2·sin(3.4°), i.e. **the move is projected onto the wall and
  the tangential part is kept (wall sliding)**. At z = -4.972883 the camera stopped completely (a corner). No bounce,
  no jitter. `fixtures/tours/701504257/walk-into-wall.txt`. Confirms §6.7.
- Open edges without a wall segment: not tested.

### §6.8 Gravity vs snap (checklist "Gravity vs snap")
- `SCENE.TRYHARDWARE` / `Try3DHardware` return `Failed` in the VM, so both Encarta and tourhost run the software
  device. Raising the eye 0.6 units above the ground with `CAMERA.POS` puts it back at ground + HEIGHT on the very next
  sample (≤ 11 ms): **snap mode in software**. Walking up the Roman Forum stair raised Y from 0.323 to 0.698 smoothly
  with the steps (no falling or lag). `fixtures/tours/701505285-stairs-walk.txt`. Gravity mode could not be observed:
  this VM has no D3D hardware device. Recommendation for a port: implement snap mode (what any user without a 3D card,
  and this reference VM, sees).

### §6.4 / §4 Floors (checklist "Floors")
- Roman Forum, MOVETO to (11.05, -, 2.6) and walking north up stair 1: the walk-map cell 657 enter macro fired
  `USERLEVELCHANGE:QTSREFID=701505293` at the MOVETO, Y followed the steps across the 1stFloor → stair1 switch with no
  visible discontinuity, and the walk ended against a wall at the landing (z = 3.8899). Backed by
  `fixtures/tours/701505285-stairs-walk.txt`. Ordering of enter macros that switch walk maps: not resolved.

### §4.2 Site-map marker (checklist "Site map")
- For the six Acropolis stops, the "you are here" marker in Encarta's site map sits exactly where §4.2 predicts
  (`px = 41 + round(44.5 + (x+1)·8.9)`, `py = 7 + round(91.5 - (z-0.65)·10)`, site-map window origin at screen (1,129)).
  World +Z is up the map. The view wedge points up the map for yaw 3.4° and 19°, down for 197° and 204°, down-right for
  155°, and is about 80° wide. Confirms §4.2 for an unrotated map. `fixtures/tours/701504257/stops/*.png`.
  The rotated maps (Persepolis, Azuchi) were not checked.

### §4.3 / §8 Hotspots, captions, click targets (checklist "Hotspot click", "Label/hotspot visibility")
- **Click target.** Clicking the caption "Athena Nike Frieze" at the Temple of Athena Nike stop fires
  `HOTSPOTCLICKED:HOTSPOTID=701504869`, and Encarta then **replaces the whole main view** with the media page
  `msencxml://content/681500403` (picture + caption + credit), not the tour's side pane. The Back button returns to the
  tour at the same pose. **Correction to §4.3** ("opens that hotspot's media in the tour's side pane").
- **hotspot id → media.** Each hotspot id's data record (DataDlx, group `virttour`) carries `destmedia="<id>"` plus
  `x`/`y`; 701504869 has `destmedia="681500403"`, matching what Encarta opened. The full table (119 rows: stop id,
  hotspot id, title, destmedia, destination group and title) is `fixtures/tours/hotspots-destmedia.txt`. Several
  destinations are `group="flash"` interactives ("Propylaea, Now and Then" → 681514263); those need Flash, which is not
  installed in the VM (see "Unresolved").
- **Tour content record** (answers §11 first item, from the data, confirmed by the menus): the tour record lists
  `children/media` = the Tour Stops in menu order; each stop record has `default/@coord3d` (the camera command string
  Encarta sends), `children/hotspot`, `children/label`, `children/site`, `children/article`. The Tour Stops menu shows
  one submenu per stop whose first entry moves to the stop and whose other entries are the stop's hotspots (titles from
  the hotspot records). `fixtures/tours/tour-stops.txt`.
- **Captions before the first step.** At the Acropolis start no captions are shown
  (`fixtures/tours/701504257/encarta-start-screen.png`); after choosing a stop from the menu they are
  (`stops/01-temple-of-athena-nike.png`). The Colosseum shows captions immediately after loading
  (`fixtures/tours/701505918/loading/003299.png`), because its default stop command starts with a dummy MOVETO away and
  back, which re-enters the hotspot cell. So "no captions before the first step" holds only for tours whose default stop
  is a plain MOVETO to the start pose (Acropolis, Persepolis, Abu Simbel). The "Show Labels and Hot Spots" toggle was not
  tested.
- **Caption colours.** Normal caption text and frame read back as (198,211,231) on the 16-bit screen, which is 0xC0D1E3
  quantised to RGB 565; black fill; the hovered caption turns text and frame white (255,255,255); the pointer over it
  is a hand cursor. `fixtures/tours/701504257/harness-hotspot-hover.png`. Confirms §8.5.3 colours.
- **Display pixel format (§5.9).** The green value 0xD3 above can only come from 6-bit green: the VM display is
  **RGB 565**.
- Caption pixel widths: not measured yet, but every `viewpoints/*.png` shows captions at known camera poses, so widths
  can be measured offline against the spec's rule.

### §7.2.6 Flicker (checklist "Flicker")
- At the Athena Parthenos stop the lit strip seen through the Parthenon doorway changes brightness between frames
  sampled 105 ms apart (mean luminance of the strip jumps among 94-118 with no smooth transitions), while the rest of
  the view is static. Consistent with step changes every 0.1 s within a range of the base colour.
  `fixtures/tours/701504257/anim-athena-parthenos/`. The 0.4 s cadence and the steady torches of Persepolis/Colosseum
  were not checked.

### §8.4.3 / §3.5 Loading screen (checklist "Loading")
- While a tour loads, the 616x420 control shows a black screen with a ring icon labelled "LOADING" on the left and a
  framed text block "How to navigate a 3D Ancient Site" (Use your mouse / Use your keyboard / Jump to tour stops)
  on the right. Visible for about 2 s (Colosseum: 0.67 s to 2.65 s after the request). Then the scene appears, then the
  site map and the stop text. `fixtures/tours/701505918/loading/000669.png`. The spinner cadence was not resolved (the
  load is too short in this VM to sample 16 frames).
- Host layout: the control sits at screen (289,195), 616x420, with a 1 px black frame drawn by the host, on a
  (49,69,107) background; site map at (1,129)-(173,323), stop text below it, menu bar "Music | Tour Stops | Start Over |
  Appears in | Credits" at the top right, tour title bottom left.
- **Load time** in tourhost (disk mode): 1.6-2.3 s per tour (`viewpoints/index.json`).

### Named viewpoints for side-by-side checks (§10 request)
- Every Tour Stop of all 7 tours, rendered at its own camera command string, with the camera read back:
  `fixtures/tours/<id>/viewpoints/` (44 images including the 7 start poses). Y differs from the commanded Y where
  ground following applied, e.g. Persepolis Gate of Xerxes commanded y 4.106655, settled 4.020147.

### Not resolved for tours
- LOD switch distance, Acropolis water ripple, PVS on/off comparison, back-face culling, cloud statues, sprite yaw sign,
  filmstrip fps, cursor zones, grip.cur, junk in hotspot strings, maximised (MaxView) mode, rotated site maps,
  "Show Labels and Hot Spots" toggle. All are now cheap to do with tourhost (`vm/tour.py`).
- **ITS mode in tourhost**: with `script.itsfileurl=<8.3 path>` returning 1, `script.load=load.3sc` still loads nothing
  (no `WORLDLOADED`). Encarta itself loads from the containers, so something else is needed (perhaps
  `SCENE.LOADFILESFROMITS` or a host-provided storage); not needed for the measurements, which used disk mode.

---

## Article corpus (content converter and renderer, Step 2/4 parity)

60 fixture folders in `fixtures/articles/` (index: `fixtures/articles/INDEX.md`): 19 encyclopedia articles (long
survey articles truncated at 30 screens: France, United Kingdom, World War II, China, Brazil, Japan-style country
articles, United States History, Shakespeare, Bach, Lincoln, Einstein, Moon, Internet, Periodic Table, Ancient Egypt,
plus the outline view of World War II), sidebars (3), tables and Quick Facts (4), Facts and Figures (1), timeline topic
and timeline event (2), quotation (1), archive yearbook article (1), media (image, video with captions, two audio items,
animation, map, historical map, chart, flag and anthem, panorama, composite), 3 Encarta Kids articles, 5 dictionary /
thesaurus / translation entries, and 5 articles chosen for special glyphs. Observations made while capturing:

- **Generic URL.** `msencarta://id=<id>` lets Encarta pick the view for any class (media, sidebars, tables, timeline
  items); unknown ids show "Page or keyword not found" plus the Help window.
- **Short articles inside longer ones.** Requesting Al-Karaji 761578291, Aberration 761576625, Additive Identity
  761552816 or American Society for Information Science 761564555 opens the containing articles Mathematics, Optics,
  Algebra and Library (institution).
- **Sidebar-style views use two iframes** (`FrameHdr` 72 pt high and `FrameBody`), so their text is not in the top
  document. `capture.py` now appends the frame documents to `article.html`/`article.txt`.
- **Video** (461546454 The Battle of Midway): plays in the media view with a caption band under the picture
  ("Sound Effect" / "(planes whirring, guns firing)"). `fixtures/articles/461546454/screen.png`.
- **Pronunciation**: no article title showed a pronunciation control; pronunciations (respelling plus speaker icon)
  appear in the Dictionary entries (`fixtures/articles/dict-encyclopedia/`).
- **Quick-verify links**: in-text links render as `<A ... href="msencarta://view=ERSArticleView/id=..." linktype="qv">`
  in blue #314dce.
- **Dictionary/Thesaurus/Translations** are a separate program (`EDICT.EXE`, window "Encarta Dictionaries", 765x480 at
  111,112) with its own HTML view, opened by the feature ids 701712323 / 701712324 / 701767016.
- **Encarta Kids** is a second `ENCARTA.EXE` (`C:\Program Files\Microsoft Encarta\Encarta Kids 2009\`, window class
  `K09ADXRC_ENCARTA`), started by the feature id 701712340; its own instance accepts `msencarta://id=<kids id>`.
- **Flash Player is not installed** in the guest; Flash features (class 2215 Kids features, `group="flash"` media,
  timeline event pop-ups with media) show "The feature is not available because it requires a Flash player".
- "On this day" item: not found in the 2009 UI (no feature record, no home-page panel). Not captured.

## content-schema.md (questions from the content converter)

1. **Sidebars 461575610 and 631522251 recaptured** with frame support: `article.txt` now has 31 kB and 8.8 kB of text
   (`fixtures/articles/461575610/`, `.../631522251/`, also 762504311). The sidebar layout: header frame with the sidebar
   banner, body frame with intro paragraph in italic, rule, headline, then sections with headings.
2. **Alternative title.** East Timor 461510929 (data `<alttitle>Timor-Leste</alttitle>`): the first paragraph renders as
   `<SPAN class=inline_title>East Timor</SPAN> or <SPAN class=inline_title>Timor-Leste</SPAN>,&nbsp;independent&nbsp;republic ...`,
   i.e. both titles bold in the inline-title position joined by a plain " or ". Encarta also glues the first words after
   the inline title with no-break spaces. `fixtures/articles/461510929/article.html`, `screen.png`.
3. **Specialty-font characters are glyphs, not Unicode fractions.** In Musical Rhythm 761553014 the XML
   `<fs type="2">½</fs>` renders as a span in font "MS Reference Specialty" whose glyph for U+00BD is a stacked
   **6 over 8** time signature ("6/8 meter"), and U+00AC renders as stacked **2 over 4**. So a converter must map
   Specialty code points through that font's glyph table (or render with the font), not show "½".
   `fixtures/articles/glyphs/specialty-761553014.png`.
4. **Kids articles.** Title in orange (#FF7800, MS Reference Sans Serif 20 pt) at the top left; a "Look at Pictures"
   (or "Read Article") button top right; media as a two-column grid of rounded orange-bordered tiles on the left, each
   with a blue caption and a corner badge (speaker = audio, "+" = picture, hand = interactive); text column on the right
   in large type; section headings are orange upper-case sans-serif (`P class=sectitle`, e.g. "POLITICIAN, LAWYER,
   FATHER") with no visible section numbers (the `secprefix` attribute is present but not shown); facts as a green-headed
   two-column table "Facts About France". The left tiles scroll together with the text. `fixtures/articles/kids-701639453/pages/01.png`,
   `kids-701639372/screen.png`.
5. **Private-use glyphs** (rendered by Encarta's reference fonts): U+F73D and U+F73E (article F 761551508, "Its forms were
   , , F, or ⊐") show two archaic digamma letter forms; U+F71B (Communism 761572241, "Milošević") shows a c with an
   accent above (intended ć); U+F700 (Lillehammer 761580611, "Lågen") shows a with ring above (å).
   Crops: `fixtures/articles/glyphs/f73d-f73e-761551508.png`, `f71b-761572241.png`, `f700-761580611.png`.
6. **Inline media placement** (full stitched captures `fixtures/articles/761553483/full.png` United Kingdom,
   `761568934/full.png` France, `761563737/full.png` World War II): three columns. The section's lead media item
   (flag+anthem for countries) is placed in the text column at the top of the introduction; further images sit in the
   left margin column, top-aligned with the paragraph or section they belong to; the right column holds modules
   (World Atlas map, Quick Facts, Related Articles, Web Sites) at the top and occasional images further down; inline
   tables ("Geography of United Kingdom") span the text column; sidebar teasers ("Great Works of Literature") appear as
   blue boxes inside the text column.

---

## mad-styling.md (atlas checklist §16)

Fixed setup for all shots: map pane 812x518 px at screen (27,214), Map Customizer defaults (see `fixtures/atlas/INDEX.md`).
The scale bar is drawn at the lower right (x 856-1007, y ~706) as alternating segments with labels in miles.

- **V1, unit of `alt` and P/m.** Graticule lines detected in open-ocean views centred on (0°, 25°W):

  | alt | grid step | px per degree | alt x px/deg |
  |---|---|---|---|
  | 6378 | 5° | 21.8 | 139,000 |
  | 4000 | 5° | 34.8 | 139,200 |
  | 3000 | 2° | 46.4 | 139,200 |
  | 2000 | 2° | 69.8 | 139,600 |
  | 1000 | 1° | 139.5 | 139,500 |
  | 400 | 30' | 349.5 | 139,800 |

  Scale is exactly inversely proportional to `alt`: **pixels per km at the map centre = K / alt with K ≈ 1252**
  (equator; 139,500 / 111.32 km per degree). Scale bars agree: alt 6378 → 400 mi = 126 px (K 1248); alt 3000 →
  200 mi = 134 px (K 1249). `fixtures/atlas/V1/`.
- **P/m (§2.1: px per km = P / (2.5·m·a), so P/m = 2.5·K).** Method: graticule line spacing near the view centre
  (lines detected as 1-px columns/rows darker than both neighbours over ≥ 40% of the pane), converted with
  111.32 km per degree of longitude at the equator, 111.2 km per degree of latitude, and 111.32·cos φ km per degree of
  longitude elsewhere; cross-checked with the scale bar and with Paris–Orléans.

  | where | pane (px) | measurement | K = px/km × alt | P/m |
  |---|---|---|---|---|
  | 0°, 25°W, alt 1000 | 812×518 | meridians 1° = 139.5 px | 1253 | **3133** |
  | 0°, 25°W, alt 400…6378 (6 alts) | 812×518 | see table above | 1249–1256 | 3123–3140 |
  | 48.4°N 2.1°E, alt 400 | 812×518 | Paris–Orléans 350.7 px / 111.0 km; N-S 3.164, E-W 3.11 px/km | 1264 | 3160 (±1%, marker centres) |
  | 60°S 30°W, alt 1000 | 812×518 | parallels 1° = 140 px; meridians 1° = 70 px (= 140·cos 60°) | 1259 | 3148 |
  | 60°S 30°W, alt 3000 | 812×518 | parallels 2° = 93 px; meridians 2° = 47 px | 1254 | 3135 |
  | 65°N 20°W, alt 3000 | 812×518 | meridians 2° = 39.3 px (= 46.5·cos 65°) | 1254 | 3135 |
  | 0°, 25°W, alt 1000, **window restored** | 720×477 | meridians 1° = 128.75 px, parallels 1° = 129 px | 1157 | 2891 |

  Conclusions: (1) **P/m ≈ 3135 (±0.5%) for the maximised 812×518 map pane**, the same at the equator, 48°N, 60°S and
  65°N. The scale on screen is the true ground scale at the view centre and is isotropic (E-W spacing of meridians
  shrinks exactly with cos φ, N-S spacing of parallels does not change with latitude). There is **no 1/cos φ
  magnification** in Encarta; a MapLibre style compiled at latitude 0 must apply cos φ itself. The 3,200 read from the
  50°N shot is the true value within measurement error, and the working value 2400 is 1.31× too small (labels would
  appear one third of a zoom step too late). (2) **P/m scales with the map pane height**: 3133/518 = 6.05 and
  2891/477 = 6.06 (the width ratio 720/812 = 0.887 does not fit the scale ratio 0.923; the height ratio 0.921 does).
  So `P/m ≈ 6.05 × pane height in px` (the §2.1 hypothesis was 4 × height). Equivalently the view has a fixed
  vertical angle of 2·atan(0.5/2.42) = 23.3° and `alt` is the camera distance in km.
  Fixtures: `fixtures/atlas/V1/ocean-*.png`, `V1/restored-window-0N25W-alt1000-s0.png`, `V2/`.
- **V2, Paris–Orléans.** At alt 400 Orléans is outside the pane when the map is centred on Paris; centred at
  48.38N 2.13E both are visible: Paris capital marker centre (482.0, 304.2), Orléans marker (380, 639.7): 350.7 px for
  111.0 km = **3.16 px/km** (K = 1264 at 48°N; N-S 3.164, E-W 3.11). Scale bar at alt 400: 30 mi = 152 px (3.15 px/km,
  K 1259). At alt 150 the scale bar is 10 mi = 136 px (8.45 px/km, K 1268); Paris–Orléans (927 px) does not fit.
  The zoom slider position differs between the two alt-400 views (Paris centre vs midpoint), so the slider is not a
  pure function of `alt`. `fixtures/atlas/V2/`.
- **V4, style id → menu name.** The style list (right panel; menu order Comprehensive, Political, Physical Features,
  Statistical, Earth By Day, Earth At Night, Tectonic, Ecoregions, Climate, Temperature (January), Temperature (July),
  Precipitation (January), Precipitation (July), Precipitation (Annual), Population Density, Languages, Religions,
  Time Zones, Outline, The Moon):

  | id | name | id | name |
  |---|---|---|---|
  | 0 | Comprehensive | 13 | Time Zones |
  | 1 | Political | 14 | Statistical (also what the menu entry selects; zooms out to a globe with the statistics bar) |
  | 2 | Tectonic | 15 | Climate |
  | 3 | Physical Features | 16 | Precipitation (Annual) |
  | 4 | not in the menu: relief, rivers and graticule only, no labels or borders | 17 | Outline |
  | 5 | Earth By Day | 18 | not in the menu: place symbols only on a pale base, no labels |
  | 6 | Earth At Night | 19 | not in the menu: the Geography Quiz map (question panel, highlighted country) |
  | 7 | Ecoregions | 20 | The Moon |
  | 8 | Temperature (January) | 21-23 | not in the menu: Moon without labels (identical) |
  | 9 | Temperature (July) | 24 | Languages |
  | 10 | Precipitation (January) | 25 | Religions |
  | 11 | Precipitation (July) | 26 | not in the menu: physical base with borders and region names, no cities |
  | 12 | Population Density | 27-33 | identical to style 0 (fallback) |

  Note for V5/V10: the checklist's guesses do not match: 10 is Precipitation (January) and 17 is Outline; the political
  map with country fills is style 1. `fixtures/atlas/V4/eu-alt3000-s<id>.png`, `V4/panel/`.
- **V3** (50N 10E alt 3000 style 0): `fixtures/atlas/V3/`. Graticule drawn; capitals with square symbols; country names
  in red upper-case; city names in mixed case with white halo on the physical base.
- **V5** (style 10 and style 1): `fixtures/atlas/V5/`.
- **V6** London alt 50 and 20: street-level city map (yellow base, major roads orange with labels "A40(M)" style
  shields, landmarks with red symbols). `fixtures/atlas/V6/`.
- **V7** 40N 30W alt 3000: the graticule is drawn over the ocean. `fixtures/atlas/V7/`.
- **V8** road shields: New York alt 30 shows US interstate shields (95, 495, 278); the Netherlands alt 30 shows blue
  rectangular A-road shields (A4, A2, A12, A27, A16, A50) and E-road shields (E30, E25, E31). `fixtures/atlas/V8/`.
- **V9** Vienna alt 800: the city is labelled "WIEN" (upper case in a box); the Danube itself carried no visible label
  at this view, only "Krems an der Donau". Unresolved for U+0004/U+0002. `fixtures/atlas/V9/`.
- **V10** style 17 at alt 20000: Outline style, not an Atlas Locator. `fixtures/atlas/V10/`.
- **V11** hover: moving the pointer over France (political) or Lake Geneva changes nothing on the map except the
  cursor (pixel diff covers only the cursor). A **left click** opens a hit-list menu of everything under the point, e.g.
  "Limoges, Auvergne, Western Europe, West Central Europe, Massif central, Loire Basin, France", each with its symbol;
  each entry has a submenu "Map / Read Article". Hovering the menu entries does not highlight the feature either.
  This is also how article links work from the map. `fixtures/atlas/V11/`.
- **V12** alt 20000: the globe fills the pane (clipped), Africa centred. Map Customizer "Flat Map View" switches to a
  world map in **Miller cylindrical**: meridians every 15° at 29.52 px (1.968 px/deg), parallels at y = 471 - 112.76 ·
  1.25·ln tan(45° + 0.4φ) (15°N at 441, 30°N 410, 45°N 376, 60°N 336, 75°N 285; all within 1 px), prime meridian at
  x = 414, map x extent 79-788 (360°, starting at about 170°W). `fixtures/atlas/V12/`.
- **V13** MinRect: Aruba alt 5 is shown at about the alt-150 scale (10 mi bar), so zooming is limited there; Venice alt 5
  zooms to a 500-yard scale bar (street level). Open ocean is likewise limited: alt 50 and alt 150 at 0°/25°W are
  identical. `fixtures/atlas/V13/`, `V1/ocean-0N25W-alt50-s0.png`.
- **V14** statistical map: classes are shades from pale peach to dark red-brown (literacy rate: higher = darker),
  water pale grey-blue, no-data countries light grey/white. Bottom bar: a rank chart "Linear by Rank", the indicator
  title and source, buttons Statistics Table / Choose Statistic / Details; hovering a country shows "<country> <value>
  (<year>)". `fixtures/atlas/V14/`.
- Place search from the map was not exercised beyond the click menu.

---

## msworld.md

- **Statistics, item column C = year span.** Hovering countries on the "Forest, annual rate of deforestation" map shows
  "Peru 0.13 (1990-2005)", "Brazil 0.54 (1990-2005)"; their rows have year 1990 and C = 15. Literacy rows with C = 0
  show "(1995)" or "(2005)". So **the displayed period is year–(year + C) when C > 0**. Confirms the hypothesis.
  `fixtures/atlas/V14/hover/`.
- **Item note** ("estimate", "N/A") is not shown in the hover readout or the Statistics Table (Rank / Country / Value
  with bars, sorted by rank or alphabetically). Column B (4 vs 8 for literacy, 16-21 for deforestation) is not visible
  anywhere in these views: unresolved. `fixtures/atlas/V14/statistics-table.png`.
- **Details** shows the indicator definition text (the indicator's content id). **Statistics Center** lists categories
  (Basic Indicators, All Statistics, Agriculture, Communications, Economy, Education, Energy and Minerals, Environment,
  Geography, Health, Population, Trade, Transportation). `fixtures/atlas/V14/choose-statistic.png`.
- **Geography Quiz difficulty names** (D1-D4): Navigator (easy), Explorer (not so easy), Globetrotter (difficult),
  World Class (most difficult). A game has 20 questions with 5 answer buttons; a wrong answer greys that button and the
  offered points drop along the D*.TXT line 2 (Navigator 5 → 4; World Class 25 → 20). Questions seen: "Which country is
  this?" (country highlighted yellow on the Earth By Day map, i.e. style 19) and "Which city is this?" (city marker).
  `fixtures/geoquiz/`. Line 3 (0 0 0 … 4 4 4): unresolved; it did not change the number of answer buttons.

## p2mtbl.md

- From the now-readable DATA*.AKC records (data cross-check, not VM): **kind 13** ids are all `group="maptrektour"`
  records (Map Trek stops such as "Extreme Heat", "Ice Sheets: July Temperature"); **kind 16** ids are `group="aboutmap"`
  records (1,135 of 1,766; landmark notes such as "Temple of Heaven", "Arc de Triomphe", "Buckingham Palace"; the rest
  are not in DataStd/DataDlx); **kind 14** is the country flag image (`group="media"`, "Afghanistan Flag"); **kind 15**
  the national anthem audio (`group="audio"`, "National Anthem of Algeria"); kind 1 is the `audionested` "Flag and
  Anthem" item. On-screen labels of the photo themes: not observed (unresolved).

## timeline.md (§6 item 3)

- Timeline items open in the native Timeline view by `msencarta://id=<item id>`. `fixtures/timeline/`.
- **Importance**: First Moon Landing (importance 1) is drawn with a larger title font than Battle of Hastings
  (importance 3); both are point events (square + title).
- **Span flag 0x01**: World War II and Han Dynasty are drawn as a horizontal orange bar from start to end with an
  upper-case boxed label ("WORLD WAR II"). Jesus Christ (flags 0x0E, no 0x01, has an end date) is drawn as a point at
  4 BC. Confirms 0x01 = draw as period.
- **Circa 0x02/0x08**: the event pop-up of Jesus Christ shows the date line "**4? BC - AD 29?**": circa is shown as a
  trailing question mark on the year, not "c.". Point labels on the timeline show no circa mark.
- **Kind 3 = essay**: "Rise of Pastoralism in Asia" (kind 3, flags 0x14, no span flag) is drawn as a two-tone striped
  bar with a thumbnail and the label "HISTORICAL ESSAY: RISE OF PASTORALISM IN ASIA". Confirms kind 3 = essay; essays
  are drawn as periods regardless of flag 0x01.
- Event pop-ups with audio/video ask for Flash (Shakespeare 1741575958).

## mindmaze.md

- MindMaze is **not reachable in the 2009 UI**: there is no feature record for it among the 27 `group="feat"` records
  (Dictionary, Thesaurus, Translate a Word x8, Timeline, College Prep Online, Geography Quiz, Encarta Kids, Quick Search
  x6, Encarta on the Web), it is not in the Features menu or Games & Fun Stuff, and opening
  `msencarta://view=MindmazeView/haspopup=0/killpopup=1` (the URL in ENCARTA.EXE) closed Encarta without a message.
  The answer location word and the category names could therefore not be observed. The question bank is a leftover.

## shockwave.md

- **City Climate and Population Chart are not launchable in Encarta 2009.** No data record in DataStd, DataDlx or the
  Kids DATA*.AKC files has `group="cityclim"`, `"citypop"` or `"director"`; the launch-filter ids (461535120,
  461549846, 461537355, 461575121) are in neither the catalog nor the title index, and opening
  `msencarta://id=461535120` shows "Page or keyword not found". No Shockwave player is needed for parity. (An official
  Adobe Shockwave 12.3.5.205 installer is still downloadable and is kept in `vm/downloads/shockwave/`, unused.)
  `fixtures/shockwave/461535120-noplayer.png`.

---

## VM helper changes (vm/, local)

- `capture.py`: `--max-pages` (default 30) and a `truncated` flag; `--kids` for the Kids instance; a generic scroll-pane
  finder (Kids `textscrollarea`, sidebar iframes); frame documents appended to `article.html`/`.txt`; the id check
  accepts any URL form (`msencarta://id=`).
- `mapshot.py`: open an atlas URL and screenshot after the view settles (+ JSON with the URL).
- `tour.py` + `guest/tourhost.cs`: our own host for the 3DX control (Do, events, timed sampling while keys are held
  through QMP). Needs `hostfwd_add` for port 7778 after every start (`tour.py setup` does it and rebuilds).
- `vmctl.py` honours `VM_QMP_TIMEOUT`; `savevm.sh` uses 900 s (a 1 GB snapshot took longer than the old 30 s).
- New live snapshot **`tours-ready`**: Encarta on its home page, tour files extracted to `C:\tours\<nnnnn>\`,
  tourhost built in `C:\tools`.
- Sound (stretch item 4): not attempted.

---

## UI parity pass (2026-10-14): menus, dialogs, panoramas, Map Trek, index, quiz, tour pitch

Captures in `fixtures/ui/` (subfolders `vtour/`, `panorama/`, `maptrek/`, `kids/`, `items/`, `print/`, `copy/`, `help/`,
`favorites-files/`), `fixtures/geoquiz/1*.png`, `fixtures/geoquiz/nav-*`, `fixtures/tours/701505917/sitemap/`,
`fixtures/tours/701504257/pitch-limit-log.txt` and `encarta-music-*.png`. Not yet listed in the fixtures' INDEX.md files.
Started from `tours-ready`; the guest now also has a file printer "PS" (HP LaserJet 4000 PS driver, port `C:\out.ps`)
and `C:\tools\clipdump.exe` (our clipboard dumper); no snapshot was saved with them. Measurements only, wording paraphrased.

### Features menu (`features-menu.png`, `.html`, `features-flyouts.html`)
- An HTML flyout 292 px wide under the Features button, vertical gradient #E2F0D9 → white. Three groups, each with a
  picture on the left, a bold green (#5B9510, 15 px) heading and plain entries; a hovered entry gets a green rounded
  highlight with white text. Each group ends in a "More" entry with an arrow that opens a second flyout after a 100 ms
  hover. A "Browse all" entry (Tahoma 14 px, green) at the bottom opens a "Features" browse page (`features-browse-all.png`).
- Order and content ids: **Dictionary & Foreign Language Tools**: Dictionary 701712323, Thesaurus 701712324,
  Translations 701767016, World Languages Interactivity 701878967. **Geography Tools**: World Atlas 701713739, Quick
  Search: Maps 701766739, Country Facts and Figures 701713705, More → World Statistical Map 701713537, Flags of the World
  701713732, Map Treks 701713686, U.S. State Facts and Figures 701713708, Canadian Province & Territory Facts and
  Figures 701713706. **Explore**: 3D Ancient Sites 701713683, Encarta Kids 701712340, Panoramas 701713680, More →
  Timeline 701779281, Quick Search: Photos & More 701779282, Virtual Tours 701713684, Videos 701713681, Encarta on the Web
  701835573. Browse all → 701766702. (A third flyout 701780438 is registered but held only World Languages Interactivity.)
- Most entries are browse pages (home-page records); Geography Quiz and Games are not in this menu.

### Menu bar (`menubar-*.png`)
- File, Edit, Favorites, Help, classic Win32 menus. File has a "Go to" submenu (`menubar-File-goto.png`), Print and
  Options (Ctrl+O). Edit: Copy, Read Aloud submenu (`menubar-Edit-readaloud.png`), Find in Article. Favorites: Add to
  Favorites…, Organize Favorites…, a separator, then folders as submenus and favorites as items.
- Help (`help-menu-1..6.png`): Help (F1) opens the HTML Help window on ERL.CHM; an "Encarta Search Bar" dialog; a further
  CHM topic; an entry that opens Internet Explorer (online page; fails offline); Customer Support Options (PSS.CHM in
  an HTML Help window); About Microsoft Encarta Premium 2009 (dialog with Credits / Contributors / Consultants buttons
  that switch a scrolling list, `about-*.png`).
- Keyboard shortcuts (from the help topic): Alt+Home home, Alt+←/→ back/forward, Alt+T Features menu, Ctrl+S search box,
  Ctrl+C copy, Ctrl+P print, Ctrl+M atlas, Ctrl+D dictionaries, Ctrl+Q geography quiz, Ctrl+Z MindMaze (dead in 2009),
  Ctrl+O options, Ctrl+F find, Ctrl+L closed captions, Ctrl+U update, Ctrl+T timeline, timeline +/− zoom, panorama
  Shift/Ctrl zoom, F8/F9 read aloud start/stop.

### Options dialog (`options-1..5.png`)
- Window "Options" (#32770), white, about 676×411 at (175,165). Left: a list of five pages (General, Encarta Updates,
  Closed Captioning, World Atlas, Quality) on #F7FBFF with a #424142 frame; the selected row is #7396BD with white text.
  Right: a two-tone heading ("<page>" in light #C6D7EF + "Options" in #7396BD), #C6D7EF rules, section labels in
  dark #21304A, OK/Cancel buttons with a #7396BD border at the bottom right.
- Controls: General: play startup sound (check), play menu and button sounds (check), article text size Small / Medium /
  Large (radio). Encarta Updates: a check-for-updates button. Closed Captioning: one check box (captions for audio and
  video). World Atlas: always linear statistical data (check), crosshairs at map centre (check), lat/long format
  degrees-minutes / degrees-minutes-seconds / decimal (radio), units metric / U.S. (radio). Quality: CEIP check box.

### Help system (`help/`, `help-f1.png`)
- F1 and Help › Help open an HTML Help (HH) window titled "Help" (contents tree left, topic right, toolbar Hide / Back /
  Forward / Print, tabs Contents and Search). Source: `ERL.CHM` (87 files, 65 kB; the Kids copy is byte-identical);
  `PSS.CHM` is a single support-contacts page. TOC (`help/erl/ERL06.hhc`): Getting started (6 topics), About tools and
  features (6), Exploring tools and features (6), Copying information (5), Printing information (5), Accessibility (5),
  Troubleshooting (9), Glossary. Extracted text: `help/erl-text.txt`. "Page not found" pages also pop the Help window.

### Search box, ▾ tab, Quick Search (`search-*.png`, `quicksearch-*.png`)
- Autocomplete is a word-wheel dropdown under the box while typing. Enter opens a **results panel docked at the right**
  (x≈650–1024, full height) with collapsible category bars Best Matches / Articles / Photos & More / Web Links /
  Dictionaries / Maps (Quotations appears when present), each with an icon and "(count)"; one category is open at a
  time (+/− at right). No-result categories show a short message and a "Search all <kind>" link. Two check boxes at
  the bottom: automatically display best match; keep search results list open.
- **The ▾ tab under the search box toggles this panel** (▾ shows it, ▴ when open). With "keep open" off the panel hides
  when you click the page; it does not hide on mouse-out alone (`search-closed-on-page-click.png`).
- Quick Search (Features › Quick Search: Maps / Photos & More) is a small modal dialog titled "Quick Search" with one
  text box, autocomplete and Go; Enter closes it and opens the same right panel with only that category
  (`quicksearch-maps-result.png`).

### Print and copy (`print/`, `copy/`, `print-*.png`, `copy-dialog-*.png`)
- File › Print / Ctrl+P on an article first shows Encarta's own dialog: radio Whole Article Text / Whole Article with
  Media / Selected Text (enabled only with a selection), Print and Cancel; for a picture: Picture / Picture plus caption.
  Articles longer than 5 pages ask for confirmation (`print-longer-than-5-pages.png`), then the Windows Print dialog.
- Output (`print/zebra-text.pdf`, 5 pages; `zebra-media.pdf`, 7 pages): running header with the title left and "Page n"
  right; title, numbered section heads, paragraphs, "Contributed By" + name; a copyright line after the text and again
  as page footer. With media, pictures are placed beside the text with captions.
- Edit › Copy shows the same style of dialog (Whole Article Text / Selected Text; for pictures Picture / Picture plus
  caption). Clipboard: Text, Unicode, HTML Format (SourceURL `msencxml://content/<id>?…&display=copytext`, own print
  style sheet), RTF. Plain Ctrl+C on a selection copies the selection **plus** a bold copyright line as a new paragraph
  (HTML) / new line (text). Copy and print always append that line.

### Favorites (`favorites-*.png`, `favorites-files/`)
- Add to Favorites: dialog with a Name box (default "Article - <title>" for articles, the plain title for media), a folder
  tree ("Favorites" root), New Folder… (sub-dialog "Create New Folder"), OK/Cancel. Organize Favorites: tree with
  context menu and tooltip buttons for new folder / rename / delete / move.
- Storage: one `.erl` file per favorite under `%APPDATA%\Microsoft\Encarta Reference Library\Favorites\<folders>\`, in
  Internet-shortcut INI format: `URL=msencarta://view=ERSArticleView/display=article/id=<id>/xst:secstate=,1,/sku=E09AXLRD`
  (media: `view=ERSMediaView/id=<id>/sku=…`), IconFile = ENCARTA.EXE, plus an `[Encarta]` section `TiconIndex=1`.
  Folders are real directories and show as submenus of the Favorites menu.

### Parental controls (`dict-*.png`, `search-bitch-*.png`)
- Set only in the Encarta Dictionaries window (its "Parental Controls" button): a small dialog with one check box and
  OK / Apply / Cancel. Stored machine-wide as `HKLM\SOFTWARE\Microsoft\Microsoft Reference\E09AXLRD\EDICT`
  `ParentalControls` DWORD (1/0); help says it needs Administrator or Power User rights and the list is fixed.
- Effect: flagged headwords disappear from the dictionary word wheel and lookups (a lookup of a flagged word lands on the
  nearest unflagged entry, e.g. a derived adjective). Entries that merely carry an "offensive" sense label but are not
  flagged stay. The main search's Dictionaries category behaves the same. **Nothing else is filtered**: articles, web
  links and media are unaffected (also stated in the help).

### Map Trek (`maptrek/`)
- Features › Map Treks opens a browse page of trek categories (one per atlas style); a trek opens the atlas at stop 1
  **and a modeless dialog "Map Trek"** (HTML, brown #765C26 body, `maptrek-dialog.html`): right-aligned banner "map trek"
  (Tahoma 23 pt, #CEB57B) over a rule; left a white text box (trek title bold #765C26, then the stop: 64×64 thumbnail
  with a caption under it, bold place name, text); right a white stop list (Tahoma 8 pt, current row white on #A9A9A9)
  with a dark footer "Stop n of N"; buttons New Trek (left), Back, Next (right), brown with #CEB57B border, disabled
  ones with #CEB57B text.
- Changing stop navigates the main window to `msencarta://view=MapView/cmd=map/lat=…/long=…/alt=…/style=…`. New Trek
  closes the dialog and returns to the Map Treks page.

### Panoramas (2D virtual tours and singles)
Measured on Westminster Abbey (461545486), Mont-Saint-Michel (461549315) and Alcatraz (461545621); `vtour/`.
- **Layout.** Tour page: green (#B5CB94) panel at (194,173)-(832,676); view (picture area) **624×289 px** at screen
  (200,192)-(824,481) with a 1 px black frame; menu bar "Tour Stops | Tour Settings | Credits | Appears in" above it;
  "360 VIEW" caption under it; site map at the lower left (≈190×146); scene title and scrolling text at the lower right;
  tour title large white on black at the bottom. Tour Settings has two checked toggles: show labels and hot spots;
  auto-rotate 360 views. Tour Stops: one entry per scene (four-way arrow icon) followed by its hotspots (picture, audio
  icons); the current scene has a bullet. Credits opens a small window; Appears in a list menu.
- **(a) Opening yaw.** Every scene measured opens with the **picture's left edge (x = 0 = W) at the view centre**
  (view centre matched at x = W−4 in all cases), whatever `panangle` is: Nave and Sanctuary (panangle 0), Alcatraz
  Broadway (0), Mont-Saint-Michel On the Island (250) and Cloister (30). So `panangle` is not the opening yaw. It gives
  the map orientation instead (see (e)). Singles: the `.svj` ActiveX viewer (REFSV.DLL, CLSID 8399E3C5-…) stays black in
  this VM, and the Panoramas feature list opens the Flash versions (Flash missing), so `initangle` could not be measured.
- **(b) Projection, FOV, horizon.** Planar perspective from a cylinder: the horizontal shift between frames grows from
  26 px at the view centre to 44 px at the edges, fitting `dx = A + B·x²` with f ≈ 297 px (Westminster, W = 1824). The
  scale at the view centre is 1:1 with the picture (cylinder radius = focal length = W/2π: 290 px for W 1824, 229 for
  1440). So the FOV is **not fixed**: hfov = 2·atan(312/(W/2π)) ≈ 94° for W 1824, 107° for 1440, matching the site-map
  wedge widths (92–95°, 105°). The view shows 290 picture rows centred on the **middle row** (rows 46–335 of 380,
  77–366 of 443, 84–373 of 456): horizon = middle row. A vertical drag shifts the window up to the picture top (row 3
  reached); no zoom was seen with Shift/Ctrl.
- **(c) Auto-pan.** On by default when a tour opens (the setting then persists for the session). The view jumps in
  discrete steps 7–8 times per second (6–7 px, sometimes a double step) for ≈ 20°/s: **one full turn in 18.3 s**
  (Westminster, W 1824, and Alcatraz, W 1440 alike, so it is angular). Direction: picture x increases (view turns right).
  A click on the view pauses it; it resumed by itself about 9–10 s later. Dragging: rotation speed proportional to the
  pointer's horizontal distance from the press point, ≈ 4 px/s per px (10 px → 38 px/s, 25 → ≈ 98, 50 → ≈ 176,
  100 → ≈ 380 px/s), pointer right of the press point turns right.
- **(d) Hotspots and labels.** All hotspots are **always visible** (no hover needed): a box with black fill, 1 px border
  and text in light green (181,203,148), Tahoma ≈ 8 pt, a type icon at the left (square-in-square picture, speaker
  audio, four-way arrow scene link) and the hotspot title. Hover: border and text turn white, hand cursor. The box sits
  just above / beside the `quadrant` area (Richard II: box bottom row ≈ quadrant top). Text labels (`label` x,y) are
  bold white with a black outline plus a thin white leader line from the text down to the object. A one-line bold
  white instruction across the top of the view is removed after the first drag.
- **(e) Site map.** Yes, there is a **view wedge**: peach tint (247,227,214) from the current scene's point, as wide as
  the hfov (≈ 94°). Its screen direction (clockwise from map-right) = **view yaw − panangle**, where view yaw = 360·x/W
  of the picture column at the view centre: Nave 0° → −2.4°, 10.1° → 9.2°, 65.3° → 67.0°; On the Island (250) opens at
  110° (= −250), Cloister (30) at −30.7°. Points: scenes are green-bordered boxes with a four-way arrow, pictures small
  grey squares, audio a speaker; hovering one shows a black tooltip with its title.

### Kids chrome and home (`kids/`)
- Window "Microsoft Encarta Kids 2009". Top bar lime green (#9CE721 → #B5F331) with round buttons: Home (house), Back,
  Forward, Copy, Print (each with a yellow tooltip and a small arrow), a menu bar File / Edit / View / Favorites / Tools /
  Help (dark green #006500 text) and a "search" box with a green arrow; Encarta Kids logo at the right. Menus are lime
  (#B5F331) with dark green text: File Print/Exit; Edit Copy/Find in this Article; View Go to ▸, Closed Captioning ▸;
  Favorites Add/Organize; Tools Find, Read Aloud ▸, Menu and Button Sounds ▸, Startup Sound ▸; Help Help (F1),
  Troubleshooting ▸, About Encarta.
- Home: orange swoosh background, logo in the middle, ten yellow tilted panels with pictures (Animals, Science, Sports
  and Recreation, The Arts, Reading and Writing / People, Places, History, Social Studies, Games and Fun Stuff!).
  Clicking a panel highlights it blue and opens a blue rounded list of subcategories with icons in the middle; a
  subcategory opens a page with the category title in orange, the subcategory list at left (current one highlighted)
  and a 7-column grid of 64-px tiles (photo or a generic "book" icon) with captions.

### Other item views (`items/`)
- Web link (762518793): opens Internet Explorer on `encartaupdate.msn.com/teleport/teleport.aspx?…&tname=weblinks&…&ud=<id>`
  (a redirector, not the stored URL); offline it fails. Encarta shows no page of its own.
- Quotation author (1861680808) and contributor (761601779) ids: "Page or keyword not found" (no view of their own).
  Authors appear only inside quotation pages (green header band "quotations", category, quote, rule, author name with
  dates, description, source). Contributors appear as a popup "Credentials" (#A09683 brown, name with degree bold, one
  line of credentials) from the article's "Contributed By" link (`contributor-popup.png`, `.html`).
- Topic timeline (461575603, 701767509): opens the native Timeline view zoomed to the topic's span with the topic title
  large and grey at the lower left; "View as list" shows the topic's events 25 at a time with dates (circa as "?"),
  title and text, and previous/next 25 buttons.

### Index (`index-*.png`)
- No index browser exists (no menu, feature or view). Index terms appear **inside search results**: the phrase
  "people buried at the abbey" gives a Best Matches entry "Westminster Abbey – people buried at the Abbey" with a (+)
  that expands in place into sub-terms (composers, explorer, …), each expanding to article leaves (title + snippet).
  Opening a leaf opens the article at the paragraph (`jump=p1&mark=1`) with that paragraph's background **#FFFAB8**.
  Opening an index-term id directly gives "Page or keyword not found".

### Tour Music (3D tours, `fixtures/tours/701504257/encarta-music-*.png`)
- The Music entry has a speaker icon and toggles between a pressed look (lighter blue box) and normal; nothing else
  changes visually. No sound device exists in the VM, so what plays was not heard. The data has one
  "<tour> 3DVT: Ambient Audio" record per tour (701504966–701504972, a 88–105 s WMA each), which no tour record links;
  they are the likely source. **Done in the client:** Music loops the tour's ambient record (off at start, which is a guess).

### Geography Quiz, full Navigator game (`fixtures/geoquiz/1*.png`, `nav-*`)
- Start dialog "Geography Quiz": name box, four difficulty radios, OK/Cancel. Map (style 19) with a locator globe, a
  zoom slider (9 ticks, −/+) and hand / zoom tools. Question types seen: country (highlighted yellow), city (ring
  marker), continent.
- **After a correct answer** the next question appears within one frame (< 80 ms): no feedback screen, no pause; the
  map recentres for the next question. After a wrong answer the button greys out and the points drop (5 → 4).
- **End:** after question 20 the clicked button turns yellow (255,211,82) and a modal "Geography Quiz - Scores" dialog
  opens at once: heading "scores", a high-score table (rank, name, points, level; 19×5 + 4 = 99 here), buttons Exit and
  Try Again. Try Again reopens the start dialog with the name kept; Cancel there returns to the previous page.

### Tours: pitch limit and rotated site maps
- **PITCH? per tick** (tourhost, Acropolis, PITCHSPEED 6, four yaws; `fixtures/tours/701504257/pitch-limit-log.txt`):
  steps are 6° ± 1.2·10⁻⁴ (float noise). A step is taken only if the **resulting** |pitch| ≤ 45: from 38.9 it reaches
  44.9; from 39.0001, 39.001, 39.01, 39.1 it is refused; from exactly 39 (read back 39.000000) it is accepted in 9 of
  12 runs and refused in 3, depending on float noise at different yaws. So the rule is `|p + Δ| ≤ 45` on the
  float-noisy value, with no extra tolerance. `CAMERA.PITCH=` with |v| ≥ 45 stores ±44.9; 44.95 is kept.
- **Rotated site map (Persepolis, `SM_ROTATEFRAME=-90`, `fixtures/tours/701505917/sitemap/`).** §4.2 confirmed: at
  Apadana Stairs the marker centre is exactly at the predicted (96,118) in window coordinates, and at the other three
  stops it sits on its location box. The wedge (≈ 80° wide) points at the predicted heading at all four stops
  (expected 102/180/72/196°, measured 102.2/177.5/71.2/194.5°).
- **Highlighted-location look:** when the marker is on a location box (Terrace Stairs, Gate of Xerxes, Apadana Interior)
  the four-way marker is drawn in light blue **0x6371B5** instead of dark blue 0x103463; at Apadana Stairs (no box) it is
  dark blue.

### Not done in this pass
- Azuchi site map; single-panorama `initangle` (viewer black, Flash missing); panorama zoom; what the Music toggle
  plays (no audio device); the Options dialog was read from the captures and the help, not each control exercised;
  INDEX.md entries for the new captures; no new snapshot saved.
