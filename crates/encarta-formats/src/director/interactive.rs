//! Converters for the two live Director interactives: City Climate (`ia/clim_CCL`) and Population
//! Chart (`ia/pop_CPO`). Both keep their data as Lingo literals in field members; we turn that into
//! our own JSON (shapes documented in `docs/formats/shockwave.md` §7).

use super::cast::{DirectorFile, Member, MemberType};
use super::extract::text_xtra_plain;
use super::locate::find_ci;
use super::proplist::{self, Lingo};
use super::sound;
use super::text::field_text;
use anyhow::{Context, Result, bail, ensure};
use serde::Serialize;
use serde_json::{Map, Number, Value as Json};
use std::collections::BTreeMap;
use std::path::Path;

/// The two unit systems, in the order the data lists them.
pub const UNIT_SYSTEMS: [&str; 2] = ["imperial", "metric"];

fn open(dir: &Path, rel: &str) -> Result<DirectorFile> {
    let p = find_ci(dir, rel)
        .filter(|p| p.is_file())
        .with_context(|| format!("{rel} not found in {}", dir.display()))?;
    let b = std::fs::read(&p).with_context(|| format!("reading {}", p.display()))?;
    DirectorFile::parse(&b).with_context(|| format!("parsing {}", p.display()))
}

/// Field members of a cast file in member order: (member, text).
fn fields(f: &DirectorFile) -> Result<Vec<(&Member, String)>> {
    f.members()
        .filter(|(_, m)| m.kind == MemberType::Field)
        .map(|(_, m)| {
            let id = m
                .data_of("STXT")
                .with_context(|| format!("field {} has no STXT", m.number))?;
            Ok((m, field_text(f.resource(id).context("STXT missing")?)?))
        })
        .collect()
}

fn field<'a>(fs: &'a [(&Member, String)], name: &str) -> Result<&'a str> {
    fs.iter()
        .find(|(m, _)| m.name.eq_ignore_ascii_case(name))
        .map(|(_, t)| t.as_str())
        .with_context(|| format!("no field named {name:?}"))
}

fn lingo(text: &str, what: &str) -> Result<Lingo> {
    proplist::parse(text).with_context(|| format!("parsing {what}"))
}

