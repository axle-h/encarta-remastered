/**
 * The `.3sc` command interpreter's front end (tours-3d.md §3.2–§3.6): one command string (a `do()`
 * call, a macro body, a walk-map hotspot string or an element macro) becomes a list of steps with
 * the same shapes the converter writes (docs/tour-scene.md "Steps").
 *
 * Coordinates in the result are glTF space (z negated), as in scene.json. Targets are not resolved
 * here: the interpreter resolves `id` against the live state when it runs a step, like the original.
 */
import type { MacroEvent, RGB, Step } from "../types";

/** C `atof`: leading whitespace skipped, longest numeric prefix, 0 when there is none. */
export function atof(s: string): number {
  const m = /^[ \t\n\v\f\r]*([+-]?(?:\d+\.?\d*|\.\d+)(?:[eE][+-]?\d+)?)/.exec(s);
  return m && m[1] ? Number(m[1]) : 0;
}

/** C `atoi`. */
export function atoi(s: string): number {
  const m = /^[ \t\n\v\f\r]*([+-]?\d+)/.exec(s);
  return m && m[1] ? Number.parseInt(m[1], 10) : 0;
}

/** Upper-cases ASCII letters before the first `^` (§3.2 step 4.1). */
export function foldCase(s: string): string {
  const i = s.indexOf("^");
  const upper = (t: string) => t.replace(/[a-z]+/g, (c) => c.toUpperCase());
  return i < 0 ? upper(s) : upper(s.slice(0, i)) + s.slice(i);
}

const asciiUpper = (t: string) => t.replace(/[a-z]+/g, (c) => c.toUpperCase());

type Subsystem = "SCENE" | "SCRIPT" | "" | "CAMERA" | "WALKMAP" | "LIGHTS" | "3D" | "MODEL" | "2D" | "TIMER" | null;
const SUBSYSTEMS = new Set(["SCENE", "SCRIPT", "", "CAMERA", "WALKMAP", "LIGHTS", "3D", "MODEL", "2D", "TIMER"]);

/** One `;`-free statement after folding: `subsystem "." [id "."] property [op value]`. */
export interface Statement {
  text: string;
  subsystem: Subsystem;
  subsystemText: string;
  /** Object id (upper-cased name, or `[n]`) for 3D/MODEL, 2D and LIGHTS paths with two parts. */
  object: string | null;
  property: string;
  op: "=" | "(" | "?" | null;
  value: string;
}

const hasObjects = (s: Subsystem) => s === "3D" || s === "MODEL" || s === "2D" || s === "LIGHTS";
const isScene = (s: Subsystem) => s === "SCENE" || s === "SCRIPT" || s === "";

