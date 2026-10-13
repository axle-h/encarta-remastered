//! Cast libraries and cast members: `KEY*`, `MCsL`, `CAS*`, `CASt`, `LctX`/`Lnam`, `DRCF`.
//! Spec: `docs/formats/shockwave.md` §3. Inner chunks are big-endian; `KEY*` is little-endian.

use super::bytes::{Cursor, FourCC, be_i16, be_i32, be_u16, be_u32, mac_roman};
use super::container::{Container, FileKind};
use anyhow::{Context, Result, ensure};
use serde::Serialize;
use std::collections::HashMap;

/// Cast id that owns the members of a stand-alone cast file, and of a movie's first cast.
pub const DEFAULT_CAST_ID: i32 = 1024;

/// One `KEY*` entry: `child` (a resource id) belongs to `owner` (a cast id or a `CASt` id).
#[derive(Debug, Clone, Copy, Serialize)]
pub struct KeyEntry {
    pub child: u32,
    pub owner: i32,
    pub kind: FourCC,
}

pub fn parse_key_table(b: &[u8]) -> Result<Vec<KeyEntry>> {
    let mut c = Cursor::new(b);
    let entry_size = c.u16_le()? as usize;
    let _header_size = c.u16_le()?;
    let _capacity = c.u32_le()?;
    let used = c.u32_le()? as usize;
    ensure!(entry_size == 12, "KEY* entry size {entry_size}");
    let mut out = Vec::with_capacity(used);
    for _ in 0..used {
        let child = c.u32_le()?;
        let owner = c.i32_le()?;
        let kind = FourCC::reversed(c.take(4)?);
        out.push(KeyEntry { child, owner, kind });
    }
    Ok(out)
}

/// A Director "list chunk": a header ending at `data_offset`, then a u16 item count, the item
/// offsets (u32 each), the total item length (u32) and the items. Returns (header, items).
pub fn parse_list(b: &[u8]) -> Result<(&[u8], Vec<&[u8]>)> {
    let data_offset = be_u32(b, 0).context("list header")? as usize;
    ensure!(
        data_offset <= b.len(),
        "list data offset {data_offset} past end {}",
        b.len()
    );
    let mut c = Cursor::at(b, data_offset);
    let n = c.u16()? as usize;
    let mut offs = Vec::with_capacity(n + 1);
    for _ in 0..n {
        offs.push(c.u32()? as usize);
    }
    let total = c.u32()? as usize;
    offs.push(total);
    let base = c.pos();
    ensure!(base + total <= b.len(), "list items run past end");
    let mut items = Vec::with_capacity(n);
    for w in offs.windows(2) {
        ensure!(w[0] <= w[1], "list offsets not increasing");
        items.push(&b[base + w[0]..base + w[1]]);
    }
    Ok((&b[..data_offset], items))
}

