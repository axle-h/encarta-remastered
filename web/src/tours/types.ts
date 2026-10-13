/**
 * Types of the converter's tour output (`out/tours/<id>/scene.json`), transcribed from
 * docs/tour-scene.md. World coordinates are glTF space (x, y, −z of the disc) unless noted.
 */

export type Vec3 = [number, number, number];
export type RGB = [number, number, number];

export interface TourScene {
  format: "encarta-tour-scene";
  version: number;
  tour: { id: string; contentId: number; name: string };
  files: { glb: string; pvs: string };
  camera: CameraInfo;
  scene: SceneSwitches;
  model: Model;
  objects: ObjectInfo[];
  lights: Lights | null;
  clouds: CloudPlacement[];
  walkmaps: WalkMaps;
  layers: LayerFile[];
  hud: Hud;
  cursors: Cursors;
  hotspots: HotspotLink[];
  siteMap: SiteMap;
  macros: MacroDef[];
  alarms: AlarmDef[];
  events: { origin: string; text: string; name: string; pairs: [string, string][] }[];
  loadSequence: SeqStep[];
}

export interface CameraInfo {
  start: { position: Vec3; yawDeg: number; pitchDeg: number };
  fovDeg: number;
  height: number;
  speedFly: number;
  speedTurn: number;
  pitchSpeed: number;
  pitchLimitDeg: number;
  collisionRadius: number;
  climbRate: number;
  near: number;
  nearHistory: number[];
  far: number | null;
  backgroundColor: RGB | null;
  clearBackground: number;
  timeFactor: { ratePerSecond: number; min: number; max: number };
}

export interface SceneSwitches {
  gravity: number;
  ambient: RGB | null;
  specular: boolean;
  highSpecular: boolean;
  displayFps: boolean;
  halfResDriving: boolean;
  fog: { color: RGB; start: number; end: number; enabled: boolean };
  modelOn: boolean;
  lightsOn: boolean;
  layerOn: boolean;
  walkmapOn: boolean;
  timerOn: boolean;
  alarmsEnabled: boolean;
  calc64bit: number | null;
  lodDistance: number;
}

export interface MaterialInfo {
  index: number;
  name: string;
  textureName: string | null;
  textureFile: string | null;
  gltfTexture: number | null;
  keyed: boolean;
}

export interface DrawUnit {
  node: string;
  nodeIndex: number;
  mesh: number;
  primitive: number;
  material: number;
  triangles: number;
  vertices: number;
  merged: boolean;
}

export interface Model {
  file: string;
  worldBounds: { min: Vec3; max: Vec3 };
  worldSize: number;
  materials: MaterialInfo[];
  drawUnits: DrawUnit[];
  meshes: { index: number; name: string; faces: number; node: string | null; parts: [number, number, number][] }[];
  lod: { switchDistance: number; metric: string; pairs: { lowDetail: string; base: string | null; centre: Vec3 | null }[] };
}

export interface ObjectState {
  visible: boolean;
  sprite: boolean;
  noIllumination: boolean;
  water: boolean;
  appliedTexture?: [string, string];
  filmstrip?: [number, number];
  cloud?: [string, number];
  cloudOffset?: number;
  other?: Record<string, string>;
}

export interface ObjectInfo {
  node: string;
  index: number;
  centre: Vec3 | null;
  bounds: { min: Vec3; max: Vec3 } | null;
  firstFaceMaterial: number | null;
  textureRecord: number | null;
  state: ObjectState;
  filmstripFrames: number | null;
  lodLowDetail: string | null;
  lodOf?: string | null;
  cloud: string | null;
}

export interface LightInfo {
  index: number;
  name: string;
  position: Vec3;
  range2: number;
  colour: RGB;
  fileColour: RGB;
  on: boolean;
  baked: boolean;
  faceEntries: number;
  flicker: { period: number; savedColour: RGB; slot: number; vertices: number } | null;
}

