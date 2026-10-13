//! `encarta-convert content-survey`: element and attribute statistics over every decoded XML source.
//!
//! Writes `out/report/content-survey.json` (full) and `out/report/content-survey.txt` (readable).
//! The survey is what `docs/content-schema.md` was written from, and it is re-run by the
//! completeness check: every element/attribute it finds must be mapped or listed as dropped.

use super::sources::{self, Source};
use super::xml::{self, Element};
use crate::Paths;
use anyhow::{Context, Result};
use serde::Serialize;
use std::collections::BTreeMap;
use std::fmt::Write as _;

/// Distinct values kept per attribute before the value set is reported as open.
const VALUE_CAP: usize = 40;

#[derive(Default, Clone, Serialize)]
pub struct AttrStats {
    pub count: u64,
    pub numeric: u64,
    /// Up to `VALUE_CAP` distinct values with counts; `open` is set past that.
    pub values: BTreeMap<String, u64>,
    pub open: bool,
}

#[derive(Default, Clone, Serialize)]
pub struct ElementStats {
    pub count: u64,
    /// Occurrences that directly contain non-whitespace text.
    pub with_text: u64,
    /// Occurrences with no content at all.
    pub empty: u64,
    pub attrs: BTreeMap<String, AttrStats>,
    pub parents: BTreeMap<String, u64>,
    pub children: BTreeMap<String, u64>,
    pub max_depth: usize,
    /// Smallest record id where the element occurs (for looking it up).
    pub example: String,
}

#[derive(Default, Clone, Serialize)]
pub struct GroupStats {
    pub records: u64,
    pub bytes: u64,
    pub parse_errors: u64,
    pub first_error: Option<String>,
    pub roots: BTreeMap<String, u64>,
    pub bom: u64,
    pub decls: BTreeMap<String, u64>,
    pub comments: u64,
    pub processing_instructions: u64,
    pub cdata: u64,
    pub doctypes: BTreeMap<String, u64>,
    pub unknown_entities: BTreeMap<String, u64>,
    /// Text that still looks like an escaped entity after XML unescaping (`&amp;eacute;` → `&eacute;`).
    pub literal_entities_in_text: BTreeMap<String, u64>,
    pub elements: BTreeMap<String, ElementStats>,
    /// Text of font-switch runs, `type:text` → count (to build the symbol-font mapping).
    pub font_runs: BTreeMap<String, u64>,
}

fn bump(m: &mut BTreeMap<String, u64>, k: &str, n: u64) {
    if let Some(v) = m.get_mut(k) {
        *v += n;
    } else {
        m.insert(k.to_string(), n);
    }
}

/// Attributes whose whole value set is kept (small vocabularies the converter must map one by one).
fn cap_for(element: &str, attr: &str) -> usize {
    match (element, attr) {
        ("entity", "tag") | (_, "group") | (_, "type") | (_, "ticon") | (_, "subgroup") => 5000,
        _ => VALUE_CAP,
    }
}

fn add_value(a: &mut AttrStats, v: &str, n: u64, cap: usize) {
    if let Some(c) = a.values.get_mut(v) {
        *c += n;
    } else if a.values.len() < cap && v.len() <= 80 {
        a.values.insert(v.to_string(), n);
    } else {
        a.open = true;
    }
}

/// Finds `&name;` patterns in already-unescaped text.
pub fn literal_entities(text: &str, mut f: impl FnMut(&str)) {
    let b = text.as_bytes();
    let mut i = 0;
    while let Some(off) = text[i..].find('&') {
        let s = i + off + 1;
        let mut e = s;
        while e < b.len() && e - s < 32 && (b[e].is_ascii_alphanumeric() || b[e] == b'#') {
            e += 1;
        }
        if e > s && e < b.len() && b[e] == b';' {
            f(&text[s..e]);
        }
        i = s;
    }
}

