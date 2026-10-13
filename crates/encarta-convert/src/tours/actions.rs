//! `.3sc` statements converted into structured actions for the web runtime.
//!
//! Every action keeps the statement text it came from (`src`), so a runtime or a reviewer can trace
//! it back to the disc. The conversion follows the interpreter rules of tours-3d.md §3 and the
//! per-handler value rules of §6–§8, including their quirks:
//! - case folding before the first `^`; ids are resolved case-insensitively, first match;
//! - an id that names nothing keeps `target: null` (the original fails silently);
//! - `3D` flag properties look only at the first character after `=` (§7.4), so the glued
//!   Acropolis `HIDEALL` statement becomes `WATER=0`;
//! - `2D.<id>.ANCHORPOINT` uses only x, y, z (§8.4);
//! - a command that starts with `RUN(` runs only its `RUN(…)` pieces (§3.4); a `RUN(…)` piece
//!   inside an ordinary statement list has no subsystem and is ignored;
//! - `LIGHTS.TOGGLE`, `WALKMAP.TOGGLE` and `2D.TOGGLE` change nothing (§7.2.4, §6.10, §8.3).
//!
//! World coordinates in actions (`point`, camera `pos`, light `pos`) are converted to glTF space
//! (z negated); yaw values stay in the original degrees (see docs/tour-scene.md).

use std::cell::RefCell;
use std::collections::BTreeSet;

use encarta_formats::tours::layer2d::Layer;
use encarta_formats::tours::script::{self, AlarmAction, AlarmCommand, Command, ObjectId, Op, Statement, Subsystem};
use encarta_formats::tours::textures::{self, ImageFormat};
use encarta_formats::tours::tour::Tour;
use serde::Serialize;

