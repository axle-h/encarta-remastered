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
- **Superseded (2026-10-15, debugger):** the software device runs **gravity mode**; the "next sample" was simply
  more than 20 ticks later. See "Flash and debugger pass" below.

### §6.4 / §4 Floors (checklist "Floors")
- Roman Forum, MOVETO to (11.05, -, 2.6) and walking north up stair 1: the walk-map cell 657 enter macro fired
  `USERLEVELCHANGE:QTSREFID=701505293` at the MOVETO, Y followed the steps across the 1stFloor → stair1 switch with no
  visible discontinuity, and the walk ended against a wall at the landing (z = 3.8899). Backed by
  `fixtures/tours/701505285-stairs-walk.txt`. Ordering of enter macros that switch walk maps: see "Tours pass 2" below
  (one switch per crossing, no switch back).

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
  is a plain MOVETO to the start pose (Acropolis, Persepolis, Abu Simbel). The "Show Labels and Hot Spots" item: see
  "Tours pass 2" below (not in the 2009 UI).
- **Caption colours.** Normal caption text and frame read back as (198,211,231) on the 16-bit screen, which is 0xC0D1E3
  quantised to RGB 565; black fill; the hovered caption turns text and frame white (255,255,255); the pointer over it
  is a hand cursor. `fixtures/tours/701504257/harness-hotspot-hover.png`. Confirms §8.5.3 colours.
- **Display pixel format (§5.9).** The green value 0xD3 above can only come from 6-bit green: the VM display is
  **RGB 565**.
- Caption pixel widths: measured, see "Tours pass 2" below.

### §7.2.6 Flicker (checklist "Flicker")
- At the Athena Parthenos stop the lit strip seen through the Parthenon doorway changes brightness between frames
  sampled 105 ms apart (mean luminance of the strip jumps among 94-118 with no smooth transitions), while the rest of
  the view is static. Consistent with step changes every 0.1 s within a range of the base colour.
  `fixtures/tours/701504257/anim-athena-parthenos/`. The 0.4 s cadence and the steady torches: see "Tours pass 2" below.

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

### Tours pass 2 (2026-10-15): the §10 open items (tourhost, software device; `fixtures/tours/*/README.txt`)
- **Flicker (§7.2.6).** `LIGHTS.<id>.COLOR?` returns the current flicker colour, so it was sampled every 5 ms: the 0.1-s
  `_TORCH1` changes every 100-118 ms (mean 109 ms = 7 ticks of 15.6 ms), the 0.4-s `_TORCH2` every 402-421 ms (26 ticks):
  the accumulator restarts at 0, so periods round up to whole ticks. Values are `trunc(k × saved)` with k down to 0.4.
  Persepolis and Colosseum have no `_TORCH` lights (`COLOR?` and `FLICKER?` answer `Failed`) and no flames; two dumps
  0.3 s apart at every stop of both tours are identical. Abu Simbel `_TORCH36` (stored with a trailing space) and
  `_TORCH39` answer `Failed` too, so they never flicker; `_TORCH35`/`37` answer their colour and `FLICKER?` 0.4.
- **Lighting (§7.2.3, §5.4).** With `LIGHTS.OFF` the Parthenon columns (material faceColor 0.29) and walls (0.22) show
  their texels unchanged (~230 against texture means 225-231): **faceColor is ignored**. Lit, the unlit floor is
  texel × 55/255 for `AMBIENT` 60 (565 rounding): modulate, no overbright. Abu Simbel (ambient 0): `SCENE.AMBIENT=60,60,60`
  after loading brightens every surface and turns 1,351 black pixels non-black, so ambient is the base of every lit
  vertex and faces no light reaches are black at 0.
- **Water (§7.4.6).** In acr.3wm cells 1145-1148 (`3D.n_00water.WATER?` = 1) the pond changes in 30 % of its pixels
  between dumps 0.2 s apart, with lights off; at z 3.3, just outside (`WATER?` = 0), 1 pixel. Ripple rate not measured.
- **Sprites (§7.4.1).** Torch_19 seen from yaws 0-315 in 45° steps is always face-on (a wrong sign would turn it
  edge-on at 45°). At yaw 0 and pitch −40, `3D.Torch_19.SPRITE=1` and `=0` give identical images (rotation by yaw only,
  identity at yaw 0, no pitch); at yaw 20 they differ.
- **Film strips (§7.4.4).** Two torches (Erechtheum Torch_09/10, `16,.07`), dumps every 12 ms: frame changes 72-96 ms
  apart, mean 80.6 ms (5 ticks, about 12.4 fps, not 14.3), on the same ticks for both torches, which show different
  frames (random start frame).
- **Cloud statues (§7.3.4).** With `SCENE.FOGSTART=9.95`/`FOGEND=10` the Bronze Athena is re-rendered at 8.95 units and
  frozen at 9.05 (= 0.9 × FOGEND): seen from the west at 9.5 it shows the image of the last render (the south view, or
  after a render at 8.5 from the west, that view, also from the south). Software mode re-renders on each draw (the image
  changes between 1° steps around the statue). The hardware 5° rule cannot be seen (no 3D device).
- **LOD (§7.6).** `CAMERA.FOV=6` on newcolumn07 from the south, eye 1.36 above it: the column turns into its darker
  `lod_` mesh between XZ distance 12.99 and 13.01 (a 3-D distance rule would switch at XZ 12.93).
- **PVS (§6.6).** The six Acropolis stops before and after `WALKMAP.DELETE.VISIBILITY` match except at Erechtheum (and a
  few pixels at Bronze Athena): with the PVS, the far Parthenon columns are the `lod_` meshes; without it, the full
  columns (5.2 units away). The cell's list names `lod_newcolumn*` for most columns and `newcolumn30-39` for others: **over
  a visibility cell the lists choose base or `lod_` meshes, with no distance swap.** Sprites in no PVS list are drawn:
  Torch_19 shows from cell 1508, whose list lacks it.
- **Back faces (§5.9).** `WALKMAP.OFF`, eye below the terrain looking up: the terrain and floors vanish (sky behind),
  only walls facing the eye show. Back faces are culled.
- **Cursors (§6.9, §8.5.3).** Read with GetCursorInfo (tourhost `cursor`): the centre cursor for |dx| ≤ 29 and
  |dy| ≤ 59 px from the control centre, left/right when |dy| ≤ |dx| (also on the exact diagonal), up/down otherwise.
  Pressing and dragging in the view keeps the zone cursor; `grip.cur` never appeared. Pressed on a caption the hotspot
  cursor stays while dragged off it, the camera does not move, and releasing elsewhere fires nothing.
- **Captions (§8.4.2, §8.5).** System font widths captured (GetCharWidth32 on the control DC, face "System",
  `fixtures/tours/701504257/captions/`); with them the §8.4.2 rule gives all 55 hotspot-box widths in the viewpoint
  captures, "Titus" (Wm 32) only with the scale as float32 (52, not 53). A caption shows only strictly inside the view:
  last drawn at left 1, right edge 614 (W − 2) and bottom 418 (H − 2); one that touches or crosses an edge is dropped and
  cannot be clicked. The cone: with `CAMERA.FOV=100` "Parthenon Frieze" is shown at dot 0.8729 and hidden at 0.8692
  (threshold 100·π/360 = 0.8727, 29.2°); hidden this way, its last rectangle still gives the hotspot cursor and a click
  fires `HOTSPOTCLICKED`. At FOV 80 the cone (45.7°) lies just outside the view's corners (45.4°). Icons show upright
  (TV, arrows); `_L` labels are white bold with a 1-px black rim, aliased.
- **"Show Labels and Hot Spots".** Not in Encarta 2009's tour UI: the menu bar is Music, Tour Stops, Start Over,
  Appears in, Credits; the Tour Stops menu lists only stops and hotspots; right-clicking the control shows nothing, and the
  File/Edit menus have no such item. The strings `To&ur Stops`, `&Show Labels and Hot Spots`, `Tour &Settings` remain in
  ENCARTAR.DLL.
- **Floors (§6.5).** Roman Forum stair 1, active map read every ~10 ms (`WALKMAP.[9].OVER?` / `[28].OVER?`, collision
  cells that exist only in 1stFloor / stair1): walking up, the map switches once on entering 1stFloor cell 614 (enter =
  load stair1) and stays stair1; walking down, once in stair1 cell 29 and stays 1stFloor. The old map's exit macro does
  not switch back: the runtime's `settle` rule.
- **Open edges (§6.7).** Roman Forum 1stFloor, x 11.794: collision cell 9 ends at z 7.744 (`OVER?` TRUE at 7.7, FALSE at
  7.8), the nearest wall is 0.72 away; walking north crosses it at full speed and goes on. Leaving the coverage does not
  stop a move.
- **Loading spinner (§8.4.3).** Dumps every 15 ms during an Acropolis load: the dot steps around the ring 16 times,
  90-120 ms apart, mean 110 ms (7 ticks: the 0.1-s alarm restarts its clock when it fires); the loading screen lasts
  2.0 s (345 ms to 2,340 ms).
- **Hotspot-string junk (§6.2).** Cells whose strings carry junk (acr cell 1055, the Roman Forum stair bands) behaved as
  their clean scripts say in every walk above; no stray effect was seen.
- **`LIGHTS.TOGGLE`** answers `Success:-1`, `LIGHTS.ON?` stays TRUE and the view does not change (§7.2.4 confirmed).
  **`3D.TOGGLEHIDDEN`** flips `VISIBLE?` of every object (1 → 0 → 1), but the first toggle left the Parthenon view
  unchanged and a second one (after hiding Sky03 and newcolumn05) brought those back and hid the ground: not understood.
- **Quirk:** after loading the Roman Forum in tourhost, the stop command's `MOVETO=160,…` left yaw `-1.#IND00`; only
  `CAMERA.HOMEPOV` (then `CAMERA.HEIGHT=.16`) cleared it.

