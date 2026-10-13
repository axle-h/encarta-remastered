//! Reader for the content catalog (`CATALOG.DLX/catalog2`, `CATALOG.KDC/catalog2`).
//!
//! Spec: `docs/formats/search-catalog-misc.md` §3. The catalog maps every content
//! id (`refId`) and media file name to a `(package, local id)` location:
//!
//! * `guid.lst` gives each package slot a GUID, and the `[Packages]` section of
//!   `DELUXE.INI` / `KIDSCD.INI` gives each GUID a file name (`ContStd.akc`).
//! * `content.ecn`, `data.ecn` and `relart.ecn` are numeric-key tables.
//! * `baggage.ecs` and `generic.ecs` are string-key tables.

use anyhow::{Context, Result, bail, ensure};
use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// A decoded catalog location: package slot plus record number inside it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
pub struct Location {
    pub package: u8,
    pub local_id: u32,
}

impl Location {
    /// Decodes the packed u32: byte 0 = package, byte 1 = local-id bits 16..24,
    /// bytes 2..4 (u16) = local-id bits 0..16.
    pub fn from_packed(v: u32) -> Self {
        let b = v.to_le_bytes();
        Location {
            package: b[0],
            local_id: (b[1] as u32) << 16 | u16::from_le_bytes([b[2], b[3]]) as u32,
        }
    }

    pub fn to_packed(self) -> u32 {
        assert!(self.local_id < 1 << 24, "local id does not fit in 24 bits");
        let lo = (self.local_id & 0xFFFF) as u16;
        u32::from_le_bytes([
            self.package,
            (self.local_id >> 16) as u8,
            lo as u8,
            (lo >> 8) as u8,
        ])
    }
}

fn u32_at(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes(b[o..o + 4].try_into().unwrap())
}

/// Reads the trailing count and checks the 4-byte magic.
fn table_count(b: &[u8], magic: &[u8; 4]) -> Result<usize> {
    ensure!(b.len() >= 8, "catalog table too short ({} bytes)", b.len());
    ensure!(
        &b[..4] == magic,
        "bad magic {:02x?}, expected {:?}",
        &b[..4],
        magic
    );
    Ok(u32_at(b, b.len() - 4) as usize)
}

/// Numeric-key table (`ECN\0`): parallel arrays of locations and ascending refIds.
#[derive(Debug, Clone, Default)]
pub struct Ecn {
    ids: Vec<u32>,
    locations: Vec<Location>,
}

impl Ecn {
    pub fn parse(b: &[u8]) -> Result<Self> {
        let n = table_count(b, b"ECN\0")?;
        ensure!(
            b.len() == 8 + 8 * n,
            "ECN size {} does not match count {n}",
            b.len()
        );
        let locations = (0..n)
            .map(|i| Location::from_packed(u32_at(b, 4 + 4 * i)))
            .collect();
        let ids: Vec<u32> = (0..n).map(|i| u32_at(b, 4 + 4 * n + 4 * i)).collect();
        ensure!(
            ids.windows(2).all(|w| w[0] < w[1]),
            "ECN ids are not strictly ascending"
        );
        Ok(Ecn { ids, locations })
    }

    pub fn len(&self) -> usize {
        self.ids.len()
    }

    pub fn is_empty(&self) -> bool {
        self.ids.is_empty()
    }

    pub fn get(&self, ref_id: u32) -> Option<Location> {
        self.ids
            .binary_search(&ref_id)
            .ok()
            .map(|i| self.locations[i])
    }

    pub fn iter(&self) -> impl Iterator<Item = (u32, Location)> + '_ {
        self.ids.iter().copied().zip(self.locations.iter().copied())
    }
}

/// String-key table (`ECS\0`): locations plus fixed-width, space-padded UTF-16 keys.
#[derive(Debug, Clone, Default)]
pub struct Ecs {
    /// Key width in UTF-16 code units.
    pub width: usize,
    keys: Vec<String>,
    locations: Vec<Location>,
}

