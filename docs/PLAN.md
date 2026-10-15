# Plan

**Goal:** every piece of content on the Encarta Premium 2009 DVD, usable in a modern web browser and
served by a Rust server, with **no content lost and no feature dropped for lack of effort**.

**Architecture:** an offline, one-time **converter** reads the disc and writes modern formats into
`out/`. A **server** serves `out/` over HTTP, and a **web client** renders it. Work follows the
[clean-room policy](CLEAN_ROOM.md). Format knowledge lives in [`docs/formats/`](formats/inventory.md).

Effort key: S ≈ days, M ≈ 1–2 weeks, L ≈ 2–4 weeks.

## State

| Step | State | Docs |
|---|---|---|
| 0 Foundations: container reader, XP reference VM | done | [vm.md](vm.md) |
| 1 Decoders (`crates/encarta-formats`) | done | [formats/](formats/inventory.md) |
| 2 Converter (`encarta-convert content / media / atlas / tours / search / soundfont`) | done | [content-schema.md](content-schema.md), [media.md](media.md), [atlas-output.md](atlas-output.md), [tour-scene.md](tour-scene.md) |
| 3 Server (`crates/encarta-server`), deployed | done | [api.md](api.md), `k8s/README.md` |
| 4 Web client core | done | [web-client.md](web-client.md) |
| 5 Atlas | done | [atlas-client.md](atlas-client.md) |
| 6 3D tours | done | [tour-runtime.md](tour-runtime.md) |
| 7 Games | done; all ten board games played to an ending under Ruffle, picture answers included | [games.md](games.md) |
| 8 Parity audit | in progress: the list below | [parity.md](parity.md) |

Dropped by choice: parental controls (the dictionary's offensive-word filter); like Wikipedia, nothing is hidden.

## Next (priority order)

Nothing queued; what is left (mostly blocked: no printer, unread code) is under "Needs the VM" below.

Done 2026-10-15 (VM pass 4, 3D device): WineD3D 1.7.55 (wine9x) over Mesa9x 23.1.9 llvmpipe, app-local in `C:\d3d`
(`vm/d3d.sh`, snapshot `d3d-ready`; the control needs three in-memory cdb patches on WineD3D, vm.md). **Cloud step**
confirmed: 0.09 rad from the last render (5.1° kept, 5.2° re-rendered), forced at the unwrapped 2π crossing (§7.3.4).
**Ground mode corrected**: every walk step rewrites the walker flag, so hardware is gravity throughout and software
snaps after the first walk step (§6.8). **MaxView** is the normal tour page (control fixed 616×420 beside a 172-px
panel, FOV 80, §4.0), not a hardware mode; `TRYFULLSCREEN` is unreachable in 2009. Hardware filters textures
(bilinear, mipmaps), software point-samples (§5.10). The runtime now defaults to the hardware device (Encarta
installs `DxHardware` = 1), with the ground mode and texture filtering following `device`; the dev page has a
Device selector (tour-runtime.md).

Done 2026-10-15 (VM pass 3, the rest of "Needs the VM"; three VM lanes from `flash-ready`, cdb, nothing installed):
**Atlas** (from MOBB200/ENCARTA.EXE code, checked against the captures): point labels try above right, below right,
above left, below left (centred first without a symbol) at added costs 250–1000, but the cheapest box by what lies
under it and the neighbours' reserved boxes wins, so the order only breaks ties; one shield per run, centre cost plus
neighbour cost; the click menu is MOBB's sort (label hits, points, routable roads, lines, areas; class scale; GeoObj),
and the time-zone exclusion now goes by row (it listed time zones and dropped Mexico/US shield roads; mad-styling.md
§4.5, §7.4, §8.1; `atlas/style.rs`, `vector.rs`, `web/src/atlas/hit.ts`). **Tours**: the water ripple steps once per
drawn frame below 80 fps (skip `trunc(fps·0.025)−1` above 40 fps), with the step's border, rounding and drop rules
corrected; `3D.TOGGLEHIDDEN` is `VISIBLE` on every object, and the odd picture is `CAMERA.CLEARBACKGROUND=0` keeping
the colour buffer, now reproduced; ITS mode reads the same files through ENCARTA.EXE's own `msencdata:` protocol, so
nothing changes (tours-3d.md §2.2, §7.4.3, §7.4.6, §10; `sim/effects.ts`, `render/view.ts`). **Sounds**: 578/579 are
the tour site-map hover and pick sounds, now played on our site maps; the other unheard ids are never played by the
original; dialog buttons tick on rollover (media.md §9, `uiSounds.ts`, `TourView.tsx`). **Placement**: the empty
`Child_s<n>` slots are the reader's reserved Archives ("History" section, else first free slot) and Further Reading
(last free slot) items; placement now matches all 1,660 captured items (content-schema §2, `content/article.rs`).
**Music**: three anthems and two MIDI test files of our own recorded through GS; the synth now follows GS on pan
(balance), mod wheel (6 Hz, held notes), pedal threshold, release by family and drum levels (media.md §8); MindMaze
music has no 2009 playback path. **Data**: MSWORLD column B is significant digits, TIMELINE field 5 a UTF-16 flag,
offset A never read, P2MTBL `8`/`1` the directory count and first key, and LEX body +0x0C–0x12 the descriptor region
lengths (cdb: MSSPELL3 opens EESPLA01 on the first search). Unit tests: web 360+, `encarta-convert` pass. Re-run
`encarta-convert content`, `ENCARTA_ATLAS_ONLY=style` and `=vector encarta-convert atlas`. Not run in a browser.

Done 2026-10-15 (VM with Flash Player and a debugger): Flash Player 10.3.181.16 ActiveX and cdb (Debugging Tools 6.12)
installed in the guest, live snapshot `flash-ready`, `vm/dbg.sh` (vm.md). **Panoramas**: single panoramas open with the
view centre `initangle` degrees right of the picture's *middle* (180 = the seam), not of its left edge, and auto-pan at
`auto` view px per Flash timer tick (≈ 52/s, ≈ 26 °/s, was 3 °/s); fov 80 and the horizon row confirmed
(`panorama/params.ts` `openingYaw`, `measured.ts` `SINGLE`; web-client.md). **Tours**: Encarta's software device runs
gravity mode (walker flag set; the eye falls `vfall += GRAVITY` per tick), now the default for both devices; the
pitch keys' 45° test is on the pitch derived from the float32 look vector, which each tick's fall and landing make
drift below the stored `PITCH?` value, now modelled (`sim/camera.ts` `lookY`/`lookPitch`; tours-3d.md §6.8, §6.9,
tour-runtime.md). **Interactives**: seven control types compared with Ruffle: drawing matches, Tahoma for Trebuchet
MS rewraps a few paragraphs, nothing extra clipped (vm-findings "Flash and debugger pass"). Unit tests updated and
added (web 348 pass); not run in a browser.

Done 2026-10-15 (Geography Quiz fanfare): a right pick plays a 2 s fanfare of our own, synthesised in the browser with
Web Audio (C-major arpeggio of sawtooth voices into a held chord; `games/geoquiz/fanfare.ts`, games.md). Played whatever
the menu-sounds option says. Unit tests added; not heard in a browser.

Decided by Alex 2026-10-15: keep our MIDI reverb and 0.3 s ring-out (Encarta's output is dry and mono and cuts at the
last event, media.md §8); keep the tour menu's "Show Labels and Hot Spots" as an extra (Encarta 2009 has only its
strings).

