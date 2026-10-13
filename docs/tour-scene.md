# Converted 3D tours: `out/tours/<tourId>/`

`encarta-convert tours` writes one directory per tour (container number, e.g. `04257`), plus
`out/tours/index.json`. This document is the contract between the converter
(`crates/encarta-convert/src/tours*`) and the three.js runtime (plan Step 6). The data formats and
the original engine behaviour are specified in [formats/tours-3d.md](formats/tours-3d.md) (cited
here as §n); this file describes only what the converter writes.

| File | Contents |
|---|---|
| `scene.glb` | glTF 2.0 binary: every `.x` mesh (merged where possible), baked vertex lighting, flicker weights, embedded textures, point-cloud statues |
| `scene.json` | the scene description (schema below) |
| `pvs.bin` | walk-map visibility runs (layout below) |
| `images/` | images the scripts or 2D layers name: HUD/loading images, `APPLYTEXTURE`/`FILMSTRIP` textures, the icon strip |
| `cursors/` | every `.cur` of the container as PNG (hotspots in `scene.json`) |

The 2D-layer images in `images/` keep the disc files' row order, which is bottom-up: the engine shows
them flipped vertically, so a runtime must flip them when drawing 2D layers (this also gives spinner
film strips their frame order). 3D textures need no flip. The icon strip is already in display orientation.

Output is deterministic and idempotent: a second run rewrites nothing, and stale files in a tour
directory are removed. `ENCARTA_TOURS=04257,05919` converts only some tours.

## Conventions

- **Coordinates.** glTF space: right-handed, +Y up, original model units. Every world position
  from the disc (meshes, camera, lights, walk maps, anchors, clouds) is converted with
  `x' = x, y' = y, z' = −z` (§5.6). Triangle winding is reversed, UVs are unchanged (no V flip;
  keep three.js `texture.flipY = false`, the GLTFLoader default).
- **Yaw** stays in the original degrees: the look direction is `(sin yaw, 0, −cos yaw)` in glTF
  space, so three.js `camera.rotation.y = −yaw·π/180` (with rotation order `YXZ`). **Pitch** is in
  degrees, positive looks up.
- Exceptions, marked where they occur: the site-map parameters stay in original coordinates
  (`z_original = −z'`), and walk-map grid quantisation (§6.3) must use `gz = trunc((bboxMax.z − z')/(bboxMax.z − bboxMin.z)·4095)`.
- Names are the disc's: glTF node names are `.x` mesh names (case kept); scripts address them
  case-insensitively, first match (§7.1). Every `target` in `scene.json` is already the resolved
  name, or `null` when the original would fail silently.
- Every converted statement keeps `src`, the statement text as written on the disc (case kept;
  the interpreter folds case before the first `^`, §3.2).

## `scene.glb`

- **Nodes.** One root node per *separate* mesh, in `.x` order, named after the mesh and translated
  to the mesh's object centre (mean of face centres, §7.11; the pivot for sprites, clouds and LOD
  distance). Then one node per material named `merged:<texture name>` holding every mesh that no
  statement can address. Then one `cloud:<placeholder>` node per point-cloud placement.
  A mesh is *separate* when any statement anywhere (load scripts, every macro, alarms, walk-map
  hotspot strings, 2D element macros) names it in a `3D.<id>` statement, when it is part of a
  `lod_` pair, or when it carries a cloud. `scene.json` `model.meshes` maps every original mesh to
  its node or merged triangle ranges.
- **Node extras**: `meshIndex` (original index); on a LOD base `lod: { lowDetail, switchDistance: 13, metric }`;
  on a `lod_` mesh `lodOf` (base name or `null` for the orphan) and `neverDrawnAlone: true`;
  merged nodes `merged: true, material`; cloud nodes `cloudOf, file`.
- **Primitives**: one per (node, material), `TRIANGLES`, fan-triangulated faces in face order, so
  each original face is one contiguous run of `faceVertices − 2` triangles.
  Attributes: `POSITION`, `NORMAL` (unit length), `TEXCOORD_0` (tiles far outside 0…1: the
  samplers use `REPEAT`), `COLOR_0` (`VEC4` normalised `UNSIGNED_BYTE`), and on primitives touched
  by a flickering light `_FLICKER_SLOTS[_n]` (`VEC4 UNSIGNED_BYTE`, slot numbers) and
  `_FLICKER_WEIGHTS[_n]` (`VEC4 FLOAT`). Vertices are split per (position, normal, colour, flicker
  weights); three.js names the custom attributes `_flicker_slots`, `_flicker_weights`, `_flicker_slots_1`, ….