impl Ecs {
    pub fn parse(b: &[u8]) -> Result<Self> {
        let n = table_count(b, b"ECS\0")?;
        if n == 0 {
            ensure!(b.len() == 8, "empty ECS has trailing bytes");
            return Ok(Ecs::default());
        }
        let key_bytes = b
            .len()
            .checked_sub(8 + 4 * n)
            .context("ECS shorter than its count")?;
        ensure!(
            key_bytes.is_multiple_of(n),
            "ECS key area {key_bytes} not a multiple of count {n}"
        );
        let width_bytes = key_bytes / n;
        ensure!(
            width_bytes.is_multiple_of(2) && width_bytes > 0,
            "odd ECS key width {width_bytes}"
        );
        let locations = (0..n)
            .map(|i| Location::from_packed(u32_at(b, 4 + 4 * i)))
            .collect();
        let base = 4 + 4 * n;
        let mut keys = Vec::with_capacity(n);
        for i in 0..n {
            let raw = &b[base + i * width_bytes..base + (i + 1) * width_bytes];
            let units: Vec<u16> = raw
                .as_chunks::<2>()
                .0
                .iter()
                .map(|c| u16::from_le_bytes(*c))
                .collect();
            let s = String::from_utf16(&units).context("ECS key is not valid UTF-16")?;
            keys.push(s.trim_end_matches(' ').to_string());
        }
        ensure!(
            keys.windows(2).all(|w| w[0] < w[1]),
            "ECS keys are not strictly ascending"
        );
        Ok(Ecs {
            width: width_bytes / 2,
            keys,
            locations,
        })
    }

    pub fn len(&self) -> usize {
        self.keys.len()
    }

    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }

    /// Looks a key up. All keys on the disc are lower-case ASCII, so the query is lower-cased.
    pub fn get(&self, key: &str) -> Option<Location> {
        let k = key.trim_end_matches(' ').to_lowercase();
        self.keys.binary_search(&k).ok().map(|i| self.locations[i])
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, Location)> + '_ {
        self.keys
            .iter()
            .map(String::as_str)
            .zip(self.locations.iter().copied())
    }
}

/// Parses `guid.lst`: UTF-16LE (no BOM), CRLF-separated, one slot per line.
/// Slot index = 0-based line number; blank lines are unused slots.
pub fn parse_guid_list(b: &[u8]) -> Result<Vec<Option<String>>> {
    ensure!(b.len().is_multiple_of(2), "guid.lst has odd length");
    let mut units: Vec<u16> = b
        .as_chunks::<2>()
        .0
        .iter()
        .map(|c| u16::from_le_bytes(*c))
        .collect();
    if units.first() == Some(&0xFEFF) {
        units.remove(0);
    }
    let text = String::from_utf16(&units).context("guid.lst is not valid UTF-16")?;
    let mut lines: Vec<&str> = text.split("\r\n").collect();
    if lines.last() == Some(&"") {
        lines.pop(); // the final CRLF terminates the last line
    }
    Ok(lines
        .into_iter()
        .map(|l| {
            let l = l.trim();
            (!l.is_empty()).then(|| l.to_ascii_uppercase())
        })
        .collect())
}

/// Parses the `[Packages]` section of `DELUXE.INI` / `KIDSCD.INI`: `File.ext={GUID}`.
/// Returns upper-case GUID → file name.
pub fn parse_packages_ini(text: &str) -> HashMap<String, String> {
    let mut out = HashMap::new();
    let mut in_section = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_section = line.eq_ignore_ascii_case("[Packages]");
            continue;
        }
        if !in_section {
            continue;
        }
        if let Some((name, guid)) = line.split_once('=') {
            let guid = guid.trim();
            if guid.starts_with('{') {
                out.insert(guid.to_ascii_uppercase(), name.trim().to_string());
            }
        }
    }
    out
}

#[derive(Debug, Clone, Serialize)]
pub struct Package {
    pub index: u8,
    pub guid: String,
    /// File name from the INI (`ContStd.akc`), if the GUID is listed there.
    pub file: Option<String>,
}

/// Which table a location came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Table {
    Content,
    Data,
    Relart,
    Baggage,
    Generic,
}

impl Table {
    pub const ALL: [Table; 5] = [
        Table::Content,
        Table::Data,
        Table::Relart,
        Table::Baggage,
        Table::Generic,
    ];

    pub fn file_name(self) -> &'static str {
        match self {
            Table::Content => "content.ecn",
            Table::Data => "data.ecn",
            Table::Relart => "relart.ecn",
            Table::Baggage => "baggage.ecs",
            Table::Generic => "generic.ecs",
        }
    }
}

