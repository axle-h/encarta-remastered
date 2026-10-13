//! Builds the search files from `out/content.sqlite` (schema: docs/content-schema.md §9).
//!
//! What is indexed:
//! - every item except index terms, home-screen pages and unclassified items, with its title,
//!   PFX aliases (`alias`), section titles (`section`) and text (`paragraph`, or the caption for
//!   media items);
//! - every paragraph of every article, for deep links;
//! - every dictionary, thesaurus and bilingual entry (`dict_entry`, forms from `dict_form`);
//! - the word wheel (`alias`) and item titles for autocomplete;
//! - the lexicon word lists (`lexicon_word`) for spelling, with corpus word frequencies.

use crate::build::{BuildOptions, BuildStats, IndexBuilder, ItemDoc, ParaDoc};
use crate::schema::{CATEGORIES, EDITIONS};
use crate::suggest::SuggestInput;
use crate::text::{collapse_ws, html_to_text, truncate_words};
use anyhow::{Context, Result};
use rusqlite::{Connection, OpenFlags};
use std::collections::HashMap;
use std::path::Path;
use std::time::Instant;

/// Search category and ranking weight of an `item.kind`; `None` = not indexed.
pub fn category_of(kind: &str) -> Option<(&'static str, f64)> {
    Some(match kind {
        "article" => ("article", 1.0),
        "yearbook-article" => ("article", 0.6),
        "about" | "feature" | "quickstart" | "yearbook-intro" => ("article", 0.4),
        "sidebar" => ("sidebar", 0.7),
        "further-reading" => ("other", 0.3),
        "media" | "audio" | "audio-set" | "video" | "interactive" | "composite-media" | "panorama"
        | "virtual-tour" => ("media", 0.55),
        "table" | "facts" => ("media", 0.5),
        "virtual-tour-node" | "maptrek-tour" | "maptrek-stop" => ("media", 0.4),
        "map" => ("map", 0.5),
        "map-about" => ("map", 0.4),
        "quotation" => ("quotation", 0.45),
        "quotation-author" => ("quotation", 0.5),
        "timeline-event" => ("timeline", 0.5),
        "timeline-topic" => ("timeline", 0.45),
        "weblink" | "live-link" => ("web", 0.25),
        "contributor" => ("other", 0.35),
        _ => return None, // index-term, home-page, home-background, home-theme, unclassified
    })
}

fn dict_weight(kind: &str) -> f64 {
    match kind {
        "dictionary" => 0.4,
        "thesaurus" => 0.3,
        _ => 0.2,
    }
}

fn cat_index(c: &str) -> u8 {
    CATEGORIES.iter().position(|x| *x == c).unwrap_or(CATEGORIES.len() - 1) as u8
}

fn edition_bit(e: &str) -> u8 {
    EDITIONS.iter().position(|x| *x == e).map_or(0, |i| 1 << i)
}

struct Item {
    ref_id: u64,
    kind: String,
    title: String,
    title_html: Option<String>,
    sort_title: Option<String>,
    full_title: Option<String>,
    alt_title: Option<String>,
    editions: u8,
    caption: Option<String>,
    credit: Option<String>,
    preview: Option<String>,
}

struct Para {
    ref_id: u64,
    kind: String,
    para_id: u64,
    section_id: Option<u64>,
    ordinal: i64,
    text: String,
}

fn year_label(y: i64, circa: bool) -> String {
    let base = if y < 0 { format!("{} BC", -y) } else { y.to_string() };
    if circa { format!("c. {base}") } else { base }
}

