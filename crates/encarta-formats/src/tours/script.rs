//! `.3sc` scripts (spec §3): the file loader (lines, comments, macro blocks) and the command and
//! statement parser.
//!
//! The loader quirks are kept on purpose: TABs are deleted, only a `'` in the first column starts
//! a comment, and the lines of a `BEGIN(…)`…`END(…)` block are glued together with no separator,
//! so a statement without its trailing `;` merges with the next line (Acropolis `HIDEALL`).
//! Statements are split at every `;` (no protection by parentheses or `^…^`), and case-folded up
//! to the first `^`.

use serde::Serialize;

use super::cp1252;

/// One command delivered by the file loader: a plain line, or a whole glued macro block.
#[derive(Debug, Clone, Serialize)]
pub struct LoadedLine {
    /// 1-based number of the first source line.
    pub line: usize,
    /// 1-based number of the last source line (differs for macro blocks).
    pub last_line: usize,
    /// The command text exactly as the interpreter receives it (TABs removed, no case folding).
    pub text: String,
}

/// Splits a script file into commands (spec §3.1, steps 3–6).
///
/// A line ends at CR (an LF right after it is swallowed); a bare LF is ordinary text. A block that
/// is still open at the end of the file is delivered as it is (this does not happen on the disc).
pub fn load_lines(data: &[u8]) -> Vec<LoadedLine> {
    let mut lines = Vec::new();
    let mut start = 0;
    let mut i = 0;
    while i < data.len() {
        if data[i] == b'\r' {
            lines.push(&data[start..i]);
            i += 1;
            if data.get(i) == Some(&b'\n') {
                i += 1;
            }
            start = i;
        } else {
            i += 1;
        }
    }
    if start < data.len() {
        lines.push(&data[start..]);
    }

    let mut out = Vec::new();
    let mut block: Option<LoadedLine> = None;
    for (n, raw) in lines.into_iter().enumerate() {
        let no_tabs: Vec<u8> = raw.iter().copied().filter(|&b| b != b'\t').collect();
        if no_tabs.is_empty() || no_tabs[0] == b'\'' {
            continue;
        }
        let text = cp1252(&no_tabs);
        let upper = text.to_ascii_uppercase();
        let line = n + 1;
        match &mut block {
            Some(b) => {
                b.text.push_str(&text);
                b.last_line = line;
                if upper.starts_with("END(") {
                    out.push(block.take().unwrap());
                }
            }
            None if upper.starts_with("BEGIN(") => block = Some(LoadedLine { line, last_line: line, text }),
            None => out.push(LoadedLine { line, last_line: line, text }),
        }
    }
    out.extend(block);
    out
}

/// What one `Do()` call or one loaded line means (spec §3.2–3.4).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum Command {
    /// `?`: help text of every subsystem.
    Help,
    /// `STARTMACROLOG` (true) / `STOPMACROLOG` (false): debugging aid.
    MacroLog(bool),
    /// `BEGIN(name);body…END(name);`.
    Define(MacroDef),
    /// `DELETE(name)`.
    Delete(String),
    /// `RUN(a);RUN(b);…`: the upper-cased macro names, in order. Pieces that are not `RUN(` are
    /// kept in `ignored`.
    Run { names: Vec<String>, ignored: Vec<String> },
    /// Everything else: `;`-separated statements (empty pieces dropped).
    Statements(Vec<Statement>),
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MacroDef {
    /// Upper-cased name.
    pub name: String,
    /// The body text exactly as stored (not case-folded); run it with [`parse_command`].
    pub body: String,
    /// False when no `END(name` was found and the body runs to the end of the command.
    pub terminated: bool,
}

impl MacroDef {
    /// The body as the interpreter runs it.
    pub fn command(&self) -> Command {
        parse_command(&self.body)
    }
}

