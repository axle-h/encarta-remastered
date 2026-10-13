//! Completeness report: every catalog id and AKC record accounted for, every surveyed element and
//! attribute either consumed by the converter or listed here as dropped with a reason.

use super::survey::GroupStats;
use super::track::Tracker;
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashMap};
use std::fmt::Write;

/// Source parts the converter deliberately does not carry over: `(source prefix, element, attribute
/// or "" for the element, reason)`. A source prefix matches labels that start with it.
pub const DROPPED: &[(&str, &str, &str, &str)] = &[
    ("edict/", "WAV_AUDIO_PATH", "", "build-machine path (\\\\TKXTICTFS\\…) to a source recording; the shipped audio is FG_AUDIO_WAV"),
    ("edict/", "MP3_AUDIO_PATH", "", "build-machine path to a source recording; the shipped audio is FG_AUDIO_WAV"),
];

/// Whole sources that are not converted here (none at present).
pub const OMITTED_SOURCES: &[(&str, &str)] = &[];

#[derive(Default)]
pub struct SourceRep {
    pub records: usize,
    pub statuses: BTreeMap<String, u64>,
    pub errors: Vec<String>,
    pub error_count: usize,
}

#[derive(Default)]
pub struct Report {
    pub timings: Vec<(String, f64)>,
    pub counts: BTreeMap<String, u64>,
    pub catalog_ids: HashMap<(&'static str, &'static str), Vec<u32>>,
    pub catalog_keys: HashMap<(&'static str, &'static str), Vec<String>>,
    pub sources: BTreeMap<String, SourceRep>,
    pub done: HashMap<u32, Vec<&'static str>>,
    pub trackers: BTreeMap<String, Tracker>,
    pub baggage_xml: Vec<(String, String)>,
    pub notes: Vec<String>,
    out: Value,
    headline: String,
}

