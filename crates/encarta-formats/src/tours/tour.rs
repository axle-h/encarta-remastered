//! A whole tour: the container directory, everything its entry script (`load.3sc`) loads, and a
//! cross-reference check.
//!
//! Loading follows the scripts in execution order (spec §3.9): nested `SCENE.LOAD`s run in place,
//! `RUN(…)` runs macro bodies, and each referenced file is parsed the first time it is named.
//! Statements are checked against the state at the time they run (e.g. `2D._0` against
//! `begin.32d`). Strings that only run later (unrun macros, alarm macros, walk-map hotspot
//! strings, 2D element macros) are checked against the final state.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::Serialize;

use super::cloud::{self, Cloud};
use super::layer2d::{self, Layer};
use super::lights::{self, LightFile};
use super::script::{self, AlarmCommand, Command, Event, FileRef, ObjectId, Script, Statement, Subsystem};
use super::textures::{self, ImageInfo, Resolution};
use super::viewpoints::{self, ViewPoints};
use super::walkmap::{self, WalkMap};
use super::xfile::{self, MaterialRef, XFile};

/// The entry script the host runs.
pub const ENTRY_SCRIPT: &str = "load.3sc";

/// File classes by extension (spec §5.7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
pub enum FileClass {
    Model,
    Script,
    WalkMap,
    Lights,
    Layer2d,
    Cloud,
    ViewPoints,
    Image,
    Cursor,
    /// Authoring leftovers: `error.log`, `Thumbs.db`, `.psd`.
    Junk,
    Unknown,
}

pub fn classify(name: &str) -> FileClass {
    let ext = name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default();
    match ext.as_str() {
        "x" => FileClass::Model,
        "3sc" => FileClass::Script,
        "3wm" => FileClass::WalkMap,
        "3lt" => FileClass::Lights,
        "32d" => FileClass::Layer2d,
        "3cl" => FileClass::Cloud,
        "3vp" => FileClass::ViewPoints,
        "bmp" | "jpg" | "jpeg" | "gif" | "png" => FileClass::Image,
        "cur" => FileClass::Cursor,
        "log" | "db" | "psd" => FileClass::Junk,
        _ => FileClass::Unknown,
    }
}

/// The files of one tour container, with case-insensitive lookup.
#[derive(Debug, Clone, Serialize)]
pub struct Container {
    pub dir: PathBuf,
    /// Stored file names, sorted.
    pub files: Vec<String>,
    #[serde(skip)]
    by_lower: HashMap<String, usize>,
}

impl Container {
    pub fn open(dir: &Path) -> Result<Container> {
        let mut files = Vec::new();
        for e in std::fs::read_dir(dir).with_context(|| format!("reading {}", dir.display()))? {
            let e = e?;
            if e.file_type()?.is_file() {
                files.push(e.file_name().to_string_lossy().into_owned());
            }
        }
        files.sort();
        let by_lower = files.iter().enumerate().map(|(i, f)| (f.to_lowercase(), i)).collect();
        Ok(Container { dir: dir.to_path_buf(), files, by_lower })
    }

    /// Stored name for a case-insensitive basename lookup.
    pub fn find(&self, name: &str) -> Option<&str> {
        self.by_lower.get(&textures::basename(name).to_lowercase()).map(|&i| self.files[i].as_str())
    }

    pub fn read(&self, name: &str) -> Result<Vec<u8>> {
        let stored = self.find(name).with_context(|| format!("{name} not in {}", self.dir.display()))?;
        std::fs::read(self.dir.join(stored)).with_context(|| format!("reading {stored}"))
    }

    pub fn of_class(&self, class: FileClass) -> impl Iterator<Item = &str> {
        self.files.iter().map(String::as_str).filter(move |f| classify(f) == class)
    }
}

/// A statement as it ran (or would run) during loading, with where it came from.
#[derive(Debug, Clone, Serialize)]
pub struct Executed {
    pub origin: String,
    pub statement: Statement,
}

#[derive(Debug, Clone, Serialize)]
pub struct Named<T> {
    pub file: String,
    pub data: T,
}