/// Parses one command string.
pub fn parse_command(text: &str) -> Command {
    if text == "?" {
        return Command::Help;
    }
    let upper = text.to_ascii_uppercase();
    if upper.starts_with("STARTMACROLOG") {
        return Command::MacroLog(true);
    }
    if upper.starts_with("STOPMACROLOG") {
        return Command::MacroLog(false);
    }
    if upper.starts_with("RUN(") {
        let mut names = Vec::new();
        let mut ignored = Vec::new();
        for piece in text.split(';').filter(|p| !p.is_empty()) {
            let up = piece.to_ascii_uppercase();
            match up.strip_prefix("RUN(") {
                Some(rest) => names.push(rest.split(')').next().unwrap_or("").to_string()),
                None => ignored.push(piece.to_string()),
            }
        }
        return Command::Run { names, ignored };
    }
    if upper.starts_with("BEGIN(") {
        let after = &text[6..];
        let (name, body_start) = match after.find(");") {
            Some(i) => (&after[..i], 6 + i + 2),
            None => (after.split(')').next().unwrap_or(""), text.len()),
        };
        let name = name.to_ascii_uppercase();
        let body_all = &text[body_start.min(text.len())..];
        let end_marker = format!("END({name}");
        let (body, terminated) = match body_all.to_ascii_uppercase().find(&end_marker) {
            Some(i) => (&body_all[..i], true),
            None => (body_all, false),
        };
        return Command::Define(MacroDef { name, body: body.to_string(), terminated });
    }
    if let Some(rest) = upper.strip_prefix("DELETE(") {
        return Command::Delete(rest.split(')').next().unwrap_or("").to_string());
    }
    Command::Statements(text.split(';').filter(|p| !p.is_empty()).map(parse_statement).collect())
}