/// `#AnnualAverage` → `annual_average`, `DaysWPrecip` → `days_w_precip`.
pub fn snake_id(s: &str) -> String {
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if c.is_ascii_uppercase() {
            if i > 0 {
                out.push('_');
            }
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

/// A data value string: `""` and `"na"` → null; numbers keep the precision they were written with.
fn data_number(v: &Lingo, issues: &mut Vec<String>, at: &str) -> Json {
    let s = match v {
        Lingo::Str(s) => s.trim(),
        Lingo::Int(i) => return Json::from(*i),
        Lingo::Float(f) => return Number::from_f64(*f).map_or(Json::Null, Json::Number),
        _ => {
            issues.push(format!("{at}: unexpected value {v:?}"));
            return Json::Null;
        }
    };
    if s.is_empty() || s.eq_ignore_ascii_case("na") {
        return Json::Null;
    }
    // metric precipitation totals are written with digit grouping ("1,290")
    let grouped = s.contains(',')
        && s.split('.').next().is_some_and(|int| {
            let int = int.trim_start_matches('-');
            let mut groups = int.split(',');
            groups.next().is_some_and(|g| (1..=3).contains(&g.len()))
                && groups.all(|g| g.len() == 3)
                && int.bytes().all(|b| b.is_ascii_digit() || b == b',')
        });
    let plain;
    let s = if grouped {
        plain = s.replace(',', "");
        plain.as_str()
    } else {
        s
    };
    match serde_json::from_str::<Number>(s) {
        Ok(n) => Json::Number(n),
        Err(_) => {
            issues.push(format!("{at}: not a number: {s:?}"));
            Json::Null
        }
    }
}

/// The quoted title in `a/lclstrng.cct` (`"City Climate"`).
fn title(dir: &Path) -> Result<String> {
    let f = open(dir, "a/lclstrng.cct")?;
    let fs = fields(&f)?;
    let t = field(&fs, "title")?;
    Ok(match proplist::parse(t) {
        Ok(Lingo::Str(s)) => s,
        _ => t.trim().to_owned(),
    })
}

#[derive(Debug, Clone, Serialize)]
pub struct Narration {
    pub file: String,
    pub duration_secs: f64,
}

fn narration(dir: &Path, name: &str) -> Option<Narration> {
    let p = find_ci(dir, name).filter(|p| p.is_file())?;
    let b = std::fs::read(&p).ok()?;
    let (_, _, s) = sound::swa_to_mp3(&b).ok()?;
    Some(Narration {
        file: p.file_name()?.to_string_lossy().into_owned(),
        duration_secs: (s.duration_secs * 100.0).round() / 100.0,
    })
}

/// Number of members whose names are `<prefix><n>`, for distinct `n` (selection slots).
fn slot_count(f: &DirectorFile, prefix: &str) -> usize {
    let mut n: Vec<u32> = f
        .members()
        .filter_map(|(_, m)| {
            let rest = m.name.strip_prefix(prefix)?;
            rest.parse().ok()
        })
        .collect();
    n.sort();
    n.dedup();
    n.len()
}

// ---------------------------------------------------------------------------------------------
// City Climate

#[derive(Debug, Clone, Serialize)]
pub struct UnitSpec {
    pub name: String,
    pub abbr: String,
    pub axis_min: Json,
    pub axis_max: Json,
}

#[derive(Debug, Clone, Serialize)]
pub struct ClimateMeasure {
    /// `<category>.<measure>`, e.g. `temperature.annual_average`
    pub id: String,
    pub category: String,
    pub label: String,
    pub units: BTreeMap<String, UnitSpec>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Category {
    pub id: String,
    pub label: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct City {
    pub name: String,
    pub country: String,
    /// list label and lookup key, `"City, Country"`
    pub label: String,
    /// unit system → measure id → value (null when not available)
    pub values: BTreeMap<String, BTreeMap<String, Json>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LaunchFilter {
    pub ref_id: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub country: Option<String>,
    /// state/province code (climate: `WY`, `ONT`…; population: full name) or country
    #[serde(skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub city: Option<String>,
    /// climate: the `cities[].label` the entry selects
    #[serde(skip_serializing_if = "Option::is_none")]
    pub city_label: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ClimateStrings {
    pub add: String,
    pub remove: String,
    /// the legend under the chart (`na = Data not available`)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub not_available: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CityClimate {
    pub id: &'static str,
    pub title: String,
    pub dataset_title: String,
    pub unit_systems: Vec<String>,
    pub default_units: String,
    /// number of cities that can be charted together (selection slots)
    pub max_selection: usize,
    pub strings: ClimateStrings,
    pub categories: Vec<Category>,
    pub measures: Vec<ClimateMeasure>,
    pub cities: Vec<City>,
    pub launch_filter: Vec<LaunchFilter>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub narration: Option<Narration>,
}

fn symbol_list(v: Option<&Lingo>) -> Vec<String> {
    v.map(|l| {
        l.items()
            .iter()
            .filter_map(|i| i.as_str().map(str::to_ascii_lowercase))
            .collect()
    })
    .unwrap_or_default()
}

fn text_of(v: Option<&Lingo>) -> String {
    v.and_then(Lingo::as_str).unwrap_or_default().to_owned()
}

/// Convert `ia/clim_CCL` (the folder holding `a/lclData.cct`, `a/lclMedia.cct`, `a/lclstrng.cct`).
/// Returns the dataset and any data problems found.
pub fn city_climate(dir: &Path) -> Result<(CityClimate, Vec<String>)> {
    let mut issues = Vec::new();
    let data = open(dir, "a/lclData.cct")?;
    let media = open(dir, "a/lclMedia.cct")?;
    let fs = fields(&data)?;

    let spec = lingo(field(&fs, "Data set specs")?, "Data set specs")?;
    let cats = lingo(field(&fs, "categories")?, "categories")?;
    let list = lingo(field(&fs, "location list")?, "location list")?;

    let mut categories = Vec::new();
    let mut measures = Vec::new();
    // (category key, measure key, id) for value lookup
    let mut keys = Vec::new();
    for (ck, cv) in cats.props() {
        let cat_id = snake_id(&ck.key_text());
        categories.push(Category {
            id: cat_id.clone(),
            label: text_of(cv.get("display")),
        });
        for (mk, mv) in cv.get("subCats").map(Lingo::props).unwrap_or_default() {
            let id = format!("{cat_id}.{}", snake_id(&mk.key_text()));
            let mut units = BTreeMap::new();
            for u in UNIT_SYSTEMS {
                let Some(us) = mv.get(u) else {
                    issues.push(format!("{id}: no {u} units"));
                    continue;
                };
                units.insert(
                    u.to_owned(),
                    UnitSpec {
                        name: text_of(us.get("units")),
                        abbr: text_of(us.get("abrvUnits")),
                        axis_min: us.get("low").map_or(Json::Null, Lingo::to_json),
                        axis_max: us.get("high").map_or(Json::Null, Lingo::to_json),
                    },
                );
            }
            keys.push((ck.key_text(), mk.key_text(), id.clone()));
            measures.push(ClimateMeasure {
                id,
                category: cat_id.clone(),
                label: text_of(mv.get("display")),
                units,
            });
        }
    }

    let mut cities = Vec::new();
    for label in list.items() {
        let label = label
            .as_str()
            .context("location list entry is not a string")?;
        let Ok(text) = field(&fs, label) else {
            issues.push(format!("no record for {label:?}"));
            continue;
        };
        let rec = lingo(text, label)?;
        let name = text_of(rec.get("City"));
        // The list label is what the user sees; one record disagrees with it (Belfast's record
        // says "Ireland", its label "Great Britain"), so the country comes from the label.
        let country = match label.strip_prefix(&format!("{name}, ")) {
            Some(c) => c.to_owned(),
            None => {
                issues.push(format!("record {label:?} names city {name:?}"));
                text_of(rec.get("state"))
            }
        };
        let mut values = BTreeMap::new();
        for u in UNIT_SYSTEMS {
            let mut vals = BTreeMap::new();
            for (ck, mk, id) in &keys {
                let v = rec.path(&[u, ck, mk]);
                let j = match v {
                    Some(v) => data_number(v, &mut issues, &format!("{label} {u} {id}")),
                    None => {
                        issues.push(format!("{label}: no {u} {id}"));
                        Json::Null
                    }
                };
                vals.insert(id.clone(), j);
            }
            values.insert(u.to_owned(), vals);
        }
        cities.push(City {
            name,
            country,
            label: label.to_owned(),
            values,
        });
    }
    let record_count = fs
        .iter()
        .filter(|(m, t)| m.number > 3 && t.trim_start().starts_with('['))
        .count();
    if record_count != cities.len() {
        issues.push(format!(
            "{record_count} city records but {} listed",
            cities.len()
        ));
    }

    // launch filter: ["refId": [region], "refId": [region, city]]
    let media_fields = fields(&media)?;
    let mut launch_filter = Vec::new();
    if let Ok(t) = field(&media_fields, "data filter") {
        for (k, v) in lingo(t, "data filter")?.props() {
            let parts: Vec<String> = v
                .items()
                .iter()
                .filter_map(|i| i.as_str().map(str::to_owned))
                .collect();
            let Ok(ref_id) = k.key_text().parse::<u64>() else {
                issues.push(format!("data filter key {:?} is not a refId", k.key_text()));
                continue;
            };
            let city_label = parts.get(1).and_then(|c| {
                let hits: Vec<&City> = cities
                    .iter()
                    .filter(|x| x.name == *c || x.label.starts_with(&format!("{c}, {}", parts[0])))
                    .collect();
                match hits.as_slice() {
                    [one] => Some(one.label.clone()),
                    _ => {
                        issues.push(format!(
                            "data filter {ref_id}: city {c:?} matches {} cities",
                            hits.len()
                        ));
                        None
                    }
                }
            });
            launch_filter.push(LaunchFilter {
                ref_id,
                country: None,
                region: parts.first().cloned(),
                city: parts.get(1).cloned(),
                city_label,
            });
        }
    }
    let not_available = media
        .members()
        .filter(|(_, m)| {
            m.xtra
                .as_deref()
                .is_some_and(|x| x.eq_ignore_ascii_case("text"))
        })
        .filter_map(|(_, m)| text_xtra_plain(media.resource(m.data_of("XMED")?)?))
        .find(|t| t.contains("na ="));

    let unit_systems = symbol_list(spec.get("Measures"));
    let out = CityClimate {
        id: "clim_CCL",
        title: title(dir)?,
        dataset_title: text_of(spec.get("Title")),
        default_units: text_of(spec.get("defaultMeasures")).to_ascii_lowercase(),
        unit_systems,
        max_selection: slot_count(&media, "select"),
        strings: ClimateStrings {
            add: text_of(spec.get("addMessage")),
            remove: text_of(spec.get("removeMessage")),
            not_available,
        },
        categories,
        measures,
        cities,
        launch_filter,
        narration: narration(dir, "clim_in.swa"),
    };
    Ok((out, issues))
}

// ---------------------------------------------------------------------------------------------
// Population Chart

#[derive(Debug, Clone, Serialize)]
pub struct PopUnits {
    /// axis caption (`People per sq mi`; empty for totals)
    pub name: String,
    /// preset y-axis tick lists; the chart uses the smallest one whose top fits the selection
    pub scales: Vec<Vec<Json>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PopMeasure {
    pub id: String,
    pub label: String,
    pub units: BTreeMap<String, PopUnits>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Region {
    pub name: String,
    /// unit system → measure id → one value per census year (null before the first census)
    pub values: BTreeMap<String, BTreeMap<String, Vec<Json>>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Axis {
    pub chart_start: i64,
    pub chart_end: i64,
    pub label_step: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct PopStrings {
    pub add: String,
    pub remove: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct PopDataset {
    /// `us` or `canada`
    pub id: String,
    /// country name as the launch filter writes it (`US`, `Canada`)
    pub country: String,
    /// internal title kept from the data (`US Climate`, sic)
    pub spec_title: String,
    pub footnote: String,
    pub default_units: String,
    pub strings: PopStrings,
    pub category_label: String,
    pub years: Vec<i64>,
    pub axis: Axis,
    pub measures: Vec<PopMeasure>,
    pub regions: Vec<Region>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PopulationChart {
    pub id: &'static str,
    pub title: String,
    pub unit_systems: Vec<String>,
    /// number of series that can be charted together
    pub max_selection: usize,
    pub datasets: Vec<PopDataset>,
    pub launch_filter: Vec<LaunchFilter>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub narration: Option<Narration>,
}

fn int(v: Option<&Lingo>, what: &str) -> Result<i64> {
    v.and_then(Lingo::as_i64)
        .with_context(|| format!("{what} missing or not an integer"))
}

fn population_dataset(
    f: &DirectorFile,
    id: &str,
    country: &str,
    issues: &mut Vec<String>,
) -> Result<PopDataset> {
    let fs = fields(f)?;
    let footnote = fs
        .iter()
        .find(|(m, t)| {
            m.name.is_empty() && !t.trim().is_empty() && !t.trim_start().starts_with('[')
        })
        .map(|(_, t)| t.trim().to_owned())
        .unwrap_or_default();
    let spec = lingo(field(&fs, "Data set specs")?, "Data set specs")?;
    let cats = lingo(field(&fs, "categories")?, "categories")?;
    let list = lingo(field(&fs, "location list")?, "location list")?;

    let x = spec.get("xAxisSpecs").context("no xAxisSpecs")?;
    let (start, end, step) = (
        int(x.get("dataStart"), "dataStart")?,
        int(x.get("dataEnd"), "dataEnd")?,
        int(x.get("increment"), "increment")?,
    );
    ensure!(
        step > 0 && end >= start,
        "bad year range {start}..{end} step {step}"
    );
    let years: Vec<i64> = (start..=end).step_by(step as usize).collect();
    let axis = Axis {
        chart_start: int(x.get("chartStart"), "chartStart")?,
        chart_end: int(x.get("chartEnd"), "chartEnd")?,
        label_step: int(x.get("labelIncrement"), "labelIncrement")?,
    };

    // a single category (#Population) with three sub-categories
    let Some((cat_key, cat)) = cats.props().first() else {
        bail!("no categories")
    };
    let mut measures = Vec::new();
    let mut keys = Vec::new();
    for (mk, mv) in cat.get("subCats").map(Lingo::props).unwrap_or_default() {
        let mid = snake_id(&mk.key_text());
        let mut units = BTreeMap::new();
        for u in UNIT_SYSTEMS {
            let Some(us) = mv.get(u) else { continue };
            let scales = us
                .get("yax")
                .map(|l| {
                    l.items()
                        .iter()
                        .map(|s| s.items().iter().map(Lingo::to_json).collect())
                        .collect()
                })
                .unwrap_or_default();
            units.insert(
                u.to_owned(),
                PopUnits {
                    name: text_of(us.get("units")),
                    scales,
                },
            );
        }
        keys.push((mk.key_text(), mid.clone()));
        measures.push(PopMeasure {
            id: mid,
            label: text_of(mv.get("display")),
            units,
        });
    }

    let mut regions = Vec::new();
    for name in list.items() {
        let name = name
            .as_str()
            .context("location list entry is not a string")?;
        let Ok(text) = field(&fs, name) else {
            issues.push(format!("{id}: no record for {name:?}"));
            continue;
        };
        let rec = lingo(text, name)?;
        let mut values = BTreeMap::new();
        for u in UNIT_SYSTEMS {
            let mut per = BTreeMap::new();
            for (mk, mid) in &keys {
                let series = rec
                    .path(&[u, &cat_key.key_text(), mk])
                    .map(Lingo::items)
                    .unwrap_or_default();
                if series.len() != years.len() {
                    issues.push(format!(
                        "{id} {name} {u} {mid}: {} values for {} years",
                        series.len(),
                        years.len()
                    ));
                }
                let at = format!("{id} {name} {u} {mid}");
                per.insert(
                    mid.clone(),
                    series.iter().map(|v| data_number(v, issues, &at)).collect(),
                );
            }
            values.insert(u.to_owned(), per);
        }
        regions.push(Region {
            name: name.to_owned(),
            values,
        });
    }
    let record_count = fs
        .iter()
        .filter(|(m, t)| !m.name.is_empty() && t.trim_start().starts_with("[#imperial"))
        .count();
    if record_count != regions.len() {
        issues.push(format!(
            "{id}: {record_count} records but {} listed",
            regions.len()
        ));
    }

    Ok(PopDataset {
        id: id.to_owned(),
        country: country.to_owned(),
        spec_title: text_of(spec.get("Title")),
        footnote,
        default_units: text_of(spec.get("defaultMeasures")).to_ascii_lowercase(),
        strings: PopStrings {
            add: text_of(spec.get("addMessage")),
            remove: text_of(spec.get("removeMessage")),
        },
        category_label: text_of(cat.get("display")),
        years,
        axis,
        measures,
        regions,
    })
}

/// Convert `ia/pop_CPO` (the folder holding `dtus.cct`, `dtcanada.cct` and `a/*.cct`).
pub fn population_chart(dir: &Path) -> Result<(PopulationChart, Vec<String>)> {
    let mut issues = Vec::new();
    let us = population_dataset(&open(dir, "dtus.cct")?, "us", "US", &mut issues)?;
    let ca = population_dataset(&open(dir, "dtcanada.cct")?, "canada", "Canada", &mut issues)?;
    let media = open(dir, "a/lclMedia.cct")?;
    let mf = fields(&media)?;

    // launch filter: one `refId,country[,state or province]` line per media item
    let mut launch_filter = Vec::new();
    if let Ok(t) = field(&mf, "filter data") {
        for line in t.lines().map(str::trim).filter(|l| !l.is_empty()) {
            let mut parts = line.splitn(3, ',').map(str::trim);
            let Some(Ok(ref_id)) = parts.next().map(str::parse::<u64>) else {
                issues.push(format!("filter line {line:?} has no refId"));
                continue;
            };
            launch_filter.push(LaunchFilter {
                ref_id,
                country: parts.next().map(str::to_owned),
                region: parts.next().map(str::to_owned),
                city: None,
                city_label: None,
            });
        }
    }

    let movie = open(dir, "pop_CPO.dcr").ok();
    let max_selection =
        slot_count(&media, "colLabel").max(movie.as_ref().map_or(0, |m| slot_count(m, "vector")));
    let mut unit_systems: Vec<String> = Vec::new();
    for d in [&us, &ca] {
        for u in d.measures.iter().flat_map(|m| m.units.keys()) {
            if !unit_systems.contains(u) {
                unit_systems.push(u.clone());
            }
        }
    }
    let out = PopulationChart {
        id: "pop_CPO",
        title: title(dir)?,
        unit_systems,
        max_selection,
        datasets: vec![us, ca],
        launch_filter,
        narration: narration(dir, "pop_in.swa"),
    };
    Ok((out, issues))
}

/// Every field member of a cast as `{name: text}` (later duplicates get `#<number>` suffixes),
/// with Lingo literals parsed into JSON where they parse.
pub fn fields_json(f: &DirectorFile) -> Result<Json> {
    let mut m = Map::new();
    for (mem, text) in fields(f)? {
        let key = if mem.name.is_empty() || m.contains_key(&mem.name) {
            format!("{}#{}", mem.name, mem.number)
        } else {
            mem.name.clone()
        };
        let v = match proplist::parse(&text) {
            Ok(l @ (Lingo::List(_) | Lingo::PropList(_))) => l.to_json(),
            _ => Json::String(text),
        };
        m.insert(key, v);
    }
    Ok(Json::Object(m))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids() {
        assert_eq!(snake_id("AnnualAverage"), "annual_average");
        assert_eq!(snake_id("DaysWPrecip"), "days_w_precip");
        assert_eq!(snake_id("avgSnow"), "avg_snow");
        assert_eq!(snake_id("total"), "total");
    }

    #[test]
    fn numbers_keep_precision() {
        let mut issues = Vec::new();
        assert_eq!(
            data_number(&Lingo::Str("2.0".into()), &mut issues, "t").to_string(),
            "2.0"
        );
        assert_eq!(
            data_number(&Lingo::Str("127901".into()), &mut issues, "t").to_string(),
            "127901"
        );
        assert_eq!(
            data_number(&Lingo::Str("-11.7".into()), &mut issues, "t").to_string(),
            "-11.7"
        );
        assert_eq!(
            data_number(&Lingo::Str("na".into()), &mut issues, "t"),
            Json::Null
        );
        assert_eq!(
            data_number(&Lingo::Str("".into()), &mut issues, "t"),
            Json::Null
        );
        assert_eq!(
            data_number(&Lingo::Str("1,290".into()), &mut issues, "t").to_string(),
            "1290"
        );
        assert_eq!(
            data_number(&Lingo::Str("3,921.5".into()), &mut issues, "t").to_string(),
            "3921.5"
        );
        assert!(issues.is_empty());
        data_number(&Lingo::Str("1,29".into()), &mut issues, "t");
        assert_eq!(issues.len(), 1);
        issues.clear();
        data_number(&Lingo::Str("n/a".into()), &mut issues, "t");
        assert_eq!(issues.len(), 1);
    }
}
