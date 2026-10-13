# 3D tour runtime (`web/src/tours/`)

The browser runtime for the converted tours (plan Step 6). Its input is `out/tours/<id>/` as
described in [tour-scene.md](tour-scene.md); the behaviour it reproduces is specified in
[formats/tours-3d.md](formats/tours-3d.md) (cited as §n). It is framework-agnostic: the Step 4
client embeds it with one call.

```ts
import { mountTour } from "./tours";
const tour = await mountTour(element, {
  baseUrl: "/out/tours", tourId: "04257",
  onEvent: (e) => { /* e.text, e.name ("WORLDLOADED", "USERLOCUPDATE", "HOTSPOTCLICKED", "USERLEVELCHANGE"), e.pairs */ },
});
tour.do("2D.OFF");            // the host's "Show Labels and Hot Spots" menu item
tour.do("camera.pos?");       // "x,y,z,yaw" in the disc's coordinates, as the host's site map expects
```

`mountTour` plays the host handshake of §2.2 (`FinishedInitializing`, `script.itsfileurl`,
`script.load=load.3sc`) unless `autoLoad: false`. `sitemap.ts` has the host's site-map marker
maths (§4.1–§4.2) for hosts that draw the map.

## Running it

```sh
cd web
npm install                 # once; versions are pinned (three 0.170.0, vite 5.4.11, typescript 5.6.3, vitest 2.1.8)
npm run dev                 # http://localhost:5173/tours.html
npm test                    # vitest: unit tests, plus integration tests over out/tours when present
npm run build               # tsc --noEmit + vite build
```

In dev, `/out` is proxied to the Rust server (`ENCARTA_SERVER`, default http://127.0.0.1:8080);
`ENCARTA_DEV_OUT=1` makes Vite serve `../out` itself. In the app, tours open at `/tours/{id}`.

In development, Vite serves the converter output `../out` under `/out/` (a middleware in
`vite.config.ts`); the Rust server (Step 3) replaces it. Regenerate the tours with
`cargo run --release -p encarta-convert -- tours`. The dev page `web/tours.html` has a tour
picker, control sizes, the labels toggle, a site-map stand-in driven by `USERLOCUPDATE` →
`camera.pos?`, a `do()` console, the event log, and switches for the ground mode, PVS and FOV
axis. URL parameters (`?tour=05919&size=640x480&ground=gravity&pvs=0&fov=vertical`) make views
reproducible. `window.tour` is the handle. `tour.advance(n)` runs n ticks and draws a frame
without `requestAnimationFrame`, for tests and hidden pages.

## Architecture

| Module | Role |
|---|---|
| `types.ts` | scene.json types (from tour-scene.md) |
| `script/parse.ts` | command text → steps (§3.2–§3.6), same shapes as the converter; used for `do()` and runtime macros |
| `script/interp.ts` | macros, alarms (§3.5), events, Timer, dispatch |
| `control.ts` | `TourControl`: all control state and every subsystem handler, the load sequencer, the tick, views, queries. No DOM or WebGL, so the whole simulation runs in tests |
| `sim/walkmap.ts` | 4096-grid cell lookup with gap closing, ground planes, walk-map cache, hotspot enter/exit |
| `sim/collision.ts` | circle-vs-wall sweep, slide, corner sweep, push-out, coverage test (§6.7) |
| `sim/camera.ts` | step, time factor, pitch keys, ground snap/gravity, mouse vector, cursor zones |
| `sim/effects.ts` | flicker, 3D film strips, water ripple, cloud impostor maths and a reference rasteriser |
| `hud/caption.ts`, `hud/layer.ts`, `hud/draw.ts` | caption width rule, 2D layer state (NEXTFRAME, anchoring, cone test, hit test, hover/press, macros), canvas painter |
| `render/materials.ts`, `render/view.ts` | three.js: materials with flicker, PVS, LOD, sprites, film strips, water, cloud impostors |
| `runtime.ts` | `mountTour`: DOM, input, fixed-tick loop, cursors |
| `sitemap.ts` | `WORLDLOADED` parsing and the marker of §4.2 |
| `dev/main.ts` | the dev page |

**Loading.** The runtime executes the converter's `loadSequence` (every leaf statement
`load.3sc` ran). It suspends at `3D.LOAD` until `scene.glb` and `pvs.bin` are built, and then
resumes one top-level `SCENE.DRAW` per tick, as in §3.7. Meanwhile alarms run, so the
`begin.32d` spinner animates at 10 fps. The integration test checks that the resulting object
and element state equals the converter's `objects[].state` and `layers[].elements[].state` in all
seven tours, and that the TS parser converts every disc statement exactly like the converter.

