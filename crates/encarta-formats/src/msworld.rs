//! Atlas side files: `MSWORLD.SDB` (statistics), the Geography Quiz `Dn.TXT` / `DnPLACES.TXT`,
//! and the field layout of the `MSWORLD.ITR $WW$CTCATALOG` media records.
//! Spec: `docs/formats/msworld.md`.

use anyhow::{Context, Result, bail, ensure};
use serde::Serialize;

use crate::itir_catalog::CatalogEntry;

// ---------------------------------------------------------------- SDB

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Country {
    pub id: u32,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Category {
    pub bit: u32,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Qualifier {
    pub id: u32,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Unit {
    pub id: u32,
    pub unit: String,
    /// Alternate unit (`None` when the file says `N/A`); alternate value = value × factor + offset.
    pub alt_unit: Option<String>,
    pub factor: f64,
    pub offset: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Item {
    pub country_id: u32,
    pub value: f64,
    /// The value exactly as written.
    pub value_text: String,
    /// Digits after the decimal point.
    pub decimals: u8,
    /// Item column 4: meaning unknown.
    pub col_b: u32,
    pub year: u32,
    /// Item column 6: meaning unknown (possibly a year span).
    pub col_c: u32,
    /// `N/A` or `estimate`.
    pub note: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Indicator {
    pub id: u32,
    pub name: String,
    pub category_mask: u32,
    pub unit_id: u32,
    /// 6315xxxxx content id (probably the indicator's definition).
    pub content_id: u32,
    pub source: String,
    /// Sorted by value, descending, as on disc.
    pub items: Vec<Item>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Sdb {
    pub signature: String,
    pub revision: String,
    pub countries: Vec<Country>,
    pub categories: Vec<Category>,
    pub qualifiers: Vec<Qualifier>,
    pub units: Vec<Unit>,
    pub indicators: Vec<Indicator>,
}

impl Sdb {
    pub fn country(&self, id: u32) -> Option<&Country> {
        self.countries.iter().find(|c| c.id == id)
    }
    pub fn unit(&self, id: u32) -> Option<&Unit> {
        self.units.iter().find(|u| u.id == id)
    }
    pub fn category_names(&self, mask: u32) -> Vec<&str> {
        self.categories.iter().filter(|c| mask & c.bit != 0).map(|c| c.name.as_str()).collect()
    }
}

fn int<T: std::str::FromStr>(s: &str, what: &str) -> Result<T> {
    s.trim().parse().map_err(|_| anyhow::anyhow!("bad {what}: {s:?}"))
}

fn float(s: &str, what: &str) -> Result<f64> {
    s.trim().parse().map_err(|_| anyhow::anyhow!("bad {what}: {s:?}"))
}

fn cols<'a>(line: &'a str, n: usize, what: &str) -> Result<Vec<&'a str>> {
    let c: Vec<&str> = line.split('\t').collect();
    ensure!(c.len() == n, "{what} row has {} columns, expected {n}: {line:?}", c.len());
    Ok(c)
}

/// A section: its name and its non-blank lines.
fn sections(text: &str) -> Result<Vec<(&str, Vec<&str>)>> {
    let mut out: Vec<(&str, Vec<&str>)> = Vec::new();
    for line in text.lines() {
        let line = line.strip_suffix('\r').unwrap_or(line);
        if let Some(name) = line.strip_prefix("// ") {
            out.push((name.trim(), Vec::new()));
        } else if !line.trim().is_empty() {
            out.last_mut().context("data before the first section header")?.1.push(line);
        }
    }
    Ok(out)
}

/// Splits a counted section body (count line + rows) and checks the count.
fn counted<'a>(name: &str, body: &[&'a str]) -> Result<Vec<&'a str>> {
    let (n, rows) = body.split_first().with_context(|| format!("section {name} is empty"))?;
    let n: usize = int(n, "count")?;
    ensure!(n == rows.len(), "section {name}: count {n} but {} rows", rows.len());
    Ok(rows.to_vec())
}

/// Parses `MSWORLD.SDB` (Windows-1252 bytes).
pub fn parse_sdb(bytes: &[u8]) -> Result<Sdb> {
    let (text, _, _) = encoding_rs::WINDOWS_1252.decode(bytes);
    let secs = sections(&text)?;
    let mut it = secs.iter().peekable();
    let mut take = |want: &str| -> Result<&Vec<&str>> {
        let (name, body) = it.next().with_context(|| format!("missing section {want}"))?;
        ensure!(*name == want, "expected section {want:?}, found {name:?}");
        Ok(body)
    };
    let single = |b: &Vec<&str>, what: &str| -> Result<String> {
        ensure!(b.len() == 1, "section {what} should hold one line");
        Ok(b[0].to_string())
    };
    let signature = single(take("Signature")?, "Signature")?;
    ensure!(signature == "EVGSDB", "signature {signature:?}");
    let revision = single(take("Revision")?, "Revision")?;
    let countries = counted("Countries", take("Countries")?)?
        .iter()
        .map(|l| {
            let c = cols(l, 2, "country")?;
            Ok(Country { id: int(c[0], "country id")?, name: c[1].to_string() })
        })
        .collect::<Result<Vec<_>>>()?;
    let categories = counted("Indicator Category List", take("Indicator Category List")?)?
        .iter()
        .map(|l| {
            let c = cols(l, 2, "category")?;
            Ok(Category { bit: int(c[0], "category bit")?, name: c[1].to_string() })
        })
        .collect::<Result<Vec<_>>>()?;
    let qualifiers = counted("Qualifier List", take("Qualifier List")?)?
        .iter()
        .map(|l| {
            let c = cols(l, 2, "qualifier")?;
            Ok(Qualifier { id: int(c[0], "qualifier id")?, text: c[1].to_string() })
        })
        .collect::<Result<Vec<_>>>()?;
    let units = counted("Unit List", take("Unit List")?)?
        .iter()
        .map(|l| {
            let c = cols(l, 5, "unit")?;
            Ok(Unit {
                id: int(c[0], "unit id")?,
                unit: c[1].to_string(),
                alt_unit: (c[2] != "N/A").then(|| c[2].to_string()),
                factor: float(c[3], "unit factor")?,
                offset: float(c[4], "unit offset")?,
            })
        })
        .collect::<Result<Vec<_>>>()?;

    let mut indicators = Vec::new();
    let mut declared: Option<usize> = None;
    while let Some((name, body)) = it.next() {
        ensure!(name.ends_with("Indicator Package List"), "unexpected section {name:?}");
        let row = match body.as_slice() {
            [count, row] if declared.is_none() => {
                declared = Some(int(count, "indicator count")?);
                *row
            }
            [row] => *row,
            _ => bail!("indicator package section with {} lines", body.len()),
        };
        let c = cols(row, 6, "indicator")?;
        let (iname, ibody) = it.next().context("indicator without items section")?;
        ensure!(*iname == "Items For The Above Indicator List", "expected items section, found {iname:?}");
        let items = counted(iname, ibody)?
            .iter()
            .map(|l| {
                let c = cols(l, 7, "item")?;
                Ok(Item {
                    country_id: int(c[0], "item country")?,
                    value: float(c[1], "item value")?,
                    value_text: c[1].to_string(),
                    decimals: int(c[2], "item decimals")?,
                    col_b: int(c[3], "item column 4")?,
                    year: int(c[4], "item year")?,
                    col_c: int(c[5], "item column 6")?,
                    note: c[6].to_string(),
                })
            })
            .collect::<Result<Vec<_>>>()?;
        indicators.push(Indicator {
            id: int(c[0], "indicator id")?,
            name: c[1].to_string(),
            category_mask: int(c[2], "category mask")?,
            unit_id: int(c[3], "indicator unit")?,
            content_id: int(c[4], "indicator content id")?,
            source: c[5].to_string(),
            items,
        });
    }
    if let Some(n) = declared {
        ensure!(n == indicators.len(), "{n} indicators declared, {} found", indicators.len());
    }
    Ok(Sdb { signature, revision, countries, categories, qualifiers, units, indicators })
}

// ---------------------------------------------------------------- Geography Quiz

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct QuizSettings {
    pub questions: u32,
    /// Points for a correct answer at attempt 1, 2, 3, 4, and later.
    pub points: Vec<u32>,
    /// One value per place type; meaning unknown.
    pub per_type_unknown: Vec<u32>,
    /// Share of questions of place type 0, 1, 2.
    pub type_mix: Vec<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct QuizPlace {
    /// 0 = country, 1 = city or other place, 2 = continent.
    pub kind: u32,
    /// Containing region (atlas place id).
    pub group: u32,
    /// The place asked for (atlas place id; for kind 0 an SDB country id).
    pub entity: u32,
    /// Fourth column, always 0.
    pub extra: u32,
}

fn fields(line: &str) -> Vec<&str> {
    line.split_whitespace().collect()
}

pub fn parse_quiz_settings(bytes: &[u8]) -> Result<QuizSettings> {
    let text = std::str::from_utf8(bytes).context("quiz settings are not ASCII")?;
    let lines: Vec<&str> = text.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
    ensure!(lines.len() == 4, "quiz settings have {} lines, expected 4", lines.len());
    let ints = |l: &str, n: usize| -> Result<Vec<u32>> {
        let f = fields(l);
        ensure!(f.len() == n, "expected {n} numbers in {l:?}");
        f.iter().map(|x| int(x, "quiz number")).collect()
    };
    let questions = ints(lines[0], 1)?[0];
    let points = ints(lines[1], 5)?;
    let per_type_unknown = ints(lines[2], 3)?;
    let mix = fields(lines[3]);
    ensure!(mix.len() == 3, "expected 3 shares in {:?}", lines[3]);
    let type_mix = mix.iter().map(|x| float(x, "quiz share")).collect::<Result<Vec<_>>>()?;
    Ok(QuizSettings { questions, points, per_type_unknown, type_mix })
}

pub fn parse_quiz_places(bytes: &[u8]) -> Result<Vec<QuizPlace>> {
    let text = std::str::from_utf8(bytes).context("quiz places are not ASCII")?;
    text.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            let f = fields(l);
            ensure!(f.len() == 4, "quiz place row {l:?} has {} fields", f.len());
            Ok(QuizPlace {
                kind: int(f[0], "place type")?,
                group: int(f[1], "place group")?,
                entity: int(f[2], "place entity")?,
                extra: int(f[3], "place extra")?,
            })
        })
        .collect()
}