/// A whole catalog: package slots plus the five tables.
pub struct Catalog {
    pub packages: Vec<Option<Package>>,
    pub content: Ecn,
    pub data: Ecn,
    pub relart: Ecn,
    pub baggage: Ecs,
    pub generic: Ecs,
}

/// Where one id resolves to in one table.
#[derive(Debug, Clone, Serialize)]
pub struct Resolved {
    pub table: Table,
    pub package: u8,
    pub file: Option<String>,
    pub local_id: u32,
}

impl Catalog {
    /// Builds a catalog from the raw bytes of `guid.lst`, the INI text and the five tables.
    pub fn from_parts(guid_lst: &[u8], ini: &str, tables: [&[u8]; 5]) -> Result<Self> {
        let names = parse_packages_ini(ini);
        let slots = parse_guid_list(guid_lst)?;
        ensure!(
            slots.len() <= 256,
            "guid.lst has {} slots, more than a u8 index",
            slots.len()
        );
        let packages = slots
            .into_iter()
            .enumerate()
            .map(|(i, g)| {
                g.map(|guid| Package {
                    index: i as u8,
                    file: names.get(&guid).cloned(),
                    guid,
                })
            })
            .collect();
        let [c, d, r, b, g] = tables;
        Ok(Catalog {
            packages,
            content: Ecn::parse(c).context("content.ecn")?,
            data: Ecn::parse(d).context("data.ecn")?,
            relart: Ecn::parse(r).context("relart.ecn")?,
            baggage: Ecs::parse(b).context("baggage.ecs")?,
            generic: Ecs::parse(g).context("generic.ecs")?,
        })
    }

    /// Opens `catalog2_dir` (the unpacked `catalog2/` folder) with the given INI file.
    pub fn open(catalog2_dir: &Path, ini_path: &Path) -> Result<Self> {
        let read = |name: &str| {
            let p = catalog2_dir.join(name);
            std::fs::read(&p).with_context(|| format!("reading {}", p.display()))
        };
        let ini_bytes =
            std::fs::read(ini_path).with_context(|| format!("reading {}", ini_path.display()))?;
        let ini: String = ini_bytes.iter().map(|&c| c as char).collect(); // ASCII / Latin-1
        let t: Vec<Vec<u8>> = Table::ALL
            .iter()
            .map(|t| read(t.file_name()))
            .collect::<Result<_>>()?;
        Self::from_parts(
            &read("guid.lst")?,
            &ini,
            [&t[0], &t[1], &t[2], &t[3], &t[4]],
        )
    }

    pub fn package(&self, index: u8) -> Option<&Package> {
        self.packages.get(index as usize).and_then(|p| p.as_ref())
    }

    pub fn package_file(&self, index: u8) -> Option<&str> {
        self.package(index).and_then(|p| p.file.as_deref())
    }

    fn resolved(&self, table: Table, loc: Location) -> Resolved {
        Resolved {
            table,
            package: loc.package,
            file: self.package_file(loc.package).map(str::to_string),
            local_id: loc.local_id,
        }
    }

    /// Every numeric-table location of a refId (content, data, relart).
    pub fn resolve_id(&self, ref_id: u32) -> Vec<Resolved> {
        [
            (Table::Content, &self.content),
            (Table::Data, &self.data),
            (Table::Relart, &self.relart),
        ]
        .into_iter()
        .filter_map(|(t, ecn)| ecn.get(ref_id).map(|l| self.resolved(t, l)))
        .collect()
    }

    /// Every string-table location of a key (baggage, generic).
    pub fn resolve_key(&self, key: &str) -> Vec<Resolved> {
        [
            (Table::Baggage, &self.baggage),
            (Table::Generic, &self.generic),
        ]
        .into_iter()
        .filter_map(|(t, ecs)| ecs.get(key).map(|l| self.resolved(t, l)))
        .collect()
    }

    pub fn ecn(&self, t: Table) -> Option<&Ecn> {
        match t {
            Table::Content => Some(&self.content),
            Table::Data => Some(&self.data),
            Table::Relart => Some(&self.relart),
            _ => None,
        }
    }