export function parseStatement(raw: string): Statement {
  const text = foldCase(raw);
  const dot = text.indexOf(".");
  const subsystemText = dot < 0 ? text : text.slice(0, dot);
  const rest = dot < 0 ? "" : text.slice(dot + 1);
  const subsystem: Subsystem = dot >= 0 && SUBSYSTEMS.has(subsystemText) ? (subsystemText as Subsystem) : null;
  const opAt = rest.search(/[=(?]/);
  const path = opAt < 0 ? rest : rest.slice(0, opAt);
  const op = opAt < 0 ? null : (rest[opAt] as "=" | "(" | "?");
  const value = opAt < 0 ? "" : rest.slice(opAt + 1);
  const pdot = path.indexOf(".");
  if (pdot >= 0 && hasObjects(subsystem)) {
    return { text, subsystem, subsystemText, object: path.slice(0, pdot), property: path.slice(pdot + 1), op, value };
  }
  return { text, subsystem, subsystemText, object: null, property: path, op, value };
}

/** The text after the first `.`. */
const restOf = (st: Statement) => st.text.slice(st.subsystemText.length + 1);
/** The argument as handlers see it: the value with one trailing `)` removed. */
const argOf = (st: Statement) => (st.value.endsWith(")") ? st.value.slice(0, -1) : st.value);
const numbersOf = (st: Statement) => argOf(st).split(",").map(atof);
function boolOf(st: Statement): boolean | null {
  const v = argOf(st).trim();
  if (v === "1" || v === "TRUE" || v === "ON" || v === "ACTIVE") return true;
  if (v === "0" || v === "FALSE" || v === "OFF") return false;
  return null;
}
/** 3D flag properties look only at the first character after `=` (§7.4). */
function flagOf(st: Statement): boolean | null {
  const c = st.value[0];
  return c === "0" ? false : c === "1" ? true : null;
}
function textOf(st: Statement): string {
  const v = argOf(st);
  if (!v.startsWith("^")) return v;
  const rest = v.slice(1);
  const end = rest.indexOf("^");
  return end < 0 ? rest : rest.slice(0, end);
}
const rgbOf = (st: Statement): RGB => {
  const n = numbersOf(st);
  return [Math.trunc(n[0] ?? 0), Math.trunc(n[1] ?? 0), Math.trunc(n[2] ?? 0)];
};

/** `SCENE.` keywords in match order: the first prefix that matches wins (§3.3). */
const SCENE_KEYWORDS = [
  "WORLDINFO?", "FORCESOFTWARE", "TRYHARDWAREFULLSCREEN", "TRYHARDWARE", "TRYFULLSCREEN", "KILLFULLSCREEN",
  "DELETEALL", "FOGCOLOR", "FOGSTART", "FOGEND", "HALFRESDRIVING", "HIGHSPECULAR", "SPECULAR", "DISPLAYFPS",
  "LOADFILESFROMITS", "LOAD", "GRAVITY", "ITSFILEURL", "CURSORS", "CURSOR", "EVENT", "WORKINGPATH", "RUN", "DRAW",
  "SHOWCOLLISION", "AMBIENT", "ALARM.", "CLEARSEQUENCE", "ADDSEQUENCE",
];

function sceneKeyword(st: Statement): string | null {
  if (!isScene(st.subsystem)) return null;
  const rest = restOf(st);
  const k = SCENE_KEYWORDS.find((kw) => rest.startsWith(kw));
  return k ? k.replace(/[.?]$/, "") : null;
}

/** Parses an event payload: `NAME` or `NAME:key=value,key=value,…`. */
export function parseEvent(text: string): { text: string; name: string; pairs: [string, string][] } {
  const c = text.indexOf(":");
  const name = c < 0 ? text : text.slice(0, c);
  const rest = c < 0 ? "" : text.slice(c + 1);
  const pairs = rest
    .split(",")
    .filter((p) => p.length > 0)
    .map((p): [string, string] => {
      const e = p.indexOf("=");
      return e < 0 ? [p, ""] : [p.slice(0, e), p.slice(e + 1)];
    });
  return { text, name, pairs };
}

type A = Step extends infer S ? (S extends { src: string } ? Omit<S, "src"> : never) : never;
const noop = (reason: string): A => ({ op: "noop", reason });
const query = (st: Statement): A => ({ op: "query", subsystem: st.subsystem ?? "?", what: restOf(st).replace(/\?+$/, "") });

function sceneStatement(st: Statement): A {
  const kw = sceneKeyword(st);
  // Events (§3.6).
  if (kw === "EVENT" && st.op !== null && st.op !== "?") {
    const ev = parseEvent(argOf(st));
    return { op: "event", ...ev };
  }
  // Alarms (§3.5).
  if (st.object === null && st.property.startsWith("ALARM.")) {
    const path = st.property.slice("ALARM.".length);
    if (path === "NEW") {
      const parts = st.value.split(",");
      const head = parts.slice(0, 4);
      const macroText = parts.slice(4).join(",").replace(/\)+$/, "");
      return {
        op: "alarmNew",
        active: atoi(head[0] ?? "") === 1,
        maxExecutions: atoi(head[1] ?? ""),
        interval: atof(head[2] ?? ""),
        name: head[3] ?? "",
        macroText,
        steps: parseCommand(macroText),
      };
    }
    if (path === "ACTIVATE") return { op: "alarmsEnable", on: true };
    if (path === "DEACTIVATE") return { op: "alarmsEnable", on: false };
    const d = path.indexOf(".");
    const name = d < 0 ? path : path.slice(0, d);
    const action = d < 0 ? "" : path.slice(d + 1);
    if (action === "ACTIVATE") return { op: "alarm", name, action: "activate" };
    if (action === "DEACTIVATE") return { op: "alarm", name, action: "deactivate" };
    if (action === "ACTIVE.TOGGLE" || (action === "ACTIVE" && st.op === "=" && argOf(st) === "TOGGLE")) return { op: "alarm", name, action: "toggle" };
    if (action === "DELETE") return { op: "alarm", name, action: "delete" };
    if (action === "ADDSEQUENCE") return { op: "alarm", name, action: "addSequence", value: argOf(st) };
    if (action === "CLEARSEQUENCE") return { op: "alarm", name, action: "clearSequence" };
    return noop(`unknown alarm command ${action}`);
  }
  if (!kw) return noop("unknown SCENE command");
  const rest = restOf(st);
  let v = rest.slice(kw.length);
  if (v.startsWith("=") || v.startsWith("(")) v = v.slice(1);
  if (v.endsWith(")")) v = v.slice(0, -1);
  const flag = (s: string) => ["1", "TRUE", "ON"].includes(s.trim());
  const set = (prop: string, value: unknown): A => ({ op: "setScene", prop, value });
  switch (kw) {
    case "WORLDINFO":
      return query(st);
    case "FORCESOFTWARE":
    case "TRYHARDWAREFULLSCREEN":
    case "TRYHARDWARE":
    case "TRYFULLSCREEN":
    case "KILLFULLSCREEN":
      return { op: "device", command: kw };
    case "DELETEALL":
      return { op: "deleteAll" };
    case "DRAW":
      return { op: "draw" };
    case "LOAD":
    case "RUN":
      return { op: "loadScript", file: v.trim() };
  }
  if (rest.endsWith("?")) return query(st);
  switch (kw) {
    case "FOGCOLOR":
      return { op: "fogColor", rgb: rgbOf(st) };
    case "FOGSTART":
      return set("fogStart", atof(v));
    case "FOGEND":
      return set("fogEnd", atof(v));
    case "HALFRESDRIVING":
      return set("halfResDriving", flag(v));
    case "HIGHSPECULAR":
      return set("highSpecular", flag(v));
    case "SPECULAR":
      return set("specular", flag(v));
    case "DISPLAYFPS":
      // The 2nd character after the keyword is tested: `=1` turns the overlay on (§3.3).
      return set("displayFps", rest[kw.length + 1] === "1");
    case "LOADFILESFROMITS":
      return set("loadFilesFromIts", flag(v));
    case "GRAVITY":
      return set("gravity", atof(v));
    case "ITSFILEURL":
      return set("itsFileUrl", v);
    case "WORKINGPATH":
      return set("workingPath", v);
    case "SHOWCOLLISION":
      return set("showCollision", flag(v));
    case "CURSORS":
      return set("cursors", flag(v));
    case "CURSOR": {
      const a = argOf(st);
      const c = a.indexOf(",");
      const slot = atoi(c < 0 ? a : a.slice(0, c));
      const file = c < 0 ? "" : a.slice(c + 1).trim();
      return { op: "cursor", slot, file, image: null };
    }
    case "AMBIENT":
      return { op: "ambient", rgb: rgbOf(st) };
    case "CLEARSEQUENCE":
    case "ADDSEQUENCE":
      return noop("alarm sequence command without an alarm name");
  }
  return noop("unknown SCENE command");
}

