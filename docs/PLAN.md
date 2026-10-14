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

1. **Atlas**: alternate label style (w6), area-pen masks, road-shield source, flat-view re-tiling off the main
   thread, time zones in the click menu (V11 shows none). M
2. **Performance**: `Cache-Control` on API responses; WOFF2 fonts (a converter step; keep the TTFs, which Ruffle
   needs). S
3. **Sounds**: startup and menu sounds (the Options setting is stored only). S

### Needs the VM
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