/// A Pascal string item (length byte, bytes, optional NUL), Mac Roman.
pub fn pascal_item(item: &[u8]) -> String {
    match item.split_first() {
        Some((&n, rest)) => mac_roman(&rest[..(n as usize).min(rest.len())]),
        None => String::new(),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MemberType {
    Bitmap,
    FilmLoop,
    Field,
    Palette,
    Picture,
    Sound,
    Button,
    Shape,
    Movie,
    DigitalVideo,
    Script,
    Text,
    Ole,
    Transition,
    Xtra,
    Unknown(u32),
}

impl MemberType {
    pub fn from_code(c: u32) -> Self {
        use MemberType::*;
        match c {
            1 => Bitmap,
            2 => FilmLoop,
            3 => Field,
            4 => Palette,
            5 => Picture,
            6 => Sound,
            7 => Button,
            8 => Shape,
            9 => Movie,
            10 => DigitalVideo,
            11 => Script,
            12 => Text,
            13 => Ole,
            14 => Transition,
            15 => Xtra,
            n => Unknown(n),
        }
    }
    pub fn label(self) -> String {
        match self {
            MemberType::Unknown(n) => format!("type{n}"),
            t => serde_json::to_value(t)
                .ok()
                .and_then(|v| v.as_str().map(str::to_owned))
                .unwrap_or_default(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ScriptKind {
    Score,
    Movie,
    Parent,
    Other(u16),
}

/// One cast member (`CASt`) with the data resources `KEY*` attaches to it.
#[derive(Debug, Clone, Serialize)]
pub struct Member {
    /// member number within its cast (slot index + the cast's first member number)
    pub number: u32,
    /// `CASt` resource id
    pub resource: u32,
    pub kind: MemberType,
    pub name: String,
    /// other info-list items that are present (authoring path, file name …)
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub info: Vec<String>,
    /// 1-based index into the cast's `LctX` entries (script members, and members with a script)
    pub script_id: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub script_kind: Option<ScriptKind>,
    /// Xtra name for Xtra members (`vectorShape`, `text`, `swa` …)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub xtra: Option<String>,
    /// data resources linked by `KEY*` (fourCC, resource id)
    pub data: Vec<(FourCC, u32)>,
    #[serde(skip)]
    pub specific: Vec<u8>,
}

impl Member {
    pub fn data_of(&self, kind: &str) -> Option<u32> {
        let k = FourCC::new(kind);
        self.data.iter().find(|(f, _)| *f == k).map(|&(_, id)| id)
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct CastLib {
    pub name: String,
    /// authoring path of an external cast (`C:\dirsrc\dswMedia\…\x.cst`); empty for internal casts
    pub path: String,
    pub cast_id: i32,
    pub first_member: u32,
    pub last_member: u32,
    /// `CAS*` resource id, when this file holds the cast's members
    pub index_resource: Option<u32>,
    pub members: Vec<Member>,
    /// `Lnam` names of the cast's script context
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub names: Vec<String>,
    /// `LctX` script entries: `Lscr` resource id per script id (index 0 = script id 1)
    #[serde(skip)]
    pub scripts: Vec<Option<u32>>,
}

impl CastLib {
    pub fn is_external(&self) -> bool {
        self.index_resource.is_none()
    }
    pub fn member(&self, number: u32) -> Option<&Member> {
        self.members.iter().find(|m| m.number == number)
    }
    pub fn member_named(&self, name: &str) -> Option<&Member> {
        self.members
            .iter()
            .find(|m| m.name.eq_ignore_ascii_case(name))
    }

    /// Handler names of a script member (from the `Lscr` handler table and `Lnam`).
    pub fn handler_names(&self, file: &Container, m: &Member) -> Vec<String> {
        let Some(Some(lscr)) = (m.script_id as usize)
            .checked_sub(1)
            .and_then(|i| self.scripts.get(i))
        else {
            return Vec::new();
        };
        let Some(b) = file.resource(*lscr) else {
            return Vec::new();
        };
        handler_name_ids(b)
            .into_iter()
            .map(|i| {
                self.names
                    .get(i as usize)
                    .cloned()
                    .unwrap_or_else(|| format!("#{i}"))
            })
            .collect()
    }
}

/// Name ids of the handlers in an `Lscr` (count u16 at 72, offset u32 at 74, 46-byte records
/// starting with an i16 name id). Only names are read, never bytecode.
pub fn handler_name_ids(lscr: &[u8]) -> Vec<i16> {
    let (Some(n), Some(off)) = (be_u16(lscr, 72), be_u32(lscr, 74)) else {
        return Vec::new();
    };
    (0..n as usize)
        .filter_map(|i| be_i16(lscr, off as usize + i * 46))
        .collect()
}

/// Stage and timing from `DRCF`.
#[derive(Debug, Clone, Serialize)]
pub struct Config {
    /// top, left, bottom, right
    pub stage: [i16; 4],
    /// first and last member number of the file's own cast
    pub members: [i16; 2],
    pub bit_depth: i16,
    pub director_version: i16,
    pub frame_rate: i16,
}

impl Config {
    pub fn parse(b: &[u8]) -> Option<Config> {
        Some(Config {
            stage: [be_i16(b, 4)?, be_i16(b, 6)?, be_i16(b, 8)?, be_i16(b, 10)?],
            members: [be_i16(b, 12)?, be_i16(b, 14)?],
            bit_depth: be_i16(b, 28)?,
            director_version: be_i16(b, 36)?,
            frame_rate: be_i16(b, 54)?,
        })
    }
    pub fn width(&self) -> i16 {
        self.stage[3] - self.stage[1]
    }
    pub fn height(&self) -> i16 {
        self.stage[2] - self.stage[0]
    }
}

/// A Director movie or cast file: the container plus its cast libraries.
#[derive(Debug, Clone)]
pub struct DirectorFile {
    pub container: Container,
    pub keys: Vec<KeyEntry>,
    pub config: Option<Config>,
    pub casts: Vec<CastLib>,
}

impl DirectorFile {
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        let container = Container::parse(bytes)?;
        let keys = match container.first("KEY*") {
            Some((_, b)) => parse_key_table(b).context("KEY*")?,
            None => Vec::new(),
        };
        let config = container.first("DRCF").and_then(|(_, b)| Config::parse(b));

        let mut casts = match container.first("MCsL") {
            Some((_, b)) => parse_cast_list(b).context("MCsL")?,
            None => Vec::new(),
        };
        // A cast file has no MCsL; its single cast is owned by the default cast id.
        if casts.is_empty() {
            let name = if container.kind == FileKind::Cast {
                "cast"
            } else {
                "Internal"
            };
            casts.push(CastLib {
                name: name.into(),
                path: String::new(),
                cast_id: DEFAULT_CAST_ID,
                // DRCF holds the cast's member range (coreUtil, widgets1 and scrolTxt start at 2)
                first_member: config.as_ref().map_or(1, |c| c.members[0].max(1) as u32),
                last_member: 0,
                index_resource: None,
                members: Vec::new(),
                names: Vec::new(),
                scripts: Vec::new(),
            });
        }

        let mut children: HashMap<i32, Vec<(FourCC, u32)>> = HashMap::new();
        for k in &keys {
            children.entry(k.owner).or_default().push((k.kind, k.child));
        }
        let find = |owner: i32, kind: &str| {
            let k = FourCC::new(kind);
            children
                .get(&owner)
                .and_then(|v| v.iter().find(|(f, _)| *f == k))
                .map(|&(_, id)| id)
        };

        // External casts carry a placeholder id (often 1024 too): only internal ones are looked up.
        for cast in casts.iter_mut().filter(|c| c.path.is_empty()) {
            let Some(index_id) = find(cast.cast_id, "CAS*") else {
                continue;
            };
            cast.index_resource = Some(index_id);
            if let Some(ctx) = find(cast.cast_id, "LctX").or_else(|| find(cast.cast_id, "Lctx")) {
                let (names, scripts) = read_script_context(&container, ctx);
                cast.names = names;
                cast.scripts = scripts;
            }
            let index = container
                .resource(index_id)
                .context("CAS* resource missing")?;
            for (slot, w) in index.as_chunks::<4>().0.iter().enumerate() {
                let id = u32::from_be_bytes(*w);
                if id == 0 {
                    continue;
                }
                let Some(cast_bytes) = container.resource(id) else {
                    continue;
                };
                let mut m = parse_member(cast_bytes).with_context(|| format!("CASt {id}"))?;
                m.number = cast.first_member + slot as u32;
                m.resource = id;
                m.data = children.get(&(id as i32)).cloned().unwrap_or_default();
                cast.members.push(m);
            }
            if cast.last_member == 0 {
                cast.last_member = cast.first_member + (index.len() / 4) as u32 - 1;
            }
        }
        Ok(DirectorFile {
            container,
            keys,
            config,
            casts,
        })
    }

    pub fn cast(&self, key: &str) -> Option<&CastLib> {
        if let Ok(n) = key.parse::<usize>() {
            return self.casts.get(n.checked_sub(1)?);
        }
        self.casts.iter().find(|c| c.name.eq_ignore_ascii_case(key))
    }

    /// All members of all casts held in this file.
    pub fn members(&self) -> impl Iterator<Item = (&CastLib, &Member)> {
        self.casts
            .iter()
            .flat_map(|c| c.members.iter().map(move |m| (c, m)))
    }

    pub fn resource(&self, id: u32) -> Option<&[u8]> {
        self.container.resource(id)
    }
}

fn parse_cast_list(b: &[u8]) -> Result<Vec<CastLib>> {
    let (header, items) = parse_list(b)?;
    let per_cast = be_u16(header, 8).filter(|&n| n > 0).unwrap_or(4) as usize;
    // item 0 is a leading (empty) item, then `per_cast` items per cast
    let mut out = Vec::new();
    for group in items.get(1..).unwrap_or(&[]).chunks(per_cast) {
        let name = group.first().map(|i| pascal_item(i)).unwrap_or_default();
        let path = group.get(1).map(|i| pascal_item(i)).unwrap_or_default();
        let ids = group.get(3).copied().unwrap_or(&[]);
        let first = be_u16(ids, 0).unwrap_or(1) as u32;
        let last = be_u16(ids, 2).unwrap_or(0) as u32;
        let cast_id = be_i32(ids, 4).unwrap_or(DEFAULT_CAST_ID);
        out.push(CastLib {
            name,
            path,
            cast_id,
            first_member: first,
            last_member: last,
            index_resource: None,
            members: Vec::new(),
            names: Vec::new(),
            scripts: Vec::new(),
        });
    }
    Ok(out)
}

fn parse_member(b: &[u8]) -> Result<Member> {
    let mut c = Cursor::new(b);
    let code = c.u32()?;
    let info_len = c.u32()? as usize;
    let specific_len = c.u32()? as usize;
    let info = c.take(info_len)?;
    let specific = c.take(specific_len)?.to_vec();
    let kind = MemberType::from_code(code);

    let (mut name, mut extra, mut script_id) = (String::new(), Vec::new(), 0);
    if info_len >= 20 {
        script_id = be_u32(info, 16).unwrap_or(0);
        let (_, items) = parse_list(info)?;
        if let Some(n) = items.get(1) {
            name = pascal_item(n);
        }
        for it in items.iter().skip(2).filter(|i| !i.is_empty()) {
            let s = pascal_item(it);
            if !s.is_empty() && s.chars().all(|ch| !ch.is_control() || ch == '\n') {
                extra.push(s);
            }
        }
    }
    let script_kind = (kind == MemberType::Script).then(|| match be_u16(&specific, 0) {
        Some(1) => ScriptKind::Score,
        Some(3) => ScriptKind::Movie,
        Some(7) => ScriptKind::Parent,
        Some(n) => ScriptKind::Other(n),
        None => ScriptKind::Other(0),
    });
    let xtra = (kind == MemberType::Xtra)
        .then(|| xtra_name(&specific))
        .flatten();
    Ok(Member {
        number: 0,
        resource: 0,
        kind,
        name,
        info: extra,
        script_id,
        script_kind,
        xtra,
        data: Vec::new(),
        specific,
    })
}

/// Xtra members' specific data starts with a u32 length and the Xtra's type name.
fn xtra_name(s: &[u8]) -> Option<String> {
    let n = be_u32(s, 0)? as usize;
    let raw = s.get(4..4 + n)?;
    Some(mac_roman(raw).trim_end_matches('\0').to_owned())
}

/// `LctX` → (`Lnam` names, `Lscr` id per script id).
fn read_script_context(file: &Container, ctx_id: u32) -> (Vec<String>, Vec<Option<u32>>) {
    let Some(ctx) = file.resource(ctx_id) else {
        return Default::default();
    };
    let count = be_u32(ctx, 8).unwrap_or(0) as usize;
    let entries_at = be_u16(ctx, 16).unwrap_or(0) as usize;
    let lnam = be_i32(ctx, 32).unwrap_or(-1);
    let scripts = (0..count)
        .map(|i| {
            be_i32(ctx, entries_at + i * 12 + 4)
                .filter(|&id| id > 0)
                .map(|id| id as u32)
        })
        .collect();
    let names = (lnam > 0)
        .then(|| file.resource(lnam as u32))
        .flatten()
        .map(parse_names)
        .unwrap_or_default();
    (names, scripts)
}

/// `Lnam`: u16 offset and u16 count at 16, then Pascal strings.
pub fn parse_names(b: &[u8]) -> Vec<String> {
    let (Some(off), Some(n)) = (be_u16(b, 16), be_u16(b, 18)) else {
        return Vec::new();
    };
    let mut out = Vec::with_capacity(n as usize);
    let mut p = off as usize;
    for _ in 0..n {
        let Some(&len) = b.get(p) else { break };
        let Some(s) = b.get(p + 1..p + 1 + len as usize) else {
            break;
        };
        out.push(mac_roman(s));
        p += 1 + len as usize;
    }
    out
}