const CAMERA_PROPS: Record<string, string> = {
  POS: "pos", MOVETO: "moveTo", MOVE: "move", PITCH: "pitch", FOV: "fov", HEIGHT: "height",
  "SPEED.FLY": "speedFly", "SPEED.TURN": "speedTurn", PITCHSPEED: "pitchSpeed",
  COLLISIONRADIUS: "collisionRadius", CLIMBRATE: "climbRate", NEAR: "near", FAR: "far",
  BACKGROUNDCOLOR: "backgroundColor", CLEARBACKGROUND: "clearBackground",
};
const CAMERA_COMMANDS: Record<string, string> = { HOME: "home", HOMEPOV: "homePov", HOMEOMNI: "homeOmni", POV: "pov", OMNI: "omni" };

function cameraStatement(st: Statement): A {
  if (st.op === "?" || st.property.endsWith("?")) return query(st);
  const command = CAMERA_COMMANDS[st.property];
  if (command) return { op: "cameraCommand", command };
  const prop = CAMERA_PROPS[st.property];
  if (!prop) return noop("unknown CAMERA property");
  const values = numbersOf(st);
  if ((prop === "pos" || prop === "moveTo" || prop === "move") && values.length >= 3) values[2] = -(values[2] ?? 0);
  return { op: "camera", prop, values };
}