/// One converted statement (or command) with its source text.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Step {
    /// The statement text as written (case preserved; the interpreter folds it, §3.2).
    pub src: String,
    #[serde(flatten)]
    pub action: Action,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "op", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum Action {
    // ---- scene / script -------------------------------------------------------------------
    /// `SCENE.DELETEALL`: full reset (§3.3).
    DeleteAll,
    /// `SCENE.DRAW`: render a frame now (a yield point while loading).
    Draw,
    /// `SCENE.LOAD` / `SCENE.RUN`: run a script file. Load sequences have these expanded in place.
    LoadScript { file: String },
    /// `SCENE.EVENT` / `SCRIPT.EVENT`: fire `Event3DX(text)` to the host.
    Event { text: String, name: String, pairs: Vec<[String; 2]> },
    /// Scene-wide settings: `gravity`, `specular`, `highSpecular`, `displayFps`, `cursors`,
    /// `halfResDriving`, `showCollision`, `fogStart`, `fogEnd`, `loadFilesFromIts`, `workingPath`,
    /// `itsFileUrl`.
    SetScene { prop: String, value: serde_json::Value },
    /// `SCENE.AMBIENT=r,g,b` (ignored when no model is loaded).
    Ambient { rgb: [i64; 3] },
    /// `SCENE.FOGCOLOR=r,g,b`.
    FogColor { rgb: [i64; 3] },
    /// `SCENE.CURSOR=slot,file`; `image` is null when the file does not exist (`grip.cur`).
    Cursor { slot: i64, file: String, image: Option<String> },
    /// Device selection (`TRYHARDWARE`, `TRYFULLSCREEN`, …): a no-op returning `Success` in a web runtime.
    Device { command: String },
    /// `SCENE.ALARM.NEW(active,max,interval,name,macro)`.
    AlarmNew { name: String, active: bool, max_executions: i64, interval: f64, macro_text: String, steps: Vec<Step> },
    /// `SCENE.ALARM.ACTIVATE` / `DEACTIVATE`: the global alarm switch (restarts every alarm clock).
    AlarmsEnable { on: bool },
    /// `SCENE.ALARM.<name>.<action>`: `activate`, `deactivate`, `toggle`, `delete`,
    /// `addSequence`, `clearSequence`.
    Alarm {
        name: String,
        action: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        value: Option<String>,
    },
    /// `BEGIN(name);…END(name);` (the body is in the scene's macro table).
    DefineMacro { name: String },
    /// `DELETE(name)`.
    DeleteMacro { name: String },
    /// `RUN(a);RUN(b)`: run macros in order; `ignored` are non-`RUN` pieces of the same command.
    RunMacros {
        names: Vec<String>,
        #[serde(skip_serializing_if = "Vec::is_empty")]
        ignored: Vec<String>,
    },
    /// A `?` query: returns a value, no side effect.
    Query { subsystem: String, what: String },
    // ---- camera ---------------------------------------------------------------------------
    /// `CAMERA.<prop>=values`: `pos` (x,y,z,yawDeg, glTF space), `moveTo`, `move` (dx,dy,dz),
    /// `pitch`, `fov`, `height`, `speedFly`, `speedTurn`, `pitchSpeed`, `collisionRadius`,
    /// `climbRate`, `near`, `far`, `backgroundColor`, `clearBackground`.
    Camera { prop: String, values: Vec<f64> },
    /// `CAMERA.HOME`, `HOMEPOV`, `HOMEOMNI`, `POV`, `OMNI`.
    CameraCommand { command: String },
    // ---- walk maps ------------------------------------------------------------------------
    /// `WALKMAP.LOAD`: switch the active map (cached by name); `resolved` is the stored file name.
    WalkmapLoad { file: String, resolved: Option<String> },
    WalkmapEnable { on: bool },
    /// `WALKMAP.TOGGLE`: negates the flag, so it can never switch the walk map off (§6.10).
    WalkmapToggle,
    /// `WALKMAP.DELETE.<what>`.
    WalkmapDelete { what: String },
    // ---- lights ---------------------------------------------------------------------------
    LightsLoad { file: String },
    /// `LIGHTS.ON` / `OFF`: global switch; off means every vertex is full-bright.
    LightsEnable { on: bool },
    /// `LIGHTS.TOGGLE`: a sign flip that changes nothing (§7.2.4).
    LightsToggle,
    LightsDelete,
    /// `LIGHTS.<id>.<prop>`: `flicker` (seconds), `on` (bool), `color` (r,g,b), `pos` (glTF
    /// space), `static` (bool). `target` is the light's name, or null when no light matches.
    Light { id: String, target: Option<String>, prop: String, value: serde_json::Value },
    // ---- model (3D) -----------------------------------------------------------------------
    ModelLoad { file: String },
    /// `3D.ON` / `OFF`: master 3D draw switch.
    ModelEnable { on: bool },
    /// `3D.CALC64BIT`, `3D.LODDISTANCE`.
    ModelSet { prop: String, value: f64 },
    /// `3D.UNHIDEALL`, `3D.TOGGLEHIDDEN`, `3D.DELETE`.
    ModelCommand { command: String },
    /// `3D.<id>.VISIBLE|SPRITE|NOILLUMINATION|WATER=0/1` (`prop` is `visible`, `sprite`,
    /// `noIllumination` or `water`). `target` is the glb node name (the first mesh whose name
    /// matches case-insensitively), or null.
    ObjectFlag { id: String, target: Option<String>, prop: String, value: bool },
    /// `3D.<id>.APPLYTEXTURE=file`: replaces the texture record of the object's first face.
    ApplyTexture { id: String, target: Option<String>, file: String, resolved: Option<String>, image: Option<String> },
    /// `3D.<id>.FILMSTRIP=frameWidth,wait,file`.
    Filmstrip { id: String, target: Option<String>, frame_width: i64, wait: f64, file: String, resolved: Option<String>, image: Option<String> },
    /// `3D.<id>.CLOUD=file` / `CLOUDSCALED=file,scale`.
    Cloud { id: String, target: Option<String>, file: String, scale: f64 },
    /// `3D.<id>.CLOUD.OFFSET=radians`.
    CloudOffset { id: String, target: Option<String>, radians: f64 },
    /// Other `3D.<id>` properties (`PLAYAVI`, `STRING`, …), kept raw.
    ObjectOther { id: String, target: Option<String>, prop: String, value: String },
    // ---- 2D layer -------------------------------------------------------------------------
    LayerLoad { file: String },
    /// `2D.ON|SHOW` / `2D.OFF|HIDE`: gate the whole layer.
    LayerEnable { on: bool },
    /// `2D.TOGGLE`: reports the flag, changes nothing (§8.3).
    LayerToggle,
    /// `2D.NEW`: delete all elements.
    LayerNew,
    /// `2D.ICONFILE=file`.
    IconFile { file: String, image: Option<String> },
    /// `2D.ICONS=n`.
    Icons { count: i64 },
    /// `2D.FONT`, `FONTSIZE`, `CAPTIONMETRICS`, `JVERSION`, `HARDWARE`, `GRID`, kept raw.
    LayerSet { prop: String, value: String },
    /// `2D.<id>.VISIBLE=bool` or `VISIBLE.TOGGLE` / `VISIBLE=TOGGLE` (`value` null).
    ElementVisible { id: String, target: Option<String>, value: Option<bool> },
    /// `2D.<id>.TEXT=^…^` (text with its case kept).
    ElementText { id: String, target: Option<String>, text: String },
    /// `2D.<id>.ANCHORPOINT=x,y,z[,…]`: only x, y, z are used (glTF space); `ignored` is the rest.
    ElementAnchor {
        id: String,
        target: Option<String>,
        point: [f64; 3],
        #[serde(skip_serializing_if = "String::is_empty")]
        ignored: String,
    },
    ElementAnchored { id: String, target: Option<String>, value: bool },
    /// `2D.<id>.ICON=n`: normal = n, pressed = n+1, hover = n+2; −1 = none.
    ElementIcon { id: String, target: Option<String>, icon: i64 },
    /// `2D.<id>.MACRO.<event>=script`: `event` is `mouseDown`, `mouseUp`, `mouseIn` or
    /// `mouseOut`; `script` is the stored (folded) text, `steps` its conversion.
    ElementMacro { id: String, target: Option<String>, event: String, script: String, steps: Vec<Step> },
    /// `2D.<id>.NEXTFRAME(n)`: advance a vertical film strip of n frames (§8.4.3).
    ElementNextFrame { id: String, target: Option<String>, frames: i64 },
    /// `2D.<id>.SKY=bool`.
    ElementSky { id: String, target: Option<String>, value: bool },
    /// Other element properties (`FONT`, `FONTSIZE`, `REDRAWLABEL`, `MOVE…`), kept raw.
    ElementOther { id: String, target: Option<String>, prop: String, value: String },
    // ---- other ----------------------------------------------------------------------------
    /// `TIMER.ON` / `OFF`.
    Timer { on: bool },
    /// A statement the original ignores or rejects; `reason` says why.
    Noop { reason: String },
}