impl GroupStats {
    pub fn add_document(&mut self, id: &str, doc: &xml::Document) {
        self.roots.entry(doc.root.name.clone()).and_modify(|c| *c += 1).or_insert(1);
        self.bom += doc.had_bom as u64;
        if let Some(d) = &doc.decl {
            bump(&mut self.decls, d, 1);
        }
        self.comments += doc.comments as u64;
        self.processing_instructions += doc.processing_instructions as u64;
        self.cdata += doc.cdata_sections as u64;
        if let Some(d) = &doc.doctype {
            bump(&mut self.doctypes, d, 1);
        }
        for e in &doc.unknown_entities {
            bump(&mut self.unknown_entities, e, 1);
        }
        self.add_element(id, &doc.root, "#root", 0);
    }

    fn add_element(&mut self, id: &str, el: &Element, parent: &str, depth: usize) {
        if el.name == "fs" {
            let k = format!("{}:{}", el.attr("type").unwrap_or(""), el.text());
            bump(&mut self.font_runs, &k, 1);
        }
        let st = self.elements.entry(el.name.clone()).or_default();
        st.count += 1;
        if st.example.is_empty() || (id.len(), id) < (st.example.len(), st.example.as_str()) {
            st.example = id.to_string();
        }
        st.max_depth = st.max_depth.max(depth);
        bump(&mut st.parents, parent, 1);
        if el.children.is_empty() {
            st.empty += 1;
        }
        let mut has_text = false;
        for c in &el.children {
            match c {
                xml::Node::Text(t) => {
                    if !t.trim().is_empty() {
                        has_text = true;
                    }
                }
                xml::Node::El(e) => bump(&mut st.children, &e.name, 1),
            }
        }
        if has_text {
            st.with_text += 1;
            bump(&mut st.children, "#text", 1);
        }
        for (k, v) in &el.attrs {
            let a = st.attrs.entry(k.clone()).or_default();
            a.count += 1;
            if !v.is_empty() && v.bytes().all(|b| b.is_ascii_digit() || b == b'-') {
                a.numeric += 1;
            }
            add_value(a, v, 1, cap_for(&el.name, k));
        }
        for c in &el.children {
            match c {
                xml::Node::Text(t) => {
                    literal_entities(t, |e| bump(&mut self.literal_entities_in_text, e, 1))
                }
                xml::Node::El(e) => self.add_element(id, e, &el.name, depth + 1),
            }
        }
    }

    pub fn merge(&mut self, o: GroupStats) {
        self.records += o.records;
        self.bytes += o.bytes;
        self.parse_errors += o.parse_errors;
        if self.first_error.is_none() {
            self.first_error = o.first_error;
        }
        for (k, v) in o.roots {
            bump(&mut self.roots, &k, v);
        }
        self.bom += o.bom;
        for (k, v) in o.decls {
            bump(&mut self.decls, &k, v);
        }
        self.comments += o.comments;
        self.processing_instructions += o.processing_instructions;
        self.cdata += o.cdata;
        for (k, v) in o.doctypes {
            bump(&mut self.doctypes, &k, v);
        }
        for (k, v) in o.unknown_entities {
            bump(&mut self.unknown_entities, &k, v);
        }
        for (k, v) in o.font_runs {
            bump(&mut self.font_runs, &k, v);
        }
        for (k, v) in o.literal_entities_in_text {
            bump(&mut self.literal_entities_in_text, &k, v);
        }
        for (name, s) in o.elements {
            match self.elements.get_mut(&name) {
                None => {
                    self.elements.insert(name, s);
                }
                Some(t) => {
                    t.count += s.count;
                    t.with_text += s.with_text;
                    t.empty += s.empty;
                    t.max_depth = t.max_depth.max(s.max_depth);
                    if (s.example.len(), &s.example) < (t.example.len(), &t.example) {
                        t.example = s.example;
                    }
                    for (k, v) in s.parents {
                        bump(&mut t.parents, &k, v);
                    }
                    for (k, v) in s.children {
                        bump(&mut t.children, &k, v);
                    }
                    for (k, a) in s.attrs {
                        let cap = cap_for(&name, &k);
                        let ta = t.attrs.entry(k).or_default();
                        ta.count += a.count;
                        ta.numeric += a.numeric;
                        ta.open |= a.open;
                        for (v, n) in a.values {
                            add_value(ta, &v, n, cap);
                        }
                    }
                }
            }
        }
    }
}