function walkmapStatement(st: Statement): A {
  if (st.op === "?" || restOf(st).endsWith("?")) return query(st);
  if (st.property === "LOAD" && st.op) return { op: "walkmapLoad", file: argOf(st).trim(), resolved: null };
  if (st.op === null) {
    if (st.property === "ON") return { op: "walkmapEnable", on: true };
    if (st.property === "OFF") return { op: "walkmapEnable", on: false };
    if (st.property === "TOGGLE") return { op: "walkmapToggle" };
    if (st.property.startsWith("DELETE.")) return { op: "walkmapDelete", what: st.property.slice(7).toLowerCase() };
  }
  return noop("unknown WALKMAP command");
}

function lightsStatement(st: Statement): A {
  if (st.op === "?" || restOf(st).endsWith("?")) return query(st);
  if (st.object === null) {
    if (st.property === "LOAD" && st.op) return { op: "lightsLoad", file: argOf(st).trim() };
    if (st.op === null) {
      if (st.property === "ON") return { op: "lightsEnable", on: true };
      if (st.property === "OFF") return { op: "lightsEnable", on: false };
      if (st.property === "TOGGLE") return { op: "lightsToggle" };
      if (st.property === "DELETE") return { op: "lightsDelete" };
    }
    return noop("unknown LIGHTS command");
  }
  const id = st.object;
  if (st.op === null && st.property === "ON") return { op: "light", id, prop: "on", value: true };
  if (st.op === null && st.property === "OFF") return { op: "light", id, prop: "on", value: false };
  if (st.op !== null) {
    if (st.property === "FLICKER") return { op: "light", id, prop: "flicker", value: atof(argOf(st)) };
    if (st.property === "COLOR") return { op: "light", id, prop: "color", value: rgbOf(st) };
    if (st.property === "POS") {
      const n = numbersOf(st);
      return { op: "light", id, prop: "pos", value: [n[0] ?? 0, n[1] ?? 0, -(n[2] ?? 0)] };
    }
    if (st.property === "STATIC") return { op: "light", id, prop: "static", value: boolOf(st) === true };
  }
  return noop("unknown LIGHTS property");
}

