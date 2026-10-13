//! Runs a tour's `load.3sc` the way the control does (§3.1–3.9) and records the state it leaves:
//! camera constants, scene switches, per-object 3D properties, light switches and flicker,
//! the 2D layers with their per-element state, macros, alarms and events.
//!
//! Nested `SCENE.LOAD`s and `RUN(…)` macros are expanded in place, so the recorded sequence holds
//! only leaf statements, in execution order, each with its origin (`file:line`, plus `> MACRO` for
//! statements that ran from a macro body).

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};

use encarta_formats::tours::layer2d::{Element, Layer};
use encarta_formats::tours::script::{self, Command, MacroDef};
use encarta_formats::tours::tour::Tour;
use serde::Serialize;

use super::actions::{self, Action, Ctx, ImageRequest, Step};

#[derive(Debug, Clone, Serialize)]
pub struct SeqEntry {
    pub origin: String,
    #[serde(flatten)]
    pub step: Step,
}

/// Per-object (mesh) properties set by statements.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ObjectState {
    pub visible: bool,
    pub sprite: bool,
    pub no_illumination: bool,
    pub water: bool,
    /// `APPLYTEXTURE` (or the texture a `FILMSTRIP` applied): (stored file, output image path).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub applied_texture: Option<(String, String)>,
    /// `FILMSTRIP`: (frame width, wait seconds).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filmstrip: Option<(i64, f64)>,
    /// `CLOUD`/`CLOUDSCALED`: (stored file, scale).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cloud: Option<(String, f64)>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cloud_offset: Option<f64>,
    /// Other properties, raw (`PLAYAVI`, `STRING`).
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub other: BTreeMap<String, String>,
}