    pub fn ecs(&self, t: Table) -> Option<&Ecs> {
        match t {
            Table::Baggage => Some(&self.baggage),
            Table::Generic => Some(&self.generic),
            _ => None,
        }
    }

    /// All `(table, location)` pairs, for validation.
    pub fn all_locations(&self) -> impl Iterator<Item = (Table, Location)> + '_ {
        let n = [Table::Content, Table::Data, Table::Relart]
            .into_iter()
            .flat_map(move |t| self.ecn(t).unwrap().iter().map(move |(_, l)| (t, l)));
        let s = [Table::Baggage, Table::Generic]
            .into_iter()
            .flat_map(move |t| self.ecs(t).unwrap().iter().map(move |(_, l)| (t, l)));
        n.chain(s)
    }
}

/// Known catalog editions on the disc and where their files are.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Edition {
    Deluxe,
    Kids,
}

pub struct EditionPaths {
    pub catalog2: PathBuf,
    pub ini: PathBuf,
    /// Directories searched (in order) for package files.
    pub package_dirs: Vec<PathBuf>,
}

impl Edition {
    /// Paths under the extracted tree (`extracted/iso`, `extracted/eit`).
    pub fn paths(self, extracted: &Path) -> EditionPaths {
        let iso = extracted.join("iso");
        let eit = extracted.join("eit");
        match self {
            Edition::Deluxe => EditionPaths {
                catalog2: eit.join("EE/ENCARTA/CATALOG.DLX/catalog2"),
                ini: iso.join("EE/ENCARTA/DELUXE.INI"),
                package_dirs: vec![iso.join("EE/ENCARTA"), iso.join("CONTENT")],
            },
            Edition::Kids => EditionPaths {
                catalog2: eit.join("EE/KIDS/CATALOG.KDC/catalog2"),
                ini: iso.join("EE/KIDS/KIDSCD.INI"),
                package_dirs: vec![iso.join("EE/KIDS"), iso.join("EE/KIDS/CONTENT")],
            },
        }
    }
}

/// Finds `name` in `dirs`, ignoring ASCII case (the disc uses upper-case 8.3 names).
pub fn find_file_ci(dirs: &[PathBuf], name: &str) -> Option<PathBuf> {
    for d in dirs {
        let Ok(rd) = std::fs::read_dir(d) else {
            continue;
        };
        for e in rd.flatten() {
            if e.file_name().to_string_lossy().eq_ignore_ascii_case(name) {
                return Some(e.path());
            }
        }
    }
    None
}

/// Per-(table, package) summary used by the `check` command and integration tests.
#[derive(Debug, Clone, Serialize)]
pub struct PackageUse {
    pub table: Table,
    pub package: u8,
    pub file: Option<String>,
    pub count: usize,
    pub max_local_id: u32,
    pub distinct_local_ids: usize,
}

impl Catalog {
    pub fn package_usage(&self) -> Vec<PackageUse> {
        let mut map: HashMap<(u8, u8), (usize, u32, std::collections::HashSet<u32>)> =
            HashMap::new();
        let mut order: Vec<(Table, u8)> = Vec::new();
        for (t, l) in self.all_locations() {
            let key = (t as u8, l.package);
            let e = map.entry(key).or_insert_with(|| {
                order.push((t, l.package));
                (0, 0, Default::default())
            });
            e.0 += 1;
            e.1 = e.1.max(l.local_id);
            e.2.insert(l.local_id);
        }
        order.sort_by_key(|&(t, p)| (t as u8, p));
        order
            .into_iter()
            .map(|(t, p)| {
                let (count, max, set) = &map[&(t as u8, p)];
                PackageUse {
                    table: t,
                    package: p,
                    file: self.package_file(p).map(str::to_string),
                    count: *count,
                    max_local_id: *max,
                    distinct_local_ids: set.len(),
                }
            })
            .collect()
    }

    /// Per package, over all tables: `(package, entries, max local id + 1, distinct local ids)`.
    /// On the disc every package is dense: the three numbers are equal.
    pub fn package_extents(&self) -> Vec<(u8, usize, u32, usize)> {
        let mut per: std::collections::BTreeMap<u8, Vec<u32>> = Default::default();
        for (_, l) in self.all_locations() {
            per.entry(l.package).or_default().push(l.local_id);
        }
        per.into_iter()
            .map(|(p, mut ids)| {
                let n = ids.len();
                let bound = ids.iter().max().map_or(0, |m| m + 1);
                ids.sort_unstable();
                ids.dedup();
                (p, n, bound, ids.len())
            })
            .collect()
    }

