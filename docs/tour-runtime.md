# 3D tour runtime (`web/src/tours/`)

The browser runtime for the converted tours (plan Step 6). Its input is `out/tours/<id>/` as
described in [tour-scene.md](tour-scene.md); the behaviour it reproduces is specified in
[formats/tours-3d.md](formats/tours-3d.md) (cited as §n). It is framework-agnostic: the Step 4
client embeds it with one call.

```ts
import { mountTour } from "./tours";
const tour = await mountTour(element, {
  baseUrl: "/out/tours", tourId: "04257",
  controlSize: [616, 420],    // Encarta's control size; scaled to fit the element, letterboxed
  initialPose: decodePose(params.get("pose")),   // coming back from a hotspot's media page
  onEvent: (e) => { /* e.text, e.name ("WORLDLOADED", "USERLOCUPDATE", "HOTSPOTCLICKED", "USERLEVELCHANGE"), e.pairs, e.destMedia */ },
});
tour.do("2D.OFF");            // the host's "Show Labels and Hot Spots" menu item
tour.do("camera.pos?");       // "x,y,z,yaw" in the disc's coordinates, as the host's site map expects
tour.goToStop(701504262);     // a Tour Stops menu entry (scene.stops)
encodePose(tour.pose());      // "x,y,z,yaw,pitch,walkmap" for the URL
```

`mountTour` plays the host handshake of §2.2 (`FinishedInitializing`, `script.itsfileurl`,
`script.load=load.3sc`) unless `autoLoad: false`, and then, as Encarta does, sends the default
Tour Stop's camera command once loading ends (or restores `initialPose` instead). `host.ts` has the
other host rules: poses, the stop commands and hotspot destinations. `sitemap.ts` has the host's
site-map marker maths (§4.1–§4.2) for hosts that draw the map.

**The app's tour page** (`web/src/pages/TourView.tsx`, route `/tours/:id`) is laid out like Encarta's
(VM captures): a 172-px column with the site map (172×194) above the stop text, and on the tour
background (49,69,107) the menu bar, the control at 616×420 with a 1-px black frame, and the tour title
("Acropolis, 400 BC") below it.
- **Site map** (§4.2, `sitemap.ts` `drawSiteMap`): the current stop's `sites[]` image, left half, with the
  right (tinted) half clipped to the view wedge and our own four-way marker on top. The marker follows
  `USERLOCUPDATE` → `camera.pos?` and is redrawn only past 2° or 2 px (`markerMoved`). The map switches on
  a stop change and on `USERLEVELCHANGE` (its `QTSREFID` is a stop id). Clicking a location box moves to
  its stop.
- **Tour Stops menu**: `scene.stops` in menu order. A stop with hotspots opens a submenu whose first entry
  (labelled with the stop title; the original label was not captured) sends the stop's camera command and
  whose other entries open the hotspots' media; a stop without hotspots is one entry. Choosing a stop also
  shows its article (`stops[].article`) below the map.
- The menu bar also has the "Labels and Hot Spots" toggle (`2D.ON`/`2D.OFF`) and Credits
  (`scene.article`). Start Over and Appears in follow the VM; Music loops the tour's ambient audio record (`web/src/tours/ambient.ts`; starts off, unverified).
- A hotspot click (or menu entry) replaces the main view with `/item/<destMedia>`, after writing the pose
  and stop to the tour's URL (`?pose=…&stop=…`, history replace), so Back reopens the tour at that pose,
  with its captions, stop text and map. Without a `destMedia` (converted without the catalog) the hotspot
  record shows in the text pane.

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
axis. The size defaults to Encarta's 616×420. URL parameters (`?tour=05919&size=640x480&ground=gravity&pvs=0&fov=vertical`) make views
reproducible. `window.tour` is the handle (in the app's dev build, `globalThis.__encartaTour`).
`tour.advance(n)` runs n ticks and draws a frame without `requestAnimationFrame`, for tests and
hidden pages.

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
| `sitemap.ts` | `WORLDLOADED` parsing, the marker of §4.2, the 2°/2-px redraw rule and the site-map painter (map halves, wedge, marker) |
| `host.ts` | the host's side: poses (`currentPose`, `encodePose`/`decodePose`, `poseCommand`), the default stop after loading, hotspot → `destMedia` |
| `sim/clock.ts` | `FixedTicker`: display frames in, whole simulation ticks out |
| `dev/main.ts` | the dev page |

**Loading.** The runtime executes the converter's `loadSequence` (every leaf statement
`load.3sc` ran). It suspends at `3D.LOAD` until `scene.glb` and `pvs.bin` are built, and then
resumes one top-level `SCENE.DRAW` per tick, as in §3.7. Meanwhile alarms run, so the
`begin.32d` spinner animates at 10 fps. The model is also held back until `minLoadSeconds` (default
2) of simulation time have passed since `script.load`: the VM showed the loading screen for about
2 s, which the original spends streaming one texture per tick; that streaming is not emulated
separately. The integration test checks that the resulting object
and element state equals the converter's `objects[].state` and `layers[].elements[].state` in all
seven tours, and that the TS parser converts every disc statement exactly like the converter.