/// A distinct (case-sensitive) texture name of the model's materials with how it resolved.
#[derive(Debug, Clone, Serialize)]
pub struct TextureRef {
    pub name: String,
    pub resolution: Resolution,
    pub file: Option<String>,
    /// False when only materials no mesh references carry this name (the engine never loads it).
    pub used: bool,
}

/// A point cloud attached to a placeholder mesh.
#[derive(Debug, Clone, Serialize)]
pub struct CloudPlacement {
    pub mesh: String,
    pub file: String,
    /// Texels per cloud unit (`CLOUD=` means 1.0; 0 is treated as 0.5 by the runtime).
    pub scale: f64,
    /// `CLOUD.OFFSET` yaw offset in radians, if set.
    pub offset: Option<f64>,
}

/// One cross-reference problem.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub enum Issue {
    /// A file named by a statement or a `.32d` element is not in the container.
    MissingFile { kind: String, name: String, origin: String },
    /// A model texture that resolves to no file (after the `jpg`/`gif` fallbacks).
    MissingTexture { name: String, used: bool },
    /// `3D.<id>` names no mesh (case-insensitive).
    UnknownMesh { id: String, origin: String },
    /// `LIGHTS.<id>` names no light (names upper-cased and compared exactly).
    UnknownLight { id: String, origin: String },
    /// `2D.<id>` names no element of the layer loaded at the time.
    UnknownElement { id: String, origin: String },
    /// `RUN(name)` of an undefined macro.
    UnknownMacro { name: String, origin: String },
    /// `SCENE.ALARM.<name>.…` for an alarm that does not exist at that time.
    UnknownAlarm { name: String, origin: String },
    /// A statement whose subsystem no handler accepts (it is ignored).
    UnknownSubsystem { statement: String, origin: String },
    /// A statement referring to an object kind before any file of that kind was loaded.
    NothingLoaded { what: String, origin: String },
    /// A material reference of a mesh that matches no top-level Material.
    UnresolvedMaterial { mesh: String, name: String },
    /// A light list entry outside the model (object, face or vertex slot).
    BadLightEntry { light: String, entry: usize },
    /// A vertex-paint record outside the model.
    BadPaint { index: usize, object: String },
    /// A visibility mask with bits past the mesh's face count.
    VisibilityBitsPastFaces { walkmap: String, cell: usize, mesh: u16 },
    /// A file that failed to parse.
    ParseFailure { file: String, error: String },
}

impl Issue {
    /// Missing files and textures: references that point at nothing on the disc.
    pub fn is_dangling_file(&self) -> bool {
        matches!(self, Issue::MissingFile { .. } | Issue::MissingTexture { .. })
    }

    /// Silent no-ops of the original (unknown object ids or macros).
    pub fn is_silent_failure(&self) -> bool {
        matches!(
            self,
            Issue::UnknownMesh { .. }
                | Issue::UnknownLight { .. }
                | Issue::UnknownElement { .. }
                | Issue::UnknownMacro { .. }
                | Issue::UnknownAlarm { .. }
                | Issue::UnknownSubsystem { .. }
        )
    }