impl Action {
    /// The `target` of an object action: `Some(None)` when the id names nothing.
    pub fn target(&self) -> Option<&Option<String>> {
        use Action::*;
        match self {
            Light { target, .. }
            | ObjectFlag { target, .. }
            | ApplyTexture { target, .. }
            | Filmstrip { target, .. }
            | Cloud { target, .. }
            | CloudOffset { target, .. }
            | ObjectOther { target, .. }
            | ElementVisible { target, .. }
            | ElementText { target, .. }
            | ElementAnchor { target, .. }
            | ElementAnchored { target, .. }
            | ElementIcon { target, .. }
            | ElementMacro { target, .. }
            | ElementNextFrame { target, .. }
            | ElementSky { target, .. }
            | ElementOther { target, .. } => Some(target),
            _ => None,
        }
    }

    /// Nested steps (alarm and element macros).
    pub fn nested(&self) -> &[Step] {
        match self {
            Action::AlarmNew { steps, .. } | Action::ElementMacro { steps, .. } => steps,
            _ => &[],
        }
    }

    pub fn is_3d_object(&self) -> bool {
        matches!(self, Action::ObjectFlag { .. } | Action::ApplyTexture { .. } | Action::Filmstrip { .. } | Action::Cloud { .. } | Action::CloudOffset { .. } | Action::ObjectOther { .. })
    }
}

/// An image the converter must write: (stored container file, apply the colour key).
pub type ImageRequest = (String, bool);

/// Name resolution and image naming for one tour.
pub struct Ctx<'a> {
    pub tour: &'a Tour,
    /// The 2D layer `2D.<id>` statements resolve against (the one loaded at the time).
    pub layer: Option<&'a Layer>,
    pub images: &'a RefCell<BTreeSet<ImageRequest>>,
}