export interface Lights {
  file: string;
  globalOn: boolean;
  ambient: RGB | null;
  flickerSets: number;
  flickerSlots: { slot: number; light: number; name: string; vertices: number }[];
  flickerModel: { minFactor: number; maxFactor: number; steps: number; interpolation: string };
  lights: LightInfo[];
}

export interface CloudPlacement {
  node: string;
  placeholder: string;
  file: string;
  points: number;
  scale: number;
  offsetRadians: number;
  textureSize: [number, number];
  quadSize: [number, number];
  base: Vec3;
  matrix: number[];
}

export interface HotspotScript {
  script: string;
  junkBytes?: number;
  steps: Step[];
}

export type Cell = { index: number; polygons: number[][] } & (
  | { kind: "ground"; plane: [number, number, number, number] | null }
  | { kind: "collision"; level: number; segments: [number, number, number, number, number, number][] }
  | { kind: "hotspot"; enter: HotspotScript; exit: HotspotScript }
  | { kind: "visibility"; pvs: { firstRun: number; runCount: number; meshes: number; faces: number } }
);

export type CellKind = Cell["kind"];

export interface WalkMapFile {
  file: string;
  version: number;
  bbox: { min: Vec3; max: Vec3 };
  cells: Cell[];
}

export interface WalkMaps {
  loadOrder: string[];
  start: string;
  grid: { size: number; gapClose: [number, number] };
  maps: WalkMapFile[];
  floors: { walkmap: string; qtsRefIds: string[] }[];
}

export type MacroEvent = "mouseDown" | "mouseUp" | "mouseIn" | "mouseOut";

export interface ElementState {
  visible: boolean;
  text: string;
  anchor: Vec3;
  anchored: boolean;
  icon: number;
  uv: [number, number, number, number];
  sky: boolean;
  macros: Partial<Record<MacroEvent, [string, Step[]]>>;
}

export interface ElementDef {
  index: number;
  name: string;
  class: "caption" | "hotspot" | "image";
  kind: "plain" | "hotspot" | "buttonStrip" | number;
  rect: [number, number, number, number];
  uv: [number, number, number, number];
  primitive: number;
  additive: boolean;
  blend: number;
  textured: boolean;
  capturesMouse: boolean;
  colorKey: boolean;
  isLabel: boolean;
  hasText: boolean;
  diffuse: string;
  textColor: string;
  frameColor: string;
  labelFill: string;
  icons: { normal: number; pressed: number; hover: number };
  font: { face: string; height: number; weight: number; charset: number };
  fileText: string;
  sourceImage: string | null;
  image: string | null;
  state: ElementState | null;
}

export interface LayerFile {
  file: string;
  version: number;
  on: boolean;
  final: boolean;
  elements: ElementDef[];
}

export interface Hud {
  iconStrip: {
    file: string;
    image: string;
    width: number;
    height: number;
    count: number;
    cellWidth: number;
    cellHeight: number;
  } | null;
  captions: {
    font: string;
    fontSizePx: number;
    height: number;
    surface: [number, number];
    captionMetrics: { verticalOffset: number; scale: number; adder: number };
  };
  settings: Record<string, string>;
}

export interface CursorSlot {
  slot: number;
  meaning: string;
  file: string;
  image: string | null;
  hotspot: [number, number] | null;
}

export interface Cursors {
  enabled: boolean;
  slots: CursorSlot[];
  files: Record<string, { image: string; width: number; height: number; hotspot: [number, number] }>;
}

export interface HotspotLink {
  element: string;
  event: string;
  contentId: number;
  contentIdText: string;
  title: string;
  icon: number;
  anchor: Vec3;
  anchored: boolean;
}

export interface SiteMap {
  xOffset: number;
  zOffset: number;
  xExtent: number;
  zExtent: number;
  mapHeight: number | null;
  fieldOfViewDeg: number;
  rotateFrame: number;
  rect: { left: number | null; top: number | null; right: number | null; bottom: number | null };
  payload: string | null;
}