impl Default for ObjectState {
    fn default() -> Self {
        ObjectState { visible: true, sprite: false, no_illumination: false, water: false, applied_texture: None, filmstrip: None, cloud: None, cloud_offset: None, other: BTreeMap::new() }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LightState {
    pub on: bool,
    /// Current base colour (r, g, b).
    pub colour: [u8; 3],
    /// Flicker period in seconds, when flickering (the light is then dynamic).
    pub flicker: Option<f64>,
    /// Colour saved when flicker was switched on (the flicker scales this).
    pub saved: [u8; 3],
    /// `STATIC=` flag (1 = static); a flickering light is dynamic regardless.
    pub static_flag: bool,
    /// Moved by `POS=` (glTF space); the original then recomputes the face lists (never on disc).
    pub moved_to: Option<[f64; 3]>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ElementState {
    pub visible: bool,
    pub text: String,
    /// Anchor point, glTF space.
    pub anchor: [f64; 3],
    pub anchored: bool,
    pub icon: i64,
    /// Texture coordinates of the quad (u0, v0, u1, v1); `NEXTFRAME` moves v.
    pub uv: [f32; 4],
    pub sky: bool,
    /// Macro strings by event (`mouseDown` …) as stored, and their conversion.
    pub macros: BTreeMap<String, (String, Vec<Step>)>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub other: BTreeMap<String, String>,
}

impl ElementState {
    fn from_file(e: &Element, ctx: &Ctx) -> ElementState {
        let mut macros = BTreeMap::new();
        for (ev, m) in [("mouseDown", &e.macro_mouse_down), ("mouseUp", &e.macro_mouse_up), ("mouseIn", &e.macro_mouse_in), ("mouseOut", &e.macro_mouse_out)] {
            if !m.is_empty() {
                macros.insert(ev.to_string(), (m.clone(), actions::convert_command(m, ctx)));
            }
        }
        let v = &e.vertices;
        ElementState {
            visible: e.visible,
            text: e.text.clone(),
            anchor: [e.anchor[0] as f64, e.anchor[1] as f64, -(e.anchor[2] as f64)],
            anchored: e.anchored,
            icon: e.icon_normal as i64,
            uv: [v[0].u, v[0].v, v[2].u, v[2].v],
            sky: false,
            macros,
            other: BTreeMap::new(),
        }
    }

    /// `NEXTFRAME(n)`: the element's image is n frames stacked vertically (§8.4.3).
    pub fn next_frame(&mut self, n: i64) {
        if n <= 0 {
            return;
        }
        let f = 1.0 / n as f32;
        let (vt, vb) = (self.uv[1], self.uv[3]);
        let mut k = (vt / f).trunc() as i64 + 1;
        if ((vb - vt) - f).abs() >= 0.001 {
            k = 0;
        }
        if k >= n {
            k = 0;
        }
        self.uv[1] = k as f32 * f;
        self.uv[3] = if k == n - 1 { 1.0 } else { (k + 1) as f32 * f };
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LayerState {
    pub file: String,
    pub on: bool,
    /// Element states in array order (draw order), by element name.
    pub elements: Vec<(String, ElementState)>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MacroState {
    pub def: MacroDef,
    pub origin: String,
    pub run_during_load: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AlarmState {
    pub name: String,
    pub active: bool,
    pub max_executions: i64,
    pub interval: f64,
    pub macro_text: String,
    pub steps: Vec<Step>,
    pub origin: String,
    /// False once `ALARM.<name>.DELETE` ran.
    pub live: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct FiredEvent {
    pub origin: String,
    pub text: String,
    pub name: String,
    pub pairs: Vec<[String; 2]>,
}

/// Everything `load.3sc` leaves behind.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoadState {
    /// Last value of each `CAMERA.<prop>` (glTF space for `pos`).
    pub camera: BTreeMap<String, Vec<f64>>,
    /// Every `CAMERA.NEAR` value in order (the near plane changes during loading in two tours).
    pub near_history: Vec<f64>,
    /// `SetScene` values by prop.
    pub scene: BTreeMap<String, serde_json::Value>,
    /// `SCENE.AMBIENT` applied after the model was loaded.
    pub ambient: Option<[i64; 3]>,
    pub fog_colour: Option<[i64; 3]>,
    pub cursor_slots: BTreeMap<i64, (String, Option<String>)>,
    pub model_file: Option<String>,
    pub model_on: bool,
    pub model_settings: BTreeMap<String, f64>,
    /// Object states by mesh index (only objects some statement touched).
    pub objects: BTreeMap<usize, ObjectState>,
    pub lights_file: Option<String>,
    pub lights_on: bool,
    pub lights: Vec<LightState>,
    pub walkmaps_loaded: Vec<String>,
    pub walkmap_active: Option<String>,
    pub walkmap_on: bool,
    pub layers: Vec<LayerState>,
    pub icon_file: Option<(String, Option<String>)>,
    pub icon_count: Option<i64>,
    pub layer_settings: BTreeMap<String, String>,
    pub macros: BTreeMap<String, MacroState>,
    pub alarms: Vec<AlarmState>,
    pub alarms_enabled: bool,
    pub events: Vec<FiredEvent>,
    pub timer_on: bool,
}

impl LoadState {
    fn reset() -> LoadState {
        LoadState { model_on: true, lights_on: true, timer_on: true, ..Default::default() }
    }

    pub fn current_layer(&self) -> Option<&LayerState> {
        self.layers.last()
    }
}

pub struct Simulation<'a> {
    tour: &'a Tour,
    images: &'a RefCell<BTreeSet<ImageRequest>>,
    pub sequence: Vec<SeqEntry>,
    pub state: LoadState,
    /// Leaf statements executed (comparable with `Tour::executed`).
    pub statements: usize,
    depth: usize,
}

impl<'a> Simulation<'a> {
    pub fn run(tour: &'a Tour, images: &'a RefCell<BTreeSet<ImageRequest>>) -> Simulation<'a> {
        let mut s = Simulation { tour, images, sequence: Vec::new(), state: LoadState::reset(), statements: 0, depth: 0 };
        s.run_script(encarta_formats::tours::tour::ENTRY_SCRIPT, "host");
        s
    }

    fn layer_data(&self) -> Option<&'a Layer> {
        let file = &self.state.current_layer()?.file;
        self.tour.layers.iter().find(|l| &l.file == file).map(|l| &l.data)
    }

    fn ctx(&self) -> Ctx<'a> {
        Ctx { tour: self.tour, layer: self.layer_data(), images: self.images }
    }

    fn push(&mut self, origin: &str, src: &str, action: Action) {
        self.sequence.push(SeqEntry { origin: origin.to_string(), step: Step { src: src.to_string(), action } });
    }

    fn run_script(&mut self, name: &str, origin: &str) {
        let tour = self.tour;
        let Some(stored) = tour.container.find(name) else {
            self.push(origin, name, Action::Noop { reason: format!("script {name} is not in the container") });
            return;
        };
        let Some(script) = tour.script(stored) else { return };
        if self.depth > 16 {
            return;
        }
        self.depth += 1;
        for c in &script.commands {
            let origin = format!("{stored}:{}", c.source.line);
            self.run_command(&c.source.text, &c.command, &origin);
        }
        self.depth -= 1;
    }

    fn run_command(&mut self, text: &str, cmd: &Command, origin: &str) {
        match cmd {
            Command::Statements(list) => {
                let pieces: Vec<&str> = text.split(';').filter(|p| !p.is_empty()).collect();
                for (raw, st) in pieces.into_iter().zip(list) {
                    self.statements += 1;
                    let action = actions::convert_statement(st, &self.ctx());
                    if let Action::LoadScript { file } = &action {
                        let file = file.clone();
                        self.run_script(&file, origin);
                        continue;
                    }
                    self.apply(&action, origin);
                    self.push(origin, raw, action);
                }
            }
            Command::Define(d) => {
                self.state.macros.insert(d.name.clone(), MacroState { def: d.clone(), origin: origin.to_string(), run_during_load: false });
                self.push(origin, text, Action::DefineMacro { name: d.name.clone() });
            }
            Command::Delete(name) => {
                self.state.macros.remove(name);
                self.push(origin, text, Action::DeleteMacro { name: name.clone() });
            }
            Command::Run { names, ignored } => {
                for n in names {
                    let Some(m) = self.state.macros.get_mut(n) else {
                        self.push(origin, &format!("RUN({n})"), Action::Noop { reason: format!("macro {n} is not defined") });
                        continue;
                    };
                    m.run_during_load = true;
                    let body = m.def.body.clone();
                    if self.depth > 16 {
                        continue;
                    }
                    self.depth += 1;
                    self.run_command(&body, &script::parse_command(&body), &format!("{origin} > {n}"));
                    self.depth -= 1;
                }
                for piece in ignored {
                    self.push(origin, piece, Action::Noop { reason: "not a RUN(…) piece of a RUN command (§3.4)".into() });
                }
            }
            Command::Help | Command::MacroLog(_) => self.push(origin, text, Action::Noop { reason: "help / macro log".into() }),
        }
    }

    fn mesh_index(&self, name: &str) -> Option<usize> {
        self.tour.model.as_ref()?.data.meshes.iter().position(|m| m.name == name)
    }

    fn object(&mut self, target: &Option<String>) -> Option<&mut ObjectState> {
        let i = self.mesh_index(target.as_deref()?)?;
        Some(self.state.objects.entry(i).or_default())
    }

    fn element(&mut self, target: &Option<String>) -> Option<&mut ElementState> {
        let name = target.as_deref()?;
        let layer = self.state.layers.last_mut()?;
        layer.elements.iter_mut().find(|(n, _)| n == name).map(|(_, e)| e)
    }

    fn apply(&mut self, a: &Action, origin: &str) {
        let tour = self.tour;
        match a {
            Action::DeleteAll => self.state = LoadState::reset(),
            Action::Event { text, name, pairs } => self.state.events.push(FiredEvent { origin: origin.to_string(), text: text.clone(), name: name.clone(), pairs: pairs.clone() }),
            Action::SetScene { prop, value } => {
                self.state.scene.insert(prop.clone(), value.clone());
            }
            Action::Ambient { rgb } => {
                if self.state.model_file.is_some() {
                    self.state.ambient = Some(*rgb);
                }
            }
            Action::FogColor { rgb } => self.state.fog_colour = Some(*rgb),
            Action::Cursor { slot, file, image } => {
                self.state.cursor_slots.insert(*slot, (file.clone(), image.clone()));
            }
            Action::AlarmNew { name, active, max_executions, interval, macro_text, steps } => {
                self.state.alarms.retain(|x| &x.name != name || !x.live);
                self.state.alarms.push(AlarmState {
                    name: name.clone(),
                    active: *active,
                    max_executions: *max_executions,
                    interval: *interval,
                    macro_text: macro_text.clone(),
                    steps: steps.clone(),
                    origin: origin.to_string(),
                    live: true,
                });
            }
            Action::AlarmsEnable { on } => self.state.alarms_enabled = *on,
            Action::Alarm { name, action, .. } => {
                if let Some(x) = self.state.alarms.iter_mut().rev().find(|x| &x.name == name && x.live) {
                    match action.as_str() {
                        "delete" => x.live = false,
                        "activate" => x.active = true,
                        "deactivate" => x.active = false,
                        "toggle" => x.active = !x.active,
                        _ => {}
                    }
                }
            }
            Action::Camera { prop, values } => {
                if prop == "near" {
                    self.state.near_history.push(values.first().copied().unwrap_or(0.0));
                }
                self.state.camera.insert(prop.clone(), values.clone());
            }
            Action::WalkmapLoad { resolved: Some(f), .. } => {
                if !self.state.walkmaps_loaded.contains(f) {
                    self.state.walkmaps_loaded.push(f.clone());
                }
                self.state.walkmap_active = Some(f.clone());
            }
            Action::WalkmapEnable { on } => self.state.walkmap_on = *on,
            Action::LightsLoad { file } => {
                if let Some(l) = tour.lights.as_ref().filter(|l| &l.file == file) {
                    self.state.lights_file = Some(file.clone());
                    self.state.lights = l.data.lights.iter().map(|l| LightState { on: true, colour: l.rgb(), flicker: None, saved: l.rgb(), static_flag: true, moved_to: None }).collect();
                }
            }
            Action::LightsEnable { on } => self.state.lights_on = *on,
            Action::LightsDelete => self.state.lights.clear(),
            Action::Light { target: Some(name), prop, value, .. } => {
                let Some(l) = tour.lights.as_ref() else { return };
                let Some(i) = l.data.lights.iter().position(|x| &x.name == name) else { return };
                let Some(ls) = self.state.lights.get_mut(i) else { return };
                match prop.as_str() {
                    "on" => ls.on = value.as_bool().unwrap_or(true),
                    "flicker" => {
                        let p = value.as_f64().unwrap_or(0.0);
                        if p > 1e-6 {
                            if ls.flicker.is_none() {
                                ls.saved = ls.colour;
                            }
                            ls.flicker = Some(p);
                        } else if ls.flicker.take().is_some() {
                            ls.colour = ls.saved;
                        }
                    }
                    "color" => {
                        if let Some(c) = value.as_array() {
                            let g = |k: usize| c.get(k).and_then(|v| v.as_i64()).unwrap_or(0) as u8;
                            ls.colour = [g(0), g(1), g(2)];
                        }
                    }
                    "pos" => {
                        if let Some(c) = value.as_array() {
                            let g = |k: usize| c.get(k).and_then(|v| v.as_f64()).unwrap_or(0.0);
                            ls.moved_to = Some([g(0), g(1), g(2)]);
                        }
                    }
                    "static" => ls.static_flag = value.as_bool().unwrap_or(true),
                    _ => {}
                }
            }
            Action::ModelLoad { file } => {
                if tour.model.as_ref().is_some_and(|m| &m.file == file) {
                    self.state.model_file = Some(file.clone());
                    self.state.objects.clear();
                }
            }
            Action::ModelEnable { on } => self.state.model_on = *on,
            Action::ModelSet { prop, value } => {
                self.state.model_settings.insert(prop.clone(), *value);
            }
            Action::ModelCommand { command } => match command.as_str() {
                "unhideall" | "togglehidden" => {
                    let n = tour.model.as_ref().map_or(0, |m| m.data.meshes.len());
                    for i in 0..n {
                        let o = self.state.objects.entry(i).or_default();
                        o.visible = command == "unhideall" || !o.visible;
                    }
                }
                _ => self.state.objects.clear(),
            },
            Action::ObjectFlag { target, prop, value, .. } => {
                if let Some(o) = self.object(target) {
                    match prop.as_str() {
                        "visible" => o.visible = *value,
                        "sprite" => o.sprite = *value,
                        "noIllumination" => o.no_illumination = *value,
                        _ => o.water = *value,
                    }
                }
            }
            Action::ApplyTexture { target, resolved: Some(f), image: Some(p), .. } => {
                if let Some(o) = self.object(target) {
                    o.applied_texture = Some((f.clone(), p.clone()));
                }
            }
            Action::Filmstrip { target, frame_width, wait, resolved, image, .. } => {
                if let Some(o) = self.object(target) {
                    if *frame_width > 0 {
                        if let (Some(f), Some(p)) = (resolved, image) {
                            o.applied_texture = Some((f.clone(), p.clone()));
                        }
                        o.filmstrip = Some((*frame_width, *wait));
                    } else {
                        o.filmstrip = None;
                    }
                }
            }
            Action::Cloud { target, file, scale, .. } => {
                if let Some(o) = self.object(target) {
                    o.cloud = Some((file.clone(), *scale));
                }
            }
            Action::CloudOffset { target, radians, .. } => {
                if let Some(o) = self.object(target)
                    && o.cloud.is_some()
                {
                    o.cloud_offset = Some(*radians);
                }
            }
            Action::ObjectOther { target, prop, value, .. } => {
                if let Some(o) = self.object(target) {
                    o.other.insert(prop.clone(), value.clone());
                }
            }
            Action::LayerLoad { file } => {
                if let Some(l) = tour.layers.iter().find(|l| &l.file == file) {
                    let ctx = Ctx { tour, layer: Some(&l.data), images: self.images };
                    let on = l.data.layer_on != 0;
                    let elements = l.data.elements.iter().map(|e| (e.name.clone(), ElementState::from_file(e, &ctx))).collect();
                    self.state.layers.push(LayerState { file: file.clone(), on, elements });
                }
            }
            Action::LayerEnable { on } => {
                if let Some(l) = self.state.layers.last_mut() {
                    l.on = *on;
                }
            }
            Action::LayerNew => {
                if let Some(l) = self.state.layers.last_mut() {
                    l.elements.clear();
                }
            }
            Action::IconFile { file, image } => self.state.icon_file = Some((file.clone(), image.clone())),
            Action::Icons { count } => self.state.icon_count = Some(*count),
            Action::LayerSet { prop, value } => {
                self.state.layer_settings.insert(prop.clone(), value.clone());
            }
            Action::ElementVisible { target, value, .. } => {
                if let Some(e) = self.element(target) {
                    e.visible = value.unwrap_or(!e.visible);
                }
            }
            Action::ElementText { target, text, .. } => {
                if let Some(e) = self.element(target) {
                    e.text = text.clone();
                }
            }
            Action::ElementAnchor { target, point, .. } => {
                if let Some(e) = self.element(target) {
                    e.anchor = *point;
                }
            }
            Action::ElementAnchored { target, value, .. } => {
                if let Some(e) = self.element(target) {
                    e.anchored = *value;
                }
            }
            Action::ElementIcon { target, icon, .. } => {
                if let Some(e) = self.element(target) {
                    e.icon = *icon;
                }
            }
            Action::ElementMacro { target, event, script, steps, .. } => {
                if let Some(e) = self.element(target) {
                    e.macros.insert(event.clone(), (script.clone(), steps.clone()));
                }
            }
            Action::ElementNextFrame { target, frames, .. } => {
                if let Some(e) = self.element(target) {
                    e.next_frame(*frames);
                }
            }
            Action::ElementSky { target, value, .. } => {
                if let Some(e) = self.element(target) {
                    e.sky = *value;
                }
            }
            Action::ElementOther { target, prop, value, .. } => {
                if let Some(e) = self.element(target) {
                    e.other.insert(prop.clone(), value.clone());
                }
            }
            Action::Timer { on } => self.state.timer_on = *on,
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(uv: [f32; 4]) -> ElementState {
        ElementState { visible: true, text: String::new(), anchor: [0.0; 3], anchored: false, icon: -1, uv, sky: false, macros: BTreeMap::new(), other: BTreeMap::new() }
    }

    #[test]
    fn next_frame_cycles_a_vertical_strip() {
        let mut e = state([0.0, 0.0, 1.0, 1.0]);
        e.next_frame(4); // whole image shown: jumps to frame 0
        assert_eq!((e.uv[1], e.uv[3]), (0.0, 0.25));
        e.next_frame(4);
        assert_eq!((e.uv[1], e.uv[3]), (0.25, 0.5));
        e.next_frame(4);
        e.next_frame(4);
        assert_eq!((e.uv[1], e.uv[3]), (0.75, 1.0));
        e.next_frame(4);
        assert_eq!((e.uv[1], e.uv[3]), (0.0, 0.25));
    }
}