/// Output path of a converted image, relative to the tour directory: `images/<stored name>` for
/// JPEGs, `images/<stored name>.png` otherwise, `images/<stored name>.key.png` for keyed copies.
pub fn image_path(tour: &Tour, stored: &str, key: bool) -> String {
    let jpeg = tour.images.get(stored).is_some_and(|i| i.format == ImageFormat::Jpeg);
    if jpeg {
        let lower = stored.to_ascii_lowercase();
        if lower.ends_with(".jpg") || lower.ends_with(".jpeg") { format!("images/{stored}") } else { format!("images/{stored}.jpg") }
    } else if key {
        format!("images/{stored}.key.png")
    } else {
        format!("images/{stored}.png")
    }
}

impl Ctx<'_> {
    fn mesh(&self, id: &ObjectId) -> Option<String> {
        let x = &self.tour.model.as_ref()?.data;
        match id {
            ObjectId::Name(n) => x.mesh_by_script_name(n).map(|(_, m)| m.name.clone()),
            ObjectId::Index(i) => x.meshes.get(*i as usize).map(|m| m.name.clone()),
        }
    }

    fn light(&self, id: &ObjectId) -> Option<String> {
        let l = &self.tour.lights.as_ref()?.data;
        match id {
            ObjectId::Name(n) => l.by_script_name(n).map(|(_, l)| l.name.clone()),
            ObjectId::Index(i) => l.lights.get(*i as usize).map(|l| l.name.clone()),
        }
    }

    fn element(&self, id: &ObjectId) -> Option<String> {
        let l = self.layer?;
        match id {
            ObjectId::Name(n) => l.element(n).map(|(_, e)| e.name.clone()),
            ObjectId::Index(i) => l.elements.get(*i as usize).map(|e| e.name.clone()),
        }
    }

    /// Resolves an image name with the texture rule (§5.5) and records it for output. 3D images are
    /// keyed when they are colour-keyed BMPs (`key: None`); 2D images follow the element flag.
    fn image(&self, name: &str, key: Option<bool>) -> (Option<String>, Option<String>) {
        let (_, found) = textures::resolve(name, |n| self.tour.container.find(n));
        let Some(stored) = found else { return (None, None) };
        let key = key.unwrap_or_else(|| self.tour.images.get(stored).is_some_and(|i| i.colour_keyed));
        self.images.borrow_mut().insert((stored.to_string(), key));
        (Some(stored.to_string()), Some(image_path(self.tour, stored, key)))
    }

    fn file(&self, name: &str) -> Option<String> {
        self.tour.container.find(name).map(str::to_string)
    }
}

/// Splits a command into its raw statement pieces exactly like the interpreter (§3.2 step 4).
fn raw_pieces(text: &str) -> Vec<&str> {
    text.split(';').filter(|p| !p.is_empty()).collect()
}

/// Converts one command string (a script line, a macro body, a hotspot or element macro string).
pub fn convert_command(text: &str, ctx: &Ctx) -> Vec<Step> {
    match script::parse_command(text) {
        Command::Help => vec![Step { src: text.into(), action: Action::Noop { reason: "help text request".into() } }],
        Command::MacroLog(_) => vec![Step { src: text.into(), action: Action::Noop { reason: "macro log (debugging aid)".into() } }],
        Command::Define(d) => vec![Step { src: text.into(), action: Action::DefineMacro { name: d.name } }],
        Command::Delete(name) => vec![Step { src: text.into(), action: Action::DeleteMacro { name } }],
        Command::Run { names, ignored } => vec![Step { src: text.into(), action: Action::RunMacros { names, ignored } }],
        Command::Statements(list) => raw_pieces(text).into_iter().zip(list).map(|(raw, st)| Step { src: raw.to_string(), action: convert_statement(&st, ctx) }).collect(),
    }
}

/// The value after a scene keyword (`DRAW)` → `)`; `LOAD(x)` → `x`).
fn scene_value<'s>(st: &'s Statement, kw: &str) -> &'s str {
    let v = st.rest().get(kw.len()..).unwrap_or("");
    let v = v.strip_prefix(['=', '(']).unwrap_or(v);
    v.strip_suffix(')').unwrap_or(v)
}