- **Materials**: one per `.x` texture name (the engine's "texture record"), all
  `KHR_materials_unlit` with `baseColorFactor` 1: the texture is modulated by `COLOR_0` only.
  `faceColor` is not applied (§5.4, a VM item); the source materials and their face colours are
  in material `extras`. Colour-keyed BMP textures are PNGs with alpha 0 on magenta (RGB555 test),
  `alphaMode: MASK`, cutoff 0.5, and a sampler without mipmaps (sampler 1); keyed texels take the
  mean colour of their opaque neighbours so filtering does not bleed magenta. JPEGs are embedded
  byte for byte; BMP and GIF become PNG. Textures resolve by the §5.5 stem rule and are sniffed by
  content. `doubleSided` is false (D3D clockwise front faces become glTF counter-clockwise; whether
  the original culled back faces is a VM item, §5.9).
- **Clouds**: one `POINTS` mesh per `.3cl` file, shared by its placements. `POSITION` holds the
  integer cloud-local coordinates (z negated), `COLOR_0` the RGB565 colour, `_KEY` the facing angle
  in degrees (`−1` for NaN keys, which are always drawn). Points keep file order (sorted by key,
  as the 500-point chunk culling of §7.3.3 needs; the last point is never drawn by the original).
  The node `matrix` places the cloud as a static 3D object equivalent to the impostor of §7.3.3
  (see `clouds[]` below).
- **Asset extras**: `worldBounds`, `worldSize` (largest extent, used for `CAMERA.FAR=-1`),
  `lodPairs`, `textures` (name, file, resolution, keyed).

### Vertex colours (static lighting)

`COLOR_0` is the corner colour of §7.2.3 for every **static** light (lights that are on and not
flickering after `load.3sc`), computed with the engine's formula: start at `SCENE.AMBIENT`, add
`trunc(colour·f)` per light whose list and vertex mask include the corner, `f = (1 − d²/range2)·(n·L̂)`,
clamped at 255; v3 paint records then force their corner colour. `SPRITE` and `NOILLUMINATION`
objects, and every vertex when no lights are loaded, are white. The bytes are the engine's
0–255 values: the original multiplied them with texel bytes in display space, so for parity
render the tours without colour-space conversion (textures and output both treated as
sRGB-encoded, e.g. `texture.colorSpace = NoColorSpace` and `renderer.outputColorSpace =
LinearSRGBColorSpace`).

Runtime terms:
- **Flicker** (§7.2.6): `final = min(255, COLOR_0 + Σᵢ trunc(currentᵢ · wᵢ))` over the flicker
  sets, where `currentᵢ = trunc(k·savedColour)` of the light in slot `slotsᵢ`, `k` redrawn
  uniformly from `[0.4, 1.0]` (512 steps) every `period` seconds, held constant in between.
  Unused entries have weight 0. Painted corners carry no flicker.
- **`LIGHTS.OFF`** (Persepolis `stair1.3wm` cell 1255 runs `LIGHTS.OFF`/`ON`): everything full-bright.
- A light that is off at the end of loading is not baked (none on the disc).

## `pvs.bin`

Little-endian runs of 8 bytes: `u32 firstTriangle, u16 drawUnit, u16 triangleCount`. A
visibility cell's `pvs: { firstRun, runCount }` selects its runs; `drawUnit` indexes
`model.drawUnits` (a glTF node/mesh/primitive). Runs are sorted and coalesced. While the eye is over
a visibility cell, only these triangles are drawn, and only for nodes that are visible (§6.6).

## `scene.json`

TypeScript notation; `Vec3 = [number, number, number]` (glTF space unless noted). Fields marked
"raw" keep the disc value unconverted.

```ts
interface TourScene {
  format: "encarta-tour-scene"; version: 1;
  tour: { id: string; contentId: number; name: string };       // contentId = 7015 + id (§1)
  coordinates: Record<string, string>;                           // the conventions above, as text
  files: { glb: "scene.glb"; pvs: "pvs.bin" };
  camera: Camera; scene: SceneSwitches; model: Model;
  objects: ObjectInfo[];          // every separate node that is a mesh
  lights: Lights | null;
  clouds: CloudPlacement[];
  walkmaps: WalkMaps;
  layers: Layer[];                // in load order: begin.32d (loading screen), lh.32d (final)
  hud: Hud; cursors: Cursors;
  hotspots: HotspotLink[];        // final-layer elements whose macro fires HOTSPOTCLICKED
  unresolvedHotspots: { origin: string; src: string }[];  // MACRO.MOUSEUP on missing elements
  siteMap: SiteMap;
  macros: Macro[]; alarms: Alarm[]; events: FiredEvent[];
  loadSequence: SeqStep[];        // every leaf statement load.3sc executed, in order
  issues: unknown[];              // the decoder's cross-reference issues (silent no-ops etc.)
  omitted: { file: string; reason: string }[];  // container files deliberately not converted
}

interface Camera {
  start: { position: Vec3; yawDeg: number; pitchDeg: number };  // last CAMERA.POS / PITCH
  fovDeg: number; height: number; speedFly: number; speedTurn: number; pitchSpeed: number;
  pitchLimitDeg: 45; collisionRadius: number; climbRate: number;
  near: number; nearHistory: number[];   // every CAMERA.NEAR in order (Colosseum, Azuchi change it)
  far: number | null;                    // −1 = 2 × worldSize; null = control default (Azuchi)
  backgroundColor: [number, number, number] | null; clearBackground: number;
  timeFactor: { ratePerSecond: 100; min: 1; max: 5 };   // k = clamp(100·dt, 1, 5) (§6.9)
}                                        // unset values take the §6.10 defaults

interface SceneSwitches {
  gravity: number; ambient: [number, number, number] | null; specular: boolean; highSpecular: boolean;
  displayFps: boolean; halfResDriving: boolean;
  fog: { color: [number, number, number]; start: number; end: number; enabled: boolean };
                                         // fog.end (default 20) also limits cloud re-rendering (§7.3.4)
  modelOn: boolean; lightsOn: boolean; layerOn: boolean; walkmapOn: boolean;
  timerOn: boolean; alarmsEnabled: boolean; calc64bit: number | null; lodDistance: number;
}

interface Model {
  file: string; glb: "scene.glb"; worldBounds: { min: Vec3; max: Vec3 }; worldSize: number;
  materials: { index: number; name: string; textureName: string | null; textureFile: string | null;
               resolution: "Exact" | "JpgFallback" | "GifFallback" | "Missing" | null;
               gltfTexture: number | null;      // the texture record APPLYTEXTURE/WATER replace
               keyed: boolean; sourceMaterials: string[]; faceColors: number[][] }[];
  drawUnits: { node: string; nodeIndex: number; mesh: number; primitive: number; material: number;
               triangles: number; vertices: number; merged: boolean }[];
  meshes: { index: number; name: string; faces: number;
            node: string | null;                        // null = merged
            parts: [drawUnit: number, firstTriangle: number, triangleCount: number][] }[];
  lod: { switchDistance: 13; metric: string; pairs: { lowDetail: string; base: string | null; centre: Vec3 | null }[] };
  mergeStats: MergeStats;                               // counts for the report
}

interface ObjectInfo {
  node: string; index: number; centre: Vec3 | null; bounds: { min: Vec3; max: Vec3 } | null;
  firstFaceMaterial: number | null; textureRecord: number | null;   // glTF texture of the first face
  state: ObjectState;                        // after load.3sc
  filmstripFrames: number | null;            // N = texture width / frame width (§7.4.4)
  lodLowDetail: string | null; lodOf: string | null | undefined; cloud: string | null;
}
interface ObjectState {
  visible: boolean; sprite: boolean; noIllumination: boolean; water: boolean;
  appliedTexture?: [file: string, image: string];   // APPLYTEXTURE or FILMSTRIP (image in images/)
  filmstrip?: [frameWidth: number, waitSeconds: number];
  cloud?: [file: string, scale: number]; cloudOffset?: number;   // radians
  other?: Record<string, string>;                     // PLAYAVI, STRING (raw)
}

interface Lights {
  file: string; globalOn: boolean; ambient: [number, number, number] | null; paintRecords: number;
  flickerSets: number;                                 // number of _FLICKER_* attribute pairs
  flickerSlots: { slot: number; light: number; name: string; vertices: number }[];
  flickerModel: { minFactor: 0.4; maxFactor: 1.0; steps: 512; interpolation: "step" };
  lights: { index: number; name: string;               // exact, may end in a space (§7.8)
            position: Vec3; range2: number; colour: [number, number, number]; fileColour: [number, number, number];
            on: boolean; baked: boolean; faceEntries: number;
            flicker: { period: number; savedColour: [number, number, number]; slot: number; vertices: number } | null }[];
}

interface CloudPlacement {          // §7.3
  node: string; placeholder: string; file: string; points: number;
  scale: number; offsetRadians: number;               // CLOUDSCALED s (0 means 0.5), CLOUD.OFFSET
  textureSize: [w: number, h: number];               // placeholder texture (impostor target)
  quadSize: [width: number, height: number];          // placeholder quad in world units
  base: Vec3;                                         // placeholder bottom centre
  matrix: number[];                                   // column-major, = the glb node matrix
}
```

The cloud `matrix` maps a stored point `(x, y, −z)` to world as
`base + R_y(α)·(a·x, b·y, a·(−z))` with `a = s·quadWidth/W`, `b = s·quadHeight/H` and
`α = −offset − π/2` (in original coordinates the rotation is `(x, z) → (x cos α − z sin α, x sin α + z cos α)`).
It is derived from the impostor algorithm of §7.3.3 (the impostor shows the orthographic view of
that fixed rotated cloud from the eye direction, with cloud y = 0 at the bottom of the quad); the
orientation is a **[VM]** item (§7.10: upright, not mirrored). A runtime that reimplements the
impostor instead uses `_KEY`, `scale`, `offsetRadians` and `textureSize`.

```ts
interface WalkMaps {
  loadOrder: string[]; start: string;                  // the last map loaded is active (§6.4)
  pvsFile: "pvs.bin"; pvsRunLayout: string;
  grid: { size: 4096; gapClose: [1, 3] };              // §6.3
  maps: { file: string; version: number; bbox: { min: Vec3; max: Vec3 }; cells: Cell[] }[];
  floors: { walkmap: string; qtsRefIds: string[] }[];  // USERLEVELCHANGE ids seen per map (convenience)
}
type Cell = { index: number; polygons: number[][] /* flat x,y,z,… per convex polygon */ } & (
  | { kind: "ground"; plane: [a: number, b: number, c: number, d: number] | null }  // a·x+b·y+c·z+d = 0
  | { kind: "collision"; level: number;
      segments: [x1: number, z1: number, x2: number, z2: number, top: number, climbable: number][] }
  | { kind: "hotspot"; enter: HotspotScript; exit: HotspotScript }
  | { kind: "visibility"; pvs: { firstRun: number; runCount: number; meshes: number; faces: number } });
interface HotspotScript { script: string; junkBytes?: number; steps: Step[] }
                                    // script = authored text (§6.2 cleaning rule); junk is dropped
```

Cells keep file order and index: lookups take the first cell of a kind containing the point (§6.3).

```ts
interface Layer {
  file: string; version: number; on: boolean; authoringIconFile: string; final: boolean;
  elements: {
    index: number; name: string; class: "caption" | "hotspot" | "image";
    kind: "plain" | "hotspot" | "buttonStrip" | number;
    rect: [left: number, top: number, right: number, bottom: number];   // client pixels, raw
    uv: [u0: number, v0: number, u1: number, v1: number];              // as in the file
    primitive: number; additive: boolean; blend: number; textured: boolean;
    capturesMouse: boolean; colorKey: boolean; isLabel: boolean; hasText: boolean;
    diffuse: string; textColor: string; frameColor: string; labelFill: string;   // "#RRGGBB"
    icons: { normal: number; pressed: number; hover: number };                  // as in the file
    font: { face: string; height: number; weight: number; charset: number };    // overridden at render (§8.4.2)
    fileText: string; sourceImage: string | null;
    image: string | null;          // images/…; ".key.png" = magenta made transparent (colorKey)
    state: ElementState | null;    // after load.3sc (for begin.32d: when lh.32d replaced it)
  }[];
}
interface ElementState {
  visible: boolean; text: string; anchor: Vec3; anchored: boolean;
  icon: number;                     // normal icon; pressed = icon+1, hover = icon+2; −1 = none
  uv: [number, number, number, number];   // after NEXTFRAME
  sky: boolean;
  macros: Record<"mouseDown" | "mouseUp" | "mouseIn" | "mouseOut", [script: string, steps: Step[]]>;
  other?: Record<string, string>;
}
interface HotspotLink {
  element: string; event: string; contentId: number; contentIdText: string;   // an Encarta media id (§4.3)
  title: string; icon: number; anchor: Vec3; anchored: boolean;
}
interface Hud {
  iconStrip: { file: string; image: string;       // already in display orientation (§8.4.1)
               width: number; height: number; count: number; cellWidth: number; cellHeight: number;
               orientation: string; triples: string[] } | null;
  captions: { font: string; fontSizePx: 13; height: 22; surface: [512, 32];
              captionMetrics: { verticalOffset: 4; scale: 0.26; adder: 82 };
              widthFormula: string; outlined: object; hotspotBox: object; coneTest: string };
  settings: Record<string, string>;               // 2D.FONT/FONTSIZE/… if a script set them
}
interface Cursors {
  enabled: boolean;                                // SCENE.CURSORS
  slots: { slot: number; meaning: string; file: string; image: string | null; hotspot: [number, number] | null }[];
  files: Record<string, { image: string; width: number; height: number; hotspot: [number, number]; invertedPixels: number }>;
}
interface SiteMap {                                // WORLDLOADED payload with §4.1 defaults
  xOffset: number; zOffset: number; xExtent: number; zExtent: number; mapHeight: number | null;
  fieldOfViewDeg: number; rotateFrame: number;
  rect: { left: number | null; top: number | null; right: number | null; bottom: number | null };
  coordinates: string; payload: string | null;     // original coordinates (z_original = −z')
}
interface Macro { name: string; body: string; terminated: boolean; origin: string; runDuringLoad: boolean; steps: Step[] }
interface Alarm { name: string; active: boolean; maxExecutions: number; interval: number;
                  macroText: string; steps: Step[]; origin: string; live: boolean }  // live = not deleted
interface FiredEvent { origin: string; text: string; name: string; pairs: [string, string][] }
type SeqStep = Step & { origin: string };          // "file:line", "> NAME" when run from a macro
```

### Steps (converted statements)

```ts
type Step = { src: string } & Action;
```

`Action` is tagged by `op`. Object actions carry `id` (the folded id as written) and `target`
(the resolved node, light or element name, or `null`). Targets of nested steps (macros, alarms,
hotspot strings) are resolved against the state after loading; a runtime should still resolve ids
itself when it runs a step, because the original resolves at execution time (e.g. the
`LOADALARM` macro names `_0`, which disappears when `lh.32d` replaces `begin.32d`).

| `op` | Fields | Statement |
|---|---|---|
| `deleteAll`, `draw` | – | `SCENE.DELETEALL`, `SCENE.DRAW` |
| `event` | `text, name, pairs` | `SCENE/SCRIPT.EVENT(…)`: fire `Event3DX(text)` (upper-cased, §3.6) |
| `setScene` | `prop, value` | gravity, specular, highSpecular, displayFps, cursors, halfResDriving, showCollision, fogStart, fogEnd, loadFilesFromIts, workingPath, itsFileUrl |
| `ambient`, `fogColor` | `rgb` | `SCENE.AMBIENT`, `SCENE.FOGCOLOR` |
| `cursor` | `slot, file, image` | `SCENE.CURSOR` |
| `device` | `command` | `TRYHARDWARE` …: no-op, returns `Success` |
| `alarmNew` | `name, active, maxExecutions, interval, macroText, steps` | `SCENE.ALARM.NEW` (macro after the 4th comma, trailing `)` trimmed, §3.5) |
| `alarmsEnable` | `on` | `SCENE.ALARM.ACTIVATE/DEACTIVATE` |
| `alarm` | `name, action, value?` | `SCENE.ALARM.<name>.ACTIVATE/DEACTIVATE/ACTIVE.TOGGLE/DELETE/ADDSEQUENCE/CLEARSEQUENCE` |
| `defineMacro`, `deleteMacro` | `name` | `BEGIN(…)`, `DELETE(…)` |
| `runMacros` | `names, ignored?` | a command starting `RUN(`; `ignored` = its non-`RUN` pieces (they do nothing) |
| `query` | `subsystem, what` | any `…?` query (no side effect) |
| `camera` | `prop, values` | `CAMERA.POS/MOVETO` (`[x,y,z,yawDeg]`), `MOVE`, `PITCH`, `FOV`, `HEIGHT`, `SPEED.FLY` → `speedFly`, … |
| `cameraCommand` | `command` | `HOME`, `HOMEPOV`, `HOMEOMNI`, `POV`, `OMNI` |
| `walkmapLoad` | `file, resolved` | `WALKMAP.LOAD`: switch to the cached map |
| `walkmapEnable` / `walkmapToggle` / `walkmapDelete` | `on` / – / `what` | `WALKMAP.ON/OFF`; `TOGGLE` can never switch off (§6.10) |
| `lightsLoad`, `lightsEnable`, `lightsToggle`, `lightsDelete` | `file` / `on` / – / – | `LIGHTS.*`; `TOGGLE` changes nothing |
| `light` | `id, target, prop, value` | `LIGHTS.<id>.FLICKER/ON/OFF/COLOR/POS/STATIC` (`prop`: flicker, on, color, pos, static) |
| `modelLoad`, `modelEnable`, `modelSet`, `modelCommand` | `file` / `on` / `prop, value` / `command` | `3D.LOAD`, `3D.ON/OFF`, `CALC64BIT`/`LODDISTANCE`, `UNHIDEALL`/`TOGGLEHIDDEN`/`DELETE` |
| `objectFlag` | `id, target, prop, value` | `3D.<id>.VISIBLE/SPRITE/NOILLUMINATION/WATER` (first character after `=` only, §7.4) |
| `applyTexture` | `id, target, file, resolved, image` | `3D.<id>.APPLYTEXTURE` (replaces the first face's texture record) |
| `filmstrip` | `id, target, frameWidth, wait, file, resolved, image` | `3D.<id>.FILMSTRIP` |
| `cloud`, `cloudOffset` | `id, target, file, scale` / `radians` | `CLOUD`, `CLOUDSCALED`, `CLOUD.OFFSET` |
| `objectOther` | `id, target, prop, value` | other `3D.<id>.*` |
| `layerLoad`, `layerEnable`, `layerToggle`, `layerNew` | `file` / `on` / – / – | `2D.LOAD`, `ON/SHOW/OFF/HIDE`, `TOGGLE` (reports only), `NEW` |
| `iconFile`, `icons`, `layerSet` | `file, image` / `count` / `prop, value` | `2D.ICONFILE`, `ICONS`, `FONT`… |
| `elementVisible` | `id, target, value` | `2D.<id>.VISIBLE` (`value: null` = toggle) |
| `elementText`, `elementAnchor`, `elementAnchored`, `elementIcon` | `text` / `point, ignored?` / `value` / `icon` | `TEXT=^…^`, `ANCHORPOINT` (x,y,z only; `ignored` = the yaw or second point, §8.4), `ANCHORED`, `ICON` |
| `elementMacro` | `id, target, event, script, steps` | `2D.<id>.MACRO.MOUSEDOWN/UP/IN/OUT` |
| `elementNextFrame`, `elementSky`, `elementOther` | `frames` / `value` / `prop, value` | `NEXTFRAME(n)`, `SKY`, others |
| `timer` | `on` | `TIMER.ON/OFF` |
| `noop` | `reason` | anything the original ignores: no subsystem (`)`, `1`, `'2D…`, a `RUN(…)` piece inside a statement list), bad flag values, unknown commands |

Quirks reproduced by the conversion rather than special-cased: the glued Acropolis `HIDEALL`
statement is one `objectFlag water=false` step on `n_00water` with the rest of the text in `src`;
`2D.<id>.ANCHORPOINT` ignores the yaw; `LOADALARM`'s macro is `2D._0.NEXTFRAME(16` and the
`SCENE.DRAW)` piece of the same line is a `draw` step; hotspot strings keep authoring slips as
`noop` steps and drop the uninitialised junk bytes (`junkBytes` counts them).

## `index.json`

`{ format: "encarta-tours-index", sceneVersion, tours: TourReport[] }`, one entry per converted tour:
file sizes, glb statistics, merge statistics, counts, and the validation checks the converter ran
(glb structure, triangle and face counts against the `.x`, node counts, cloud points and
placements, walk-map cells, PVS faces and triangles against the masks, executed statements,
lights, layer elements, hotspot macros, and "every node, light, element and image name in
scene.json exists"). The converter fails when a check fails.

## Open points for the runtime (VM items)

- Whether the PVS also applies LOD switching (low-detail `lod_` meshes appear in PVS lists), and
  what the engine's always-drawn object list holds: 9 of 33 Acropolis sprites, 19 of 132 Beaumaris
  sprites and 9 of 61 Abu Simbel sprites appear in no PVS list, so they would never be drawn over a
  visibility cell unless that list contains them (§6.6).
- Back-face culling, faceColor, the colour-key test (555 vs 565), cloud orientation, sprite
  rotation sign (§5.9, §7.10).
