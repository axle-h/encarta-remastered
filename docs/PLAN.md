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

Nothing queued. Possible follow-up, Atlas size: `earth.pmtiles` is still 399 MB; the water/land split stays for the
classes that are hit-tested (the large ones) or drawn over water by a climate
style (atlas-output.md "Draw-over masks"). Needs the VM first: does Encarta's click menu list an area under the sea?

Done 2026-10-14 (atlas size): the water parts of split polygon classes are dropped when no style draws the fill over
that style's water pixel class and the class is not interactive (`ClassUse::fill_water`, `needs_water_parts`). Saving
only 0.3 MB (399.05 → 398.79 MB; 108 classes, none large), so the plan's expectation of a real cut did not hold.
Re-run `ENCARTA_ATLAS_ONLY=vector encarta-convert atlas` to get it.

Done 2026-10-14 (sounds): the startup sound (`INTROSOUND` of ENCSPLSH.DLL), a click for buttons, menu items and tabs, and
the Kids home category hover/click sounds, extracted from the DLLs' `WAVE` resources to `out/sounds/` by
`encarta-convert content` and served at `/sounds/{name}.wav`; both Options check boxes now act (media.md §9, api.md).
Re-run `encarta-convert content` to get `out/sounds/`. Which original sound belongs to which menu or button is not
known (31906-31978, in Needs the VM).

Done 2026-10-14 (performance): `Cache-Control: public, max-age=3600` on API answers (randomised games and unseeded
Mind Maze draws, errors: `no-cache`); WOFF2 copies of the eight MS Reference faces written by `encarta-convert content`
(about 53 % smaller, TTFs kept for Ruffle), listed first in the client's `@font-face` (api.md "Fonts"). Re-run
`encarta-convert content` to get the `.woff2` files.

Done 2026-10-14 (atlas): alternative-name line (`w6`), road shields (labels of shield road classes, our own
sprites), area-pen draw-over masks, flat-view tiles in Web Workers, time zones out of the click menu (pen group
134), symbol layers stacked by priority (mad-styling.md §7.3, §8.1, §5.2, §16; atlas-output.md; atlas-client.md).

### Needs the VM
- Atlas: whether an area is listed in the click menu where it lies over the sea (decides if the water parts of
  interactive classes can go, size not measured).
- Sounds: which of the 49 ENCARTAR.DLL sounds (ids 31906-31978) the original plays for which menu or button.
- Atlas: V15 (Mumbai's "Bombay" line, Bolzano/Bozen), V16 (click on the Time Zones map), label placement cost
  (mad-styling.md §7.4, §8.1).
- Panoramas: single panoramas' `initangle`, panorama zoom.
- Tours: whether the Music toggle starts on or off and what it plays (no sound device in the guest; we loop the tour's ambient record), the exact on-box test for the light-blue marker, the Azuchi site map, the look-vector value behind
  the 45° pitch test, the remaining **[VM]** items in [tours-3d.md](formats/tours-3d.md) §10.
- UI never captured: the ▾ tab under the search box, the Quick Search dialog, the Kids menu bar, Encarta's own
  ranking; rows marked *(ours)* in [parity.md](parity.md).
- Data: content-schema §11 questions (placement: primary-item spacing in four captured sections); a few field
  meanings in the small formats (1.5).
- Interactives: Flash is not installed in the guest, so nothing to compare the Ruffle rendering with (fonts: the
  disc lacks Trebuchet MS; Vasco da Gama's ocean fill went missing once and was not reproduced).
- Music: how the original's Microsoft GS Wavetable Synth renders the MIDI music (ours plays the Fluid R3 GM
  soundfont, checked against FluidSynth only; media.md §8).
- VM tooling: sound in the guest, a debugger workflow.