fn rgb(nums: &[f64]) -> [i64; 3] {
    let g = |i: usize| nums.get(i).copied().unwrap_or(0.0) as i64;
    [g(0), g(1), g(2)]
}

fn query(st: &Statement) -> Action {
    Action::Query { subsystem: st.subsystem.label().to_string(), what: st.rest().trim_end_matches('?').to_string() }
}

fn noop(reason: &str) -> Action {
    Action::Noop { reason: reason.to_string() }
}

/// Converts one statement.
pub fn convert_statement(st: &Statement, ctx: &Ctx) -> Action {
    match st.subsystem {
        s if s.is_scene() => scene(st, ctx),
        Subsystem::Camera => camera(st),
        Subsystem::Walkmap => walkmap(st, ctx),
        Subsystem::Lights => lights(st, ctx),
        s if s.is_3d() => model(st, ctx),
        Subsystem::Layer2d => layer(st, ctx),
        Subsystem::Timer => match st.property.as_str() {
            "ON" if st.op.is_none() => Action::Timer { on: true },
            "OFF" if st.op.is_none() => Action::Timer { on: false },
            _ if st.op == Some(Op::Query) || st.property.ends_with('?') => query(st),
            _ => noop("unknown TIMER command"),
        },
        _ => noop("no known subsystem (the interpreter ignores the statement)"),
    }
}

fn scene(st: &Statement, ctx: &Ctx) -> Action {
    if let Some(ev) = st.event() {
        return Action::Event { text: ev.text.clone(), name: ev.name.clone(), pairs: ev.pairs.iter().map(|(k, v)| [k.clone(), v.clone()]).collect() };
    }
    if let Some(a) = st.alarm() {
        return match a {
            AlarmCommand::New { active, max_executions, interval, name, macro_text } => {
                let steps = convert_command(&macro_text, ctx);
                Action::AlarmNew { name, active, max_executions, interval, macro_text, steps }
            }
            AlarmCommand::ActivateAll => Action::AlarmsEnable { on: true },
            AlarmCommand::DeactivateAll => Action::AlarmsEnable { on: false },
            AlarmCommand::Named { name, action } => {
                let (action, value) = match action {
                    AlarmAction::Activate => ("activate", None),
                    AlarmAction::Deactivate => ("deactivate", None),
                    AlarmAction::Toggle => ("toggle", None),
                    AlarmAction::Delete => ("delete", None),
                    AlarmAction::AddSequence(v) => ("addSequence", Some(v)),
                    AlarmAction::ClearSequence => ("clearSequence", None),
                    AlarmAction::Other(o) => return noop(&format!("unknown alarm command {o}")),
                };
                Action::Alarm { name, action: action.into(), value }
            }
        };
    }
    let Some(kw) = st.scene_keyword() else { return noop("unknown SCENE command") };
    let v = scene_value(st, kw);
    let num = || serde_json::json!(script::atof(v));
    let flag = |s: &str| matches!(s.trim(), "1" | "TRUE" | "ON");
    let set = |prop: &str, value: serde_json::Value| Action::SetScene { prop: prop.into(), value };
    match kw {
        "WORLDINFO" | "LOADFILESFROMITS" if st.rest().contains('?') => query(st),
        "WORLDINFO" => query(st),
        "FORCESOFTWARE" | "TRYHARDWAREFULLSCREEN" | "TRYHARDWARE" | "TRYFULLSCREEN" | "KILLFULLSCREEN" => Action::Device { command: kw.into() },
        "DELETEALL" => Action::DeleteAll,
        "DRAW" => Action::Draw,
        "LOAD" | "RUN" => Action::LoadScript { file: ctx.file(v.trim()).unwrap_or_else(|| v.trim().to_string()) },
        _ if st.rest().ends_with('?') => query(st),
        "FOGCOLOR" => Action::FogColor { rgb: rgb(&st.numbers()) },
        "FOGSTART" => set("fogStart", num()),
        "FOGEND" => set("fogEnd", num()),
        "HALFRESDRIVING" => set("halfResDriving", flag(v).into()),
        "HIGHSPECULAR" => set("highSpecular", flag(v).into()),
        "SPECULAR" => set("specular", flag(v).into()),
        // The 2nd character after the keyword is tested: `=1` turns the overlay on (§3.3).
        "DISPLAYFPS" => set("displayFps", (st.rest().as_bytes().get(kw.len() + 1) == Some(&b'1')).into()),
        "LOADFILESFROMITS" => set("loadFilesFromIts", flag(v).into()),
        "GRAVITY" => set("gravity", num()),
        "ITSFILEURL" => set("itsFileUrl", v.into()),
        "WORKINGPATH" => set("workingPath", v.into()),
        "SHOWCOLLISION" => set("showCollision", flag(v).into()),
        "CURSORS" => set("cursors", flag(v).into()),
        "CURSOR" => {
            let (slot, file) = st.arg().split_once(',').unwrap_or((st.arg(), ""));
            let stored = ctx.file(file.trim());
            Action::Cursor { slot: script::atoi(slot), file: stored.clone().unwrap_or_else(|| file.trim().to_string()), image: stored.map(|s| format!("cursors/{s}.png")) }
        }
        "AMBIENT" => Action::Ambient { rgb: rgb(&st.numbers()) },
        "CLEARSEQUENCE" | "ADDSEQUENCE" => noop("alarm sequence command without an alarm name"),
        _ => noop("unknown SCENE command"),
    }
}