export interface MacroDef {
  name: string;
  body: string;
  terminated: boolean;
  origin: string;
  runDuringLoad: boolean;
  steps: Step[];
}

export interface AlarmDef {
  name: string;
  active: boolean;
  maxExecutions: number;
  interval: number;
  macroText: string;
  steps: Step[];
  origin: string;
  live: boolean;
}

/** One converted statement (docs/tour-scene.md "Steps"). */
export type Step = { src: string } & Action;
export type SeqStep = Step & { origin: string };

type Obj = { id: string; target?: string | null };

export type Action =
  | { op: "deleteAll" }
  | { op: "draw" }
  | { op: "loadScript"; file: string }
  | { op: "event"; text: string; name: string; pairs: [string, string][] }
  | { op: "setScene"; prop: string; value: unknown }
  | { op: "ambient"; rgb: RGB }
  | { op: "fogColor"; rgb: RGB }
  | { op: "cursor"; slot: number; file: string; image: string | null }
  | { op: "device"; command: string }
  | { op: "alarmNew"; name: string; active: boolean; maxExecutions: number; interval: number; macroText: string; steps: Step[] }
  | { op: "alarmsEnable"; on: boolean }
  | { op: "alarm"; name: string; action: string; value?: string }
  | { op: "defineMacro"; name: string; body?: string; steps?: Step[] }
  | { op: "deleteMacro"; name: string }
  | { op: "runMacros"; names: string[]; ignored?: string[] }
  | { op: "query"; subsystem: string; what: string }
  | { op: "camera"; prop: string; values: number[] }
  | { op: "cameraCommand"; command: string }
  | { op: "walkmapLoad"; file: string; resolved: string | null }
  | { op: "walkmapEnable"; on: boolean }
  | { op: "walkmapToggle" }
  | { op: "walkmapDelete"; what: string }
  | { op: "lightsLoad"; file: string }
  | { op: "lightsEnable"; on: boolean }
  | { op: "lightsToggle" }
  | { op: "lightsDelete" }
  | ({ op: "light"; prop: string; value: unknown } & Obj)
  | { op: "modelLoad"; file: string }
  | { op: "modelEnable"; on: boolean }
  | { op: "modelSet"; prop: string; value: number }
  | { op: "modelCommand"; command: string }
  | ({ op: "objectFlag"; prop: "visible" | "sprite" | "noIllumination" | "water"; value: boolean } & Obj)
  | ({ op: "applyTexture"; file: string; resolved: string | null; image: string | null } & Obj)
  | ({ op: "filmstrip"; frameWidth: number; wait: number; file: string; resolved: string | null; image: string | null } & Obj)
  | ({ op: "cloud"; file: string; scale: number } & Obj)
  | ({ op: "cloudOffset"; radians: number } & Obj)
  | ({ op: "objectOther"; prop: string; value: string } & Obj)
  | { op: "layerLoad"; file: string }
  | { op: "layerEnable"; on: boolean }
  | { op: "layerToggle" }
  | { op: "layerNew" }
  | { op: "iconFile"; file: string; image: string | null }
  | { op: "icons"; count: number }
  | { op: "layerSet"; prop: string; value: string }
  | ({ op: "elementVisible"; value: boolean | null } & Obj)
  | ({ op: "elementText"; text: string } & Obj)
  | ({ op: "elementAnchor"; point: Vec3; ignored?: string } & Obj)
  | ({ op: "elementAnchored"; value: boolean } & Obj)
  | ({ op: "elementIcon"; icon: number } & Obj)
  | ({ op: "elementMacro"; event: MacroEvent; script: string; steps: Step[] } & Obj)
  | ({ op: "elementNextFrame"; frames: number } & Obj)
  | ({ op: "elementSky"; value: boolean } & Obj)
  | ({ op: "elementOther"; prop: string; value: string } & Obj)
  | { op: "timer"; on: boolean }
  | { op: "noop"; reason: string };

export type Op = Action["op"];
export type StepOf<K extends Op> = Extract<Step, { op: K }>;