// ---------------------------------------------------------------- media catalog records

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MediaRecord {
    /// Catalog key: the media content id (631…).
    pub key: u32,
    pub title: String,
    /// `i` image, `w` sound, `m` animal image.
    pub kind: String,
    /// `L`: atlas place id.
    pub place_id: Option<u32>,
    /// `B`: main file name.
    pub file: Option<String>,
    /// `P`: thumbnail name.
    pub thumbnail: Option<String>,
    /// `U`: paired catalog item (None when 0).
    pub paired: Option<u32>,
    /// `M`: legacy media id.
    pub legacy_id: Option<u32>,
    /// `C`: credit line.
    pub credit: Option<String>,
    /// Fields with other letters, kept verbatim.
    pub other: Vec<String>,
}

/// Splits a media record string (`i;L36640;B14632.jpg;…;CPaul A. Souders/Corbis`).
pub fn parse_media_fields(key: u32, record: &str, title: &str) -> Result<MediaRecord> {
    let (kind, rest) = record.split_once(';').with_context(|| format!("media {key}: no type"))?;
    let mut m = MediaRecord {
        key,
        title: title.to_string(),
        kind: kind.to_string(),
        place_id: None,
        file: None,
        thumbnail: None,
        paired: None,
        legacy_id: None,
        credit: None,
        other: Vec::new(),
    };
    let mut rest = rest;
    while !rest.is_empty() {
        if let Some(credit) = rest.strip_prefix('C') {
            // The credit runs to the end, or up to a trailing ";U<digits>".
            let (credit, tail) = match credit.rfind(";U") {
                Some(i) if credit[i + 2..].bytes().all(|b| b.is_ascii_digit()) && i + 2 < credit.len() => {
                    (&credit[..i], &credit[i + 1..])
                }
                _ => (credit, ""),
            };
            m.credit = Some(credit.to_string());
            rest = tail;
            continue;
        }
        let (field, tail) = rest.split_once(';').unwrap_or((rest, ""));
        rest = tail;
        let mut chars = field.chars();
        let letter = chars.next().with_context(|| format!("media {key}: empty field"))?;
        let value = chars.as_str();
        let num = |v: &str| int::<u32>(v, &format!("media {key} field {letter}"));
        match letter {
            'L' => m.place_id = Some(num(value)?),
            'B' => m.file = Some(value.to_string()),
            'P' => m.thumbnail = Some(value.to_string()),
            'U' => m.paired = Some(num(value)?).filter(|&u| u != 0),
            'M' => m.legacy_id = Some(num(value)?),
            _ => m.other.push(field.to_string()),
        }
    }
    Ok(m)
}