function modelStatement(st: Statement): A {
  if (st.op === "?" || restOf(st).endsWith("?")) return query(st);
  if (st.object === null) {
    const p = st.property;
    if (p === "LOAD" && st.op) return { op: "modelLoad", file: argOf(st).trim() };
    if (p === "ON" && st.op === null) return { op: "modelEnable", on: true };
    if (p === "OFF" && st.op === null) return { op: "modelEnable", on: false };
    if (p === "CALC64BIT" && st.op) return { op: "modelSet", prop: "calc64bit", value: atof(argOf(st)) };
    if (p === "LODDISTANCE" && st.op) return { op: "modelSet", prop: "lodDistance", value: atof(argOf(st)) };
    if ((p === "UNHIDEALL" || p === "TOGGLEHIDDEN" || p === "DELETE") && st.op === null) return { op: "modelCommand", command: p.toLowerCase() };
    if (p === "LOADVP" || p === "APPENDVP" || p === "PROCESSVP") return noop("listed in the help text but not implemented (§7.5)");
    return noop("unknown 3D command");
  }
  const id = st.object;
  const flagProps: Record<string, "visible" | "sprite" | "noIllumination" | "water"> = {
    VISIBLE: "visible", SPRITE: "sprite", NOILLUMINATION: "noIllumination", WATER: "water",
  };
  const fp = flagProps[st.property];
  if (fp) {
    const value = flagOf(st);
    return value === null ? noop("flag value must start with 0 or 1 (§7.4)") : { op: "objectFlag", id, prop: fp, value };
  }
  switch (st.property) {
    case "APPLYTEXTURE":
      return { op: "applyTexture", id, file: argOf(st).trim(), resolved: null, image: null };
    case "FILMSTRIP": {
      const a = argOf(st);
      const parts = a.split(",");
      const frameWidth = atoi(parts[0] ?? "");
      const wait = atof(parts[1] ?? "");
      const file = parts.slice(2).join(",").trim();
      return { op: "filmstrip", id, frameWidth, wait, file, resolved: null, image: null };
    }
    case "CLOUD":
    case "CLOUDSCALED": {
      const args = argOf(st).split(",");
      const file = (args[0] ?? "").trim();
      const scale = st.property === "CLOUD" ? 1 : atof(args[1] ?? "");
      return { op: "cloud", id, file, scale };
    }
    case "CLOUD.OFFSET":
      return { op: "cloudOffset", id, radians: atof(argOf(st)) };
  }
  return { op: "objectOther", id, prop: st.property.toLowerCase(), value: st.value };
}

const MACRO_EVENTS: Record<string, MacroEvent> = { MOUSEDOWN: "mouseDown", MOUSEUP: "mouseUp", MOUSEIN: "mouseIn", MOUSEOUT: "mouseOut" };

function layerStatement(st: Statement): A {
  if (st.op === "?" || restOf(st).endsWith("?")) return query(st);
  if (st.object === null) {
    const p = st.property;
    if (p === "LOAD" && st.op) return { op: "layerLoad", file: argOf(st).trim() };
    if (st.op === null) {
      if (p === "ON" || p === "SHOW") return { op: "layerEnable", on: true };
      if (p === "OFF" || p === "HIDE") return { op: "layerEnable", on: false };
      if (p === "TOGGLE") return { op: "layerToggle" };
      if (p === "NEW") return { op: "layerNew" };
    } else {
      if (p === "ICONFILE") return { op: "iconFile", file: argOf(st).trim(), image: null };
      if (p === "ICONS") return { op: "icons", count: atoi(argOf(st)) };
      if (["FONT", "FONTSIZE", "CAPTIONMETRICS", "JVERSION", "HARDWARE", "GRID"].includes(p)) return { op: "layerSet", prop: p.toLowerCase(), value: argOf(st) };
    }
    return noop("unknown 2D command");
  }
  const id = st.object;
  const p = st.property;
  if (p === "VISIBLE" && argOf(st).trim() === "TOGGLE") return { op: "elementVisible", id, value: null };
  if (p === "VISIBLE.TOGGLE") return { op: "elementVisible", id, value: null };
  if (p === "VISIBLE") {
    const b = boolOf(st);
    return b === null ? noop("VISIBLE expects TRUE/1/FALSE/0/TOGGLE") : { op: "elementVisible", id, value: b };
  }
  if (p === "TEXT") return { op: "elementText", id, text: textOf(st) };
  if (p === "ANCHORPOINT") {
    // Split at the first two commas; the third number is read with atof (§8.4).
    const v = argOf(st);
    const c1 = v.indexOf(",");
    const c2 = c1 < 0 ? -1 : v.indexOf(",", c1 + 1);
    const x = atof(c1 < 0 ? v : v.slice(0, c1));
    const y = c1 < 0 ? 0 : atof(c2 < 0 ? v.slice(c1 + 1) : v.slice(c1 + 1, c2));
    const rest = c2 < 0 ? "" : v.slice(c2 + 1);
    const z = atof(rest);
    const r = rest.indexOf(",");
    const ignored = r < 0 ? "" : rest.slice(r);
    return ignored ? { op: "elementAnchor", id, point: [x, y, -z], ignored } : { op: "elementAnchor", id, point: [x, y, -z] };
  }
  if (p === "ANCHORED") {
    const b = boolOf(st);
    return b === null ? noop("ANCHORED expects TRUE/1/FALSE/0") : { op: "elementAnchored", id, value: b };
  }
  if (p === "ICON") return { op: "elementIcon", id, icon: atoi(argOf(st)) };
  if (p === "NEXTFRAME") return { op: "elementNextFrame", id, frames: atoi(argOf(st)) };
  if (p === "SKY") {
    const b = boolOf(st);
    return b === null ? noop("SKY expects TRUE/1/FALSE/0") : { op: "elementSky", id, value: b };
  }
  if (p.startsWith("MACRO.")) {
    const event = MACRO_EVENTS[p.slice(6)];
    if (!event) return noop("unknown 2D macro event");
    return { op: "elementMacro", id, event, script: st.value, steps: parseCommand(st.value) };
  }
  return { op: "elementOther", id, prop: p.toLowerCase(), value: st.value };
}