fn camera(st: &Statement) -> Action {
    if st.op == Some(Op::Query) || st.property.ends_with('?') {
        return query(st);
    }
    let prop = match st.property.as_str() {
        "POS" => "pos",
        "MOVETO" => "moveTo",
        "MOVE" => "move",
        "PITCH" => "pitch",
        "FOV" => "fov",
        "HEIGHT" => "height",
        "SPEED.FLY" => "speedFly",
        "SPEED.TURN" => "speedTurn",
        "PITCHSPEED" => "pitchSpeed",
        "COLLISIONRADIUS" => "collisionRadius",
        "CLIMBRATE" => "climbRate",
        "NEAR" => "near",
        "FAR" => "far",
        "BACKGROUNDCOLOR" => "backgroundColor",
        "CLEARBACKGROUND" => "clearBackground",
        c @ ("HOME" | "HOMEPOV" | "HOMEOMNI" | "POV" | "OMNI") => {
            let command = match c {
                "HOME" => "home",
                "HOMEPOV" => "homePov",
                "HOMEOMNI" => "homeOmni",
                "POV" => "pov",
                _ => "omni",
            };
            return Action::CameraCommand { command: command.into() };
        }
        _ => return noop("unknown CAMERA property"),
    };
    let mut values = st.numbers();
    if matches!(prop, "pos" | "moveTo" | "move") && values.len() >= 3 {
        values[2] = -values[2];
    }
    Action::Camera { prop: prop.into(), values }
}

fn walkmap(st: &Statement, ctx: &Ctx) -> Action {
    if st.op == Some(Op::Query) || st.rest().ends_with('?') {
        return query(st);
    }
    match (st.property.as_str(), st.op) {
        ("LOAD", Some(_)) => {
            let file = st.arg().trim().to_string();
            Action::WalkmapLoad { resolved: ctx.file(&file), file }
        }
        ("ON", None) => Action::WalkmapEnable { on: true },
        ("OFF", None) => Action::WalkmapEnable { on: false },
        ("TOGGLE", None) => Action::WalkmapToggle,
        (p, None) if p.starts_with("DELETE.") => Action::WalkmapDelete { what: p["DELETE.".len()..].to_ascii_lowercase() },
        _ => noop("unknown WALKMAP command"),
    }
}

