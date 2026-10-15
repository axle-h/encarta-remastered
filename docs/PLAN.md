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

Found in the second VM pass, not built yet:
1. Favorites folders and New Folder; the Copy dialog's "With Media" choice.
2. **Geography Quiz fanfare**: a correct answer plays a ~2 s fanfare in Encarta that is not on the disc (likely
   Windows' tada.wav); make a short fanfare of our own (synthesised, nothing from Windows). S
3. **VM with Flash Player and a debugger** (approved by Alex 2026-10-14): install both in the guest (download on the
   host, move in through the exchange disk), then settle the Flash- and debugger-blocked items under "Needs the VM".

Decided by Alex 2026-10-15: keep our MIDI reverb and 0.3 s ring-out (Encarta's output is dry and mono and cuts at the
last event, media.md §8); keep the tour menu's "Show Labels and Hot Spots" as an extra (Encarta 2009 has only its
strings).

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
- Atlas: the order of the three fallback label corners, how overlap cost weighs against it, and the shield
  centre-vs-overlap cost (mad-styling.md §8.1); the click menu's order in places. Needs the code or a debugger.
- Sounds: where the unheard ENCARTAR.DLL sounds play (578, 579, 682/31972/31978, 702/31909, 31906-31908, 31910/31911,
  31912, 31913). Not tried yet: Geography Quiz score dialog, Read Aloud, Print and Copy dialogs, Organize Favorites,
  Help windows; 578 and 579 are nearly silent and may never play (media.md §9).
- Panoramas: single panoramas' `initangle` (their viewer is Flash).
- Tours: the pitch-limit arithmetic (debugger); gravity mode, the hardware cloud step and MaxView (3D device); the
  water ripple's step rate; what `3D.TOGGLEHIDDEN` does to the picture; ITS-mode loading ([tours-3d.md](formats/tours-3d.md) §10).
- Data: the media-placement residue: an empty item slot (`Child_s<n>`) in four sections that no record we read
  explains (debugger; content-schema §11); MSWORLD.SDB column B, TIMELINE.DAT field 5 and offset A, the P2MTBL and LEX
  header words (no screen shows them).
- Interactives: Flash is not installed in the guest, so nothing to compare the Ruffle rendering with (fonts: the
  disc lacks Trebuchet MS; Vasco da Gama's ocean fill went missing once and was not reproduced).
- Music: a reference recording with pitch bends, controllers, drums and several channels; how Encarta loops
  MindMaze music.
- VM tooling: a debugger workflow.