**Tick.** The simulation runs at a fixed `tickHz` (default 64, the XP timer rate of §6.9) from an
accumulator on `requestAnimationFrame`. Each tick polls alarms, moves the camera
(`k = clamp(100·dt, 1, 5)`, so `k = 1.5625`), follows the ground, runs the hotspot check, and
advances flicker, film strips (once per tick, like the original's once per frame) and water
(at most 40 steps/s). Per-tick quantities (pitch keys, gravity) therefore keep their original
rates whatever the display refresh rate. All world state is in glTF space; queries convert back
to the disc's coordinates.

## Parity decisions

- **Colour.** Textures `NoColorSpace`, output `LinearSRGBColorSpace`, unlit `MeshBasicMaterial`
  with `COLOR_0`. This multiplies texel bytes by the baked bytes in display space, as the original
  did, with no overbright (tour-scene.md).
- **Flicker** is a vertex-shader term: `min(255, COLOR_0 + Σ trunc(current[slot]·w))` over the
  `_FLICKER_*` sets, with one `vec3` uniform per flicker slot. `LIGHTS.OFF` and full-bright
  objects are shader switches.
- **Ground mode** defaults to snap (software device, the control's default, §6.8); the
  `device: "hardware"` or `groundMode: "gravity"` options switch to gravity. A pending VM item.
- **FOV** is taken as horizontal: projection scale `K = (W/2)/tan(fov/2)`, shared by the
  renderer and caption anchoring. The `fovAxis` option switches it. A VM item (§6.13).
- **Walk-map switching** (`walkmapSwitch`, default `settle`). Read literally, §6.5 makes the
  active map flip on every tick while the eye stands in the middle strip of the overlapping stair
  bands: each map's middle strip loads the other map, and each switch re-runs the enter macro.
  `settle` treats a switch to a cached map like a first load: the new map's current cell is the
  one under the eye, its enter macro does not run, and the switch counts as seen. Crossings
  then end on the right floor with one or two switches (tested on Beaumaris `level1`/`level2`).
  `spec` keeps the literal rule. A VM item (§6.5).
- **PVS** is strict: over a visibility cell only the listed triangle runs are drawn, by rewriting
  each draw unit's index buffer when the cell changes. As a result the flames that no PVS list
  names (Acropolis 9, Beaumaris 19, Abu Simbel 9, Roman Forum 1) are not drawn over visibility
  cells. `alwaysDrawSprites` lifts this. LOD applies inside PVS mode too: a far base mesh is
  replaced by its `lod_` mesh, drawn with the `lod_` mesh's own runs.
- **LOD** switches at 13 XZ units from the base mesh centre. Orphan `lod_` meshes never draw.
- **Sprites** rotate about Y by the camera yaw (same rotation as the camera, so the quad stays
  parallel to the view plane); pitch is ignored.
- **Cloud statues** are real impostors (§7.3.3), rendered on the GPU. Each placement has a
  W×H render target matching its placeholder texture. A points pass puts each point on one
  texel at `index = row·W + W/2 + col` (rows wrap as in the original, with no per-row clipping),
  keeps the nearest by depth (`depth = (z0·cosφ − x0·sinφ)·64 + 65536`, strict less), applies the
  500-point chunk culling at 95° and skips the last point. The placeholder sprite then shows the
  target, with magenta as alpha 0. Re-rendering follows §7.3.4: in software mode every drawn
  frame, in hardware mode only after a 0.09 rad change; never when the centre is behind the eye
  or, after the first render, beyond 0.9 × FOGEND. The 19-bit fixed point is not reproduced.
  The static point-cloud nodes in the glb are hidden.
- **Water** runs the §7.4.6 ripple on a CPU height field and displaces the original bitmap into
  a `DataTexture` that replaces the texture record while on. The drop interval is not specified;
  a drop falls with probability 1/8 per step.
- **Film strips** rewrite the object's own U coordinates (`savedU/N + (frame−1)/N`), so torches
  keep independent phases although `APPLYTEXTURE` replaced their shared texture record.
- **2D layer** is drawn on a canvas overlay in CSS pixels, in element order. Captions are
  rasterised into a 512×32 canvas each and copied with point sampling: outlined style for
  `color_key` captions, box/icon/frame style for hotspots, hover and pressed colours. Captions
  that straddle an edge or fail the cone test are not drawn and cannot be hit. Release over the
  pressed element fires `MOUSEUP`. Hotspot cursor `vthot360.cur` on hover and press; otherwise
  the navigation zones choose slots 0–4.
- **2D images are drawn flipped vertically.** The disc's 2D GIFs (`dir.gif`, `dirTxt.gif`,
  `loadtxt.gif`, `loadanim.gif`) are stored upside down, and the engine shows them upright. So
  its surfaces hold rows bottom-up relative to the file, as §8.4.1 says of the icon strip. The
  converter's PNGs keep file orientation, so the painter flips them and maps the `NEXTFRAME`
  v-range from the bottom. This also gives the spinner its frame order. tour-scene.md does not
  say this yet; the 3D textures look correct as converted.
- **Caption width** uses the §8.4.2 rule with an approximation of the XP System font
  (Helvetica-Bold widths × 14.6/1000, floored). That table reproduces both widths the authoring
  tool saved: `ERS Label` 68 px → 89-px boxes and `ERS HotSpot` 83 px → 91-px boxes.
- **Far plane**: `CAMERA.FAR=-1`, or no `FAR` at all (Azuchi), gives 2 × world size. The
  control's own default is unknown.
- `CAMERA.CLEARBACKGROUND=0` is not reproduced: the frame is always cleared to the background
  colour (WebGL does not keep the previous frame).

## Known gaps

- VM items still open: tick rate, snap vs gravity, FOV axis, the walk-map switch ordering, PVS
  always-drawn list, sprite rotation sign, cloud orientation, back-face culling (faces are
  single-sided as converted; `doubleSided` option), System font widths, caption outline look,
  control size. Check `docs/vm-findings.md` when it exists and adopt measured values (the
  options above are the switches).
- Runtime changes to **baked** static lights have no visible effect: `LIGHTS.<id>.OFF/COLOR/POS`
  on non-flickering lights, `SCENE.AMBIENT` after loading, and `FLICKER` on a light that was
  static at bake time. On the disc these only name lights that do not exist.
- Not implemented (unused by the tours): climbable walls, orbit (OMNI) mode, the 2D `SKY`
  backdrop, `HALFRESDRIVING`, `SHOWCOLLISION`, `PLAYAVI`, `2D.GRID`, kind-2 button strips, and
  the right mouse button.
- Collision gathers walls from the collision cells along the move by sampling the line at grid
  spacing, rather than with the original's Bresenham walk. Cell lookup uses an inclusive
  quantised point-in-polygon test, because the rasteriser's edge rule is unknown (§6.14); results
  differ only within one grid unit.
- Text is antialiased by the browser (XP GDI drew it aliased), and Tahoma falls back to Verdana
  or DejaVu Sans where it is not installed.
- `grip.cur` is missing on the disc; slot 6 falls back to the default cursor.
- In a hidden or occluded browser window `requestAnimationFrame` stops, so the tour pauses
  (`advance()` drives it manually).

## Verification

`npm test`: 58 tests. Unit tests on synthetic data cover the parser, interpreter, macros and
alarms, walk-map lookup and gap closing, hotspot enter/exit, collision (straight, slide, corner,
endpoint, coverage), time factor, step, pitch, snap and gravity, mouse vector and cursor zones,
flicker, film strip, water, the cloud rasteriser, caption widths, NEXTFRAME, hit testing and
macros, anchoring and the cone test, and the site map. The integration tests over `out/tours`
cover parser equivalence with the converter, the post-load state, walking from the start pose,
and Beaumaris floor switching. In Chrome every tour was loaded, viewed at the start pose and
walked: the Acropolis Propylaea and Parthenon (bronze Athena impostor, water ripple, hotspots),
the Roman Forum temple steps, the Beaumaris gatehouse, the Persepolis stairway, the Colosseum
arcades, the Abu Simbel interior (torch light, captions) and the Azuchi entrance. Also checked:
the loading screen, hotspot hover/press/click, the labels toggle, drag navigation and cursors.
No VM screenshots of tours existed in `fixtures/` at the time, so there is no side-by-side
comparison yet.