fn lights(st: &Statement, ctx: &Ctx) -> Action {
    if st.op == Some(Op::Query) || st.rest().ends_with('?') {
        return query(st);
    }
    let Some(id) = &st.object else {
        return match (st.property.as_str(), st.op) {
            ("LOAD", Some(_)) => Action::LightsLoad { file: ctx.file(st.arg().trim()).unwrap_or_else(|| st.arg().trim().to_string()) },
            ("ON", None) => Action::LightsEnable { on: true },
            ("OFF", None) => Action::LightsEnable { on: false },
            ("TOGGLE", None) => Action::LightsToggle,
            ("DELETE", None) => Action::LightsDelete,
            _ => noop("unknown LIGHTS command"),
        };
    };
    let target = ctx.light(id);
    let id_text = id_label(id);
    let (prop, value) = match (st.property.as_str(), st.op) {
        ("ON", None) => ("on", true.into()),
        ("OFF", None) => ("on", false.into()),
        ("FLICKER", Some(_)) => ("flicker", script::atof(st.arg()).into()),
        ("COLOR", Some(_)) => ("color", serde_json::json!(rgb(&st.numbers()))),
        ("POS", Some(_)) => {
            let n = st.numbers();
            ("pos", serde_json::json!([n.first().copied().unwrap_or(0.0), n.get(1).copied().unwrap_or(0.0), -n.get(2).copied().unwrap_or(0.0)]))
        }
        ("STATIC", Some(_)) => ("static", (st.bool_value() == Some(true)).into()),
        _ => return noop("unknown LIGHTS property"),
    };
    Action::Light { id: id_text, target, prop: prop.into(), value }
}

fn id_label(id: &ObjectId) -> String {
    match id {
        ObjectId::Name(n) => n.clone(),
        ObjectId::Index(i) => format!("[{i}]"),
    }
}

fn model(st: &Statement, ctx: &Ctx) -> Action {
    if st.op == Some(Op::Query) || st.rest().ends_with('?') {
        return query(st);
    }
    let Some(id) = &st.object else {
        return match (st.property.as_str(), st.op) {
            ("LOAD", Some(_)) => Action::ModelLoad { file: ctx.file(st.arg().trim()).unwrap_or_else(|| st.arg().trim().to_string()) },
            ("ON", None) => Action::ModelEnable { on: true },
            ("OFF", None) => Action::ModelEnable { on: false },
            ("CALC64BIT", Some(_)) => Action::ModelSet { prop: "calc64bit".into(), value: script::atof(st.arg()) },
            ("LODDISTANCE", Some(_)) => Action::ModelSet { prop: "lodDistance".into(), value: script::atof(st.arg()) },
            (c @ ("UNHIDEALL" | "TOGGLEHIDDEN" | "DELETE"), None) => Action::ModelCommand { command: c.to_ascii_lowercase() },
            ("LOADVP" | "APPENDVP" | "PROCESSVP", _) => noop("listed in the help text but not implemented (§7.5)"),
            _ => noop("unknown 3D command"),
        };
    };
    let target = ctx.mesh(id);
    let id_text = id_label(id);
    let flag_prop = match st.property.as_str() {
        "VISIBLE" => Some("visible"),
        "SPRITE" => Some("sprite"),
        "NOILLUMINATION" => Some("noIllumination"),
        "WATER" => Some("water"),
        _ => None,
    };
    if let Some(prop) = flag_prop {
        return match st.flag() {
            Some(value) => Action::ObjectFlag { id: id_text, target, prop: prop.into(), value },
            None => noop("flag value must start with 0 or 1 (§7.4)"),
        };
    }
    match st.property.as_str() {
        "APPLYTEXTURE" => {
            let file = st.arg().trim().to_string();
            let (resolved, image) = ctx.image(&file, None);
            Action::ApplyTexture { id: id_text, target, file, resolved, image }
        }
        "FILMSTRIP" => {
            let mut parts = st.arg().splitn(3, ',');
            let frame_width = script::atoi(parts.next().unwrap_or(""));
            let wait = script::atof(parts.next().unwrap_or(""));
            let file = parts.next().unwrap_or("").trim().to_string();
            let (resolved, image) = if frame_width > 0 { ctx.image(&file, None) } else { (None, None) };
            Action::Filmstrip { id: id_text, target, frame_width, wait, file, resolved, image }
        }
        "CLOUD" | "CLOUDSCALED" => {
            let args = st.args();
            let file = args.first().map_or("", |f| f.trim());
            let scale = if st.property == "CLOUD" { 1.0 } else { args.get(1).map_or(0.0, |v| script::atof(v)) };
            Action::Cloud { id: id_text, target, file: ctx.file(file).unwrap_or_else(|| file.to_string()), scale }
        }
        "CLOUD.OFFSET" => Action::CloudOffset { id: id_text, target, radians: script::atof(st.arg()) },
        p => Action::ObjectOther { id: id_text, target, prop: p.to_ascii_lowercase(), value: st.value.clone() },
    }
}