/** Converts one parsed statement into an action. */
export function statementAction(st: Statement): A {
  switch (st.subsystem) {
    case "SCENE":
    case "SCRIPT":
    case "":
      return sceneStatement(st);
    case "CAMERA":
      return cameraStatement(st);
    case "WALKMAP":
      return walkmapStatement(st);
    case "LIGHTS":
      return lightsStatement(st);
    case "3D":
    case "MODEL":
      return modelStatement(st);
    case "2D":
      return layerStatement(st);
    case "TIMER":
      if (st.property === "ON" && st.op === null) return { op: "timer", on: true };
      if (st.property === "OFF" && st.op === null) return { op: "timer", on: false };
      if (st.op === "?" || st.property.endsWith("?")) return query(st);
      return noop("unknown TIMER command");
    default:
      return noop("no known subsystem (the interpreter ignores the statement)");
  }
}

/** Parses one command string (§3.2–§3.4) into steps. */
export function parseCommand(text: string): Step[] {
  if (text === "?") return [{ src: text, op: "noop", reason: "help text request" }];
  const upper = asciiUpper(text);
  if (upper.startsWith("STARTMACROLOG") || upper.startsWith("STOPMACROLOG")) {
    return [{ src: text, op: "noop", reason: "macro log (debugging aid)" }];
  }
  if (upper.startsWith("RUN(")) {
    const names: string[] = [];
    const ignored: string[] = [];
    for (const piece of text.split(";")) {
      if (!piece) continue;
      const up = asciiUpper(piece);
      if (up.startsWith("RUN(")) names.push(up.slice(4).split(")")[0] ?? "");
      else ignored.push(piece);
    }
    return [ignored.length ? { src: text, op: "runMacros", names, ignored } : { src: text, op: "runMacros", names }];
  }
  if (upper.startsWith("BEGIN(")) {
    const after = text.slice(6);
    const i = after.indexOf(");");
    const name = asciiUpper(i < 0 ? (after.split(")")[0] ?? "") : after.slice(0, i));
    const bodyAll = i < 0 ? "" : text.slice(6 + i + 2);
    const end = asciiUpper(bodyAll).indexOf(`END(${name}`);
    const body = end < 0 ? bodyAll : bodyAll.slice(0, end);
    return [{ src: text, op: "defineMacro", name, body, steps: parseCommand(body) }];
  }
  if (upper.startsWith("DELETE(")) {
    return [{ src: text, op: "deleteMacro", name: upper.slice(7).split(")")[0] ?? "" }];
  }
  const out: Step[] = [];
  for (const piece of text.split(";")) {
    if (!piece) continue;
    out.push({ src: piece, ...statementAction(parseStatement(piece)) } as Step);
  }
  return out;
}