### Not resolved for tours
- Pitch-limit arithmetic (needs a debugger), gravity mode and the hardware 5° cloud rule (no 3D device), maximised
  (MaxView) mode, the water ripple rate, `3D.TOGGLEHIDDEN`.
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
- **Hit sides** (2026-10-14, second pass; `fixtures/atlas/sea-click2/`, each click from a freshly opened view, checked
  unchanged before the click: a click that hits nothing re-centres the map). Menus, with our band polygons at the
  point (`vm/scratch/ptquery.py`: screen → lat/long by the orthographic globe of radius 3135·6378.2/(2.5·alt) px around
  the pane centre (433, 473), then our tiles):

  | view | point | menu | notes |
  |---|---|---|---|
  | globe 0°, 150°W, 20,000 km, s0 | open Pacific | Pacific Ocean, World | on land: United States, North America, World |
  | Tectonic (s2), 35°N 60°W, 10,000 | Atlantic, west / east of the ridge | North American Plate, Atlantic Ocean / African Plate, Atlantic Ocean | on land: North American Plate, United States, North America |
  | 17°S 146.5°E, 400 km, s0 | open water over the reef area | Great Barrier Reef Marine Park, Great Barrier Reef, Coral Sea | park (side 0) over water |
  | same | 70 km off Queensland | Coral Sea | our Australia and Coral Sea Islands Territory polygons cover it |
  | 47.5°N 3.2°W, 300 km, s0 | Belle-Île, land inside our Bay of Biscay polygon | Sauzon, Morbihan, Brittany, Belle-Île, France | no Bay of Biscay; Quiberon peninsula alike |
  | 49.21°N 2.20°W, 300 km, s0 | 0.7, 1.7, 4.8, 8, 11 km west of Jersey (sea) | Jersey (U.K.) up to 4.8 km, Channel Islands up to 11 km, British Isles | as far as our band-7 polygons reach (Jersey's ends between 4.8 and 8 km, Channel Islands' between 11 and 14 km); no water polygon there |
  | same, 2,000 km | 1.6 … 37 km west | Jersey (U.K.) at 1.6 km only; Northern Europe to 27 km; English Channel at 37 km | |
  | 47°N 5°E, 8,000 km, s0 vs Time Zones (s13) | 10 sea points 3–6 px off coasts | the same water body and statistical regions in both; Europe never with a water body; "Eastern Europe, Europe" off Klaipėda (no water polygon) | Italy and Greece listed over the sea in s0 only where their drawn labels are (Time Zones draws none) |

  Reading: PDCAltitudeGrid `b4` high nibble is the hit side (mad-styling.md §4.4): 2 (water bodies, reefs) listed
  only over water; 1 (countries, divisions, continents, islands, regions of pen group 33) left out when a water
  class is listed; 0 (World, plates, parks, statistical regions, lakes) anywhere. Labels are hit as well. The earlier
  reading "no land area over the sea" (Brittany) is replaced.
- **Point label positions** (`fixtures/atlas/labels/outback-s0-alt800.png`, Brittany 300 km): labels sit diagonally
  off the symbol, text corner at the symbol corner. Isolated places (7 of 7 in the outback) are above right; in
  Morbihan about 18 of 30 are above left, 5 below right, 5 below left, the rest above right. Lambina (SOUTH AUSTRALIA
  above it) is below right. The order after above right is not determined.
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
  this VM, and the Panoramas feature list opens the Flash versions (Flash missing then); `initangle` was measured later
  with Flash installed: "Flash and debugger pass" below.
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
- **Bar layout** (client pixels, matched against `ENCARTAR.DLL` bitmaps at < 1 % RMSE; 2026-10-15): 71 px tall, #9CE721
  for the top 21 px (the menu band) over the 2×50 tile 15050. Left cap 15060 at x 0; Home 15051 at 10, Back 15052 at
  76, Forward 15053 at 142 (all from y 0, 71 tall); joint 15054 at 178; Copy 15055 at 184 and Print 15056 at 237 (y 21,
  50 tall) with the fillers 15071 (230) and 15072 (283) between and after them; the "search" label 15057 at 307, the
  box (the middle of 15062 stretched over 417-611) and the arrow 15058 at 611, y 22; logo 15059 at the right edge.
  Button bitmaps are strips of four states: normal, mouse-over, disabled, pressed. Back, Forward, Copy and Print are
  disabled (third state) on the home and category screens. Menu titles: Tahoma 11 px at x 194-470, the open one on
  #B5F331 with no border.