    /// Anything that indicates a decoding or data error rather than a known quirk.
    pub fn is_structural(&self) -> bool {
        !self.is_dangling_file() && !self.is_silent_failure()
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Tour {
    pub container: Container,
    /// Scripts in the order they were first loaded.
    pub scripts: Vec<Named<Script>>,
    pub model: Option<Named<XFile>>,
    pub lights: Option<Named<LightFile>>,
    /// Walk maps in first-load order.
    pub walkmaps: Vec<Named<WalkMap>>,
    /// The walk map active at the end of loading (the starting map).
    pub start_walkmap: Option<String>,
    /// 2D layers in load order; the last one is the layer in effect after loading.
    pub layers: Vec<Named<Layer>>,
    pub clouds: Vec<Named<Cloud>>,
    pub cloud_placements: Vec<CloudPlacement>,
    /// Every `.3vp` in the container (no script loads them).
    pub viewpoints: Vec<Named<ViewPoints>>,
    pub textures: Vec<TextureRef>,
    /// Every image in the container, sniffed.
    pub images: BTreeMap<String, ImageInfo>,
    /// Every cursor in the container: image sizes.
    pub cursors: BTreeMap<String, Vec<textures::CursorImage>>,
    /// Cursor slots set by `SCENE.CURSOR=index,file`.
    pub cursor_slots: BTreeMap<u32, String>,
    /// Macros defined, by upper-cased name.
    pub macros: BTreeMap<String, script::MacroDef>,
    /// Names of macros that `RUN` executed during loading.
    pub macros_run: BTreeSet<String>,
    /// Alarms created during loading (name → macro text).
    pub alarms: Vec<(String, String)>,
    /// Alarms still defined at the end of loading.
    pub live_alarms: BTreeSet<String>,
    /// Events fired during loading (e.g. `WORLDLOADED`).
    pub events: Vec<Event>,
    /// Statements in execution order during loading.
    pub executed: Vec<Executed>,
    /// Statements that run only later: unrun macros, alarm macros, hotspot strings, 2D macros.
    pub deferred: Vec<Executed>,
    /// File → what referenced it first.
    pub referenced: BTreeMap<String, String>,
    pub issues: Vec<Issue>,
}

struct Loader {
    tour: Tour,
    depth: usize,
}

impl Tour {
    /// Loads a tour container directory (`…/7015/<id>.EIT/`).
    pub fn load(dir: &Path) -> Result<Tour> {
        let container = Container::open(dir)?;
        if container.find(ENTRY_SCRIPT).is_none() {
            bail!("{} has no {ENTRY_SCRIPT}", dir.display());
        }
        let tour = Tour {
            container,
            scripts: Vec::new(),
            model: None,
            lights: None,
            walkmaps: Vec::new(),
            start_walkmap: None,
            layers: Vec::new(),
            clouds: Vec::new(),
            cloud_placements: Vec::new(),
            viewpoints: Vec::new(),
            textures: Vec::new(),
            images: BTreeMap::new(),
            cursors: BTreeMap::new(),
            cursor_slots: BTreeMap::new(),
            macros: BTreeMap::new(),
            macros_run: BTreeSet::new(),
            alarms: Vec::new(),
            live_alarms: BTreeSet::new(),
            events: Vec::new(),
            executed: Vec::new(),
            deferred: Vec::new(),
            referenced: BTreeMap::new(),
            issues: Vec::new(),
        };
        let mut l = Loader { tour, depth: 0 };
        l.run_script(ENTRY_SCRIPT, "host")?;
        l.finish()?;
        Ok(l.tour)
    }

    pub fn script(&self, name: &str) -> Option<&Script> {
        self.scripts.iter().find(|s| s.file.eq_ignore_ascii_case(name)).map(|s| &s.data)
    }

    /// The 2D layer in effect after loading (`lh.32d`).
    pub fn final_layer(&self) -> Option<&Named<Layer>> {
        self.layers.last()
    }

    /// Every statement of every loaded script, including all macro bodies (run or not), with the
    /// script name.
    pub fn script_statements(&self) -> Vec<(&str, Statement)> {
        self.scripts.iter().flat_map(|s| s.data.all_statements().into_iter().map(move |(_, _, st)| (s.file.as_str(), st))).collect()
    }

    /// Statement shapes (`2D.<id>.VISIBLE=` …) over all script statements, with counts.
    pub fn statement_inventory(&self) -> BTreeMap<String, usize> {
        let mut m = BTreeMap::new();
        for (_, s) in self.script_statements() {
            *m.entry(s.shape()).or_default() += 1;
        }
        m
    }

    /// Container files no loaded data refers to (directly or through the texture fallbacks).
    pub fn unreferenced_files(&self) -> Vec<&str> {
        self.container.files.iter().map(String::as_str).filter(|f| !self.referenced.contains_key(*f)).collect()
    }

    pub fn summary(&self) -> Summary {
        let mut by_class: BTreeMap<FileClass, usize> = BTreeMap::new();
        for f in &self.container.files {
            *by_class.entry(classify(f)).or_default() += 1;
        }
        let stmts = self.script_statements();
        Summary {
            files: self.container.files.len(),
            files_by_class: by_class,
            scripts: self.scripts.len(),
            script_commands: self.scripts.iter().map(|s| s.data.commands.len()).sum(),
            script_statements: stmts.len(),
            macro_definitions: self.scripts.iter().map(|s| s.data.macro_definitions().count()).sum(),
            run_commands: self.scripts.iter().flat_map(|s| &s.data.commands).filter(|c| matches!(c.command, Command::Run { .. })).count(),
            hotspot_macros: stmts.iter().filter(|(_, s)| is_hotspot_click(s)).count(),
            meshes: self.model.as_ref().map_or(0, |m| m.data.meshes.len()),
            faces: self.model.as_ref().map_or(0, |m| m.data.meshes.iter().map(|m| m.faces.len()).sum()),
            materials: self.model.as_ref().map_or(0, |m| m.data.materials.len()),
            textures: self.textures.len(),
            lights: self.lights.as_ref().map_or(0, |l| l.data.lights.len()),
            light_entries: self.lights.as_ref().map_or(0, |l| l.data.entry_count()),
            paint_records: self.lights.as_ref().map_or(0, |l| l.data.paint.len()),
            walkmaps: self.walkmaps.len(),
            walkmap_cells: self.walkmaps.iter().map(|w| w.data.cells.len()).sum(),
            layer_elements: self.final_layer().map_or(0, |l| l.data.elements.len()),
            clouds: self.clouds.len(),
            cloud_placements: self.cloud_placements.len(),
            viewpoint_files: self.viewpoints.len(),
            images: self.images.len(),
            colour_keyed_bmps: self.images.values().filter(|i| i.colour_keyed).count(),
            cursors: self.cursors.len(),
            issues: self.issues.len(),
            dangling_files: self.issues.iter().filter(|i| i.is_dangling_file()).count(),
            silent_failures: self.issues.iter().filter(|i| i.is_silent_failure()).count(),
            structural_issues: self.issues.iter().filter(|i| i.is_structural()).count(),
            unreferenced_files: self.unreferenced_files().len(),
        }
    }
}

/// `2D.<id>.MACRO.MOUSEUP=SCRIPT.EVENT(HotSpotClicked:…)`.
pub fn is_hotspot_click(s: &Statement) -> bool {
    s.is_object_property(Subsystem::Layer2d, "MACRO.MOUSEUP") && s.value.contains("HOTSPOTCLICKED")
}

/// Counts for `tour info`.
#[derive(Debug, Clone, Serialize)]
pub struct Summary {
    pub files: usize,
    pub files_by_class: BTreeMap<FileClass, usize>,
    pub scripts: usize,
    pub script_commands: usize,
    pub script_statements: usize,
    pub macro_definitions: usize,
    pub run_commands: usize,
    pub hotspot_macros: usize,
    pub meshes: usize,
    pub faces: usize,
    pub materials: usize,
    pub textures: usize,
    pub lights: usize,
    pub light_entries: usize,
    pub paint_records: usize,
    pub walkmaps: usize,
    pub walkmap_cells: usize,
    pub layer_elements: usize,
    pub clouds: usize,
    pub cloud_placements: usize,
    pub viewpoint_files: usize,
    pub images: usize,
    pub colour_keyed_bmps: usize,
    pub cursors: usize,
    pub issues: usize,
    pub dangling_files: usize,
    pub silent_failures: usize,
    pub structural_issues: usize,
    pub unreferenced_files: usize,
}

impl Loader {
    fn issue(&mut self, i: Issue) {
        self.tour.issues.push(i);
    }

    /// Records a reference; returns the stored name, or `None` (and an issue) when absent.
    fn reference(&mut self, kind: FileRef, name: &str, origin: &str) -> Option<String> {
        match self.tour.container.find(name) {
            Some(stored) => {
                let stored = stored.to_string();
                self.tour.referenced.entry(stored.clone()).or_insert_with(|| origin.to_string());
                Some(stored)
            }
            None => {
                self.issue(Issue::MissingFile { kind: format!("{kind:?}"), name: name.to_string(), origin: origin.to_string() });
                None
            }
        }
    }

    fn parse_file<T>(&mut self, file: &str, parse: impl FnOnce(&[u8]) -> Result<T>) -> Option<T> {
        let parsed = self.tour.container.read(file).and_then(|b| parse(&b));
        match parsed {
            Ok(v) => Some(v),
            Err(e) => {
                self.issue(Issue::ParseFailure { file: file.to_string(), error: format!("{e:#}") });
                None
            }
        }
    }

    fn run_script(&mut self, name: &str, origin: &str) -> Result<()> {
        let Some(file) = self.reference(FileRef::Script, name, origin) else { return Ok(()) };
        if self.depth > 16 {
            bail!("script nesting too deep at {file}");
        }
        let data = self.tour.container.read(&file)?;
        let parsed = script::parse(&data);
        let commands = parsed.commands.clone();
        if !self.tour.scripts.iter().any(|s| s.file == file) {
            self.tour.scripts.push(Named { file: file.clone(), data: parsed });
        }
        self.depth += 1;
        for c in &commands {
            let origin = format!("{file}:{}", c.source.line);
            self.run_command(&c.command, &origin)?;
        }
        self.depth -= 1;
        Ok(())
    }

    fn run_command(&mut self, cmd: &Command, origin: &str) -> Result<()> {
        match cmd {
            Command::Statements(list) => {
                for s in list {
                    self.run_statement(s, origin)?;
                }
            }
            Command::Define(d) => {
                self.tour.macros.insert(d.name.clone(), d.clone());
            }
            Command::Delete(name) => {
                self.tour.macros.remove(name);
            }
            Command::Run { names, .. } => {
                for name in names {
                    let Some(def) = self.tour.macros.get(name).cloned() else {
                        self.issue(Issue::UnknownMacro { name: name.clone(), origin: origin.to_string() });
                        continue;
                    };
                    if self.depth > 16 {
                        bail!("macro recursion too deep at {name}");
                    }
                    self.tour.macros_run.insert(name.clone());
                    self.depth += 1;
                    self.run_command(&def.command(), &format!("{origin} (macro {name})"))?;
                    self.depth -= 1;
                }
            }
            Command::Help | Command::MacroLog(_) => {}
        }
        Ok(())
    }

    fn run_statement(&mut self, s: &Statement, origin: &str) -> Result<()> {
        self.tour.executed.push(Executed { origin: origin.to_string(), statement: s.clone() });
        self.check_names(s, origin);
        if s.is(Subsystem::Scene, "DELETEALL") {
            // Full reset of the runtime state; nothing on the disc is loaded before it.
            self.tour.macros.clear();
            self.tour.cursor_slots.clear();
        }
        if let Some(ev) = s.event() {
            self.tour.events.push(ev);
        }
        match s.alarm() {
            Some(AlarmCommand::New { name, macro_text, .. }) => {
                self.tour.alarms.push((name.clone(), macro_text.clone()));
                self.tour.live_alarms.insert(name.clone());
                self.defer(&macro_text, &format!("{origin} (alarm {name})"));
            }
            Some(AlarmCommand::Named { name, action: script::AlarmAction::Delete }) => {
                self.tour.live_alarms.remove(&name);
            }
            _ => {}
        }
        if s.is_object_property(Subsystem::Model3d, "CLOUD.OFFSET")
            && let Some(id) = s.object_name()
            && let Some(p) = self.tour.cloud_placements.iter_mut().rev().find(|p| p.mesh.eq_ignore_ascii_case(id))
        {
            p.offset = Some(script::atof(s.arg()));
        }
        let Some((kind, name)) = s.file_reference() else { return Ok(()) };
        match kind {
            FileRef::Script => self.run_script(&name, origin)?,
            FileRef::Model => {
                if let Some(file) = self.reference(kind, &name, origin)
                    && let Some(x) = self.parse_file(&file, xfile::parse)
                {
                    self.tour.model = Some(Named { file, data: x });
                    self.resolve_textures();
                }
            }
            FileRef::Lights => {
                if let Some(file) = self.reference(kind, &name, origin)
                    && let Some(l) = self.parse_file(&file, lights::parse)
                {
                    self.tour.lights = Some(Named { file, data: l });
                }
            }
            FileRef::WalkMap => self.load_walkmap(&name, origin, true),
            FileRef::Layer2d => {
                if let Some(file) = self.reference(kind, &name, origin)
                    && let Some(layer) = self.parse_file(&file, layer2d::parse)
                {
                    for e in &layer.elements {
                        if !e.image.is_empty() {
                            self.reference_image(&e.image, &format!("{file} element {}", e.name));
                        }
                    }
                    self.tour.layers.push(Named { file, data: layer });
                }
            }
            FileRef::Cloud => {
                if let Some(file) = self.reference(kind, &name, origin) {
                    if !self.tour.clouds.iter().any(|c| c.file == file)
                        && let Some(c) = self.parse_file(&file, cloud::parse)
                    {
                        self.tour.clouds.push(Named { file: file.clone(), data: c });
                    }
                    let scale = if s.property == "CLOUD" { 1.0 } else { s.args().get(1).map_or(0.0, |v| script::atof(v)) };
                    let mesh = s.object_name().unwrap_or_default().to_string();
                    self.tour.cloud_placements.push(CloudPlacement { mesh, file, scale, offset: None });
                }
            }
            FileRef::Image => self.reference_image(&name, origin),
            FileRef::Cursor => {
                if let Some((slot, _)) = s.arg().split_once(',') {
                    self.tour.cursor_slots.insert(script::atoi(slot) as u32, name.clone());
                }
                self.reference(kind, &name, origin);
            }
            FileRef::Video => {
                self.reference(kind, &name, origin);
            }
        }
        Ok(())
    }

    fn reference_image(&mut self, name: &str, origin: &str) {
        let (how, file) = textures::resolve(name, |n| self.tour.container.find(n));
        match (how, file) {
            (Resolution::Missing, _) | (_, None) => {
                self.issue(Issue::MissingFile { kind: "Image".into(), name: name.to_string(), origin: origin.to_string() })
            }
            (_, Some(f)) => {
                let f = f.to_string();
                self.tour.referenced.entry(f).or_insert_with(|| origin.to_string());
            }
        }
    }

    /// `WALKMAP.LOAD`: cached by file name; `activate` is false for strings that only run later.
    fn load_walkmap(&mut self, name: &str, origin: &str, activate: bool) {
        let Some(file) = self.reference(FileRef::WalkMap, name, origin) else { return };
        if activate {
            self.tour.start_walkmap = Some(file.clone());
        }
        if self.tour.walkmaps.iter().any(|w| w.file == file) {
            return;
        }
        let Some(model) = &self.tour.model else {
            self.issue(Issue::NothingLoaded { what: format!("model (needed to size {file})"), origin: origin.to_string() });
            return;
        };
        let counts = model.data.face_counts();
        if let Some(w) = self.parse_file(&file, |b| walkmap::parse(b, &counts)) {
            self.tour.walkmaps.push(Named { file, data: w });
        }
    }

    fn resolve_textures(&mut self) {
        let Some(model) = &self.tour.model else { return };
        let file = model.file.clone();
        let mut refs = Vec::new();
        for (name, used) in model.data.all_texture_names() {
            let (resolution, found) = textures::resolve(&name, |q| self.tour.container.find(q));
            refs.push(TextureRef { name, resolution, file: found.map(str::to_string), used });
        }
        for r in &refs {
            match &r.file {
                Some(f) => {
                    self.tour.referenced.entry(f.clone()).or_insert_with(|| format!("{file} texture {}", r.name));
                }
                None => self.tour.issues.push(Issue::MissingTexture { name: r.name.clone(), used: r.used }),
            }
        }
        self.tour.textures = refs;
    }

    /// Checks object ids (and alarm names) against the state at the time the statement runs.
    fn check_names(&mut self, s: &Statement, origin: &str) {
        if s.subsystem == Subsystem::Unknown {
            self.issue(Issue::UnknownSubsystem { statement: s.text.clone(), origin: origin.to_string() });
            return;
        }
        if let Some(AlarmCommand::Named { name, .. }) = s.alarm()
            && !self.tour.live_alarms.contains(&name)
        {
            self.issue(Issue::UnknownAlarm { name, origin: origin.to_string() });
        }
        let Some(obj) = &s.object else { return };
        let origin = origin.to_string();
        let t = &self.tour;
        let missing = match s.subsystem {
            sub if sub.is_3d() => match &t.model {
                None => Some(Issue::NothingLoaded { what: "model".into(), origin }),
                Some(m) => match obj {
                    ObjectId::Name(n) => m.data.mesh_by_script_name(n).is_none().then(|| Issue::UnknownMesh { id: n.clone(), origin }),
                    ObjectId::Index(i) => (*i as usize >= m.data.meshes.len()).then(|| Issue::UnknownMesh { id: format!("[{i}]"), origin }),
                },
            },
            Subsystem::Lights => match &t.lights {
                None => Some(Issue::NothingLoaded { what: "lights".into(), origin }),
                Some(l) => match obj {
                    ObjectId::Name(n) => l.data.by_script_name(n).is_none().then(|| Issue::UnknownLight { id: n.clone(), origin }),
                    ObjectId::Index(i) => (*i as usize >= l.data.lights.len()).then(|| Issue::UnknownLight { id: format!("[{i}]"), origin }),
                },
            },
            Subsystem::Layer2d => match t.layers.last() {
                None => Some(Issue::NothingLoaded { what: "2D layer".into(), origin }),
                Some(l) => match obj {
                    ObjectId::Name(n) => l.data.element(n).is_none().then(|| Issue::UnknownElement { id: n.clone(), origin }),
                    ObjectId::Index(i) => (*i as usize >= l.data.elements.len()).then(|| Issue::UnknownElement { id: format!("[{i}]"), origin }),
                },
            },
            _ => None,
        };
        if let Some(m) = missing {
            self.issue(m);
        }
    }

    fn defer(&mut self, text: &str, origin: &str) {
        match script::parse_command(text) {
            Command::Statements(list) => {
                for s in list {
                    self.tour.deferred.push(Executed { origin: origin.to_string(), statement: s });
                }
            }
            Command::Run { names, .. } => {
                for n in names {
                    if !self.tour.macros.contains_key(&n) {
                        self.issue(Issue::UnknownMacro { name: n, origin: origin.to_string() });
                    }
                }
            }
            _ => {}
        }
    }

    /// Deferred strings, viewpoint files, the inventory of images and cursors, and model-level
    /// cross-checks.
    fn finish(&mut self) -> Result<()> {
        // Macros that loading never ran.
        let unrun: Vec<_> = self.tour.macros.values().filter(|m| !self.tour.macros_run.contains(&m.name)).cloned().collect();
        for m in unrun {
            self.defer(&m.body, &format!("macro {} (not run during loading)", m.name));
        }
        // Walk-map hotspot strings (cleaned of junk after the last ';').
        let mut hotspot_texts = Vec::new();
        for w in &self.tour.walkmaps {
            for (cell, enter, exit) in w.data.hotspots() {
                hotspot_texts.push((enter.script.clone(), format!("{} cell {cell} enter", w.file)));
                hotspot_texts.push((exit.script.clone(), format!("{} cell {cell} exit", w.file)));
            }
        }
        // 2D element macros of every layer.
        for l in &self.tour.layers {
            for e in &l.data.elements {
                for (ev, m) in [("mousedown", &e.macro_mouse_down), ("mouseup", &e.macro_mouse_up), ("mousein", &e.macro_mouse_in), ("mouseout", &e.macro_mouse_out)] {
                    if !m.is_empty() {
                        hotspot_texts.push((m.clone(), format!("{} element {} {ev}", l.file, e.name)));
                    }
                }
            }
        }
        for (text, origin) in hotspot_texts {
            self.defer(&text, &origin);
        }
        let deferred = self.tour.deferred.clone();
        for d in &deferred {
            self.check_names(&d.statement, &d.origin);
            match d.statement.file_reference() {
                Some((FileRef::WalkMap, name)) => self.load_walkmap(&name, &d.origin, false),
                Some((FileRef::Image, name)) => self.reference_image(&name, &d.origin),
                Some((kind, name)) => {
                    self.reference(kind, &name, &d.origin);
                }
                None => {}
            }
        }
        // Junk files are listed but deliberately not referenced.
        let files = self.tour.container.files.clone();
        for f in &files {
            match classify(f) {
                FileClass::ViewPoints => {
                    if let Some(v) = self.parse_file(f, viewpoints::parse) {
                        self.tour.viewpoints.push(Named { file: f.clone(), data: v });
                    }
                }
                FileClass::Image => {
                    if let Some(info) = self.parse_file(f, textures::inspect) {
                        self.tour.images.insert(f.clone(), info);
                    }
                }
                FileClass::Cursor => {
                    if let Some(c) = self.parse_file(f, textures::parse_cursor) {
                        self.tour.cursors.insert(f.clone(), c);
                    }
                }
                FileClass::Cloud if !self.tour.clouds.iter().any(|c| &c.file == f) => {
                    // Parsed for coverage; not referenced by any statement.
                    if let Some(c) = self.parse_file(f, cloud::parse) {
                        self.tour.clouds.push(Named { file: f.clone(), data: c });
                    }
                }
                FileClass::WalkMap if !self.tour.walkmaps.iter().any(|w| &w.file == f) => {
                    // Parsed for coverage (Colosseum STAIR5.3WM); kept out of `walkmaps`.
                    if let Some(m) = &self.tour.model {
                        let counts = m.data.face_counts();
                        self.parse_file(f, |b| walkmap::parse(b, &counts));
                    }
                }
                FileClass::Script if !self.tour.scripts.iter().any(|s| &s.file == f) => {
                    // A script no other script loads.
                    let data = self.tour.container.read(f)?;
                    self.tour.scripts.push(Named { file: f.clone(), data: script::parse(&data) });
                }
                _ => {}
            }
        }
        self.cross_check_model();
        self.tour.issues.sort();
        self.tour.issues.dedup();
        Ok(())
    }

    fn cross_check_model(&mut self) {
        let Some(model) = &self.tour.model else { return };
        let x = &model.data;
        let mut issues = Vec::new();
        for m in &x.meshes {
            for r in m.material_list.iter().flat_map(|l| &l.materials) {
                if let MaterialRef::Named(n) = r
                    && x.material(n).is_none()
                {
                    issues.push(Issue::UnresolvedMaterial { mesh: m.name.clone(), name: n.clone() });
                }
            }
        }
        if let Some(l) = &self.tour.lights {
            for light in &l.data.lights {
                for (i, (f, mask)) in light.entries().enumerate() {
                    let ok = x.meshes.get(f.object as usize).and_then(|m| m.faces.get(f.face as usize)).is_some_and(|face| mask >> face.len() == 0 || face.len() >= 32);
                    if !ok {
                        issues.push(Issue::BadLightEntry { light: light.name.clone(), entry: i });
                    }
                }
            }
            for (i, p) in l.data.paint.iter().enumerate() {
                let ok = x
                    .meshes
                    .iter()
                    .find(|m| m.name == p.object)
                    .and_then(|m| usize::try_from(p.face).ok().and_then(|f| m.faces.get(f)))
                    .is_some_and(|face| usize::try_from(p.vertex_slot).is_ok_and(|s| s < face.len()));
                if !ok {
                    issues.push(Issue::BadPaint { index: i, object: p.object.clone() });
                }
            }
        }
        for w in &self.tour.walkmaps {
            for (ci, c) in w.data.cells.iter().enumerate() {
                if let walkmap::CellData::Visibility(entries) = &c.data {
                    for e in entries {
                        let faces = x.meshes[e.mesh as usize].faces.len();
                        if (faces..e.bits.len() * 8).any(|j| e.face_visible(j)) {
                            issues.push(Issue::VisibilityBitsPastFaces { walkmap: w.file.clone(), cell: ci, mesh: e.mesh });
                        }
                    }
                }
            }
        }
        self.tour.issues.extend(issues);
    }
}

/// Tour container directories under `dir`: `dir` itself when it holds `load.3sc`, otherwise its
/// subdirectories that do (e.g. `…/WORLD/7015/`).
pub fn find_tours(dir: &Path) -> Result<Vec<PathBuf>> {
    let has_entry = |d: &Path| Container::open(d).map(|c| c.find(ENTRY_SCRIPT).is_some()).unwrap_or(false);
    if has_entry(dir) {
        return Ok(vec![dir.to_path_buf()]);
    }
    let mut out = Vec::new();
    for e in std::fs::read_dir(dir).with_context(|| format!("reading {}", dir.display()))? {
        let p = e?.path();
        if p.is_dir() && has_entry(&p) {
            out.push(p);
        }
    }
    out.sort();
    Ok(out)
}
