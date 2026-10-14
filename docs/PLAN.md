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
| 2 Converter (`encarta-convert content / media / atlas / tours / search`) | done | [content-schema.md](content-schema.md), [media.md](media.md), [atlas-output.md](atlas-output.md), [tour-scene.md](tour-scene.md) |
| 3 Server (`crates/encarta-server`), deployed | done | [api.md](api.md), `k8s/README.md` |
| 4 Web client core | done | [web-client.md](web-client.md) |
| 5 Atlas | done | [atlas-client.md](atlas-client.md) |
| 6 3D tours | done | [tour-runtime.md](tour-runtime.md) |
| 7 Games | done except the board games | [games.md](games.md) |
| 8 Parity audit | in progress: the list below | [parity.md](parity.md) |

Dropped by choice: parental controls (the dictionary's offensive-word filter); like Wikipedia, nothing is hidden.

## Next (priority order)

1. **3D tours**, from the VM pass: Music from the seven "3DVT: Ambient Audio" records (701504966–701504972), the
   site-map marker in light blue 0x6371B5 on a location box. S
2. **Ruffle gaps** in the IAF player (Vasco da Gama map fills; the `MatchGame` script limit) and a test pass over
   Kids copies, DragDrop/CrossFade/ZFL/BGA controls, SWQUIZ media and timeline pop-ups. 860 interactives. M
3. **Board-game engine** for the 10 `BGAControl` games (Flash boards → Ruffle); their quizzes play without boards. L
4. **Geography Quiz**: the VM's full score table, Try Again reopening the start dialog, the 9-tick slider look. S
5. **MIDI soundfont**: 236 music files sound synthetic. M
6. **Timeline**: open items in the Timeline view at the event; media in event pop-ups; essay bar pictures. S
7. **Accessibility**: media viewer focus trap, timeline list alternative, axe pass. S
8. **Article layout**: the inline-table box border; country introductions and overflow into the next section
   (content-schema §11). S
9. **Atlas**: alternate label style (w6), area-pen masks, road-shield source, flat-view re-tiling off the main
   thread, time zones in the click menu (V11 shows none). M
10. **Performance**: `Cache-Control` on API responses; WOFF2 fonts (a converter step). S
11. **Sounds**: startup and menu sounds (the Options setting is stored only). S

### Needs the VM
- Panoramas: single panoramas' `initangle`, panorama zoom.
- Tours: what the Music toggle plays (no sound device in the guest), the Azuchi site map, the look-vector value behind
  the 45° pitch test, the remaining **[VM]** items in [tours-3d.md](formats/tours-3d.md) §10.
- UI never captured: the ▾ tab under the search box, the Quick Search dialog, the Kids menu bar, Encarta's own
  ranking; rows marked *(ours)* in [parity.md](parity.md).
- Data: content-schema §11 questions; a few field meanings in the small formats (1.5).
- VM tooling: sound in the guest, a debugger workflow.