fn layer(st: &Statement, ctx: &Ctx) -> Action {
    if st.op == Some(Op::Query) || st.rest().ends_with('?') {
        return query(st);
    }
    let Some(id) = &st.object else {
        return match (st.property.as_str(), st.op) {
            ("LOAD", Some(_)) => Action::LayerLoad { file: ctx.file(st.arg().trim()).unwrap_or_else(|| st.arg().trim().to_string()) },
            ("ON" | "SHOW", None) => Action::LayerEnable { on: true },
            ("OFF" | "HIDE", None) => Action::LayerEnable { on: false },
            ("TOGGLE", None) => Action::LayerToggle,
            ("NEW", None) => Action::LayerNew,
            ("ICONFILE", Some(_)) => {
                let file = st.arg().trim().to_string();
                let stored = ctx.file(&file);
                let image = stored.as_ref().map(|s| format!("images/{s}.strip.png"));
                Action::IconFile { file: stored.unwrap_or(file), image }
            }
            ("ICONS", Some(_)) => Action::Icons { count: script::atoi(st.arg()) },
            (p @ ("FONT" | "FONTSIZE" | "CAPTIONMETRICS" | "JVERSION" | "HARDWARE" | "GRID"), Some(_)) => Action::LayerSet { prop: p.to_ascii_lowercase(), value: st.arg().to_string() },
            _ => noop("unknown 2D command"),
        };
    };
    let target = ctx.element(id);
    let id = id_label(id);
    match st.property.as_str() {
        "VISIBLE" if st.arg().trim() == "TOGGLE" => Action::ElementVisible { id, target, value: None },
        "VISIBLE.TOGGLE" => Action::ElementVisible { id, target, value: None },
        "VISIBLE" => match st.bool_value() {
            Some(v) => Action::ElementVisible { id, target, value: Some(v) },
            None => noop("VISIBLE expects TRUE/1/FALSE/0/TOGGLE"),
        },
        "TEXT" => Action::ElementText { id, target, text: st.text_value().to_string() },
        "ANCHORPOINT" => {
            // Split at the first two commas; the third number is read with atof (§8.4).
            let v = st.arg();
            let mut parts = v.splitn(3, ',');
            let x = script::atof(parts.next().unwrap_or(""));
            let y = script::atof(parts.next().unwrap_or(""));
            let rest = parts.next().unwrap_or("");
            let z = script::atof(rest);
            let ignored = rest.find(',').map_or("", |i| &rest[i..]).to_string();
            Action::ElementAnchor { id, target, point: [x, y, -z], ignored }
        }
        "ANCHORED" => match st.bool_value() {
            Some(value) => Action::ElementAnchored { id, target, value },
            None => noop("ANCHORED expects TRUE/1/FALSE/0"),
        },
        "ICON" => Action::ElementIcon { id, target, icon: script::atoi(st.arg()) },
        "NEXTFRAME" => Action::ElementNextFrame { id, target, frames: script::atoi(st.arg()) },
        "SKY" => match st.bool_value() {
            Some(value) => Action::ElementSky { id, target, value },
            None => noop("SKY expects TRUE/1/FALSE/0"),
        },
        p if p.starts_with("MACRO.") => {
            let event = match &p["MACRO.".len()..] {
                "MOUSEDOWN" => "mouseDown",
                "MOUSEUP" => "mouseUp",
                "MOUSEIN" => "mouseIn",
                "MOUSEOUT" => "mouseOut",
                _ => return noop("unknown 2D macro event"),
            };
            let script = st.value.clone();
            let steps = convert_command(&script, ctx);
            Action::ElementMacro { id, target, event: event.into(), script, steps }
        }
        p => Action::ElementOther { id, target, prop: p.to_ascii_lowercase(), value: st.value.clone() },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use encarta_formats::tours::script::parse_statement;

    #[test]
    fn raw_pieces_match_the_interpreter_split() {
        assert_eq!(raw_pieces("a;;b;"), ["a", "b"]);
        assert_eq!(scene_value(&parse_statement("SCENE.DRAW)"), "DRAW"), "");
        assert_eq!(scene_value(&parse_statement("scene.load(geo.3sc)"), "LOAD"), "GEO.3SC");
    }
}