Done 2026-10-15 (Favorites folders, Copy With Media): Add to Favorites has "Create in:" and New Folder…; Organize
Favorites has New Folder, folder rename/delete and a per-page folder choice; folders show as submenus of the Favorites
menu (one level; `app/favorites.ts`, `AddFavoriteDialog.tsx`, `pages/FavoritesPage.tsx`). The adult Copy dialog offers
Whole Article With Media (`commands.ts`). Unit and component tests added; not run in a browser.

Done 2026-10-15 (double-click lookup): double-clicking a word in an article or Map Trek stop text opens the dictionary
on it (`/dictionary?q=`, `/kids/dictionary?q=` in Kids); `web/src/app/wordLookup.ts`, web-client.md. Not run in a browser.

Done 2026-10-15 (VM pass 2, "Needs the VM"): **atlas**: the `b4` high nibble of PDCAltitudeGrid is the hit side
(2 water: listed only over water; 1 land: dropped only when a water class is listed; 0 anywhere: World, plates,
parks, statistical regions); only water-side classes are split at the coast, drawn labels are clickable, and point
labels go above right first, then the other corners (mad-styling.md §4.4, §7.4; earth.pmtiles about 404.9 MB).
**UI**: every *(ours)* row captured (vm-findings "UI pass 2", parity.md): the Copy source line, search-panel
geometry and colours (Quotations only in Quick Search), games under Home › Games & Fun Stuff, Map Trek panel,
yellow-arrow tooltips, Read Aloud states, Add to Favorites name dialog. **Data**: DATAF `pn` is a page number
(ten sections a page, unused in 2009); `STE`/`STC` are never read (Premium uses `DLX`); heading look follows section
depth only; the right-column modules (lists, order, sorting, row counts) are built (`article/modules.ts`, checked
against 25 captures); quiz `D*.TXT` line 3 is how many wrong answers come from the answer's own region
(`same_region`, `games.rs`). **Tours**: 13 of the §10 items measured (tours-3d.md §10, vm-findings "Tours pass
2"): tick-rounded flicker, flame and spinner periods; lighting without faceColor; clouds, sprites, water, back
faces; LOD at 13 XZ units with per-cell full-or-`lod_` choice and always-drawn sprites; caption widths from the
captured System font widths, strict-inside clipping, 1 px `_L` outline; open edges do not stop the camera; floors
switch once. **Music**: our synth matches the GS renders in tempo (0.15 %), onsets (7 ms), programs and velocity
law; a looped song now restarts from its initial controller state (media.md §8). **Sounds**: a second search
(dictionary, players, Visual Browser, quizzes, dialogs, Map Trek, Kids pages) found none of the unheard sounds.

Done 2026-10-14 (VM pass, "Needs the VM"): sound in the guest (USB audio hot-plug recorded to a WAV, vm.md); interface
sounds mapped by matching the recording against the DLL sounds (rollover tick 31971, press/release click 31973, Kids
rollover 31975, timeline 575; menu sounds off by default; media.md §9); tour Music heard (on at start, ambient record,
restarts, loops); the site-map marker blinks dark/light blue every 0.715 s (tours-3d.md §4.2), Azuchi's rotated site
map matches; atlas V15 (alternative-name line with a rule between the lines) and V16 (no time zone on its own map)
confirmed, and no land area is listed over the sea, so area classes are split at the coast and only land parts are
hit-tested (earth.pmtiles 398.79 → 400.28 MB: splitting the unmasked interactive classes costs more than dropping the
water parts saves; superseded by VM pass 2's hit sides); timeline pop-up dates ("120,000 years ago", "4.60 billion years ago"); sidebars `SBA`/`SBP` look
like every other sidebar; the docked search panel, ▾ tab, Quick Search dialog and the Kids menu bar with its submenus
built from the captures; MIDI items autoplay; GS Wavetable reference renders saved (`fixtures/music/`); search ranking
compared on 8 queries, with two fixes (no Kids-only items in the default search; a title minus its parenthetical is an
exact title; vm-findings.md).

Done 2026-10-14 (atlas size): the water parts of split polygon classes are dropped when no style draws the fill over
that style's water pixel class and the class is not interactive (`ClassUse::fill_water`, `needs_water_parts`). Saving
only 0.3 MB (399.05 → 398.79 MB; 108 classes, none large), so the plan's expectation of a real cut did not hold.
Re-run `ENCARTA_ATLAS_ONLY=vector encarta-convert atlas` to get it. (Superseded by the VM pass above: the interactive
condition is gone.)

Done 2026-10-14 (sounds): the startup sound (`INTROSOUND` of ENCSPLSH.DLL), a click for buttons, menu items and tabs, and
the Kids home category hover/click sounds, extracted from the DLLs' `WAVE` resources to `out/sounds/` by
`encarta-convert content` and served at `/sounds/{name}.wav`; both Options check boxes now act (media.md §9, api.md).
Re-run `encarta-convert content` to get `out/sounds/`. The per-control mapping was measured in the VM pass above.

Done 2026-10-14 (performance): `Cache-Control: public, max-age=3600` on API answers (randomised games and unseeded
Mind Maze draws, errors: `no-cache`); WOFF2 copies of the eight MS Reference faces written by `encarta-convert content`
(about 53 % smaller, TTFs kept for Ruffle), listed first in the client's `@font-face` (api.md "Fonts"). Re-run
`encarta-convert content` to get the `.woff2` files.

Done 2026-10-14 (atlas): alternative-name line (`w6`), road shields (labels of shield road classes, our own
sprites), area-pen draw-over masks, flat-view tiles in Web Workers, time zones out of the click menu (pen group
134), symbol layers stacked by priority (mad-styling.md §7.3, §8.1, §5.2, §16; atlas-output.md; atlas-client.md).

### Needs the VM
Left after VM pass 3 (vm-findings "VM pass 3"):
- Tours: colour-key fringes on a real 2009 driver and the control's full-screen look (WineD3D shows neither);
  whether the pitch-limit drift (§6.9) is the same in snap mode.
- Sounds: Print's dialogs (no printer in the guest); the Kids process was not logged (its code names none of the
  unheard ids).
- Data: LEX body u32s +0x04/+0x20, the header stamp's meaning and the copyright's trailing u16 (not read on the search
  path); TIMELINE offset A is never read by the viewer.
- Atlas: the costs of the alternative-name layouts and which label kinds are placed first (code not read). MapLibre
  cannot weigh candidate costs or place one shield per run, so both stay approximated.
- Music: MindMaze music cannot be observed (no MIDI playback path in 2009).