/// Builds `out_dir` (normally `out/search`) from the content database. The build goes to
/// `<out_dir>.partial` and is renamed into place at the end.
pub fn build_from_content_db(db: &Path, out_dir: &Path, opts: &BuildOptions) -> Result<BuildStats> {
    let t0 = Instant::now();
    let conn = Connection::open_with_flags(db, OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX)
        .with_context(|| format!("opening {}", db.display()))?;
    let partial = out_dir.with_extension("partial");
    if partial.exists() {
        std::fs::remove_dir_all(&partial)?;
    }
    let mut b = IndexBuilder::create(&partial, opts)?;
    let log = |msg: String| eprintln!("[search {:6.1}s] {msg}", t0.elapsed().as_secs_f64());

    // Popularity: cross-references into an item, plus articles that show it (media).
    let mut inlinks: HashMap<u64, u32> = HashMap::new();
    for sql in [
        "SELECT target_ref_id, COUNT(*) FROM link WHERE target_ref_id IS NOT NULL AND target_ref_id != ref_id GROUP BY target_ref_id",
        "SELECT target_ref_id, COUNT(*) FROM assoc WHERE direction = 'forward' AND target_ref_id IS NOT NULL GROUP BY target_ref_id",
    ] {
        let mut st = conn.prepare(sql)?;
        let mut rows = st.query([])?;
        while let Some(r) = rows.next()? {
            *inlinks.entry(r.get::<_, i64>(0)? as u64).or_default() += r.get::<_, i64>(1)? as u32;
        }
    }
    let pop = |r: u64| (1.0 + inlinks.get(&r).copied().unwrap_or(0) as f64).ln();
    log(format!("{} items with inbound links", inlinks.len()));

    // Items.
    let mut items: Vec<Item> = Vec::new();
    {
        let mut st = conn.prepare(
            "SELECT ref_id, kind, title, title_html, sort_title, full_title_html, alt_title_html, in_deluxe, in_kids,
                    caption_html, credit, preview_html FROM item ORDER BY ref_id",
        )?;
        let mut rows = st.query([])?;
        while let Some(r) = rows.next()? {
            let kind: String = r.get(1)?;
            if category_of(&kind).is_none() {
                continue;
            }
            let html = |i: usize| -> rusqlite::Result<Option<String>> {
                Ok(r.get::<_, Option<String>>(i)?.map(|h| html_to_text(&h)).filter(|s| !s.is_empty()))
            };
            items.push(Item {
                ref_id: r.get::<_, i64>(0)? as u64,
                kind,
                title: collapse_ws(&r.get::<_, Option<String>>(2)?.unwrap_or_default()),
                title_html: r.get(3)?,
                sort_title: r.get(4)?,
                full_title: html(5)?,
                alt_title: html(6)?,
                editions: (if r.get::<_, i64>(7)? != 0 { 1 } else { 0 }) | (if r.get::<_, i64>(8)? != 0 { 2 } else { 0 }),
                caption: html(9)?,
                credit: r.get(10)?,
                preview: html(11)?,
            });
        }
    }
    let item_pos: HashMap<u64, usize> = items.iter().enumerate().map(|(i, it)| (it.ref_id, i)).collect();
    log(format!("{} indexable items", items.len()));

    // Aliases (also the autocomplete source).
    let mut aliases: HashMap<u64, Vec<String>> = HashMap::new();
    // Whole-title keys (collection 0 of the word wheel) for the exact-title boost. Collection 1
    // holds partial keys ("Paris" for "Judgment of Paris") and must not count as exact titles.
    let mut exact_keys: HashMap<u64, Vec<String>> = HashMap::new();
    {
        let mut st = conn.prepare(
            "SELECT ref_id, edition, collection, sort_title, display_title, display_html FROM alias",
        )?;
        let mut rows = st.query([])?;
        while let Some(r) = rows.next()? {
            let ref_id = r.get::<_, i64>(0)? as u64;
            let edition: String = r.get(1)?;
            let collection: i64 = r.get(2)?;
            let sort: String = r.get::<_, Option<String>>(3)?.unwrap_or_default();
            let display: String = r.get::<_, Option<String>>(4)?.unwrap_or_else(|| sort.clone());
            let html: Option<String> = r.get(5)?;
            let Some(item) = item_pos.get(&ref_id).map(|&i| &items[i]) else { continue };
            let (cat, w) = category_of(&item.kind).unwrap();
            let coll_w = if collection == 0 { 1.0 } else { 0.6 };
            b.add_suggestion(SuggestInput {
                key_text: if sort.trim().is_empty() { display.clone() } else { sort.clone() },
                display: display.clone(),
                display_html: html.filter(|h| *h != display),
                title: (item.title != display).then(|| item.title.clone()),
                ref_id,
                category: cat_index(cat),
                editions: edition_bit(&edition),
                score: (w * coll_w * (1.0 + pop(ref_id))) as f32,
            });
            let names = if collection == 0 { vec![sort, display] } else { vec![display] };
            for s in names {
                let s = collapse_ws(&s);
                if s.is_empty() || s == item.title {
                    continue;
                }
                if collection == 0 {
                    let k = exact_keys.entry(ref_id).or_default();
                    if !k.contains(&s) {
                        k.push(s.clone());
                    }
                }
                let v = aliases.entry(ref_id).or_default();
                if !v.contains(&s) {
                    v.push(s);
                }
            }
        }
    }
    // Article titles that the word wheel spells differently ("Michelle Kwan" vs "Kwan, Michelle").
    for it in &items {
        let (cat, w) = category_of(&it.kind).unwrap();
        if cat == "article" && !it.title.is_empty() {
            b.add_suggestion(SuggestInput {
                key_text: it.title.clone(),
                display: it.title.clone(),
                display_html: it.title_html.clone().filter(|h| *h != it.title),
                title: None,
                ref_id: it.ref_id,
                category: cat_index(cat),
                editions: it.editions,
                score: (w * 0.9 * (1.0 + pop(it.ref_id))) as f32,
            });
        }
    }
    log(format!("{} items with aliases", aliases.len()));

    // Section titles.
    let mut sections: HashMap<(u64, u64), String> = HashMap::new();
    let mut headings: HashMap<u64, Vec<String>> = HashMap::new();
    {
        let mut st = conn.prepare("SELECT ref_id, section_id, title FROM section")?;
        let mut rows = st.query([])?;
        while let Some(r) = rows.next()? {
            let (rid, sid) = (r.get::<_, i64>(0)? as u64, r.get::<_, i64>(1)? as u64);
            let title = collapse_ws(&r.get::<_, Option<String>>(2)?.unwrap_or_default());
            if title.is_empty() {
                continue;
            }
            headings.entry(rid).or_default().push(title.clone());
            sections.insert((rid, sid), title);
        }
    }

    // Timeline dates and categories for summaries.
    let mut timeline: HashMap<u64, String> = HashMap::new();
    {
        let mut st = conn.prepare(
            "SELECT ref_id, start_year, start_circa, end_year, categories, regions FROM timeline_event",
        )?;
        let mut rows = st.query([])?;
        while let Some(r) = rows.next()? {
            let rid = r.get::<_, i64>(0)? as u64;
            let mut parts = Vec::new();
            if let Some(y) = r.get::<_, Option<i64>>(1)? {
                let mut s = year_label(y, r.get::<_, Option<i64>>(2)?.unwrap_or(0) != 0);
                if let Some(e) = r.get::<_, Option<i64>>(3)?.filter(|e| *e != y) {
                    s.push_str(&format!("–{}", year_label(e, false)));
                }
                parts.push(s);
            }
            for i in [4, 5] {
                if let Some(s) = r.get::<_, Option<String>>(i)?.filter(|s| !s.is_empty()) {
                    parts.push(s);
                }
            }
            timeline.insert(rid, parts.join(" · "));
        }
    }

    // Items merged with their paragraphs (both in ref_id order).
    let mut st = conn.prepare("SELECT ref_id, kind, para_id, section_id, ordinal, text FROM paragraph ORDER BY ref_id")?;
    let mut paras = st
        .query_map([], |r| {
            Ok(Para {
                ref_id: r.get::<_, i64>(0)? as u64,
                kind: r.get(1)?,
                para_id: r.get::<_, i64>(2)? as u64,
                section_id: r.get::<_, Option<i64>>(3)?.map(|v| v as u64),
                ordinal: r.get::<_, Option<i64>>(4)?.unwrap_or(0),
                text: collapse_ws(&r.get::<_, Option<String>>(5)?.unwrap_or_default()),
            })
        })?
        .peekable();
    let mut n = 0usize;
    for it in &items {
        let (cat, weight) = category_of(&it.kind).unwrap();
        let mut body = String::new();
        let mut heads = headings.remove(&it.ref_id).unwrap_or_default();
        let mut first: Option<(i64, String)> = None;
        // Skip paragraphs of refs that are not indexed items.
        while paras.peek().is_some_and(|p| p.as_ref().is_ok_and(|p| p.ref_id < it.ref_id)) {
            paras.next();
        }
        while paras.peek().is_some_and(|p| p.as_ref().map_or(true, |p| p.ref_id == it.ref_id)) {
            let p = paras.next().unwrap()?;
            if p.text.is_empty() {
                continue;
            }
            if p.kind == "heading" {
                heads.push(p.text.clone());
            } else {
                if !body.is_empty() {
                    body.push('\n');
                }
                body.push_str(&p.text);
                if matches!(p.kind.as_str(), "p" | "intro") && first.as_ref().is_none_or(|(o, _)| p.ordinal < *o) {
                    first = Some((p.ordinal, p.text.clone()));
                }
            }
            b.add_para(&ParaDoc {
                ref_id: it.ref_id,
                category: cat,
                item_kind: it.kind.clone(),
                editions: it.editions,
                section_title: p.section_id.and_then(|s| sections.get(&(it.ref_id, s)).cloned()),
                kind: p.kind,
                para_id: p.para_id,
                section_id: p.section_id,
                ordinal: p.ordinal.max(0) as u64,
                text: p.text,
            })?;
        }
        let mut al = aliases.remove(&it.ref_id).unwrap_or_default();
        let mut keys = exact_keys.remove(&it.ref_id).unwrap_or_default();
        for t in [&it.full_title, &it.alt_title, &it.sort_title].into_iter().flatten() {
            let t = collapse_ws(t);
            if !t.is_empty() && t != it.title && !al.contains(&t) {
                al.push(t.clone());
            }
            if !t.is_empty() && t != it.title && !keys.contains(&t) {
                keys.push(t);
            }
        }
        if body.is_empty() {
            for t in [&it.caption, &it.preview, &it.credit].into_iter().flatten() {
                if !body.is_empty() {
                    body.push('\n');
                }
                body.push_str(t);
            }
        }
        let summary = match (timeline.get(&it.ref_id), first.map(|f| f.1).or(it.preview.clone()).or(it.caption.clone())) {
            (Some(t), Some(s)) => format!("{t} — {}", truncate_words(&s, 280)),
            (Some(t), None) => t.clone(),
            (None, Some(s)) => truncate_words(&s, 300),
            (None, None) => it.full_title.clone().unwrap_or_default(),
        };
        b.add_item(&ItemDoc {
            ref_id: it.ref_id,
            category: cat,
            item_kind: it.kind.clone(),
            editions: it.editions,
            dictionary: None,
            title: it.title.clone(),
            title_html: it.title_html.clone(),
            aliases: al,
            keys,
            headings: heads,
            body,
            summary,
            prior: weight * (1.0 + 0.2 * pop(it.ref_id)) * if it.editions == 2 { 0.6 } else { 1.0 },
        })?;
        n += 1;
        if n.is_multiple_of(100_000) {
            log(format!("{n} items"));
        }
    }
    drop(paras);
    drop(st);
    log(format!("{n} items, {} paragraphs", b.stats().para_docs));

    // Dictionary entries.
    let kinds: HashMap<String, String> = conn
        .prepare("SELECT name, kind FROM dictionary")?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<rusqlite::Result<_>>()?;
    let mut forms: HashMap<(String, u64), Vec<String>> = HashMap::new();
    {
        let mut st = conn.prepare("SELECT dictionary, ref_id, form FROM dict_form WHERE kind != 'headword'")?;
        let mut rows = st.query([])?;
        while let Some(r) = rows.next()? {
            forms.entry((r.get(0)?, r.get::<_, i64>(1)? as u64)).or_default().push(r.get(2)?);
        }
    }
    let mut st = conn.prepare("SELECT dictionary, ref_id, headword, summary, html FROM dict_entry")?;
    let mut rows = st.query([])?;
    let mut nd = 0;
    while let Some(r) = rows.next()? {
        let dict: String = r.get(0)?;
        let ref_id = r.get::<_, i64>(1)? as u64;
        let headword = collapse_ws(&r.get::<_, Option<String>>(2)?.unwrap_or_default());
        let summary = collapse_ws(&r.get::<_, Option<String>>(3)?.unwrap_or_default());
        let body = html_to_text(&r.get::<_, Option<String>>(4)?.unwrap_or_default());
        let kind = kinds.get(&dict).cloned().unwrap_or_else(|| "dictionary".into());
        b.add_item(&ItemDoc {
            ref_id,
            category: "dictionary",
            prior: dict_weight(&kind),
            item_kind: kind,
            editions: 1,
            keys: Vec::new(),
            aliases: forms.remove(&(dict.clone(), ref_id)).unwrap_or_default(),
            dictionary: Some(dict),
            title: headword,
            title_html: None,
            headings: Vec::new(),
            body: body.chars().take(20_000).collect(),
            summary: truncate_words(&summary, 300),
        })?;
        nd += 1;
    }
    log(format!("{nd} dictionary entries"));

    // Lexicons for spelling.
    let names: Vec<String> =
        conn.prepare("SELECT name FROM lexicon ORDER BY rowid")?.query_map([], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?;
    let mut lex: Vec<(String, Vec<String>)> = names.into_iter().map(|n| (n, Vec::new())).collect();
    {
        let pos: HashMap<String, usize> = lex.iter().enumerate().map(|(i, (n, _))| (n.clone(), i)).collect();
        let mut st = conn.prepare("SELECT lexicon, word FROM lexicon_word")?;
        let mut rows = st.query([])?;
        while let Some(r) = rows.next()? {
            let name: String = r.get(0)?;
            if let Some(&i) = pos.get(&name) {
                lex[i].1.push(r.get(1)?);
            }
        }
    }
    log("indexing done, committing and merging".into());
    let stats = b.finish(opts, &lex)?;
    if out_dir.exists() {
        std::fs::remove_dir_all(out_dir)?;
    }
    std::fs::rename(&partial, out_dir)?;
    log(format!(
        "done: {} item docs, {} paragraph docs, index {:.0} MB, total {:.0} MB",
        stats.item_docs,
        stats.para_docs,
        stats.index_bytes as f64 / 1e6,
        stats.total_bytes as f64 / 1e6
    ));
    Ok(stats)
}