- **Category screen** (`02-animals.png`): the category's own 1024×600 `hsbg` already shows its panel highlighted and
  the blue board; the other nine panels carry their pictures as on the home screen. Subcategory icons (each
  subcategory's `hthm` picture, 44×42) at x 249 from y 27, 42 px apart, titles 3 px after them, white, regular 15 px
  (the template `mainhome_kids.xsl` says bold; the VM draws it regular), #FFB400 under the pointer.
- **Subcategory screen** (`03-mammals.png`, template `homescreens.xsl`): white page with the 800×520 "Subcategory
  Background" at the top left; title 40 px #FF8400 at (9, 10); subcategories with icons at x 10 from y 74, 44 px
  apart, text from x 63 (176 px cell); the current one, and the one under the pointer, on the 241×42
  `KIDS_SUBHS_SUBCAT_SEL` arrow in #FF9600. Grid from (239, 66): tiles 106×120, the 64-px picture with a 1-px black
  frame 18 px in and 5 px down, a 13 px black caption (100 px wide, at most 3 lines); under the pointer the tile gets
  the `KIDS_SUBHS_TOPIC_SEL` card and a white frame. The scroll bar is green (face #8EE01E, track #A7EC2B). Every tile
  starts as the `KIDS_UNKNOWN_BG` "book" and swaps in its thumbnail once that has loaded, so the capture's books on
  Aardvark, Armadillo, Lemurs, Big Cats … are thumbnails not yet loaded (all of them have one). Clicking another
  subcategory switches the grid inside the page (no history entry).

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
- The Music entry has a speaker icon and toggles between a pressed look (lighter blue box) and normal.
- **Heard (2026-10-14, USB audio in the guest, matched by envelope correlation):** it is **on** when a tour opens (pressed
  look) and starts when the world has loaded. It plays the tour's "<tour> 3DVT: Ambient Audio" record (701504966-701504972,
  88-105 s WMA; Acropolis correlation 0.81-0.98, Azuchi 0.98), which no tour record links. Off stops it; on again restarts
  it **from the beginning**; at the end it loops. Done in the client.

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
- **Marker colour: it blinks.** Sampled every 6 ms (Azuchi, on the Grounds box and after walking off it), the four-way
  marker alternates between dark blue 0x103463 and light blue **0x6371B5**, each phase about 0.715 s (0.706-0.724). The
  earlier single screenshots (light on three Persepolis boxes, dark at Apadana Stairs; mixed at Azuchi) just caught one
  phase, so there is no "on a location box" look. Done in the client (`MARKER_BLINK_MS`; steady with reduced motion).

- **Azuchi site map (`SM_ROTATEFRAME=-90`, `fixtures/tours/701505920/sitemap/`).** Marker centres in window coordinates,
  measured/predicted: Grounds (139,157)/(139,157) (clamped), Donjon (100,153)/(100,153), Living Quarters (102,139)/(102,139),
  Octagonal Chamber (88,153)/(89,153). Wedge centres 183.5/177.5/122.5/171.5 against 185/179/124/172 degrees. Captions and
  elevation per stop: Level 1, Level 1, Level 3, Level 6, matching each stop's site. No client change needed.

### Search ranking (8 queries, `fixtures/search/<q>-best.png`, `-articles.png`)

Docked results panel, top 6 visible (titles are index-term/alias display titles; the parenthesis names the article it
opens). Best Matches is always 25 (a cap). Counts are Articles / Photos & More / Web Links / Dictionaries / Maps.

| Query | Counts | Encarta Articles top 6 | Ours `kind=article` (n), top 6 |
|---|---|---|---|
| zebra | 95 / 16 / 3 / 19 / 0 | Zebra; Ass, related to zebra; Horse, related to zebra; Quagga, extinct type of zebra; Zebra Quick Facts (table); Zebra Fish | 94: Zebra; Zebras (Kids); Zebra Fish; Zebra Mussel; Zebras and Gnus; Quagga |
| einstein | 101 / 16 / 9 / 0 / 0 | Einstein (Albert Einstein); E=mc², applied to nuclear weapons; E=mc², applied to the Sun; E=mc², mass-energy equation; Gravitation theories developed by Einstein; Theory of Relativity formulated by Einstein | 238: Albert Einstein; Einsteinium; Bose-Einstein Condensate; Albert Einstein (Kids); Gravitational Lens; Brownian Motion |
| world war | 44 / 11 / 1 / 3 / 0 | World War II; World War I; World War (index term); World War I Timeline Event; World War II Timeline Event; Adolf Hitler, Nazi dictator of Germany during World War II | 12026: World War II; World War I; Cold War; Korean War; Vietnam War; Warsaw |
| jazz | 45 / 16 / 21 / 19 / 0 | Jazz; African American Music; Jazz Dance; Latin Jazz; Ragtime, influence on jazz; Blues, relationship to jazz | 579: Jazz; Free Jazz; Latin Jazz; Jazz Standard; Jazz Dance; The Jazz Singer |
| volcano | 74 / 9 / 9 / 7 / 12 | Volcano; Cinder Cone; Cotopaxi; Lava; Magma; Mount Etna | 624: Volcano; Shield Volcano; Mayon Volcano; Volcano Island (Philippines); Volcano Islands (Japan); Volcanoes |
| shakespeare | 56 / 10 / 3 / 0 / 0 | Shakespeare (William Shakespeare); Shakespeare's Sonnets (audio); Drama in Shakespeare's era; English Literature during the Renaissance; Shakespeare's Poetry; Hamlet's Soliloquy | 641: William Shakespeare; Shakespeare's Sonnets; Shakespeare in Love; Royal Shakespeare Company; Stratford-upon-Avon; Much Ado About Nothing |
| dna | 80 / 4 / 19 / 21 / 0 | DNA (Deoxyribonucleic Acid); DNA Fingerprinting; Genetic Engineering; Genetics; Human Genome Project; Molecular biology, research using DNA | 332: DNA Fingerprinting; Deoxyribonucleic Acid; Genetics; Polymerase Chain Reaction; Nucleic Acids; Arthur Kornberg |
| rome | 99 / 4 / 2 / 1 / 6 | Rome (Italy); Rome (Georgia); Rome (New York); Ancient Rome; Forums, ancient places of assembly in Rome; Roman Art and Architecture | 2637: Rome (Italy); Ancient Rome; Rome (Georgia); University of Rome; Rome (Kids); Rome (New York) |

Best Matches is the Articles order with other kinds interleaved (tables, pictures, timeline events, dictionary entries;
world war: World War II, World War I, World War, "world war" dictionary entry, two timeline events; zebra: Zebra, Ass,
Horse, Quagga, Zebra Quick Facts table, Burchell's Zebra picture). Our all-kinds top 6 overlaps it about as much as the
article list does.

Agreement (by target article, run on `out/search` with the release server): top-1 7/8 (miss: dna, ours puts DNA
Fingerprinting above Deoxyribonucleic Acid); top-6 overlap 19/48 (zebra 3, einstein 1, world war 2, jazz 3, volcano 1,
shakespeare 2, dna 3, rome 4); best-matches equivalent about 17/48. Where both list an article its rank differs by at most
2 (Rome (New York) 3 vs 6 is the largest). Total counts differ by design: Encarta's 44-101 look title/index-term based
(zebra 95 vs our 94 is a coincidence), ours are full-text (238-12026; "world war" prefix-matches Warsaw and War).

Fixed afterwards (verified by re-measuring with a rebuilt index in `target/agent-rank/out/search`; `out/search` was not
rebuilt, run `encarta-convert search` to pick the fixes up):
- Kids-only articles (Zebras, Rome, Albert Einstein) no longer appear in the default search (no `edition`); `edition=kids`
  (what the Kids UI sends) still finds them, `edition=all` returns both.
- A title minus its trailing parenthetical is an exact title ("rome" matches "Rome (Georgia)"): Rome (Italy), Ancient Rome,
  Rome (Georgia), Rome (New York) now come first (Encarta: Italy, Georgia, New York, Ancient Rome).
After: top-1 still 7/8, top-6 overlap 20/48 (einstein 2 as Relativity moved into the top 6; the rest unchanged); zebra count
87 (Encarta 95).

Still different, not changed:
- Encarta ranks index-term aliases ("Ass, related to zebra", "E=mc², applied to the Sun") that open another article; we
  show only the target, titled by the article (Albert Einstein, not "Einstein").
- Encarta matches whole words in titles/aliases; we also rank prefix and stem neighbours (Einsteinium, Zebra Mussel,
  Volcano Island, Warsaw).

### Sound and follow-up pass (2026-10-14; sound via USB audio, vm.md)
- **Interface sounds** (Options › "Play menu and button sounds" ticked; the recording matched against the ENCARTAR.DLL
  WAVE resources played through the same chain; ids with identical audio grouped): rollover **31971** (= 31914/31915/31935)
  when the pointer enters an enabled toolbar button (Back, Forward, Home, Features, search Go, the Back ▾) or an atlas text
  button (Map Legend, About this Map), and on each highlight change in a menu-bar menu (pointer, arrows, Esc); click
  **31973** (= 31916/31917/31936/31974/31976/31977) on mouse-down **and** mouse-up of the round toolbar buttons (adult and
  Kids), once for menu titles, check boxes, dialog buttons, atlas tools, zoom, style arrows and Kids home panels; Kids
  round buttons roll over with **31975**; timeline controls (zoom, era links, View as List, next 25, Main Timeline,
  Compare) play **575**. Page links play nothing of Encarta's, only Windows' "Start Navigation" click (Windows XP
  Start.wav). Options › Default sets startup sound on, menu and button sounds **off**. Not heard anywhere tried: 578, 579,
  682/31972/31978, 702/31909, 31906-31908, 31910/31911, 31912, 31913.
- **Search panel:** opening a hit leaves the panel open; a new search opens the category that was open last.
- **Atlas V15** (`fixtures/atlas/v15/`): "Mumbai" over "Bombay", "Merano" over "Meran", with a 1 px rule between the lines as
  wide as the wider line. **V16** (`fixtures/atlas/v16/`): on the Time Zones map a click on France lists Western Europe,
  West Central Europe, France, Europe (no time zone; the title stays "France | Europe"); a click on the Atlantic lists only
  Atlantic Ocean. **Sea clicks** (`fixtures/atlas/sea-click/`): 1-3 km off Brittany only "Bay of Biscay" (and a nearby
  city point) is listed, though France, Brittany and Morbihan polygons cover the point; on Lake Geneva Vaud and Switzerland
  are listed with the lake. (Explained by the hit sides, "mad-styling.md" section above.)
- **Timeline pop-up dates** (`fixtures/timeline/dates/`, 23 events): see timeline.md §3.2 ("June 16, 1963",
  "120,000 years ago", "2.50 million years ago", "4.60 billion years ago", "8000 BC").
- **Sidebars `SBA`, `SBP`** (761593235, 761594207): the same purple "sidebar" banner as the other categories.
- **Panorama tours** have no zoom and no keyboard control (wheel, +/−, arrows, Page Up/Down, A/Z with the viewer
  focused and auto-rotate off).
- **Music items** start playing when their page opens; five GS Wavetable renders in `fixtures/music/`.
- **Kids submenus** (`fixtures/ui/kids/sub-*.png`): View › Go to (Back Alt+Left, Forward Alt+Right, Home Page
  Alt+Home), Closed Captioning Ctrl+L (On/Off); Tools › Find in this Article... Ctrl+F, Read Aloud F8 (Whole Article,
  Selected Text, Stop Reading F9, Pause), Menu and Button Sounds (On/Off), Startup Sound (On/Off); Help › Microsoft
  Encarta Help F1, Troubleshooting (Diagnosing Problems, Contacting Technical Support), About Encarta.

### Where the unheard sounds play: second search (2026-10-14; USB audio recording, menu sounds on)
- Listened to (each action timed against the recording, slices matched with `sndmatch.py`): the dictionary window (word
  list, tabs, Go, speaker icon), Games & Fun Stuff folders, the video player (pause, stop, scrub; the video's own sound
  masks the controls), the audio-set player (thumbnails, play, stop), Visual Browser (hover, click, wheel, arrows),
  Quick Search: Maps dialog, Add to Favorites with New Folder, Map Trek (dialog Next / Back / list, marker, legend),
  Geography Quiz (start, wrong, right), Options (all five pages, check boxes), Kids: Games and Fun Stuff menu, Video
  Vault video, search results, article, Look at Pictures, Tools menu. Features-menu flyout items, Kids menu entries and
  media thumbnails are silent. Controls inside the dictionary window, dialogs' page lists and tab lists play nothing.
- **Result: none of 578, 579, 682/31972/31978, 702/31909, 31906-31908, 31910/31911, 31912, 31913 was heard.** Every
  Encarta sound matched was `tick` (31914 group) or `click` (31916 group), in the places the first pass lists. 578 and
  579 are nearly inaudible (peak 0.03 / 0.08), so they are probably never played. Flash pages (Jewel Game, Dogs'
  Gone to the Dogs) show "requires a Flash player" and play nothing.
- **Geography Quiz:** a wrong answer greys the button and drops the offered points (5 → 4) with only the click; a correct
  answer plays a loud (peak 0.9) 1.9-2.0 s fanfare 0.4 s after the click while the map flies to the next question. Its
  length and envelope match XP's `C:\WINDOWS\Media\tada.wav` (171,100 bytes, 1.94 s; envelope correlation 0.68), and no
  `ENCARTAR.DLL` / `ENCSPLSH.DLL` resource is 2 s long, so it is the Windows sound, not Encarta's.
- A click on a window that a modal dialog blocks plays Windows' 0.4 s "ding" (looks like an unmatched sound; ignore it).
- Not tried: the Geography Quiz score dialog, Read Aloud, Print / Copy dialogs, Organize Favorites, the Help windows,
  Kids Read Aloud and its other games (all Flash), `World Languages Interactivity`.

### Not done in this pass
- single-panorama `initangle` (Flash viewer); where the unheard sounds play (searched again, not found; see above); a listening comparison of the GS renders
  with our synth; the Options dialog was read from the captures and the help, not each control exercised; INDEX.md
  entries for the new captures; no new snapshot saved (usb-audio cannot be part of one).

## UI pass 2 (2026-10-14): the *(ours)* rows of parity.md

Captures in `fixtures/ui/r2/` (clipboard texts read with `window.clipboardData.getData` through `vm/agent.py js`).
Started from `encarta-home`; the Read Aloud part with `--wav` (USB audio) and Encarta restarted after the device
installed (Encarta decides at start-up that there is no sound card). No snapshot saved. Wording paraphrased.

### Copy source line (`dict-copy-*.txt`, `kids-copy-*.txt`, `copy-dialog-fullarticle.png`; earlier `copy/`)
- The line is "Microsoft® Encarta® 2009." followed by "© 1993-2008 Microsoft Corporation. All rights reserved." In the
  HTML flavour the first sentence is bold; the adult article and Kids texts have a space before each ® (an artefact of
  their HTML), the dictionary's does not. No title, URL or date is added; Kids says "Encarta 2009" too.
- Placement: selection (adult Ctrl+C, Kids Copy with a selection) → selection, one line break, the line. Whole Kids
  article (Copy button, no selection) → title, blank line, text, **blank line**, the line. Whole dictionary entry
  (Copy link, no selection) → entry text, blank line, the line. Adult Edit › Copy › Whole Article Text → text ending
  with "Contributed By:" and the name, one line break, the line.
- Dictionary Copy with a selection inside the entry copies **only the selection** (no line). Kids Copy shows no dialog.
- On the full-article view the adult Copy dialog offers Whole Article Text / Whole Article With Media (the outline view
  offers Selected Text instead, earlier capture).

### Menus (`readaloud-menu-*.png`, earlier `menubar-*.png`)
- File: Print… Ctrl+P | Go to ▸ (Back Alt+Left Arrow, Forward Alt+Right Arrow, separator, Home Page Alt+Home) |
  Options… Ctrl+O | Exit, separators between the groups. Edit: Copy… Ctrl+C | Read Aloud F8 ▸ | Search Encarta Ctrl+S
  (lens icon) | Find in this Article… Ctrl+F (lens icon). Kids Favorites: Add to Favorites… / Organize Favorites… /
  separator / "<empty>" (disabled) when there are none.
- Features menu order: already captured in the first pass (`features-menu.html`); nothing new.

### Games & Fun Stuff (`games-fun-stuff.*`, `games-geography.png`)
- No menu or Features entry leads to the games. They are on Home › Games & Fun Stuff (home record 701712546): folders
  Animals & Science, Arts & Culture, Geography, History, Tours, Videos. Geography holds the jigsaw puzzles, Continental
  Quiz, Geography Quiz, Postcards from Africa, See the Sights (Canada, Europe, U.S.), U.S. State Capitals, then a
  "Tours & Panoramas" group. Geography Quiz is otherwise only Ctrl+Q.

### Search panel (`search-quote-*.png`, `search-panel-resized-empty-cat.png`; colours from `search-results-*.png`)
- Geometry at 1024×768: panel from x 646 (1 px #5A555A rule) to the window edge, 378 px including the rule, from the
  chrome's bottom (y 109) to the bottom of the window; face #D6D3CE in 16-bit colour (= button face #D4D0C8). A
  splitter on the left edge (grip of dots at mid-height) resizes it: widest 498 px (edge at x 526), narrowest 308 px.
- Category bars: x 659–1020 (12 px from the rule, 3 px from the edge), 32 px tall every 38 px, vertical gradient white
  → #E7DFDE, 1 px #ADAEA5 bottom edge, rounded corners; 16 px icon at x 668, label at x 694 (≈ 15 px, #424142), "+" in
  #848684 at the right. The open category is a white box with a #B5B2B5 border and a white header.
- Hit rows 56 px tall: 34 px picture at x 671, bold title at x 729 (#292829), snippet under it in plain text; a kind word
  ("Table", "Picture") follows some titles in plain weight. The first row is highlighted #84C321 with white text; the
  hovered row #CEF79C with #5A555A text. **In the Articles category the rows show titles only** (no snippet).
- An empty category shows "No results were found in <category>" and "Check the spelling or try a new search." (bold,
  no link). Dictionaries (0) and Maps (0) bars stay listed.
- **Quotations never appears**: searches for zebra, shakespeare, "to be or not to be" and liberty all listed the same six
  bars, though Best Matches held a quotation ("Suicide: HAMLET To be, or not …", quotation icon). The first pass's
  "appears when present" was not seen.

### Map Trek (`maptrek-open.png`, `maptrek-closed-go-tooltip.png`, `maptrek-dblclick-word.png`)
- Window "Map Trek" (Win32 dialog with caption and close box) 456 px wide at (567,45); the HTML body measures
  (from `maptrek/maptrek-dialog.html`): side margins 17 / 10 px; text box 257×178 at top 46, stop list 150×178 at
  x 280; list footer 19 px, #595959 with #C5C5C5 text; trek title 8 pt bold #765C26, stop text 8 pt #353535 (a 64 px
  picture at the left with its caption in 8 pt #6F6F70, text indented 83 px); list items Tahoma 8 pt #353535; buttons
  17 px tall at top 232 (New Trek 66 px at the left; Back and Next 44 px, 4 px apart, ending at the list's right edge).
- Hover: an enabled button turns #CEB57B; a list row does not change (hand cursor). A stop without a picture shows the
  bold place name and text only.
- Closing the dialog leaves the map at the stop (title "Map Trek" over the map, the treks' markers); nothing reopens it.
- Double-clicking a word in the stop text opens Encarta Dictionaries on that word (Encarta's general double-click
  lookup; "expanses" → not found).

### Arrow tooltips (`tip-*.png`)
- Tool buttons show a tooltip after hovering: box #FFDFA5 (16-bit), 1 px #00205A border, #00205A Tahoma 8 pt text
  wrapped at about 120 px, and a 14 px #FFB621 strip with a dark blue arrow pointing at the button. Placement is relative
  to the button: centred below it with the strip on top (↑) when it fits (adult Home "Home"; Kids Back, Copy, Print,
  search arrow); at the button's lower right with the strip at the left (↖) near the left edge (adult Back, Kids Home);
  at the lower left with the strip at the right (↗) near the right edge (adult search arrow). Atlas controls use other
  sides (← to the right of Map Legend and +, ↓ above the tool icons).
- Texts: Back = "Back to <previous page title>"; adult Home = "Home"; Kids Home: open the Kids home page; Kids Copy /
  Print: copy / print the article or picture; search arrows: show search results for the words in the box. Disabled
  buttons (Forward with no history), Features, the ▾ tab, the Back ▾ and Look at Pictures show none.

### Read Aloud (`readaloud-*.png`, `kids-readaloud-*.png`)
- Without a sound device: "Text Cannot Be Read Aloud" message box (adult and Kids). With USB audio: speech plays (the
  WAV grows), and **nothing in the page is highlighted** (adult Zebra, Kids Elephants).
- Submenu states: idle → Whole Article enabled, Selected Text enabled only with a selection, Stop Reading F9 and Pause
  disabled; reading → Whole Article and Selected Text disabled, Stop Reading and Pause enabled; paused → Pause reads
  **Continue**. Stop ends it. F8 alone did not start reading from the article page (focus), the menu did.

### Favorites (`kids-favorites-add.png`, earlier `favorites-*.png`)
- Kids uses the same Add to Favorites dialog as the adult edition: ≈ 307 px wide, body #ADBEE7, "favorites" banner in
  #EFF3FF over a 1 px #EFF3FF rule, bold #21304A labels "Name:" (text selected, "Article - <title>") and "Create in:"
  (folder tree), New Folder / OK / Cancel buttons #C6D7EF with a #7396BD border. Storage per Windows user: not observable
  from the UI beyond the first pass's `.erl` files.

## Data pass (2026-10-15): content-schema §11 and the small formats

Most answers come from the existing article captures (the `id=ModuleColumn` part of each `article.html`, and the
`Child_s<n>` blocks of the body) compared with `out/content.sqlite`; scripts in `target/agent-data/` (local). The VM
was used for the Geography Quiz, the atlas media question, the timeline search and the module buttons, started from
`encarta-home`, no snapshot saved.

### Right-column modules (25 encyclopedia captures, all agree)
- **Order** (fixed; a module is left out when it has nothing): World Atlas, Quick Facts, Related Articles, Quotations,
  Web Sites, Primary Sources, Sidebars, one module per virtual tour ("Virtual Tour" for `2DVT`, "3D Ancient Site" for
  `3DVT`, DATA order), Further Reading, Archives, Visual Browser, then the grey online modules "radio" and "news".
  Header bars: grey (World Atlas, Quotations, tours), orange (Quick Facts, Related Articles, Web Sites, Visual Browser),
  light orange (Primary Sources, Sidebars, Further Reading, Archives); list modules print the count, "Web Sites (16)".
- **World Atlas**: present exactly when the article has a `maps` forward association; the module id and the
  `MapView/eid=` are that map's (= the article's) `geocode` (France 84, United Kingdom 242, East Timor 7299303,
  Lillehammer 18674). Moon has a geocode but no `maps` row and no module. The picture is a live render of the place.
- **Quick Facts**: the article's facts table (a `table` whose DATA has `quickfacts`, or the `factsandfigures` item),
  13 of 13. Shown: the table's first five entries with their continuation values / sub-entries, the **fifth cut after
  its first value** (Lincoln "Terms In Office" shows 1861-1865 only; Shakespeare "Milestones" one line; France
  "Population growth" its first sub-entry "Population growth rate"). The Facts and Figures `quick="1"` marks are not
  used (France shows "Population growth", which has none). "More" opens the table (`ERSMediaView`).
- **Related Articles**: the first five top-level nodes of the **DLX** REL tree in stored order, headings included
  (East Timor "island of Timor", "Southeast Asia" …), 26 of 26. Horse and Lillehammer, whose `STD` trees differ, match
  only the `DLX` one; Algebra and Mathematics, whose `STC,STE` trees differ, match `DLX`. "More" opens the related view.
- **Quotations (N)**: N = the forward `quotation` rows (Shakespeare 765, Einstein 61, Lincoln 38); the box shows the
  first row's title ("Philosophy: For there was never yet philosopher That could…") between quote marks, credit the
  article's person; "Show All" opens `display=quotationlist`.
- **Web Sites (N)**: every forward `weblink` row, sorted by title (22 of 22); each row: icon, title, description
  (the caption), address; three rows, then (+)/"Show All".
- **Primary Sources (N)** and **Sidebars (N)**: the forward `sidebar` rows split by DATA `type`: `SBD SBH SBI SBL SBO`
  (case-sensitive: Shakespeare's eight lower-case `sbl` excerpts are under Sidebars) are Primary Sources, every other
  type (`SBC SBS SBX SBZ SPC CUS SHE SBG ZPA sbl` seen) is Sidebars. Both sorted by title, three rows shown.
- **Sort order** of these lists: case ignored, apostrophes and hyphens ignored (Windows word sort: "American Memory" <
  "America's Story", "Brazilian Embassy" < "Brazil's Site", "Einstein: Image" < "Einstein-Haus"), punctuation before
  letters ("Brazil [Foreign…" < "Brazil.com" < "Brazil: Consular"), inner markup ignored ("From the Northern Ireland…" <
  "From *The School for Scandal*"), but a title that **starts** with markup sorts first: United Kingdom's Primary
  Sources open with "*God Save the Queen*", whose DATA title is `<title>⏎\t\t<it>God Save the Queen</it>⏎\t</title>`,
  i.e. the sort key is the stored text including its leading white space.
- **Further Reading (N)**: the forward `furtherread` rows in DATA order (22 of 22; not by `sequence`), five shown;
  "Show All" opens `display=furtherread`.
- **Archives (N)**: the forward `archive` rows by DATA `year`, then title, each "1938: France"; five shown.
- **Virtual Tour / 3D Ancient Site**: the tour's title, its `picon` banner (244×68) and "Take the Tour"; the module
  carries the tour's section number (`secprefix="III.A."`).
- Absent: Visual Browser only on F 761551508, which has no forward associations at all. "radio": the article's
  `livemedia` Radio row ("Listen to radio from France"); "news": country articles (MSNBC regional news); both are
  links to encartaupdate.msn.com.
- **Buttons** (`vm/agent.py js`, France): Web Sites, Primary Sources, Sidebars and Archives "Show All" call
  `ExpandArtMod(box, 3|5, …)`: the box grows in place to its full height (Sidebars 100 → 241 px) and the button reads
  "Collapse". Further Reading "Show All" and Quotations "Show All" navigate instead.

### Placement residue (content-schema §2)
- The body DOM has **empty item slots**: `<DIV id=Child_s<n> parsec="n"></DIV>` where a placed item's block would be,
  42 in the corpus. In the four residue sections there is exactly one, at the paragraph that `⌊free/items⌋` spacing
  needs to be "used": Moon §2 before p28, United Kingdom §1 before p131, Mathematics §1 before p2, East Timor §13
  before p53. Treating that slot as used (as a timeline-stream slot is) makes all ten residue items land where the VM
  has them; the other empty slots fall at section ends or in sections without items.
- What the reader laid out there is not in the data we read: none of the four sections has a forward row, reverse row,
  `par` row, outline entry or paragraph markup that the others lack. It needs the debugger (which record the native
  layout puts into that slot).

### Section headings
- The heading look follows the **depth** only: classes `l1sectnumber`/`l1title` at the top level whatever the type
  (type 4 United States History, 5 Zebra, 6 Musical Rhythm: all "II. TITLE"), `l2…` at depth 2 (types 5, 6, 7), `l3…`,
  `l4…`. The titles are stored in their display case (top-level titles upper case in the data).

### DATAF `pn` (data only)
- `pn` occurs only together with `paragraphid` (700,635 article entries and a few of other groups) and never
  decreases with the paragraph's reading order inside one article (3,000 of 3,000 articles). It is a **page number**:
  ten sections to a page in reading order (`pn = ⌊section number / 10⌋ + 1`, exact for 3,702 of 3,942 sectioned
  articles, the rest off by one at a page boundary); all 16,314 articles without sections have `pn = 1`. The 2009 reader
  has no pages; index leaves open at the paragraph (`jump=p<paragraphid>`, UI pass).

### Editions `STE`/`STC`
- Premium shows the `DLX` trees (Related Articles above). `ENCARTA.EXE` and `ENCARTAU.DLL` contain the strings `DLX`
  and `STD` but neither `STE` nor `STC`: this disc's program never selects those trees. Their meaning (student
  editions?) stays a guess and does not matter for this disc.

### Geography Quiz: `D*.TXT` line 3 (`fixtures/geoquiz/globetrotter-choices.png`, `explorer-choices.png`)
- 17 Globetrotter (line 3 = 2 2 2) and 13 Explorer (1 1 1) questions, answers found by clicking until the question
  advanced, each button mapped to its pool row's region (`group`): every Globetrotter question had **exactly two**
  wrong answers from the answer's region (Canada: one, its region has only Mexico besides), every Explorer question
  **exactly one** (Russia: none, alone in its region; the continent question trivially; the last question's answer
  was not confirmed and is left out); the other wrong answers were never from the answer's region.
  With the earlier Navigator (0) and World Class (4, all East Asia) shots: line 3 is the number of the four wrong
  answers drawn from the answer's own region, the rest from other regions. (Continents all share region 39070.)

### Atlas media (P2MTBL photo themes)
- The 2009 atlas has no media layer: the map styles (Comprehensive … The Moon), the Map Customizer (places, borders,
  labels, transportation, physical and other features, reference lines, hill shading, place-name language) and the
  pushpin palette (user symbols) offer no photos or sounds, and a place's click-menu submenu is only Map / Read Article
  (France, which has a "photo:people" and a music link). So the photo theme labels are not shown anywhere.
  `fixtures/atlas/media/`.

### Timeline search (`fixtures/timeline/search-*.png`)
- Find Topic or Year › "Aachen" lists event titles ("Charlemagne Builds the Palatine Chapel at Aachen", twice);
  "charities" finds the `charity` events (Mother Teresa, Salvation Army); "Aix-la-Chapelle" is not found. The term
  entry's offset A (timeline.md §5) is not visible in the UI.

### Not settled
- MSWORLD.SDB column B and the qualifier list: no view shows them (statistics hover, table, Details checked earlier).
- TIMELINE.DAT descriptor field 5 and term offset A; P2MTBL header words; LEX header words: internal, no UI.
- MindMaze: not reachable in 2009 (earlier pass), so nothing to observe.

## Flash and debugger pass (2026-10-15)

Installed in the guest (approved by Alex 2026-10-14): Adobe Flash Player 10.3.181.16 ActiveX (Adobe's own archive
zip `fp_10.3.181.16_archive.zip`, from the Internet Archive copy, sha256 0baeefc1…), and Debugging Tools for Windows
6.12 (`dbg_x86.msi` from download.microsoft.com). Both are in the live snapshot `flash-ready` (vm.md). Encarta must be
restarted once after Flash is installed; after that every interactive opens in Flash.

### Single panoramas (`target/agent-flash/seq*`, five scenes, view centre matched to the image SWF's JPEG)
- **Opening yaw.** The first frame shows the view centre `initangle` degrees **right of the picture's middle**: picture
  x = W/2 + initangle·W/360 (mod W). Mosque of Sidi Sahab (305, W 1947): predicted x 675.9, seen 675.5; Fatehpur Sikri (180,
  W 3138): 0, seen 3137.5 (the seam); Organ Pipe Cactus (44, W 1606): 999.3, seen 998.5; Tivoli Gardens (252, W 1844):
  368.8, seen 367; Rievaulx Abbey (62, W 1995): 1341.1, seen 1340.5. FlashVR's script clamps `initangle` to
  ±(part/2 − fov/2) when `part` < 360, which also reads it as an angle from the middle.
- **Field of view and horizon.** At the view centre the picture shows at f/r = 1.05 (Mosque), so f = 325 px on the
  544-px view: fov 80 as written. The view's middle row is picture row 272.5, the horizon our image-SWF reader gives.
- **Auto-pan** runs at open, to the right (picture x increasing), after a fade-in. Its speed is `auto` (3) view pixels
  per tick of a 5 ms interval timer that Flash ran 42–58 times a second here: 125–175 view px/s, 22–31 °/s (five
  scenes; a full turn is 2πf ≈ 2,044 view px, about 13 s).

### Interactives: Flash 10.3 vs Ruffle 0.7.1 (`fixtures/interactives/<id>/screen.png`; montages in `target/agent-ruffle/compare/`)
Seven items, one per control type (Vasco da Gama, Propylaea, Venus, Periodic Table, Castle, Counterpoint, See the Sights
of Europe), our client in headless Chromium against the real server. Propylaea, Venus, Periodic Table and the DragDrop
board match (DragDrop's photo order is random in both). Vasco da Gama's ocean fill is there in Flash, as in our runs. The
only differences are text: Trebuchet MS (XP has it, the disc does not; we use Tahoma) sets narrower in ours, so some
paragraphs wrap differently (Vasco's second paragraph 6 lines instead of 5, Castle's description 3 lines instead of 2,
Castle's title 116 px instead of 128 px), and Counterpoint's intro line is about 5 % wider. No text is clipped in ours
that is not clipped in Flash. Flash's land fills in Vasco show a fine dither, which is the guest's 16-bit display.

### Tours under the debugger (cdb attached to tourhost and to ENCARTA.EXE; `vm/dbg.sh`)
- **Gravity mode on the software device.** Walker +0x54 is 1 in ENCARTA.EXE's control at the Acropolis start (and in
  tourhost) — but only until the first walk step, which sets it to 0 on the software device (VM pass 4 below);
  GRAVITY 0.003 is at the control's static +0x11b0c8. A write breakpoint on the eye's Y shows each tick fall
  by `vfall` and land in the same tick. An eye raised 0.6 units fell 0.003, 0.006, …, 0.057 and landed on tick 20. This
  supersedes "§6.8 Gravity vs snap" above (tours-3d.md §6.8).
- **Pitch limit.** A breakpoint on the pitch step (FUN_10010f80) logging eye, target and stored pitch: the step tests
  the pitch derived from the look vector, which drifts below the stored pitch (`PITCH?`) because each tick's fall and
  landing round eye and target to float32 (20.999359 vs 21.000036 after about a minute; four 6° steps accepted, the last
  at 44.999359). A float32 model of the fall and landing matched 1,133 logged ticks bit for bit (tours-3d.md §6.9).

### Still open
The atlas label-corner order and click-menu order, the media-placement residue (`Child_s<n>`) and the other
debugger-only data questions were not taken up in this pass; the debugger workflow is now there for them. Not tried
with Flash yet: the unheard sounds in Kids Read Aloud and the Kids games, which are Flash.

## VM pass 3 (2026-10-15): the last "Needs the VM" items

Three VM lanes ran in parallel (`vm/` and two reflink copies with their own ports, all from `flash-ready`); nothing
was installed in the guest. The hardware cloud step and MaxView (3D device) were not attempted.

### Atlas: label, shield and click-menu order (static, MOBB200.DLL and ENCARTA.EXE; no VM)
Checked against `fixtures/atlas/labels/`, `sea-click/`, `sea-click2/`, V11.
- Point labels (mad-styling.md §7.4): candidates above right, below right, above left, below left (centred first
  when the point has no symbol), with fixed added costs 250/500/750/1000 (centred 0). A box outside the pane or
  over a placed label is rejected; the rest add a background cost (pixel classes under the box: 0 over water,
  251 over land, 1000–2000 across a coast, +1000–3000 over label/line-class pixels) and a neighbour cost (the
  reserved boxes of other labelled points, 4750 down to 251 by priority group; 5000 for the pane centre).
  Cheapest wins, ties to the earlier candidate. So the order matters only between equal costs: isolated labels
  above right (7 of 7 in the outback), and the Morbihan mix (18 of 30 above left) comes from the costs.
- Shields (§8.1): one per on-screen run at most; candidates cost the centre term plus the same neighbour cost (no
  background, no position step); cheapest wins; a same-text shield within ~160 px drops the run's shield.
- Click menu (§4.5): MOBB sorts by rank (label hit of a routable road 0, other label hits 1, points 2, routable
  roads 3, lines 4, areas 5), then PDCAttribute scale ascending, then GeoObj key and entry descending; at most 4
  routable roads; ENCARTA.EXE keeps the order. Every captured menu fits, e.g. V11 "Limoges, Auvergne (250, key
  208), Western Europe (250, key 8 entry 25), West Central Europe (entry 24), Massif central (400), Loire Basin
  (420, key 60), France (420, key 0)"; "Vevey, Vaud, Lake Geneva, Rhône, Alps, Switzerland" (400s by key 64, 60,
  24); "Italy, Southern Europe, Western Europe, Mediterranean Sea" over the sea (Italy through its drawn label).
- Converter bug found: `encarta:menu_excluded_classes` held class ids, not rows, so the client listed time zones
  and left out the Mexico/US shield roads (rows 209–211). Fixed (styles stage).

### Tours: the last §10 items (cdb in tourhost and ENCARTA.EXE)
- **Water ripple step rate (§7.4.6).** A cdb log in tourhost at the Acropolis pond (`acr.3wm` cells 1145–1148,
  software device), breakpoints past the step routine's water gate (0x10035F37), at the step proper (0x10035F95) and
  at the drop try (0x100364A2): 641 frames in 10.0 s (64.1 fps), 631 steps, 419 drop tries. Every frame stepped except
  after two 4-ms frames, read as 252 fps, which set a skip of 5. Matches the code: after a step, skip =
  `trunc(fps × 0.025) − 1` above 40 fps (fps = 1 / last frame time, the DISPLAYFPS number), so one step per frame below
  80 fps; drop wait `rand() & 1` steps (2/3 tries per step expected, 0.664 measured). Static reading of the same code
  also corrected the step border (2 texels, not 1), the rounding (arithmetic shifts), the drop position rule
  (x = rand mod W, kept only when 3 < x < W − 3) and the display (a linear offset in the bottom-up surface, so
  horizontal displacements run on into the next row; the picture is the pre-step field). tours-3d.md §7.4.6.
- **`3D.TOGGLEHIDDEN` (§7.4.3).** It flips the same flag as `VISIBLE` on every object, and both draws honour it. The odd
  pictures come from `CAMERA.CLEARBACKGROUND=0` (every load.3sc): only depth is cleared, the colour clear (0x10065990)
  runs only with CLEARBACKGROUND on. Acropolis start: after the toggle every `VISIBLE?` is 0 and the picture is
  unchanged (5 px), also after a 37° turn; `3D.Sky03.VISIBLE=1` then draws its mountain band over the frozen frame,
  and after another turn both bands show (trails); a second toggle gives the normal view. This also explains pass 2's
  "brought those back and hid the ground".
- **ITS mode (§2.2).** cdb on ENCARTA.EXE (module `ACTIVEX3DX3`, from `Reference 2003\ACTIVEX3DX3.OCX`, byte-identical
  to the disc's `ACTX3DX3.OCX`) while opening the Acropolis from Features → 3D Ancient Sites logged these `Do` calls:
  `SCENE.HALFREZDRIVING=1`, `script.itsfileurl=C:\Program Files\Microsoft Encarta\2009 Encarta Contents\E09AXLRD\world\7015\04257.EIT`,
  `script.load=load.3sc`, the default stop's `CAMERA.MOVETO=…;CAMERA.PITCH=21.000036;`, then `camera.pos?` once a
  second. The file reader (0x100456A0, URL moniker `msencdata:<container>::<name>`) ran 190 times on 95 names (scripts,
  `ACR.X`, textures, `.3lt`, `.3wm`, `.3cl`, `.32d`), never on a `.cur`. `msencdata` is not in the registry: ENCARTA.EXE
  registers it per process (IInternetSession::RegisterNameSpace, class in MSENCDAT.DLL), which is why tourhost loads
  nothing in ITS mode. Same files and name rules as disk mode: nothing a port models changes.
- **Spaces in `Do` values are kept** (correction of the §2/§3 entry above): tourhost `script.itsfileurl=` with the long
  path returns 1 and `ITSFILEURL?` gives it back upper-cased with its spaces; `SCENE.WORKINGPATH=C:\a b\c d` reads back
  `C:\A B\C D`. The old `-2` was a long path into `Encarta Premium DVD 2009\` instead of `2009 Encarta Contents\`.
- **Cursors** (static, checked against the files): the `CURSOR=index,file` handler passes both to a routine that
  ignores them and loads the control's own cursor resources once; slots 0–6 = resources 0xA6, 0xA3, 0xA2, 0xA5, 0xA4,
  0xA1, 0xA1. The first five have the same bitmaps and hotspots as `sv`, `svright`, `svup`, `svleft`, `svdown.cur`;
  0xA1 is the hand of `vthot360.cur` (same picture, hotspot 13,6). tours-3d.md §6.9.
- **Camera quirk:** in tourhost a second `CAMERA.POS` to a new X and Z (after a first one) left yaw `-1.#IND00`; a
  `CAMERA.PITCH=` right before each POS avoided it (tours-3d.md §6.10).
- Helpers and logs: `target/agent-tours3/` (`its.cdb`, `water.cdb`, `its-full.log`, `water1.log`, `py/wstat.py`,
  `py/curres.py`, screenshots `cmp1`–`cmp4.png`).

### Sounds and placement residue (cdb in ENCARTA.EXE, with usb-audio)
Lane `target/vm2` from `flash-ready` with `--wav` (usb-audio) so the sound path is live. cdb attached to ENCARTA.EXE
with breakpoints on the `ENCARTAR.DLL` sound loader (`ENCARTA+0x11860`, logs the resource id and a stack), on
`winmm` `PlaySoundA/W` and `sndPlaySoundW`, and (second run) on `FindResourceA` for the style records below; the
"menu and button sounds" bit was set in memory (`DAT_006f0f44 | 1`). Logs: `C:\inst\snd.log` (not kept); helper
scripts in `target/agent-enc3/` (`snd.cdb`, `restart.sh`, `sl.py`, `press.py`, `scripts/*.py`).

#### Interface sounds (media.md §9)
- **578 / 579 are the tour site-map sounds.** 578 when the pointer enters a stop on a site map, 579 on the click that
  moves there; both played synchronously (`sndPlaySoundA` flags 4). Seen in 3D Ancient Sites (Colosseum: the plan's
  squares) and Virtual Tours (Paris: the highlighted green squares; grey squares and the you-are-here arrow silent).
  Static: `ENCARTA.EXE` site-map classes (`CSiteMap` `FUN_00502c80`/`FUN_00502930`, `CVirtToursWnd`
  `FUN_005210d0`/`FUN_00522430`, which waits 150 ms after 579 before moving).
- **The others never play.** The process plays `ENCARTAR.DLL` sounds only through the one loader (plus the splash's
  `PlaySoundA`); `ENCARTA.EXE` names no id of 682, 702, 31906-31913, 31972, 31978 (no immediate, no data word), no
  other module of the process imports a sound API (only `EDICT.EXE`, a separate process, for pronunciations), and the
  XSL/JS templates call `PlaySoundAsync` only for 31973 and 31978+N (N = 1..10). In the resource data, 702 occurs only
  in two `SHRL` style records (`RCDATA` 835, 860: the « » chevron buttons, bitmaps 1062/1063) that no code loads, and
  31913 only in `RCDATA` 31940, a child of the default dialog-frame record 31939 under key 0x17, which the frame
  loader skips. Kids buttons take 31975-31977 from `RCDATA` 25506/25507 (keys 0x4015-0x4017).
- Exercised under the log: Features menu, 3D tour (site map, Tour Stops, Music), Virtual Tour, Organize Favorites
  (folder buttons, New Folder, rename, caption ? and ×), Add to Favorites, World Atlas (pushpin palette and placing,
  pin balloon, map context menu, Map Customizer radios/checks/buttons), Games & Fun Stuff, the Africa jigsaw (Flash),
  media list and picture page. Only 31971/31973/31974/31914/31916/31917, 575-free views, 578 and 579 appeared. Print
  could not be reached ("There is no default printer"); Kids runs as a second process the log did not cover.
- Encarta's own dialogs tick on button rollover (31914, = `tick`) and click on press (31916, = `click`); the
  map context menu (a Windows menu) and message boxes are silent.

#### Placement residue (content-schema §2)
- The empty `Child_s<n>` item slots are **module items** the reader's layout adds (`AREF/COMPNTS/MSENCXML.DLL`,
  decompiled to `analysis/decompiled/ghidra/MSENCXML/`; `Gamma::DoPlacement` = `FUN_458e1760`, module pass
  `FUN_458f4f00`, run after the sidebar/timeline passes and before the primary pass). The template draws an `assoc`
  only for sidebars, small composites and items with `data/files`, so these stay empty.
- **Archives** (any forward `archive` row): the first free paragraph slot of the section titled exactly "History"
  (case-insensitive), else the first free slot over all sections in reading order (`FUN_458f5520`). **Further
  Reading** (any `furtherread` row): the last free slot of the last section. Free = no sidebar or timeline item, and
  not slot 0 of a section with primary items. Live weather would also take a first free slot; live radio/news, Facts
  and Figures and the `minimob` map go after the first section's last paragraph.
- That explains the four residue sections: Moon §2 and the Mathematics and United Kingdom introductions are Archives
  (no section is titled exactly "History"; "History of Mathematics", "History of the United Kingdom"), East Timor §13
  is Further Reading. It also matches the other empty slots in the captures: one before the last section's last
  paragraph in nearly every article (Further Reading), History sections (`s117@0/0`, `s75@0/1`) and the pairs before
  Facts and Figures in country introductions. With the rule our placement matches the VM for 1,660 of 1,660 items.

### Data fields (static: ATCONT.DLL, ENCARTA.EXE, MSSPELL3.DLL)
- **MSWORLD.SDB column B** (`ATCONT.DLL`, `CStatDatabaseImpl` loader and value getters): significant digits. Values returned
  to the UI (after `value x factor + offset`) are rounded to B digits, then printed with `decimals` places. B of 0 or above 15
  means no rounding. All stored values already fit B digits, so the primary unit never changes. The note column is parsed as
  an integer qualifier id, so "N/A"/"estimate" read as 0 and are never shown. Sections are read by position; header needs
  `EVGSDB` and a revision of at least 9031006.
- **TIMELINE.DAT descriptor field 5**: flag "titles are UTF-16" (`ENCARTA.EXE` `FUN_005e3cb0`/`FUN_005e4630`; file name is
  `ENCARTAR.DLL` string 10159; loader `FUN_005e40a0`). **Term "offset A"**: the viewer never reads the term table or reference
  pool at all (pointers are computed and unused); no meaning recoverable, not shown.
- **P2MTBL.DAT header** (`ATCONT.DLL` `CP2MObject`): `8` is the directory entry count (header bytes read = 8 x 8 + 4), and the
  `1` is the first key of entry 0; entries are {first place id, block offset} with an end sentinel {last id + 1, 0}.
- **LEX**: see the next section (the static reading that the open routine rejects our files was wrong).

### LEX speller loader (cdb in ENCARTA.EXE)
- ENCARTA.EXE loads MSSPELL3.DLL; the speller is initialised lazily on the first search (`SpellerInit`, then
  `SpellerOpenLex` on `EESplA01.lex`, language 0xFFFF). The open succeeds (out record holds the lexicon object, the u32 at
  0x04 = 0x0100020E, LCID 0x409); `SpellerCheck`-side calls then run once per word. Dictionary/thesaurus/bilingual LEX files
  were not opened during searches.
- The zero u32 after the copyright is not a rejection: it selects the new-format path where the body starts right there.
  `FUN_3f005561` parses the body; body +0x0C/+0x0E/+0x10/+0x12 are the u16 lengths of the three code-descriptor regions
  (at +0x30/+0x34/+0x38); data breakpoints saw them read once, during open. The header stamp is read once (returned to the
  caller). The copyright trailing u16 is never read in the search path.
- Open: suggestion behaviour (the DLL's own) is not needed; dictionary use of MSSPELL3 not observed.

### Music: GS Wavetable controllers, drums, release (media.md §8)
Three more music items recorded in Encarta's own playback (the anthems of Kiribati 461533840, Sao Tome and Principe
461533985, China 461533664: `fixtures/music/<id>-gs.wav`), plus two MIDI files of our own (single notes and controller
changes, 118 s and 158 s) played through the same GS device with `mplayer2.exe /play /close` and compared with our
synth on an `OfflineAudioContext` (aligned by onset).

- **Disc MIDI use** (scan of all 244): no pitch bend off centre, no RPN, sysex, CC 120/121/123. Moving controllers are
  expression, volume (a fade), mod wheel (1–15) and the pedal (0/127); 99 % of drum hits are crash 49/57; programs mostly
  48, 49, 60, 56, 57, 58, 47, 0; pans 32–96. Conclusion: the disc does not exercise bend or RPN.
- **GS pan is a balance**: R − L −6.7, −3.7, −2.0, 0, +2.0, +3.9, +7.0, +22.7 dB at CC 16, 32, 48, 64, 80, 96, 112, 127
  (far side ≈ square root of the distance to the edge). Equal-power gives −14, −7.7, −3.5 …
- **Mod wheel**: 6.0 Hz vibrato, ±41 cents at 127, ∝ value, immediate, follows the wheel during a held note.
- **Pedal**: any non-zero value (1, 32, 63, 64) holds the note.
- **Volume/expression**: squared law (−12 dB at 64), act on held notes. **Bend** ±2 semitones default (+199/−200), RPN 0
  sets the range (12 → +1,200 cents). **Velocity**: squared up to 96, flatter above (96 → 127: +0.7…+3.9 dB).
- **Release**: brass, reeds, pipes ≤ 30 ms; strings/ensembles about half of Fluid's; organ a fifth; piano the same.
- **Reverb/chorus CC 91/93**: no audible effect in GS.
- **Drums**: Fluid's crashes/kick/snare/side stick 5–10 dB below GS; toms equal. GS's timpani stops after 0.4 s.
- **Playback timing**: tempo agrees within 0.4 % (China +0.36 %, Kiribati −0.14 %).
- **MindMaze**: `ENCARTA.EXE` has `CMindmazeView` and a baggage-URL helper (`FUN_005045b0`, `FUN_005044a0`) but no MIDI
  playback (sound imports: `sndPlaySoundA`, `PlaySoundA`, `mciSendCommandA` only to stop/close all); looping cannot be
  observed.
Method: `target/vm3/start.sh --loadvm flash-ready --wav`, WAV sliced by file size per action.

### Still open
The hardware cloud step and MaxView (no 3D device; done in VM pass 4 below). Print's sounds (no printer in the guest) and a cdb log of the Kids
process (the code names none of the unheard ids, so nothing is expected there). LEX: body u32s +0x04 and +0x20, the
header stamp's meaning and the copyright's trailing u16 (never read on the search path); the other LEX files were not
opened by searches. TIMELINE offset A is never read by the viewer. MindMaze music cannot be observed (2009 has no MIDI
path for it). Atlas: the cost of the alternative-name layouts and which label kinds are placed first.

## VM pass 4 (2026-10-15): 3D device (WineD3D + Mesa)

**Setup.** The Cirrus guest has no Direct3D HAL, so the tour control always ran its software device. This pass gave it
one with WineD3D built for Windows on a software OpenGL, never replacing anything in `C:\WINDOWS`:
- WineD3D: the wine9x build of Wine 1.7.55 (JHRobotics, release v1.7.55.45, `-mmx` zip; its readme supports 2k/XP).
  Its `winedd.dll` is Wine's ddraw; used as `ddraw.dll` (and as `wddraw.dll`, the same file) with `wined3d.dll`.
- OpenGL: Mesa9x 23.1.9 llvmpipe `opengl32.dll` (JHRobotics, v23.1.9.138, the win95 build: no SSE needed, imports
  only what XP has). mesa-dist-win 17.0.1 and 17.1.8 were tried first and do not load on XP (their LLVM imports
  `SHGetKnownFolderPath`, `RegGetValueW`, `GetFinalPathNameByHandleW`); Mesa dropped XP in 17.2.
- Downloads in `target/dl-d3d/<name>/` (sha256 in docs/vm.md), moved in through the exchange disk to `C:\d3d`.
  `vm/d3d.sh` and snapshot `d3d-ready` (docs/vm.md).
- Loading: tourhost loads the control through COM with the altered search path (the OCX's folder, then system32), so a
  `ddraw.dll` next to tourhost.exe is not used; `tourhost.exe.local` (DLL redirection) fixes that. ENCARTA.EXE has a
  manifest (.local is ignored) and mshtml's `ddrawex.dll` loads the system ddraw early (and crashes on Wine's), so in
  Encarta Wine's ddraw is loaded as `C:\d3d\wddraw.dll` by cdb at the entry point and the control's two DirectDraw
  import thunks are pointed at it.
- The control rejects WineD3D without three in-memory patches (cdb, nothing on disk; `vm/d3d/patch2.py`):
  1. Its render-state setup treats any failure as fatal; Wine returns `E_NOTIMPL` for `D3DRENDERSTATE_BORDERCOLOR` (43).
     The failure branch after that call (+0x67451) is NOPed.
  2. In hardware mode the control clears the z-buffer each frame by locking it and filling `0xFFFFFFFF` (+0x6625A);
     wined3d never uploads CPU writes to a depth surface, so nothing passed the depth test. Replaced by a
     `Blt(DDBLT_DEPTHFILL)`.
  3. Wine 1.7 ignores the D3DVIEWPORT2 clip volume the control relies on for its aspect ratio (dvClipX −1, dvClipY 0.68,
     dvClipWidth 2, dvClipHeight 1.36 at 616×420) and answers each `SetViewport2` by setting the projection to its own
     clip matrix alone, after the control has set its projection. The same patched block sets the control's projection
     again before `BeginScene`, with `_22` × W/H. A viewport `Clear` would also reset the projection, hence the depth fill.

  After that, `Try3DHardware` returns True, `ACTIVEX3DX3+0x11C954` (hardware flag) is 1 and the three Acropolis stops
  render with the same geometry and camera as the software device (`fixtures/tours/701504257/hw/compare-montage.png`).
  `Force3DSoftware` on Wine's ddraw draws nothing (black), so software runs use the system ddraw (`vm/d3d.sh tour-off`).

**Device probe (why the Cirrus card fails, static, OCX 0x100600F0 / 0x1005FC30).** At construction the control
requires: DirectX ≥ 5 (the SDK's GetDXVersion), `DDCAPS_3D` and `DDCAPS2_CANRENDERWINDOWED`, a display deeper than
8 bpp, an `IDirect3D2` whose enumerated device has a *hardware* description with the display depth in its render depths,
RGB colour model with Gouraud RGB shading (or mono), no square-only textures, and a viewport it can create. Status codes
go to `+0x270` (3 = no `DDCAPS_3D`, the Cirrus case). `Try3DHardware` then builds the device; any failure there sets
the flag back to 0 and the control stays on software.

### §7.3.4 Cloud re-render step (cdb + screenshots, `fixtures/tours/701504257/hw/`)
Bronze Athena (centre −2.40049, 0.54274, −2.67199, offset −π/2), eye orbiting at 2 units facing it, a cdb breakpoint on
the cloud's surface Lock (0x10019C4E) logging phi:
- 1° steps (`cloud-1deg/`, `cloud-1deg.txt`, `-cdb.txt`): re-rendered at 81, 87, 91, 97° (statue box changes by 15–19
  grey levels on those steps, ~1 otherwise). **Every 6th 1° step**, except where phi wraps 2π→0.
- 0.1° steps (`cloud-tenth-deg-cdb.txt`): renders 5.2° apart: 5.1° (0.0890 rad) is skipped, 5.2° (0.0908) renders.
  Confirms the 0.09 rad threshold, measured from the **last render**, not the last frame.
- The wrap: phi is in [0, 2π) and the test is on the raw difference, so crossing 0 (geometric 90° for an offset of −π/2)
  forces a render after 4° (357° → 1°).
- Behind the eye and 0.9 × FOGEND hold in hardware mode (`cloud-rules-cdb.txt`): facing away, no render in 21 steps;
  FOGEND 10: renders every 6° at 3-D distance 8.903, none at 9.103, again at 8.903.

### Ground mode (correction of "Tours under the debugger", §6.8; `hw/gravity-mode.txt`)
The walk routine (0x10022000) sets walker +0x54 on **every walk step**: 1 (gravity) if the hardware flag or the
second flag 0x1011C958 is set, else 0 (snap). 0x1011C958 is the renderer's +0x10F0, only ever initialised to 0. The
initial value is 1, which is what the earlier pass read at the start pose. Measured: hardware +0x54 = 1 while walking;
software (system ddraw) +0x54 = 0 after the first step. An eye raised 0.6 with `CAMERA.POS` falls gradually before
any walk step (0.205, 0.196, 0.184, 0.151, … sampled every ~20 ms) and is on the ground by the first sample after it. So software users (and everyone in the VM) get
gravity only until they first walk, then snap; hardware users always get gravity.

### MaxView (§4; cdb in ENCARTA.EXE, static layout, `fixtures/tours/maxview/`)
- **MaxView is the normal tour page.** The media type table maps `virttour` to `CMaxView`, and the subtype `3DVT` to
  `CMax3DVTView`; opening the Acropolis (`msencarta://id=701504257`) constructs `CMax3DVTView`, `CMax3DVTWnd` and
  `C3DVirtToursWnd`, in that order. There is no other tour window and no "maximise" step. `CMaxView` is the family of
  full-pane media views (also `MaxTimelineWnd`, `MaxMapWnd`, 2D tours `maxvtwnd`, `maxshockwavewnd`).
- It does not depend on the 3D device: the host calls `Try3DHardware` when the undocumented profile value
  `DxHardware` (HKLM `Software\Microsoft\Microsoft Reference\E09AXLRD`, profile id 0x10D
  `PROFILE_UNDOC_3DVT_USEHARDWAREACCEL`, installed as 1, no UI) is set, else `Force3DSoftware`; nothing else changes.
  With the WineD3D device (`encarta-acropolis-hardware.png`) the page is pixel-identical outside the 3D picture.
- Layout (C3DVirtToursWnd OnSize, 0x4D6D50; client cx × cy): site map (0, 19, 172 × 194); stop text from y 214 to the
  bottom, 172 wide; the control is always **616 × 420**, at x = 172 + (cx − 790)/2 (1 if cx − 174 < 617) and
  y = 19 + (cy − 495)/2 (1 if cy − 75 < 421), clipped to the client; the title banner at (172, cy − 53), 54 high; the
  menu items right-aligned along the top. Maximised at 1024×768: client 1022 × 629, control at (288, 86), screen
  (289, 195). Restoring the window keeps 616 × 420 and re-centres it (`encarta-acropolis-restored-window-hardware.png`).
  FOV stays the scripts' 80. It is left like any page (Back, Home, links).
- The control's own full-screen mode (`TRYFULLSCREEN`, `TRYHARDWAREFULLSCREEN`; hidden `_dot` button, Colosseum
  `LARGE` macro, never reachable in 2009): a modal loop in `Do` that hides and maximises the host's main window, renders
  at the display mode's size, and ends on Esc (DirectInput) or `KILLFULLSCREEN`; `TRYHARDWAREFULLSCREEN` goes back to
  software afterwards if it was software. Tried in tourhost on WineD3D: entered and left with Esc (`Do` returned
  `Success`), but the picture was broken (the patches assume 616 × 420), so its look was not observed.

### Other hardware-mode differences
- Textures are filtered (bilinear, mipmaps) in hardware; software samples point. Same geometry, camera and captions.
- The colour-keyed statue shows no magenta fringe on WineD3D, which applies the key before filtering; a 2009 driver
  could differ (§5.10 stays open for real hardware).
- Per-frame work moves to the device: the control sets world/view/projection with `SetTransform` and draws
  `D3DLVERTEX` fans with `DrawIndexedPrimitive` (lighting is still the control's own, in the vertex colours); the cloud
  image goes through a system-memory copy (§7.3.4 step 5).

## VM pass 5 (2026-10-15): tours, decided without a 2009 driver

No VM was run: the three questions were settled from the control's code (static, `analysis/decompiled/`), the
converted data and the earlier cdb logs. Scripts in the session scratchpad; nothing new in `fixtures/`.

### Snap-mode pitch drift (§6.8, §6.9)
- **The x87 runs at single precision.** A float32-per-operation model of the ground step (0x10026543–0x100265C3:
  `targetY = HEIGHT + g.y` on the FPU stack, g.y stored as float32) replays `target/agent-flash/fall.log` (the
  "Tours under the debugger" write breakpoint on the eye's Y, software device, gravity at the Acropolis start)
  **bit for bit: 1,090 writes of E.y, T.y and vfall in its two runs**; 53- and 64-bit intermediates fail at the first
  write. This corrects §6.9's "`targetY − E.y` computed in higher precision".
- Snap mode runs the same landing instructions (δ = target − E.y added to eye and target, then a second landing if
  E.y is still below). On level ground it settles after the first tick and never drifts; on slopes the ground point
  depends on E.y, and where the eye alternates between two float32 heights T.y walks. Census over all 12,036 ground
  cells (centroid, pitch 21°, 2,000 ticks at rest): 39 drift in snap mode, 146 in gravity mode. At the Acropolis start
  pose snap mode is drift-free (T.y − E.y identical at tick 10 and 3,840), so the fourth 6° step from 21.000036 would
  be refused there. Conclusive from the model, so not checked under cdb. Runtime: `sim/camera.ts` `followGround`,
  `sim/walkmap.ts` `groundY`.

### Colour-key fringes (§5.5, §5.10; inferred, not observed)
- The device setup (0x10067390–0x10067648) sets `COLORKEYENABLE` 1, `ALPHATESTENABLE` 0, `ALPHABLENDENABLE` 0.
  Microsoft's D3D driver pipeline (DDK; NVIDIA, "Color Key in Direct3D", 2000) sets a keyed texel's alpha to 0 before
  filtering and keeps its colour, and with alpha test off the driver tests alpha > 0 itself. So a 2009 driver drew a
  magenta fringe about one texel wide on every cut-out (more where magnified, pink shimmer on distant keyed
  lattices, keyed textures having no mipmaps). WineD3D's clean edges do not follow that pipeline. Implemented for the
  hardware device (`render/materials.ts`, `render/view.ts`); checked in headless Chrome at the Bronze Athena and
  Athena Parthenos stops (fringe on hardware, none on software).

### Full screen (§4.0.1; reachable, look inferred)
- **Reachable after all**, as an easter egg: `_dot` (4×4 black, top-left) runs `SCENE.TRYHARDWAREFULLSCREEN` on
  mouse-down in five tours, and one walk-map hotspot cell per tour makes it visible (acr 1709, rom 672, bea 845,
  per 1251, col 1114, abu 615, azu 115; Persepolis's macro loads `stair1.3wm`, Azuchi's is empty). Without a D3D device
  the command fails and nothing happens, which is why the VM never showed it.
- Static reading: exclusive DirectDraw at the current display mode, viewport = the whole screen, exclusive
  DirectInput (no cursor), no events to the host, Esc or `KILLFULLSCREEN` to leave, host window restored afterwards.
  The runtime enters the browser's full screen on that press (hardware device only) at the screen's size with the same
  horizontal FOV (`control.ts` `fullScreen`, `runtime.ts`).

## VM pass 5 (2026-10-15): print, Kids sounds, unread data
Lane `target/vm1` (agent 7807, tour 7808, VNC :8) from `d3d-ready`. Sounds as in pass 3: `--wav` usb-audio, cdb on the
process with breakpoints on the `ENCARTAR.DLL` loader (`0x411860`) and every `winmm` play API, slices matched with
`sndmatch.py`. Helpers and logs: `target/agent-vmsounds/` (`act.sh`, `k.sh`, `pressk.py`, `sl.py`, `snd.cdb`, `wp/*.cdb`,
`wp/*.log`). Captures: `fixtures/ui/print2/`.

### Print (media.md §9)
- Printer: XP's built-in "MS Publisher Imagesetter" PostScript driver on `FILE:` (`printui.dll,PrintUIEntry /if ... /r
  "FILE:"`, driver from `C:\WINDOWS\Driver Cache`), made default. Not in any snapshot: without it Print says "There is no
  default printer" (`no-default-printer.png`).
- Paths: article File › Print / Ctrl+P → Encarta's Print dialog (`encarta-print-article.png`; picture variant
  `encarta-print-picture.png`; timeline variant "All events on this page / All events in current timeline", which also
  switches the timeline to its list view, `encarta-print-timeline.png`; Kids variant with only Whole Article Text,
  `kids-print-article.png`). For a long article both **Print** and **Print Setup** ask "longer than 5 pages" (Yes/No;
  `encarta-longer-than-5.png`; No cancels) and then open the same Windows Print dialog (`windows-print-general.png`,
  `-options.png`). The timeline's Print prints straight to the default printer. The atlas map's File › Print opens the
  old-style Windows Print dialog directly (`atlas-print-windows.png`); the dictionary's Print link opens the Windows
  dialog from `EDICT.EXE` (`dictionary-print-windows.png`). No Print Preview anywhere.
- Sounds: Encarta's dialog push buttons tick 31914 on rollover and click 31916 on release; radios click 31917; the
  long-article prompt clicks 31916 on Yes/No with no rollover; the Windows dialogs, "Print to File" and printing are
  silent apart from Windows' navigation click around the dictionary's print. From the keyboard (arrows, Tab, Space,
  Enter, Esc) the dialog plays nothing; the Options dialog's check boxes and radios are likewise silent from the keyboard.
- Menus (adult and Kids, same code): entering a menu-bar title ticks 31971 (menu open or not); clicking an enabled item
  plays 31973 on press and 31974 on release (both the `click` audio); disabled items and separators are silent. Alt+F
  and arrow keys tick. A release that lands on a round toolbar button after a menu closes plays its 31974 (artefact).

### Kids process sounds (cdb on Kids `ENCARTA.EXE`, the same binary as the adult one)
- Exercised: home (all category panels, subcategory lists), category pages and tiles, articles, Look at Pictures and the
  media player, search (results, "Sorry" page, misspellings), Launch Activity (Flash interactive), Games and Fun Stuff
  (Word Play, Spelling Bee), all six menus and their submenus, Find in This Article, Add to Favorites, Organize Favorites,
  About (its four buttons), Print, Copy, Read Aloud (F8). Kids has no dictionary window (a double-click only selects)
  and no timeline.
- Ids played: 31971 (menu titles and highlights), 31973/31974 (menu items), 31975 (round buttons and search arrow
  rollover, enabled only), 31976/31977 (round button press/release), 31914/31916/31917 (dialogs), 31979-31988 (home
  panels) and 31973 on a panel click. Nothing else, so **no unheard id plays in Kids either**. Flash games, the video
  and audio players and Read Aloud make their own sound outside these APIs.
- Kids' flag word (`0x6F0F44`) read 7 with menu sounds on at attach time, and its registry key
  (`HKCU\Software\Microsoft\Microsoft Reference\K09ADXRC`) stores no settings: Kids' "Menu and Button Sounds" is **on
  by default**, the adult option off.
- Web client (`web/src/settings/uiSounds.ts`, `settings.tsx`, `kids/KidsShell.tsx`): menu-bar titles tick on rollover;
  menu-bar items click on press and release (the Features flyout keeps only its rollover tick, unchanged); keyboard
  clicks are silent; Kids uses its own `kidsMenuSounds`, default on. The print dialog already matched (push buttons tick
  and click, radios click). Print Setup and the "longer than 5 pages" prompt were added afterwards (media.md §9).

### Unread data: LEX and TIMELINE.DAT (formats/lex.md, timeline.md)
- **Static** (`analysis/decompiled/lexscan/NOTES.txt`, new Ghidra export of `EDICTEIT.EBK`): of all PE files on the disc only
  `MSSPELL3.DLL` holds the LEX magic and only `ENCARTA.EXE` names `timeline.dat`. LEX opens come from `ENCARTA.EXE` (search
  speller) and `EDICTEIT.EBK` in `EDICT.EXE` (a looked-up word that is not found). The stamp dword is copied out once and
  both callers drop it; the copyright is copied only into a caller buffer, which neither passes; the walker reads body
  +0x05..+0x07 only for arc flags 0x004/0x008, absent from all 12 files; body +0x20 is the count of an empty table, read
  at open. The parser takes the stream offset from body +0x28 (lex.md said +0x14). TIMELINE: the term-table,
  reference-pool and item-table pointers are write-only globals.
- **Watchpoints** (cdb `ba`, armed by a breakpoint right after the file is located): adult `ENCARTA.EXE` (about 20
  searches, misspellings with "did you mean" lists and an auto-correction; control watch on the first stream node hit
  232 times), Kids (EESPLA01 is re-opened and freed for every search), `EDICT.EXE` (DICTA, THESA and all eight BIL files
  through misspelled dictionary, thesaurus and translation lookups in every pair). Hits only at open: version check and
  stamp copy, body +0x20 count, and `IsTextUnicode`'s scan of the whole view (the map routine's text sniff, which also
  catches the previous file's watchpoints when the next lexicon is mapped at the same address). Timeline (adult):
  watches on both pointer globals and on offset A of terms 0 and 11,374 stayed silent through zoom, pan, era links,
  pop-ups, Find Topic or Year (five searches), Category and Place, Compare and View as List. Kids never calls the loader.
- Result: the copyright's trailing u16, body +0x04 and TIMELINE offset A are **never read by Encarta 2009**; the stamp is
  read only to be copied and dropped (no effect); body +0x20 is an (empty) table count.

### Still open
Whether a click on a submenu parent (an item with ▸) also plays 31973/31974 (not tried); the Features flyout items'
sound (pass 2 heard none; our client now matches and is silent there).