impl Command {
    /// The statements this command runs directly (none for macro definitions and runs).
    pub fn statements(&self) -> &[Statement] {
        match self {
            Command::Statements(s) => s,
            _ => &[],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Subsystem {
    Scene,
    Script,
    /// A statement starting with `.` (handled like `SCENE`).
    Empty,
    Camera,
    Walkmap,
    Lights,
    /// `3D`.
    Model3d,
    /// `MODEL` (same handler as `3D`).
    Model,
    /// `2D`.
    Layer2d,
    Timer,
    /// Any other text before the first `.`: the statement is ignored and produces no result.
    Unknown,
}

impl Subsystem {
    fn from_text(s: &str) -> Subsystem {
        match s {
            "SCENE" => Subsystem::Scene,
            "SCRIPT" => Subsystem::Script,
            "" => Subsystem::Empty,
            "CAMERA" => Subsystem::Camera,
            "WALKMAP" => Subsystem::Walkmap,
            "LIGHTS" => Subsystem::Lights,
            "3D" => Subsystem::Model3d,
            "MODEL" => Subsystem::Model,
            "2D" => Subsystem::Layer2d,
            "TIMER" => Subsystem::Timer,
            _ => Subsystem::Unknown,
        }
    }

    /// The scene/script handler (`SCENE`, `SCRIPT` or empty).
    pub fn is_scene(self) -> bool {
        matches!(self, Subsystem::Scene | Subsystem::Script | Subsystem::Empty)
    }

    /// The model-object handler (`3D` or `MODEL`).
    pub fn is_3d(self) -> bool {
        matches!(self, Subsystem::Model3d | Subsystem::Model)
    }

    /// Handlers whose statements may address an object: `3D`/`MODEL`, `2D`, `LIGHTS`.
    pub fn has_objects(self) -> bool {
        self.is_3d() || matches!(self, Subsystem::Layer2d | Subsystem::Lights)
    }

    /// Canonical spelling for shape keys.
    pub fn label(self) -> &'static str {
        match self {
            Subsystem::Scene => "SCENE",
            Subsystem::Script => "SCRIPT",
            Subsystem::Empty => "",
            Subsystem::Camera => "CAMERA",
            Subsystem::Walkmap => "WALKMAP",
            Subsystem::Lights => "LIGHTS",
            Subsystem::Model3d => "3D",
            Subsystem::Model => "MODEL",
            Subsystem::Layer2d => "2D",
            Subsystem::Timer => "TIMER",
            Subsystem::Unknown => "?",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum ObjectId {
    /// Upper-cased name (mesh, light or 2D element).
    Name(String),
    /// `[n]` form: zero-based index.
    Index(u32),
}

impl ObjectId {
    pub fn name(&self) -> Option<&str> {
        match self {
            ObjectId::Name(n) => Some(n),
            ObjectId::Index(_) => None,
        }
    }
}

/// The character that separates the property path from its value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Op {
    /// `=`
    Assign,
    /// `(`
    Call,
    /// `?`
    Query,
}

impl Op {
    pub fn char(self) -> char {
        match self {
            Op::Assign => '=',
            Op::Call => '(',
            Op::Query => '?',
        }
    }
}

/// One statement after splitting and case folding (spec §3.8:
/// `subsystem "." [object-id "."] property [op value]`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Statement {
    /// The folded statement text.
    pub text: String,
    pub subsystem: Subsystem,
    /// Text before the first `.` (the subsystem as written, folded).
    pub subsystem_text: String,
    /// Object id for `3D`/`MODEL`, `2D` and `LIGHTS` statements whose path has two or more parts.
    pub object: Option<ObjectId>,
    /// The rest of the path, e.g. `VISIBLE`, `MACRO.MOUSEUP`, `ALARM.NEW`, `SPEED.FLY`.
    pub property: String,
    pub op: Option<Op>,
    /// Everything after the op character, unmodified (may carry an unbalanced `)`).
    pub value: String,
}

/// Upper-cases ASCII letters before the first `^` (spec §3.2 step 4.1).
pub fn fold_case(s: &str) -> String {
    match s.find('^') {
        Some(i) => format!("{}{}", s[..i].to_ascii_uppercase(), &s[i..]),
        None => s.to_ascii_uppercase(),
    }
}

/// Parses one `;`-free statement.
pub fn parse_statement(raw: &str) -> Statement {
    let text = fold_case(raw);
    let (subsystem_text, rest) = match text.find('.') {
        Some(i) => (&text[..i], &text[i + 1..]),
        None => (text.as_str(), ""),
    };
    let subsystem = if text.contains('.') { Subsystem::from_text(subsystem_text) } else { Subsystem::Unknown };
    let op_at = rest.find(['=', '(', '?']);
    let (path, op, value) = match op_at {
        Some(i) => {
            let op = match rest.as_bytes()[i] {
                b'=' => Op::Assign,
                b'(' => Op::Call,
                _ => Op::Query,
            };
            (&rest[..i], Some(op), &rest[i + 1..])
        }
        None => (rest, None, ""),
    };
    let (object, property) = match path.split_once('.') {
        Some((id, prop)) if subsystem.has_objects() => {
            let object = match id.strip_prefix('[').and_then(|s| s.strip_suffix(']')).and_then(|n| n.parse().ok()) {
                Some(n) => ObjectId::Index(n),
                None => ObjectId::Name(id.to_string()),
            };
            (Some(object), prop.to_string())
        }
        _ => (None, path.to_string()),
    };
    Statement { text: text.clone(), subsystem, subsystem_text: subsystem_text.to_string(), object, property, op, value: value.to_string() }
}

/// C `atof`: leading whitespace skipped, longest numeric prefix, 0 when there is none.
pub fn atof(s: &str) -> f64 {
    let s = s.trim_start();
    let b = s.as_bytes();
    let mut end = 0;
    let mut best = 0;
    if end < b.len() && (b[end] == b'+' || b[end] == b'-') {
        end += 1;
    }
    let mut digits = false;
    while end < b.len() && b[end].is_ascii_digit() {
        end += 1;
        digits = true;
        best = end;
    }
    if end < b.len() && b[end] == b'.' {
        end += 1;
        while end < b.len() && b[end].is_ascii_digit() {
            end += 1;
            digits = true;
            best = end;
        }
    }
    if digits && end < b.len() && (b[end] == b'e' || b[end] == b'E') {
        let mut e = end + 1;
        if e < b.len() && (b[e] == b'+' || b[e] == b'-') {
            e += 1;
        }
        let start = e;
        while e < b.len() && b[e].is_ascii_digit() {
            e += 1;
        }
        if e > start {
            best = e;
        }
    }
    if !digits {
        return 0.0;
    }
    s[..best].parse().unwrap_or(0.0)
}

/// C `atoi`.
pub fn atoi(s: &str) -> i64 {
    let s = s.trim_start();
    let b = s.as_bytes();
    let mut end = usize::from(!b.is_empty() && (b[0] == b'+' || b[0] == b'-'));
    while end < b.len() && b[end].is_ascii_digit() {
        end += 1;
    }
    s[..end].parse().unwrap_or(0)
}

/// `SCENE.` commands in match order (spec §3.3: the first prefix that matches wins). Longer
/// keywords are placed before keywords that are their prefixes.
const SCENE_KEYWORDS: &[&str] = &[
    "WORLDINFO?",
    "FORCESOFTWARE",
    "TRYHARDWAREFULLSCREEN",
    "TRYHARDWARE",
    "TRYFULLSCREEN",
    "KILLFULLSCREEN",
    "DELETEALL",
    "FOGCOLOR",
    "FOGSTART",
    "FOGEND",
    "HALFRESDRIVING",
    "HIGHSPECULAR",
    "SPECULAR",
    "DISPLAYFPS",
    "LOADFILESFROMITS",
    "LOAD",
    "GRAVITY",
    "ITSFILEURL",
    "CURSORS",
    "CURSOR",
    "EVENT",
    "WORKINGPATH",
    "RUN",
    "DRAW",
    "SHOWCOLLISION",
    "AMBIENT",
    "ALARM.",
    "CLEARSEQUENCE",
    "ADDSEQUENCE",
];

impl Statement {
    /// The text after the first `.` (what the subsystem handler receives).
    pub fn rest(&self) -> &str {
        self.text.get(self.subsystem_text.len() + 1..).unwrap_or("")
    }

    /// For scene/script statements: the command keyword the handler matches by prefix, e.g.
    /// `DRAW` for `SCENE.DRAW)` (left over from a `;` split).
    pub fn scene_keyword(&self) -> Option<&'static str> {
        if !self.subsystem.is_scene() {
            return None;
        }
        let rest = self.rest();
        SCENE_KEYWORDS.iter().copied().find(|k| rest.starts_with(k)).map(|k| k.trim_end_matches(['.', '?']))
    }

    /// True for `<subsystem>.<property>` with no object. Scene handlers match `SCENE`, `SCRIPT`
    /// and empty alike, and by keyword prefix.
    pub fn is(&self, subsystem: Subsystem, property: &str) -> bool {
        if subsystem.is_scene() {
            return self.scene_keyword() == Some(property) || (self.subsystem.is_scene() && self.property == property);
        }
        let sub = if subsystem.is_3d() { self.subsystem.is_3d() } else { self.subsystem == subsystem };
        sub && self.object.is_none() && self.property == property
    }

    /// True for `<subsystem>.<id>.<property>`.
    pub fn is_object_property(&self, subsystem: Subsystem, property: &str) -> bool {
        let sub = if subsystem.is_3d() { self.subsystem.is_3d() } else { self.subsystem == subsystem };
        sub && self.object.is_some() && self.property == property
    }

    pub fn object_name(&self) -> Option<&str> {
        self.object.as_ref().and_then(ObjectId::name)
    }

    /// The argument as handlers see it: the value with one trailing `)` removed.
    pub fn arg(&self) -> &str {
        self.value.strip_suffix(')').unwrap_or(&self.value)
    }

    /// The argument split at commas.
    pub fn args(&self) -> Vec<&str> {
        self.arg().split(',').collect()
    }

    /// Numbers of a comma list, each parsed like `atof`.
    pub fn numbers(&self) -> Vec<f64> {
        self.args().into_iter().map(atof).collect()
    }

    /// Generic boolean: `1`/`TRUE`/`ON`/`ACTIVE` or `0`/`FALSE`/`OFF`.
    pub fn bool_value(&self) -> Option<bool> {
        match self.arg().trim() {
            "1" | "TRUE" | "ON" | "ACTIVE" => Some(true),
            "0" | "FALSE" | "OFF" => Some(false),
            _ => None,
        }
    }

    /// `3D` flag properties (VISIBLE, SPRITE, NOILLUMINATION, WATER): only the first character
    /// after `=` counts, the rest is ignored (spec §7.4).
    pub fn flag(&self) -> Option<bool> {
        match self.value.as_bytes().first() {
            Some(b'0') => Some(false),
            Some(b'1') => Some(true),
            _ => None,
        }
    }

    /// `^…^` text (e.g. `2D._H1.TEXT=^Map^`): the case-preserved text between the carets; without
    /// carets, the argument.
    pub fn text_value(&self) -> &str {
        let v = self.arg();
        match v.strip_prefix('^') {
            Some(rest) => rest.split('^').next().unwrap_or(rest),
            None => v,
        }
    }

    /// `SCENE.EVENT(x)` / `SCENE.EVENT=x` / `SCRIPT.EVENT(x)`: the event text fired to the host.
    pub fn event(&self) -> Option<Event> {
        (self.is(Subsystem::Scene, "EVENT") && self.op.is_some_and(|o| o != Op::Query)).then(|| Event::parse(self.arg()))
    }

    /// `SCENE.ALARM.…` commands.
    pub fn alarm(&self) -> Option<AlarmCommand> {
        if !self.subsystem.is_scene() || self.object.is_some() {
            return None;
        }
        let path = self.property.strip_prefix("ALARM.")?;
        Some(match path {
            "NEW" => {
                let mut parts = self.value.splitn(5, ',');
                let active = atoi(parts.next().unwrap_or("")) == 1;
                let max_executions = atoi(parts.next().unwrap_or(""));
                let interval = atof(parts.next().unwrap_or(""));
                let name = parts.next().unwrap_or("").to_string();
                let macro_text = parts.next().unwrap_or("").trim_end_matches(')').to_string();
                AlarmCommand::New { active, max_executions, interval, name, macro_text }
            }
            "ACTIVATE" => AlarmCommand::ActivateAll,
            "DEACTIVATE" => AlarmCommand::DeactivateAll,
            named => {
                let (name, action) = named.split_once('.').unwrap_or((named, ""));
                let action = match (action, self.op) {
                    ("ACTIVATE", _) => AlarmAction::Activate,
                    ("DEACTIVATE", _) => AlarmAction::Deactivate,
                    ("ACTIVE.TOGGLE", _) => AlarmAction::Toggle,
                    ("ACTIVE", Some(Op::Assign)) if self.arg() == "TOGGLE" => AlarmAction::Toggle,
                    ("DELETE", _) => AlarmAction::Delete,
                    ("ADDSEQUENCE", _) => AlarmAction::AddSequence(self.arg().to_string()),
                    ("CLEARSEQUENCE", _) => AlarmAction::ClearSequence,
                    (other, _) => AlarmAction::Other(other.to_string()),
                };
                AlarmCommand::Named { name: name.to_string(), action }
            }
        })
    }

    /// Files this statement makes the runtime read, with the kind of reference.
    pub fn file_reference(&self) -> Option<(FileRef, String)> {
        let s = self.subsystem;
        let file = |v: &str| v.trim().to_string();
        if s.is_scene() {
            if let Some(k @ ("LOAD" | "RUN")) = self.scene_keyword() {
                let v = &self.rest()[k.len()..];
                let v = v.strip_prefix(['=', '(']).unwrap_or(v);
                return Some((FileRef::Script, file(v.strip_suffix(')').unwrap_or(v))));
            }
            if self.scene_keyword() == Some("CURSOR") && self.op == Some(Op::Assign) {
                return self.arg().split_once(',').map(|(_, f)| (FileRef::Cursor, file(f)));
            }
        }
        if self.op.is_none_or(|o| o == Op::Query) {
            return None;
        }
        match (s, self.object.is_some(), self.property.as_str()) {
            (s, false, "LOAD") if s.is_3d() => Some((FileRef::Model, file(self.arg()))),
            (Subsystem::Lights, false, "LOAD") => Some((FileRef::Lights, file(self.arg()))),
            (Subsystem::Walkmap, false, "LOAD") => Some((FileRef::WalkMap, file(self.arg()))),
            (Subsystem::Layer2d, false, "LOAD") => Some((FileRef::Layer2d, file(self.arg()))),
            (Subsystem::Layer2d, false, "ICONFILE") => Some((FileRef::Image, file(self.arg()))),
            (s, true, "CLOUD") if s.is_3d() => Some((FileRef::Cloud, file(self.arg()))),
            (s, true, "CLOUDSCALED") if s.is_3d() => Some((FileRef::Cloud, file(self.arg().split(',').next().unwrap_or("")))),
            (s, true, "APPLYTEXTURE") if s.is_3d() => Some((FileRef::Image, file(self.arg()))),
            (s, true, "FILMSTRIP") if s.is_3d() => self.arg().splitn(3, ',').nth(2).map(|f| (FileRef::Image, file(f))),
            (s, true, "PLAYAVI") if s.is_3d() => Some((FileRef::Video, file(self.arg()))),
            _ => None,
        }
    }

    /// A generalised shape such as `2D.<id>.VISIBLE=` for inventories.
    pub fn shape(&self) -> String {
        let mut s = String::from(self.subsystem.label());
        s.push('.');
        if self.object.is_some() {
            s.push_str("<id>.");
        }
        s.push_str(&self.property);
        if let Some(op) = self.op {
            s.push(op.char());
        }
        s
    }
}

/// The kind of file a statement references.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
pub enum FileRef {
    Script,
    Model,
    Lights,
    WalkMap,
    Layer2d,
    Cloud,
    Image,
    Cursor,
    Video,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum AlarmCommand {
    /// `ALARM.NEW(active, maxExecutions, interval, name, macro)`; `macro_text` is everything after
    /// the 4th comma with trailing `)` trimmed.
    New { active: bool, max_executions: i64, interval: f64, name: String, macro_text: String },
    ActivateAll,
    DeactivateAll,
    Named { name: String, action: AlarmAction },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum AlarmAction {
    Activate,
    Deactivate,
    Toggle,
    Delete,
    AddSequence(String),
    ClearSequence,
    Other(String),
}

/// An `Event3DX` payload: `NAME` or `NAME:key=value,key=value,…`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Event {
    pub text: String,
    pub name: String,
    pub pairs: Vec<(String, String)>,
}

impl Event {
    pub fn parse(text: &str) -> Event {
        let (name, rest) = text.split_once(':').unwrap_or((text, ""));
        let pairs = rest
            .split(',')
            .filter(|p| !p.is_empty())
            .map(|p| {
                let (k, v) = p.split_once('=').unwrap_or((p, ""));
                (k.to_string(), v.to_string())
            })
            .collect();
        Event { text: text.to_string(), name: name.to_string(), pairs }
    }

    /// Case-insensitive key lookup.
    pub fn get(&self, key: &str) -> Option<&str> {
        self.pairs.iter().find(|(k, _)| k.eq_ignore_ascii_case(key)).map(|(_, v)| v.as_str())
    }
}

/// A loaded script file: every command the loader delivers, parsed.
#[derive(Debug, Clone, Serialize)]
pub struct Script {
    pub commands: Vec<ScriptCommand>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ScriptCommand {
    pub source: LoadedLine,
    pub command: Command,
}

pub fn parse(data: &[u8]) -> Script {
    Script { commands: load_lines(data).into_iter().map(|source| ScriptCommand { command: parse_command(&source.text), source }).collect() }
}

impl Script {
    /// Statements run directly by the file plus those inside macro definitions it contains, each
    /// with its command index and whether it sits in a macro body.
    pub fn all_statements(&self) -> Vec<(usize, bool, Statement)> {
        let mut out = Vec::new();
        for (i, c) in self.commands.iter().enumerate() {
            match &c.command {
                Command::Statements(s) => out.extend(s.iter().cloned().map(|s| (i, false, s))),
                Command::Define(d) => out.extend(d.command().statements().iter().cloned().map(|s| (i, true, s))),
                _ => {}
            }
        }
        out
    }

    pub fn macro_definitions(&self) -> impl Iterator<Item = &MacroDef> {
        self.commands.iter().filter_map(|c| match &c.command {
            Command::Define(d) => Some(d),
            _ => None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loader_lines_comments_tabs_and_blocks() {
        let src = b"'comment\r\n SCENE.DRAW\r\n\tCAMERA.FOV=80\rA\nB\r\nBEGIN(HideAll);\r\n2D._L1.VISIBLE=0;\r\n'skipped\r\n3d.n_00water.WATER=0\r\n3d.Box23a.VISIBLE=0\r\n2D._dot.VISIBLE=0;\r\nEND(HideAll);\r\nlast";
        let l = load_lines(src);
        let texts: Vec<_> = l.iter().map(|l| l.text.as_str()).collect();
        assert_eq!(
            texts,
            vec![
                " SCENE.DRAW",
                "CAMERA.FOV=80",
                "A\nB",
                "BEGIN(HideAll);2D._L1.VISIBLE=0;3d.n_00water.WATER=03d.Box23a.VISIBLE=02D._dot.VISIBLE=0;END(HideAll);",
                "last"
            ]
        );
        assert_eq!((l[3].line, l[3].last_line), (5, 11), "CR alone ends a line; bare LF does not");

        let Command::Define(d) = parse_command(&l[3].text) else { panic!() };
        assert_eq!(d.name, "HIDEALL");
        assert!(d.terminated);
        let body = d.command();
        let s = body.statements();
        assert_eq!(s.len(), 2);
        assert_eq!(s[1].text, "3D.N_00WATER.WATER=03D.BOX23A.VISIBLE=02D._DOT.VISIBLE=0");
        assert_eq!(s[1].object, Some(ObjectId::Name("N_00WATER".into())));
        assert_eq!(s[1].property, "WATER");
        assert_eq!(s[1].flag(), Some(false));
    }

    #[test]
    fn statements_fold_and_split() {
        let Command::Statements(s) = parse_command("2D._H26.TEXT=^Coloring the Sculptures^;2D._H26.ANCHORPOINT=0.42,1.4,3.9,183.6;2d._H26.MACRO.MOUSEUP=SCRIPT.EVENT(HotSpotClicked:hotSpotID=701504853);") else {
            panic!()
        };
        assert_eq!(s.len(), 3);
        assert_eq!(s[0].text_value(), "Coloring the Sculptures");
        assert_eq!(s[1].numbers()[..3], [0.42, 1.4, 3.9]);
        assert_eq!(s[2].property, "MACRO.MOUSEUP");
        assert_eq!(s[2].value, "SCRIPT.EVENT(HOTSPOTCLICKED:HOTSPOTID=701504853)");
        let Command::Statements(inner) = parse_command(&s[2].value) else { panic!() };
        let ev = inner[0].event().unwrap();
        assert_eq!(ev.name, "HOTSPOTCLICKED");
        assert_eq!(ev.get("hotspotid"), Some("701504853"));
        assert_eq!(s[2].shape(), "2D.<id>.MACRO.MOUSEUP=");
    }

    #[test]
    fn alarm_split_quirk() {
        let Command::Statements(s) = parse_command("SCENE.ALARM.NEW(1,-1,0.1,LOADALARM,2d._0.nextframe(16);scene.draw)") else { panic!() };
        assert_eq!(s.len(), 2);
        assert_eq!(
            s[0].alarm(),
            Some(AlarmCommand::New { active: true, max_executions: -1, interval: 0.1, name: "LOADALARM".into(), macro_text: "2D._0.NEXTFRAME(16".into() })
        );
        assert!(s[1].is(Subsystem::Scene, "DRAW"));
        let Command::Statements(s) = parse_command("SCRIPT.ALARM.NEW(1,-1,1.0,USERLOCUPDATE,SCRIPT.EVENT(USERLOCUPDATE))") else { panic!() };
        let Some(AlarmCommand::New { macro_text, .. }) = s[0].alarm() else { panic!() };
        assert_eq!(macro_text, "SCRIPT.EVENT(USERLOCUPDATE");
        let Command::Statements(m) = parse_command(&macro_text) else { panic!() };
        assert_eq!(m[0].event().unwrap().text, "USERLOCUPDATE");
        let Command::Statements(s) = parse_command("SCENE.ALARM.LOADALARM.DELETE") else { panic!() };
        assert_eq!(s[0].alarm(), Some(AlarmCommand::Named { name: "LOADALARM".into(), action: AlarmAction::Delete }));
    }

    #[test]
    fn runs_macros_and_odd_statements() {
        assert_eq!(parse_command("RUN(HIDEALL);"), Command::Run { names: vec!["HIDEALL".into()], ignored: vec![] });
        assert_eq!(parse_command("run(a);x;RUN(b)"), Command::Run { names: vec!["A".into(), "B".into()], ignored: vec!["x".into()] });
        assert_eq!(parse_command("?"), Command::Help);
        assert_eq!(parse_command("delete(foo)"), Command::Delete("FOO".into()));
        let Command::Statements(s) = parse_command("2D._L1.VISIBLE=1;RUN(HIDEALL);junk\u{e8}") else { panic!() };
        assert_eq!(s[1].subsystem, Subsystem::Unknown);
        assert_eq!(s[2].subsystem, Subsystem::Unknown);
        let Command::Statements(s) = parse_command(".DRAW;3D.[12].VISIBLE=0;2D.LOAD=begin.32d;CAMERA.SPEED.FLY=.002;WALKMAP.LOAD=level2.3wm") else { panic!() };
        assert!(s[0].is(Subsystem::Scene, "DRAW"));
        assert_eq!(s[1].object, Some(ObjectId::Index(12)));
        assert_eq!(s[2].file_reference(), Some((FileRef::Layer2d, "BEGIN.32D".into())));
        assert!(s[3].object.is_none() && s[3].property == "SPEED.FLY" && s[3].numbers() == [0.002]);
        assert_eq!(s[4].file_reference(), Some((FileRef::WalkMap, "LEVEL2.3WM".into())));
        let Command::Statements(s) = parse_command("3d.Torch_1.FILMSTRIP=16,.07,fireAnm.bmp;3d.n_00statue.cloudscaled=cartidm.3cl,0.88;SCENE.CURSOR=6,grip.cur") else { panic!() };
        assert_eq!(s[0].file_reference(), Some((FileRef::Image, "FIREANM.BMP".into())));
        assert_eq!(s[1].file_reference(), Some((FileRef::Cloud, "CARTIDM.3CL".into())));
        assert_eq!(s[2].file_reference(), Some((FileRef::Cursor, "GRIP.CUR".into())));
    }

    #[test]
    fn c_number_parsing() {
        assert_eq!(atof(" -2.5e1x"), -25.0);
        assert_eq!(atof(".07"), 0.07);
        assert_eq!(atof("abc"), 0.0);
        assert_eq!(atof("3.97,183"), 3.97);
        assert_eq!(atoi("-1,"), -1);
        assert_eq!(atoi("x"), 0);
    }
}