**Tick.** The simulation runs at a fixed `tickHz` (default 64: the VM's idle rate, `SW 65.8`)
fed by `FixedTicker` from `requestAnimationFrame`, so nothing depends on the display's refresh rate.
Each tick polls alarms, moves the camera (`k = clamp(100·dt, 1, 5)`, so `k = 1.5625`), follows the
ground, runs the hotspot check, and advances flicker, film strips (once per tick, like the
original's once per frame) and water (at most 40 steps/s). Walking is 100 × `SPEED.FLY` units/s
and turning 200 × `SPEED.TURN` °/s, as measured, for any tick rate from 20 to 100 Hz; pitch keys
step exactly `PITCHSPEED` per tick (the VM's walking rate of ~47 ticks/s makes pitch slower there).
All world state is in glTF space; queries convert back to the disc's coordinates.

## Parity decisions

- **Colour.** Textures `NoColorSpace`, output `LinearSRGBColorSpace`, unlit `MeshBasicMaterial`
  with `COLOR_0`. This multiplies texel bytes by the baked bytes in display space, as the original
  did, with no overbright (tour-scene.md).
- **Flicker** is a vertex-shader term: `min(255, COLOR_0 + Σ trunc(current[slot]·w))` over the
  `_FLICKER_*` sets, with one `vec3` uniform per flicker slot. `LIGHTS.OFF` and full-bright
  objects are shader switches.
- **Ground mode** is snap (software device), as in the VM (§6.8); the `device: "hardware"` or
  `groundMode: "gravity"` options switch to gravity, which the VM could not show.
- **FOV** is horizontal (measured, §6.10): projection scale `K = (W/2)/tan(fov/2)`, shared by the
  renderer and caption anchoring. `fovAxis` remains as a switch for experiments.
- **Control size.** The app mounts the control at 616×420, Encarta's size, and scales it to the
  page (`controlSize`). The 2D layer and the loading screen are laid out in those pixels and the
  vertical field of view follows from it, so views and caption positions match the VM's captures.
  Without `controlSize` (the dev page) the control takes the element's size.
- **Host start.** After `WORLDLOADED` the default stop's command is sent on the next tick (VM:
  Encarta does this). Its away-and-back MOVETO in the multi-floor tours re-enters the start cell,
  which is why the Colosseum shows captions at once and the Acropolis does not.
- **Replies** follow the VM: `script.load` says `Failed` (and loads), `SCENE.DISPLAYFPS` says
  `Failed` (and switches), `script.itsfileurl` gives `1`, or `-2` for a path with spaces.
- **MOVETO to the current position** keeps a valid pose; the original's NaN (§6.10) is not
  reproduced. `poseCommand` moves far away first anyway, like the disc's stop commands.
- **Walk-map grid cells** use the scan-line span rule of §6.14 (a grid column belongs to a
  polygon when the polygon's span on that row overlaps it). The earlier "grid point inside the
  quantised polygon" test put the Colosseum stop "Seating Areas" on the wrong seating step.
- **Caption colours** are the file's 0xC0D1E3 (text and frame), white on hover and press. The
  VM's 565 display shows them as (198,211,231); the runtime does not quantise.
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
  target, with magenta as alpha 0 (surface row `r` goes to render-target row `H − 1 − r`, matching the
  converter's V flip; VM: upright, not mirrored, correct side). Re-rendering follows §7.3.4: in software mode every drawn
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
- **Surfaces are bottom-up** (§5.6). The engine's texture surfaces start with the image's bottom
  row, for 2D and 3D images alike. In 3D the converter flips V (`v' = 1 − v`), so glTF textures and
  `APPLYTEXTURE` images are used as loaded; the cloud impostor writes surface row `r` to render-target
  row `H − 1 − r`. In 2D the painter draws the images flipped and maps the `NEXTFRAME` v-range from
  the bottom (the disc's 2D GIFs are stored upside down and show upright; this also gives the spinner
  its frame order).
- **Caption width** uses the §8.4.2 rule with an approximation of the XP System font
  (Helvetica-Bold widths × 14.6/1000, floored). That table reproduces both widths the authoring
  tool saved: `ERS Label` 68 px → 89-px boxes and `ERS HotSpot` 83 px → 91-px boxes.
- **Far plane**: `CAMERA.FAR=-1`, or no `FAR` at all (Azuchi), gives 2 × world size. The
  control's own default is unknown.
- `CAMERA.CLEARBACKGROUND=0` is not reproduced: the frame is always cleared to the background
  colour (WebGL does not keep the previous frame).

## Known gaps

- VM items still open (tours-3d.md §10): LOD switching, the water ripple, PVS on/off and the
  always-drawn list, back-face culling (faces are single-sided as converted; `doubleSided`
  option), cloud-statue update steps and freeze distance, sprite yaw sign, film-strip fps, cursor zones, gravity mode, the walk-map switch ordering,
  System font widths, the `_L` caption outline look, rotated site maps, open edges.
- Textures are mipmapped and filtered; the VM's software rasteriser samples without mipmaps, so its
  distant surfaces look sharper and noisier.
- The 45° pitch test uses the exact pitch, where the original tests a value derived from its float
  look vector: from the Acropolis start pitch 21.000036 the fourth 6° step is refused here
  (45.000036) but was accepted in the VM (44.998962). How that value is computed is not specified,
  and float32 look-vector arithmetic does not reproduce the −0.001074° error (tours-3d.md §6.9), so
  it is not emulated.
- The VM's loading takes 1.6–2.3 s depending on the tour; the data does not explain the spread
  (tours-3d.md §8.4.3), so the runtime keeps one minimum (2 s).
- Site map: the highlighted-location look (§4.2 step 5) and the original marker icon are not
  reproduced; the rotated frames follow §4.2 and agree with the map art, unconfirmed in the VM.
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

`npm test`: 83 tour tests (logic 37, vm 21, disc 29 with `out/tours`). `logic.test.ts` covers the parser, interpreter, macros and alarms, walk-map
lookup and gap closing, hotspot enter/exit, collision, time factor, step, pitch, snap and gravity,
mouse vector and cursor zones, flicker, film strip, water, the cloud rasteriser, caption widths,
NEXTFRAME, hit testing and macros, anchoring and the cone test, and the site map. `vm.test.ts`
checks each VM measurement on synthetic scenes: walk and turn rates at 64 and 47 ticks/s, display
frame rates from 24 to 240 fps, the pitch sequences and Home, wall sliding at yaw 93.4 and the
corner stop, upper-cased events and `USERLOCUPDATE` every 1.00 s, command replies, MOVETO to the
current position, caption colours and cursor, when captions first show (plain vs away-and-back stop
commands), flicker steps, the 2-s loading screen with its 10-fps spinner, hotspot destinations,
pose round trips, and the grid span rule. The integration tests over `out/tours` cover parser
equivalence with the converter, the post-load state, walking from the start pose, Beaumaris
floor switching, and that each stop's site-map marker falls on its location box.

Against the VM captures (`fixtures/tours/`, private): every Tour Stop of the 7 tours (47 views
including the start poses), run through the runtime with its own camera command, gives the VM's
replies and read-back pose (x, ground-followed y, z, yaw, pitch) within 2.1·10⁻⁴, and renders the
same image apart from filtering and lighting details: with the V flip every textured surface shows the
captured part of its image (Acropolis frieze, coffers, pediment, backdrop and railings, Persepolis
frieze and bulls, Abu Simbel statues), and the cloud statues match in orientation. The converted stops
and all 119 hotspot destinations equal the captured tables. In Chrome, in the app at 616×420: the
Colosseum loading screen matches the VM's layout and lasts 2.0 s, then the default stop shows the
same captions at the same pixels (±1 px); the Acropolis start (no captions) and Temple of Athena
Nike, Persepolis Gate of Xerxes and Abu Simbel Pillared Hall match the captures in pose, framing,
textures and caption placement; hovering a
caption turns it white with the hotspot cursor; clicking "Athena Nike Frieze" opens
`/item/681500403`, and Back returns to the tour at the same pose with its captions. All 7 tours
load in the app.

The tour page against the Encarta screen captures (Acropolis stops): same layout, the site map's image,
marker position and wedge match pixel for pixel at the Temple of Athena Nike stop (apart from the marker
icon); in Chrome, the Tour Stops menu and a click on a map location move to the stop (pose as captured,
stop text and map updated, Persepolis's rotated map included), a hotspot entry opens its media page,
and Back restores pose, stop text and map.