impl Report {
    pub fn add_source(&mut self, label: &str, records: usize, done: &[(u32, &'static str)], errors: &[String]) {
        let r = self.sources.entry(label.to_string()).or_default();
        r.records += records;
        for (id, st) in done {
            *r.statuses.entry(st.to_string()).or_default() += 1;
            if *id != 0 {
                self.done.entry(*id).or_default().push(st);
            }
        }
        r.error_count += errors.len();
        r.errors.extend(errors.iter().take(20).cloned());
    }

    pub fn add_tracker(&mut self, label: &str, t: Tracker) {
        self.trackers.entry(label.to_string()).or_default().merge(t);
    }

    fn tracker_for(&self, label: &str) -> Option<&Tracker> {
        self.trackers
            .get(label)
            .or_else(|| self.trackers.get(label.split('/').next().unwrap_or("")))
    }

    pub fn finish(&mut self, inventory: &BTreeMap<String, GroupStats>, rows: &HashMap<&'static str, u64>, secs: f64) {
        // 1. catalog ids
        let mut catalog = serde_json::Map::new();
        let mut unexplained_ids = 0u64;
        let mut total_ids = 0u64;
        let mut keys: Vec<_> = self.catalog_ids.keys().copied().collect();
        keys.sort();
        for k in keys {
            let ids = &self.catalog_ids[&k];
            let mut st: BTreeMap<String, u64> = BTreeMap::new();
            let mut missing = Vec::new();
            for id in ids {
                total_ids += 1;
                let s = self.done.get(id).and_then(|v| {
                    v.iter().find(|s| match k.1 {
                        "content.ecn" => s.starts_with("article") || s.starts_with("empty"),
                        "data.ecn" => **s == "item" || **s == "index_tree",
                        _ => **s == "related_tree",
                    })
                });
                match s {
                    Some(s) => *st.entry(s.to_string()).or_default() += 1,
                    None => {
                        unexplained_ids += 1;
                        if missing.len() < 50 {
                            missing.push(*id);
                        }
                    }
                }
            }
            catalog.insert(
                format!("{}/{}", k.0, k.1),
                json!({"ids": ids.len(), "status": st, "unexplained": missing}),
            );
        }
        // string keys
        let mut key_report = serde_json::Map::new();
        let xml_status: HashMap<&str, &str> = self.baggage_xml.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
        let mut kk: Vec<_> = self.catalog_keys.keys().copied().collect();
        kk.sort();
        for k in kk {
            let mut st: BTreeMap<String, u64> = BTreeMap::new();
            for key in &self.catalog_keys[&k] {
                let s = if key.ends_with(".xml") && xml_status.contains_key(key.as_str()) {
                    let v = xml_status[key.as_str()];
                    if v.starts_with("omitted") { "sidebar xml: omitted (superseded copy)" } else { "sidebar xml: converted to article" }
                } else if key == "list/expert.xml" {
                    "contributor list → contributor table"
                } else if key.starts_with("dswmedia/quiz/") && key.ends_with(".iax") {
                    "quiz definition → quiz table"
                } else if key.ends_with(".iax") {
                    "interactive definition → interactive table (file: media converter)"
                } else {
                    "media/interactive file: media converter (joined via baggage/item_file.file_name)"
                };
                *st.entry(s.to_string()).or_default() += 1;
            }
            key_report.insert(format!("{}/{}", k.0, k.1), json!(st));
        }

        // 2. records per source
        let mut recs = serde_json::Map::new();
        let mut unexplained_records = 0u64;
        for (label, r) in &self.sources {
            let accounted: u64 = r.statuses.values().sum::<u64>() + r.error_count as u64;
            let gap = r.records as i64 - accounted as i64;
            if gap != 0 || r.error_count > 0 {
                unexplained_records += gap.unsigned_abs() + r.error_count as u64;
            }
            recs.insert(
                label.clone(),
                json!({"records": r.records, "status": r.statuses, "errors": r.error_count, "first_errors": r.errors, "unaccounted": gap}),
            );
        }

        // 3. element / attribute mapping
        let mut mapping = serde_json::Map::new();
        let mut unexplained_markup: Vec<String> = Vec::new();
        let mut dropped_markup: Vec<String> = Vec::new();
        for (label, g) in inventory {
            if let Some((_, why)) = OMITTED_SOURCES.iter().find(|(p, _)| label.starts_with(p))
                && self.trackers.get(label.as_str()).is_none_or(|t| t.elements.is_empty())
            {
                mapping.insert(label.clone(), json!({"omitted": why, "records": g.records}));
                continue;
            }
            let t = self.tracker_for(label);
            let mut els = serde_json::Map::new();
            for (name, st) in &g.elements {
                let used = t.and_then(|t| t.elements.get(name));
                let drop = |attr: &str| {
                    DROPPED
                        .iter()
                        .find(|(p, e, a, _)| label.starts_with(p) && e == name && *a == attr)
                        .map(|x| x.3)
                };
                let mut attrs = serde_json::Map::new();
                for a in st.attrs.keys() {
                    let ok = used.is_some_and(|u| u.attrs.contains_key(a));
                    let v = if ok {
                        "mapped".to_string()
                    } else if let Some(r) = drop(a) {
                        dropped_markup.push(format!("{label} <{name} @{a}>: {r}"));
                        format!("dropped: {r}")
                    } else if used.is_none() && drop("").is_some() {
                        "dropped with element".to_string()
                    } else {
                        unexplained_markup.push(format!("{label} <{name} @{a}> ({} occurrences, e.g. {})", st.attrs[a].count, st.example));
                        "UNEXPLAINED".to_string()
                    };
                    attrs.insert(a.clone(), v.into());
                }
                let status = if used.is_some() {
                    "mapped".to_string()
                } else if let Some(r) = drop("") {
                    dropped_markup.push(format!("{label} <{name}>: {r}"));
                    format!("dropped: {r}")
                } else {
                    unexplained_markup.push(format!("{label} <{name}> ({} occurrences, e.g. {})", st.count, st.example));
                    "UNEXPLAINED".to_string()
                };
                els.insert(name.clone(), json!({"count": st.count, "status": status, "attrs": attrs}));
            }
            mapping.insert(label.clone(), Value::Object(els));
        }

        // 4. notes from the trackers
        let mut notes = serde_json::Map::new();
        for (label, t) in &self.trackers {
            let mut m = serde_json::Map::new();
            for (cat, kv) in &t.notes {
                let mut c = serde_json::Map::new();
                for (k, n) in kv {
                    let ex = t.note_examples.get(&(cat.clone(), k.clone())).cloned().unwrap_or_default();
                    c.insert(k.clone(), json!({"count": n, "example": ex}));
                }
                m.insert(cat.clone(), Value::Object(c));
            }
            if !m.is_empty() {
                notes.insert(label.clone(), Value::Object(m));
            }
        }
        let problems: Vec<String> = self
            .trackers
            .iter()
            .flat_map(|(label, t)| {
                t.notes.iter().filter(|(c, _)| {
                    matches!(c.as_str(), "unknown entity" | "unknown literal entity" | "unmapped glyph" | "unmapped inline" | "unmapped data element")
                }).flat_map(move |(c, kv)| kv.iter().map(move |(k, n)| format!("{label}: {c} {k} ×{n}")))
            })
            .collect();

        let mut rows_sorted: BTreeMap<&str, u64> = BTreeMap::new();
        for (k, v) in rows {
            rows_sorted.insert(k, *v);
        }
        self.headline = format!(
            "content: {} catalog ids ({} unexplained), {} records unaccounted, {} unexplained elements/attributes, {} character problems, {:.0}s",
            total_ids,
            unexplained_ids,
            unexplained_records,
            unexplained_markup.len(),
            problems.len(),
            secs
        );
        self.out = json!({
            "headline": self.headline,
            "seconds": secs,
            "timings": self.timings,
            "rows": rows_sorted,
            "counts": self.counts,
            "catalog_ids": catalog,
            "catalog_keys": key_report,
            "records": recs,
            "baggage_xml": self.baggage_xml.iter().map(|(k, v)| json!([k, v])).collect::<Vec<_>>(),
            "markup": mapping,
            "unexplained_markup": unexplained_markup,
            "dropped_markup": dropped_markup,
            "character_problems": problems,
            "notes": notes,
            "other": self.notes,
        });
    }

    pub fn headline(&self) -> &str {
        &self.headline
    }

    pub fn to_json(&self) -> &Value {
        &self.out
    }

    pub fn summary_text(&self) -> String {
        let o = &self.out;
        let mut s = String::new();
        let _ = writeln!(s, "{}\n", self.headline);
        let _ = writeln!(s, "Rows per table:");
        if let Some(m) = o["rows"].as_object() {
            for (k, v) in m {
                let _ = writeln!(s, "  {k:<20} {v}");
            }
        }
        let _ = writeln!(s, "\nCatalog ids by status:");
        if let Some(m) = o["catalog_ids"].as_object() {
            for (k, v) in m {
                let _ = writeln!(s, "  {k}: {} ids {}", v["ids"], v["status"]);
                if v["unexplained"].as_array().is_some_and(|a| !a.is_empty()) {
                    let _ = writeln!(s, "    UNEXPLAINED e.g. {}", v["unexplained"]);
                }
            }
        }
        let _ = writeln!(s, "\nCatalog string keys:");
        if let Some(m) = o["catalog_keys"].as_object() {
            for (k, v) in m {
                let _ = writeln!(s, "  {k}: {v}");
            }
        }
        let _ = writeln!(s, "\nRecords per source:");
        if let Some(m) = o["records"].as_object() {
            for (k, v) in m {
                let _ = writeln!(s, "  {k:<28} {:>7} {} errors={} unaccounted={}", v["records"], v["status"], v["errors"], v["unaccounted"]);
            }
        }
        for (title, key) in [
            ("Unexplained elements/attributes", "unexplained_markup"),
            ("Dropped markup (with reasons)", "dropped_markup"),
            ("Character problems", "character_problems"),
        ] {
            let _ = writeln!(s, "\n{title}:");
            for x in o[key].as_array().into_iter().flatten() {
                let _ = writeln!(s, "  {}", x.as_str().unwrap_or(""));
            }
        }
        let _ = writeln!(s, "\nOmitted sources:");
        if let Some(m) = o["markup"].as_object() {
            for (k, v) in m {
                if let Some(w) = v.get("omitted") {
                    let _ = writeln!(s, "  {k} ({} records): {}", v["records"], w.as_str().unwrap_or(""));
                }
            }
        }
        let _ = writeln!(s, "\nCounts:");
        if let Some(m) = o["counts"].as_object() {
            for (k, v) in m {
                let _ = writeln!(s, "  {k:<44} {v}");
            }
        }
        let _ = writeln!(s, "\nTimings:");
        for (k, v) in &self.timings {
            let _ = writeln!(s, "  {k:<20} {v:.1}s");
        }
        s
    }
}
