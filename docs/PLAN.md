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

Nothing queued. Re-run `encarta-convert content` (timeline `date_text`, three new UI sounds), `encarta-convert search`
(ranking fixes) and `ENCARTA_ATLAS_ONLY=vector encarta-convert atlas` (sea-click split, rule glyph faces) to bring
`out/` up to date.

Done 2026-10-14 (VM pass, "Needs the VM"): sound in the guest (USB audio hot-plug recorded to a WAV, vm.md); interface
sounds mapped by matching the recording against the DLL sounds (rollover tick 31971, press/release click 31973, Kids
rollover 31975, timeline 575; menu sounds off by default; media.md §9); tour Music heard (on at start, ambient record,
restarts, loops); the site-map marker blinks dark/light blue every 0.715 s (tours-3d.md §4.2), Azuchi's rotated site
map matches; atlas V15 (alternative-name line with a rule between the lines) and V16 (no time zone on its own map)
confirmed, and no land area is listed over the sea, so area classes are split at the coast and only land parts are
hit-tested (earth.pmtiles 398.79 → 400.28 MB: splitting the unmasked interactive classes costs more than dropping the
water parts saves); timeline pop-up dates ("120,000 years ago", "4.60 billion years ago"); sidebars `SBA`/`SBP` look
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
- Atlas: label placement cost (mad-styling.md §7.4, §8.1); whether water bodies, World, tectonic plates and reefs are
  listed over the sea (assumed yes).
- Sounds: where the unheard ENCARTAR.DLL sounds play (578, 579, 682/31972/31978, 702/31909, 31906-31908, 31910/31911,
  31912, 31913): not in home, browse, search panel, article, atlas, timeline or Kids home; perhaps the dictionary
  window, media players, Visual Browser or games.
- Panoramas: single panoramas' `initangle` (their viewer is Flash).
- Tours: the look-vector value behind the 45° pitch test (needs the debugger), the remaining **[VM]** items in
  [tours-3d.md](formats/tours-3d.md) §10.
- UI: rows marked *(ours)* in [parity.md](parity.md).
- Data: content-schema §11 (primary-item spacing residue, DATAF `pn`, right-column modules); a few field meanings in
  the small formats (1.5).
- Interactives: Flash is not installed in the guest, so nothing to compare the Ruffle rendering with (fonts: the
  disc lacks Trebuchet MS; Vasco da Gama's ocean fill went missing once and was not reproduced).
- Music: a side-by-side comparison of our soundfont synth with the GS Wavetable renders in `fixtures/music/` (media.md §8).
- VM tooling: a debugger workflow.