/// Interprets scanned `MSWORLD.ITR $WW$CTCATALOG` entries (property 0 = record, 1 = title).
pub fn media_records(entries: &[CatalogEntry]) -> Result<Vec<MediaRecord>> {
    entries
        .iter()
        .map(|e| {
            ensure!(e.properties.len() == 2, "media {}: {} properties, expected 2", e.key, e.properties.len());
            parse_media_fields(e.key, &e.properties[0], &e.properties[1])
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SDB: &str = "// Signature\r\n\r\nEVGSDB\r\n\r\n// Revision\r\n\r\n1\r\n\r\n// Countries\r\n\r\n2\r\n\r\n\
3\tLand A\r\n4\tC\u{f4}te B\r\n\r\n// Indicator Category List\r\n\r\n2\r\n\r\n1\tBasic\r\n2\tFarming\r\n\r\n\
// Qualifier List\r\n\r\n1\r\n\r\n2\tN/A\r\n\r\n// Unit List\r\n\r\n2\r\n\r\n0\tacres\tN/A\t0\t0\r\n2\t\u{b0}C\t\u{b0}F\t1.8\t17.777777778\r\n\r\n\
// \u{b6} Indicator Package List\r\n\r\n2\r\n\r\n7\tHeat\t3\t2\t631000001\tSomeone\r\n\r\n\
// Items For The Above Indicator List\r\n\r\n2\r\n3\t30.5\t1\t8\t2006\t0\tN/A\r\n4\t-1.25\t2\t8\t2005\t15\testimate\r\n\r\n\
// \u{b6} Indicator Package List\r\n\r\n8\tCold\t1\t0\t631000002\tOther\r\n\r\n\
// Items For The Above Indicator List\r\n\r\n0\r\n";

    fn cp1252(s: &str) -> Vec<u8> {
        encoding_rs::WINDOWS_1252.encode(s).0.into_owned()
    }

    #[test]
    fn parses_sdb() {
        let sdb = parse_sdb(&cp1252(SDB)).unwrap();
        assert_eq!(sdb.revision, "1");
        assert_eq!(sdb.country(4).unwrap().name, "C\u{f4}te B");
        assert_eq!(sdb.unit(2).unwrap().alt_unit.as_deref(), Some("\u{b0}F"));
        assert_eq!(sdb.unit(0).unwrap().alt_unit, None);
        assert_eq!(sdb.indicators.len(), 2);
        let heat = &sdb.indicators[0];
        assert_eq!(sdb.category_names(heat.category_mask), vec!["Basic", "Farming"]);
        assert_eq!(heat.items[1].value, -1.25);
        assert_eq!(heat.items[1].value_text, "-1.25");
        assert_eq!(heat.items[1].col_c, 15);
        assert!(sdb.indicators[1].items.is_empty());
    }

    #[test]
    fn sdb_count_mismatch_fails() {
        let bad = SDB.replace("// Countries\r\n\r\n2", "// Countries\r\n\r\n3");
        assert!(parse_sdb(&cp1252(&bad)).is_err());
    }

    #[test]
    fn parses_quiz() {
        let s = parse_quiz_settings(b"20\r\n5\t4\t3\t2\t0\r\n0\t0\t0\r\n.70\t.20\t.10\r\n").unwrap();
        assert_eq!(s.questions, 20);
        assert_eq!(s.points, vec![5, 4, 3, 2, 0]);
        assert_eq!(s.type_mix, vec![0.7, 0.2, 0.1]);
        let p = parse_quiz_places(b"0\t100\t12\t0\r\n2\t200\t742\t0\r\n").unwrap();
        assert_eq!(p[1], QuizPlace { kind: 2, group: 200, entity: 742, extra: 0 });
    }

    #[test]
    fn parses_media_fields() {
        let m = parse_media_fields(5, "i;L36;B1.jpg;P1.JSM;U0;M631;CA. Person/Agency", "T").unwrap();
        assert_eq!(m.place_id, Some(36));
        assert_eq!(m.paired, None);
        assert_eq!(m.legacy_id, Some(631));
        assert_eq!(m.credit.as_deref(), Some("A. Person/Agency"));
        let w = parse_media_fields(6, "w;L9;BT1A.wav;C\"Song\" (c) X; Y. All rights reserved.;U631", "S").unwrap();
        assert_eq!(w.credit.as_deref(), Some("\"Song\" (c) X; Y. All rights reserved."));
        assert_eq!(w.paired, Some(631));
        assert_eq!(w.file.as_deref(), Some("T1A.wav"));
    }
}