/// Surveys one source's records in parallel.
pub fn survey_source(src: &Source) -> Result<GroupStats> {
    src.par_fold(GroupStats::default, |mut st, id, text| {
        st.records += 1;
        st.bytes += text.len() as u64;
        match xml::parse(text) {
            Ok(doc) => st.add_document(id, &doc),
            Err(e) => {
                st.parse_errors += 1;
                if st.first_error.is_none() {
                    st.first_error = Some(format!("{id}: {e:#}"));
                }
            }
        }
        st
    }, |mut a, b| {
        a.merge(b);
        a
    })
}

pub fn run(paths: &Paths) -> Result<()> {
    let t0 = std::time::Instant::now();
    let srcs = sources::all_xml_sources(&paths.extracted)?;
    let mut out: BTreeMap<String, GroupStats> = BTreeMap::new();
    for s in &srcs {
        let st = survey_source(s).with_context(|| format!("surveying {}", s.label))?;
        eprintln!(
            "{:<40} {:>8} records {:>12} bytes {:>4} elements ({} parse errors)",
            s.label,
            st.records,
            st.bytes,
            st.elements.len(),
            st.parse_errors
        );
        out.insert(s.label.clone(), st);
    }
    let dir = paths.out.join("report");
    std::fs::create_dir_all(&dir)?;
    std::fs::write(
        dir.join("content-survey.json"),
        serde_json::to_vec_pretty(&out)?,
    )?;
    std::fs::write(dir.join("content-survey.txt"), render_text(&out))?;
    eprintln!("survey done in {:.1}s", t0.elapsed().as_secs_f64());
    Ok(())
}

fn render_text(all: &BTreeMap<String, GroupStats>) -> String {
    let mut s = String::new();
    for (label, g) in all {
        let _ = writeln!(
            s,
            "\n######## {label}: {} records, {} bytes, roots {:?}, bom {}, decls {:?}, comments {}, PIs {}, cdata {}, doctypes {:?}",
            g.records, g.bytes, g.roots, g.bom, g.decls, g.comments, g.processing_instructions, g.cdata, g.doctypes
        );
        if g.parse_errors > 0 {
            let _ = writeln!(s, "  PARSE ERRORS {} first: {:?}", g.parse_errors, g.first_error);
        }
        if !g.unknown_entities.is_empty() {
            let _ = writeln!(s, "  unknown entity refs: {:?}", g.unknown_entities);
        }
        if !g.literal_entities_in_text.is_empty() {
            let _ = writeln!(s, "  literal &x; in text: {:?}", g.literal_entities_in_text);
        }
        let mut els: Vec<_> = g.elements.iter().collect();
        els.sort_by_key(|e| std::cmp::Reverse(e.1.count));
        for (name, e) in els {
            let _ = writeln!(
                s,
                "  <{name}> n={} text={} empty={} depth<={} ex={} parents={:?}",
                e.count, e.with_text, e.empty, e.max_depth, e.example, top(&e.parents, 8)
            );
            let _ = writeln!(s, "      children={:?}", top(&e.children, 30));
            for (k, a) in &e.attrs {
                let vals: Vec<String> = if a.open {
                    let mut v: Vec<_> = a.values.iter().collect();
                    v.sort_by(|x, y| y.1.cmp(x.1));
                    v.iter().take(8).map(|(k, n)| format!("{k}:{n}")).collect()
                } else {
                    a.values.iter().map(|(k, n)| format!("{k}:{n}")).collect()
                };
                let _ = writeln!(
                    s,
                    "      @{k} n={} numeric={}{} [{}]",
                    a.count,
                    a.numeric,
                    if a.open { " OPEN" } else { "" },
                    vals.join(", ")
                );
            }
        }
    }
    s
}

fn top(m: &BTreeMap<String, u64>, n: usize) -> Vec<(String, u64)> {
    let mut v: Vec<_> = m.iter().map(|(k, c)| (k.clone(), *c)).collect();
    v.sort_by_key(|e| std::cmp::Reverse(e.1));
    v.truncate(n);
    v
}