    /// Structural checks that need nothing but the catalog. Returns problems found.
    pub fn validate(&self) -> Vec<String> {
        let mut problems = Vec::new();
        for (t, l) in self.all_locations() {
            match self.package(l.package) {
                None => problems.push(format!(
                    "{t:?}: package slot {} is blank or out of range",
                    l.package
                )),
                Some(p) if p.file.is_none() => problems.push(format!(
                    "{t:?}: package {} GUID {} not in INI",
                    l.package, p.guid
                )),
                _ => {}
            }
        }
        problems.sort();
        problems.dedup();
        problems
    }
}

/// Path of a string-table key inside its EIT package: baggage keys live under `baggage/`,
/// generic keys are already paths (`dswmedia/…`). Matching inside the EIT ignores case.
pub fn member_path(table: Table, key: &str) -> String {
    match table {
        Table::Baggage => format!("baggage/{key}"),
        _ => key.to_string(),
    }
}

/// Fails unless every package used by the catalog exists on disc.
pub fn check_package_files(cat: &Catalog, dirs: &[PathBuf]) -> Result<Vec<(u8, String, PathBuf)>> {
    let mut used: Vec<u8> = cat.all_locations().map(|(_, l)| l.package).collect();
    used.sort();
    used.dedup();
    let mut out = Vec::new();
    for p in used {
        let Some(name) = cat.package_file(p) else {
            bail!("package {p} has no file name")
        };
        let Some(path) = find_file_ci(dirs, name) else {
            bail!("package {p} file {name} not found on disc")
        };
        out.push((p, name.to_string(), path));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn utf16(s: &str) -> Vec<u8> {
        s.encode_utf16().flat_map(|u| u.to_le_bytes()).collect()
    }

    fn ecn_bytes(entries: &[(u32, Location)]) -> Vec<u8> {
        let mut b = b"ECN\0".to_vec();
        for (_, l) in entries {
            b.extend(l.to_packed().to_le_bytes());
        }
        for (id, _) in entries {
            b.extend(id.to_le_bytes());
        }
        b.extend((entries.len() as u32).to_le_bytes());
        b
    }

    fn ecs_bytes(entries: &[(&str, Location)], width: usize) -> Vec<u8> {
        let mut b = b"ECS\0".to_vec();
        for (_, l) in entries {
            b.extend(l.to_packed().to_le_bytes());
        }
        for (k, _) in entries {
            b.extend(utf16(&format!("{k:<width$}")));
        }
        b.extend((entries.len() as u32).to_le_bytes());
        b
    }

    #[test]
    fn packed_location_layout() {
        // package 0x0A, local id 0x01_3FB1 → bytes 0A 01 B1 3F
        let l = Location::from_packed(u32::from_le_bytes([0x0A, 0x01, 0xB1, 0x3F]));
        assert_eq!(
            l,
            Location {
                package: 10,
                local_id: 0x013FB1
            }
        );
        assert_eq!(l.to_packed().to_le_bytes(), [0x0A, 0x01, 0xB1, 0x3F]);
    }

    #[test]
    fn ecn_lookup_and_iteration() {
        let e = [
            (
                5,
                Location {
                    package: 3,
                    local_id: 0,
                },
            ),
            (
                70_000_000,
                Location {
                    package: 4,
                    local_id: 200_000,
                },
            ),
            (
                761_000_001,
                Location {
                    package: 3,
                    local_id: 1,
                },
            ),
        ];
        let ecn = Ecn::parse(&ecn_bytes(&e)).unwrap();
        assert_eq!(ecn.len(), 3);
        assert_eq!(ecn.get(70_000_000), Some(e[1].1));
        assert_eq!(ecn.get(6), None);
        assert_eq!(ecn.iter().collect::<Vec<_>>(), e.to_vec());
    }

    #[test]
    fn ecn_rejects_bad_input() {
        let e = [
            (
                9,
                Location {
                    package: 1,
                    local_id: 0,
                },
            ),
            (
                3,
                Location {
                    package: 1,
                    local_id: 1,
                },
            ),
        ];
        assert!(Ecn::parse(&ecn_bytes(&e)).is_err(), "unsorted ids");
        let mut b = ecn_bytes(&e[..1]);
        b[0] = b'X';
        assert!(Ecn::parse(&b).is_err(), "bad magic");
        let mut b = ecn_bytes(&e[..1]);
        b.insert(4, 0);
        assert!(Ecn::parse(&b).is_err(), "size mismatch");
    }

    #[test]
    fn ecs_lookup() {
        let e = [
            (
                "a.gif",
                Location {
                    package: 1,
                    local_id: 7,
                },
            ),
            (
                "dir/b.swf",
                Location {
                    package: 2,
                    local_id: 0,
                },
            ),
            (
                "zz.wma",
                Location {
                    package: 1,
                    local_id: 8,
                },
            ),
        ];
        let ecs = Ecs::parse(&ecs_bytes(&e, 12)).unwrap();
        assert_eq!(ecs.width, 12);
        assert_eq!(ecs.get("dir/b.swf"), Some(e[1].1));
        assert_eq!(ecs.get("DIR/B.SWF"), Some(e[1].1));
        assert_eq!(ecs.get("nope"), None);
        assert_eq!(
            ecs.iter().map(|(k, _)| k).collect::<Vec<_>>(),
            ["a.gif", "dir/b.swf", "zz.wma"]
        );
        assert_eq!(Ecs::parse(&ecs_bytes(&[], 4)).unwrap().len(), 0);
    }

    #[test]
    fn guid_list_and_ini() {
        let lst = utf16("{AAAA}\r\n\r\n{bbbb}\r\n");
        let slots = parse_guid_list(&lst).unwrap();
        assert_eq!(
            slots,
            vec![Some("{AAAA}".into()), None, Some("{BBBB}".into())]
        );
        let ini = "[Packages]\r\nOne.akc={aaaa}\r\nTwo.eit={BBBB}\r\n\r\n[.AKC]\r\n One =0\r\nX={CCCC}\r\n";
        let m = parse_packages_ini(ini);
        assert_eq!(m.len(), 2);
        assert_eq!(m["{AAAA}"], "One.akc");
        assert!(!m.contains_key("{CCCC}"));
    }

    #[test]
    fn catalog_resolution() {
        let lst = utf16("\r\n{G1}\r\n{G2}\r\n{G3}\r\n");
        let ini = "[Packages]\nCont.akc={G1}\nData.akc={G2}\nThumb.eit={G3}\n";
        let content = ecn_bytes(&[(
            42,
            Location {
                package: 1,
                local_id: 3,
            },
        )]);
        let data = ecn_bytes(&[(
            42,
            Location {
                package: 2,
                local_id: 70_000,
            },
        )]);
        let relart = ecn_bytes(&[]);
        let baggage = ecs_bytes(
            &[(
                "x.jtn",
                Location {
                    package: 3,
                    local_id: 0,
                },
            )],
            17,
        );
        let generic = ecs_bytes(&[], 72);
        let cat =
            Catalog::from_parts(&lst, ini, [&content, &data, &relart, &baggage, &generic]).unwrap();
        let r = cat.resolve_id(42);
        assert_eq!(r.len(), 2);
        assert_eq!(
            (r[0].table, r[0].file.as_deref(), r[0].local_id),
            (Table::Content, Some("Cont.akc"), 3)
        );
        assert_eq!(
            (r[1].table, r[1].file.as_deref(), r[1].local_id),
            (Table::Data, Some("Data.akc"), 70_000)
        );
        assert_eq!(
            cat.resolve_key("x.jtn")[0].file.as_deref(),
            Some("Thumb.eit")
        );
        assert_eq!(member_path(Table::Baggage, "x.jtn"), "baggage/x.jtn");
        assert_eq!(
            member_path(Table::Generic, "dswmedia/a.swf"),
            "dswmedia/a.swf"
        );
        assert!(cat.validate().is_empty());
        assert!(cat.package(0).is_none());
        let usage = cat.package_usage();
        assert_eq!(usage.len(), 3);
        assert_eq!(
            cat.package_extents(),
            vec![(1, 1, 4, 1), (2, 1, 70_001, 1), (3, 1, 1, 1)]
        );
    }
}
