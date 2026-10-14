//! `encarta-convert content`: articles and every other text source → `out/content.sqlite`.
//! Schema and source dialect: `docs/content-schema.md`. Completeness: `out/report/content.json`.

mod article;
mod data;
mod db;
mod dict;
mod entities;
mod extras;
mod inline;
mod model;
mod pe_wave;
mod render;
mod report;
mod searchx;
mod sources;
pub mod survey;
mod tables;
mod track;
mod trees;
mod woff2;
mod xml;

use crate::Paths;
use anyhow::{Context, Result};
use db::{Buf, Sink};
use encarta_formats::catalog::{Edition, Table};
use model::Inline;
use report::Report;
use sources::{Source, SourceKind};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::Instant;
use track::Tracker;

/// Catalog facts about one content id.
#[derive(Default, Clone)]
pub struct IdInfo {
    pub deluxe: bool,
    pub kids: bool,
    pub content: Option<String>,
    pub data: Option<String>,
    pub related: Option<String>,
}

/// What later passes need to know about an item from its DATA record.
#[derive(Default, Clone)]
pub struct Summary {
    pub title: Vec<Inline>,
    pub alt_title: Vec<Inline>,
    /// `hideappears="1"`: not shown inside the articles that place it.
    pub hide: bool,
    pub group: String,
    /// `(section, item, group, sequence)` of the forward associations the article places.
    pub placements: Vec<(u32, u32, String, i64)>,
    pub xml_files: Vec<String>,
    pub thumb: Option<String>,
    pub picon: Option<(String, Option<u32>)>,
    pub image: Option<(String, Option<u32>)>,
    /// `NoSquish="1"`: the picture may not be scaled down to the text column.
    pub no_squish: bool,
    /// `children/media` members (audio sets: the picture and the sound).
    pub media_children: Vec<u32>,
    pub preview_html: String,
    /// DATA `type` (sidebar category code).
    pub subtype: Option<String>,
    pub title_html: String,
    pub caption_html: String,
    pub outline: Option<String>,
}

/// Our normalised item kind for a DATA `group` (the original value is kept in `source_group`).
pub fn kind_for(group: &str) -> &'static str {
    match group {
        "article" | "kids" => "article",
        "sidebar" => "sidebar",
        "furtherread" => "further-reading",
        "table" | "tableinline" => "table",
        "factsandfigures" => "facts",
        "quotation" => "quotation",
        "author" => "quotation-author",
        "expert" => "contributor",
        "timeevent" => "timeline-event",
        "timetopic" => "timeline-topic",
        "archive" => "yearbook-article",
        "archive_intro" => "yearbook-intro",
        "maps" => "map",
        "aboutmap" => "map-about",
        "media" => "media",
        "audio" => "audio",
        "audionested" => "audio-set",
        "video" => "video",
        "flash" => "interactive",
        "composite" => "composite-media",
        "surround" => "panorama",
        "virttour" => "virtual-tour-node",
        "vtour" => "virtual-tour",
        "maptrektour" => "maptrek-tour",
        "maptrekstop" => "maptrek-stop",
        "weblink" => "weblink",
        "livemedia" => "live-link",
        "home" => "home-page",
        "hsbg" => "home-background",
        "hthm" => "home-theme",
        "feat" => "feature",
        "quickstart" => "quickstart",
        "about" => "about",
        "indx" => "index-term",
        "TBD" => "unclassified",
        _ => "other",
    }
}

fn fmt_ed(e: Edition) -> &'static str {
    match e {
        Edition::Deluxe => "deluxe",
        Edition::Kids => "kids",
    }
}

struct Ctx<'a> {
    extracted: &'a Path,
    sink: Sink,
    ids: HashMap<u32, IdInfo>,
    rep: Report,
    /// Index term id → term (first top-level title of its index tree).
    index_titles: HashMap<u32, String>,
}

pub fn run(paths: &Paths) -> Result<()> {
    let t0 = Instant::now();
    std::fs::create_dir_all(&paths.out)?;
    let final_path = paths.out.join("content.sqlite");
    let tmp = paths.out.join("content.sqlite.partial");
    let writer = db::Writer::start(&tmp)?;
    let mut cx = Ctx {
        extracted: &paths.extracted,
        sink: writer.sink().clone(),
        ids: HashMap::new(),
        rep: Report::default(),
        index_titles: HashMap::new(),
    };

    step(&mut cx, "catalog", catalogs)?;
    let mut titles = searchx::Titles {
        primary: HashMap::new(),
    };
    step(&mut cx, "search exports", |cx| search_exports(cx, &mut titles))?;

    let akc = sources::akc_sources(&paths.extracted)?;
    let by = |p: &str| -> Vec<&Source> {
        akc.iter()
            .filter(|s| {
                let l = s.label.trim_start_matches("akc/").to_ascii_lowercase();
                l.starts_with(p)
                    && !(p == "data" && l.starts_with("dataf"))
            })
            .collect()
    };
    let mut summaries: HashMap<u32, Summary> = HashMap::new();
    step(&mut cx, "data", |cx| {
        for s in by("data") {
            data_pass(cx, s, &titles, &mut summaries)?;
        }
        Ok(())
    })?;
    step(&mut cx, "index terms", |cx| {
        for s in by("dataf") {
            tree_pass(cx, s, "index_tree")?;
        }
        Ok(())
    })?;
    let md = md_files(&paths.extracted);
    let mut used_xml: HashSet<String> = HashSet::new();
    let mut articles: HashSet<u32> = HashSet::new();
    step(&mut cx, "articles", |cx| {
        for s in by("cont") {
            content_pass(cx, s, &summaries, &md, &mut used_xml, &mut articles)?;
        }
        orphan_sidebars(cx, &summaries, &md, &used_xml, &mut articles)
    })?;
    step(&mut cx, "related", |cx| {
        for s in by("rel") {
            tree_pass(cx, s, "related_tree")?;
        }
        Ok(())
    })?;
    step(&mut cx, "dictionaries", dictionaries)?;
    step(&mut cx, "small formats", |cx| small_formats(cx, &articles))?;
    step(&mut cx, "fonts", |cx| {
        let n = copy_fonts(cx.extracted, &paths.out.join("fonts"))?;
        cx.rep.counts.insert("fonts.copied".into(), n as u64);
        Ok(())
    })?;
    step(&mut cx, "ui sounds", |cx| {
        let n = write_ui_sounds(cx.extracted, &paths.out.join("sounds"))?;
        cx.rep.counts.insert("sounds.written".into(), n as u64);
        Ok(())
    })?;
    step(&mut cx, "items without data", |cx| {
        items_without_data(cx, &summaries, &titles);
        Ok(())
    })?;

    // Survey again so the report can compare the full inventory with what was consumed.
    let mut inventory = BTreeMap::new();
    for s in sources::all_xml_sources(&paths.extracted)? {
        inventory.insert(s.label.clone(), survey::survey_source(&s)?);
    }
    let mut b = Buf::new(&cx.sink);
    b.push("meta", vec![db::s("schema_version"), db::s(db::SCHEMA_VERSION)]);
    b.push(
        "meta",
        vec![db::s("converter"), db::s(concat!("encarta-convert ", env!("CARGO_PKG_VERSION")))],
    );
    drop(b);
    let Ctx { mut rep, sink, .. } = cx;
    drop(sink);
    let counts = writer.finish()?;
    // Audio records whose sound is inside an interactive (content-schema §10).
    {
        let conn = rusqlite::Connection::open(&tmp)?;
        let n = extras::interactive_audio(&conn)?;
        rep.counts.insert("audio.from_interactive".into(), n as u64);
    }
    std::fs::rename(&tmp, &final_path)?;
    rep.finish(&inventory, &counts, t0.elapsed().as_secs_f64());
    let dir = paths.out.join("report");
    std::fs::create_dir_all(&dir)?;
    std::fs::write(dir.join("content.json"), serde_json::to_vec_pretty(&rep.to_json())?)?;
    std::fs::write(dir.join("content-summary.txt"), rep.summary_text())?;
    eprintln!("{}", rep.headline());
    eprintln!("wrote {} in {:.1}s", final_path.display(), t0.elapsed().as_secs_f64());
    Ok(())
}

fn step(cx: &mut Ctx, name: &str, f: impl FnOnce(&mut Ctx) -> Result<()>) -> Result<()> {
    let t = Instant::now();
    f(cx).with_context(|| format!("content step {name}"))?;
    let s = t.elapsed().as_secs_f64();
    eprintln!("{name:<20} {s:>6.1}s");
    cx.rep.timings.push((name.to_string(), s));
    Ok(())
}

// ---- catalog ---------------------------------------------------------------------------------

fn catalogs(cx: &mut Ctx) -> Result<()> {
    let mut b = Buf::new(&cx.sink);
    for ed in [Edition::Deluxe, Edition::Kids] {
        let cat = sources::open_catalog(cx.extracted, ed)?;
        for t in [Table::Content, Table::Data, Table::Relart] {
            for (id, loc) in cat.ecn(t).unwrap().iter() {
                let e = cx.ids.entry(id).or_default();
                match ed {
                    Edition::Deluxe => e.deluxe = true,
                    Edition::Kids => e.kids = true,
                }
                let f = cat.package_file(loc.package).map(str::to_string);
                match t {
                    Table::Content => e.content = f,
                    Table::Data => e.data = e.data.take().or(f),
                    _ => e.related = e.related.take().or(f),
                }
                cx.rep.catalog_ids.entry((fmt_ed(ed), t.file_name())).or_default().push(id);
            }
        }
        for t in [Table::Baggage, Table::Generic] {
            for (key, loc) in cat.ecs(t).unwrap().iter() {
                b.push(
                    "baggage",
                    vec![
                        db::s(key),
                        db::s(fmt_ed(ed)),
                        db::os(cat.package_file(loc.package)),
                        db::s(t.file_name()),
                    ],
                );
                cx.rep.catalog_keys.entry((fmt_ed(ed), t.file_name())).or_default().push(key.to_string());
            }
        }
    }
    Ok(())
}

fn search_exports(cx: &mut Ctx, titles: &mut searchx::Titles) -> Result<()> {
    let dir = cx.extracted.join("finddlx");
    let mut t = Tracker::default();
    for (tla, ed) in [("DLX", "deluxe"), ("KDC", "kids")] {
        let st = searchx::harvest(&dir, tla, ed, &cx.sink, &mut t, titles)?;
        cx.rep.counts.insert(format!("search.{ed}.pfx_titles"), st.pfx[0] as u64);
        cx.rep.counts.insert(format!("search.{ed}.pfx_partial"), st.pfx[1] as u64);
        cx.rep.counts.insert(format!("search.{ed}.idxe_records"), st.idxe as u64);
        cx.rep.counts.insert(format!("search.{ed}.idxe_unparsed_slots"), st.idxe_failures);
        cx.rep.counts.insert(format!("search.{ed}.vocabulary"), st.words as u64);
    }
    cx.rep.add_tracker("search/FINDDLX", t);
    Ok(())
}

// ---- generic parallel pass -------------------------------------------------------------------

struct Acc {
    buf: Buf,
    t: Tracker,
    /// Second dialect met while converting this source (sidebar XML inside the CONT pass).
    t2: Tracker,
    errors: Vec<String>,
    done: Vec<(u32, &'static str)>,
    summaries: Vec<(u32, Summary)>,
    extra: Vec<(String, String)>,
}

fn new_acc(sink: &Sink) -> Acc {
    Acc {
        buf: Buf::new(sink),
        t: Tracker::default(),
        t2: Tracker::default(),
        errors: Vec::new(),
        done: Vec::new(),
        summaries: Vec::new(),
        extra: Vec::new(),
    }
}

fn merge(mut a: Acc, b: Acc) -> Acc {
    a.t.merge(b.t);
    a.t2.merge(b.t2);
    a.errors.extend(b.errors);
    a.done.extend(b.done);
    a.summaries.extend(b.summaries);
    a.extra.extend(b.extra);
    a
}

fn run_source(cx: &Ctx, s: &Source, f: impl Fn(&mut Acc, &str, &xml::Document) + Sync + Send) -> Result<Acc> {
    let sink = cx.sink.clone();
    s.par_fold(
        || new_acc(&sink),
        |mut acc, id, text| {
            acc.t.current = id.to_string();
            match xml::parse(text) {
                Ok(doc) => f(&mut acc, id, &doc),
                Err(e) => acc.errors.push(format!("{id}: {e:#}")),
            }
            acc
        },
        merge,
    )
}

fn finish_source(cx: &mut Ctx, s: &Source, acc: Acc) -> Acc {
    let mut acc = acc;
    acc.buf.flush();
    cx.rep.add_source(&s.label, s.len(), &acc.done, &acc.errors);
    cx.rep.add_tracker(&s.label, std::mem::take(&mut acc.t));
    acc
}

// ---- DATA ------------------------------------------------------------------------------------

fn html_or_null(v: &[Inline]) -> rusqlite::types::Value {
    if v.is_empty() {
        rusqlite::types::Value::Null
    } else {
        db::s(render::inlines_html(v))
    }
}

fn data_pass(cx: &mut Ctx, s: &Source, titles: &searchx::Titles, summaries: &mut HashMap<u32, Summary>) -> Result<()> {
    let ids = &cx.ids;
    let package = s.label.trim_start_matches("akc/").to_string();
    let acc = run_source(cx, s, |acc, id, doc| {
        if doc.root.name == "list" {
            for (rid, v) in data::experts(&mut acc.t, &doc.root) {
                acc.buf.push("contributor", vec![db::i(rid), db::s(v.to_string())]);
            }
            acc.done.push((0, "contributor list"));
            let _ = id;
            return;
        }
        let d = data::convert(&mut acc.t, &doc.root);
        write_item(acc, &d, ids.get(&d.ref_id), titles, &package);
        acc.done.push((d.ref_id, "item"));
        acc.summaries.push((d.ref_id, summary_of(&d)));
    })?;
    let mut acc = finish_source(cx, s, acc);
    for (k, v) in acc.summaries.drain(..) {
        summaries.insert(k, v);
    }
    for (k, v) in acc.extra {
        cx.rep.notes.push(format!("{}: {k} {v}", s.label));
    }
    Ok(())
}

fn summary_of(d: &data::DataItem) -> Summary {
    let file = |role: &str| d.files.iter().find(|f| f.role == role).map(|f| (f.name.clone(), f.width));
    Summary {
        title: d.title.clone(),
        alt_title: d.alt_title.clone(),
        picon: file("picon"),
        image: file("image"),
        no_squish: d.attr("NoSquish") == Some("1"),
        media_children: d.children.iter().filter(|c| c.0 == "media").map(|c| c.1).collect(),
        preview_html: if d.preview.is_empty() {
            String::new()
        } else {
            let mut h = String::new();
            article::render_blocks(&d.preview, &|_| None, &mut h);
            h
        },
        subtype: d.attr("type").map(str::to_string),
        hide: d.attr("hideappears") == Some("1"),
        title_html: render::inlines_html(&d.title),
        caption_html: render::inlines_html(&d.caption),
        group: d.group.clone(),
        placements: d
            .assocs
            .iter()
            .filter(|a| a.forward && a.appears.is_none())
            .map(|a| (a.sec.unwrap_or(1), a.target, a.group.clone(), a.sequence.unwrap_or(0)))
            .collect(),
        xml_files: d.files.iter().filter(|f| f.role == "xml").map(|f| f.name.clone()).collect(),
        thumb: d
            .files
            .iter()
            .find(|f| f.role == "thumb")
            .or_else(|| d.files.iter().find(|f| f.role == "picon"))
            .map(|f| f.name.clone()),
        outline: d.outline.as_ref().map(|o| o.to_string()),
    }
}

fn write_item(acc: &mut Acc, d: &data::DataItem, info: Option<&IdInfo>, titles: &searchx::Titles, package: &str) {
    let mut title = model::squash(&model::plain(&d.title));
    if title.is_empty() {
        title = data::fallback_title(d);
    }
    let (sort, display) = titles
        .best(d.ref_id, &title)
        .map_or((title.clone(), None), |(s, h)| (s, Some(h)));
    let num = |k: &str| d.attr(k).and_then(|v| v.trim().parse::<i64>().ok());
    let info = info.cloned().unwrap_or_default();
    acc.buf.push(
        "item",
        vec![
            db::i(d.ref_id),
            db::s(kind_for(&d.group)),
            db::s(d.group.clone()),
            db::s(title),
            html_or_null(&d.title),
            db::s(sort),
            db::os(display),
            html_or_null(&d.full_title),
            html_or_null(&d.alt_title),
            db::b(info.deluxe),
            db::b(info.kids),
            db::os(info.content.clone()),
            db::s(format!("{package}.akc")),
            db::os(info.related.clone()),
            db::os(d.attr("ticon")),
            db::os(d.attr("subgroup")),
            db::os(d.attr("type")),
            db::oi(num("style")),
            db::oi(num("geocode")),
            db::os(d.attr("origin")),
            db::oi(num("revision")),
            db::oi(num("wordcount")),
            html_or_null(&d.caption),
            if d.credit.is_empty() {
                rusqlite::types::Value::Null
            } else {
                db::s(model::squash(&model::plain(&d.credit)))
            },
            html_or_null(&d.credit),
            if d.preview.is_empty() {
                rusqlite::types::Value::Null
            } else {
                let mut h = String::new();
                article::render_blocks(&d.preview, &|_| None, &mut h);
                db::s(h)
            },
            db::s({
                let mut m = data::meta_json(d);
                if d.group == "sidebar" {
                    m["sidebar_category"] = sidebar_category(d.attr("type")).into();
                }
                m.to_string()
            }),
        ],
    );
    for (k, f) in d.files.iter().enumerate() {
        acc.buf.push(
            "item_file",
            vec![
                db::i(d.ref_id),
                db::i(k as i64),
                db::s(f.role.clone()),
                db::s(f.name.clone()),
                db::s(f.raw.clone()),
                db::oi(f.width),
                db::oi(f.height),
                db::oi(f.src_id),
                db::oi(f.disc),
                rusqlite::types::Value::Null,
            ],
        );
    }
    for (k, a) in d.assocs.iter().enumerate() {
        acc.buf.push(
            "assoc",
            vec![
                db::i(d.ref_id),
                db::s(if a.forward { "forward" } else { "reverse" }),
                db::i(k as i64),
                db::i(a.target),
                db::s(a.group.clone()),
                db::oi(a.sec),
                db::oi(a.sequence),
                db::oi(a.appears),
                db::oi(a.par),
                db::b(a.reviewed),
            ],
        );
    }
    for (k, (kind, id, seq)) in d.children.iter().enumerate() {
        acc.buf.push(
            "item_child",
            vec![db::i(d.ref_id), db::i(k as i64), db::s(kind.clone()), db::i(*id), db::oi(*seq)],
        );
    }
    if let (Some(tb), Some(h)) = (d.meta.get("table"), &d.table_html) {
        acc.buf.push(
            "data_table",
            vec![
                db::i(d.ref_id),
                db::s(tb["t"].as_str().unwrap_or("table")),
                db::s(tb.to_string()),
                db::s(h.clone()),
            ],
        );
    }
}

// ---- trees -----------------------------------------------------------------------------------

fn tree_pass(cx: &mut Ctx, s: &Source, table: &'static str) -> Result<()> {
    let acc = run_source(cx, s, |acc, _id, doc| {
        let c = trees::convert(&mut acc.t, &doc.root);
        for tr in &c.trees {
            let mut row = vec![db::i(c.ref_id), db::s(tr.editions.join(","))];
            if table == "index_tree" {
                row.push(db::s(c.title.clone()));
            }
            row.push(db::s(tr.json.clone()));
            row.push(db::i(tr.nodes));
            acc.buf.push(table, row);
            acc.t.note("tree editions", &tr.editions.join(","));
        }
        acc.done.push((c.ref_id, table));
        if table == "index_tree" {
            acc.extra.push((c.ref_id.to_string(), c.title.clone()));
        }
    })?;
    let acc = finish_source(cx, s, acc);
    for (k, v) in acc.extra {
        if let Ok(k) = k.parse() {
            cx.index_titles.entry(k).or_insert(v);
        }
    }
    Ok(())
}

// ---- articles --------------------------------------------------------------------------------

fn md_files(extracted: &Path) -> HashMap<String, PathBuf> {
    let mut m = HashMap::new();
    for s in sources::eit_sources(extracted) {
        if !s.label.starts_with("md/") {
            continue;
        }
        if let SourceKind::Files(f) = &s.kind {
            for (name, p) in f {
                m.entry(name.to_ascii_lowercase()).or_insert_with(|| p.clone());
            }
        }
    }
    m
}

fn media_info(summaries: &HashMap<u32, Summary>, r: u32) -> Option<article::MediaInfo> {
    summaries.get(&r).map(|s| article::MediaInfo {
        title_html: s.title_html.clone(),
        caption_html: s.caption_html.clone(),
        thumb: s.thumb.clone(),
        picon: s.picon.as_ref().map(|f| f.0.clone()),
        image: s.image.as_ref().map(|f| f.0.clone()),
        group: s.group.clone(),
        preview_html: s.preview_html.clone(),
        category: (s.group == "sidebar").then(|| sidebar_category(s.subtype.as_deref())),
    })
}

/// The label the reader prints above a sidebar teaser, by the sidebar's DATA `type` (observed in
/// the reference VM; codes not seen there, and lower-case `sbl`, show the generic "SIDEBAR").
pub fn sidebar_category(code: Option<&str>) -> &'static str {
    match code.unwrap_or("") {
        "SBD" => "HISTORIC DOCUMENTS",
        "SBG" => "GREAT MOMENTS IN SPORTS",
        "SBH" => "HISTORIC HEADLINES",
        "SBI" => "INTERVIEWS",
        "SBL" => "GREAT WORKS OF LITERATURE",
        "SBO" => "HISTORIC SPEECHES",
        "SBS" => "WORLD OF SCIENCE",
        "SBZ" => "POINTS OF THE COMPASS",
        "SHE" => "HISTORICAL ESSAYS",
        "SPC" => "POINT/COUNTERPOINT",
        _ => "SIDEBAR",
    }
}

/// Smallest width of a picture the reader shows across the text column (narrower pictures, and
/// pictures marked `NoSquish`, go to the left margin column).
const WIDE_PICTURE: u32 = 360;

/// Which column the encyclopedia reader draws a placed item in (rules from the VM captures,
/// `docs/content-schema.md` §2).
pub fn media_column(summaries: &HashMap<u32, Summary>, r: u32, group: &str) -> article::Column {
    use article::Column::*;
    let wide = |s: &Summary| s.image.as_ref().is_some_and(|f| f.1.unwrap_or(0) >= WIDE_PICTURE) && !s.no_squish;
    match group {
        "sidebar" | "video" => Text,
        "media" => match summaries.get(&r) {
            Some(s) if wide(s) => Text,
            _ => Margin,
        },
        "audionested" => {
            // An audio set shows its picture member.
            let pic = summaries.get(&r).and_then(|s| {
                s.media_children.iter().filter_map(|c| summaries.get(c)).find(|c| c.group == "media")
            });
            match pic {
                Some(p) if p.image.is_some() => {
                    if wide(p) {
                        Text
                    } else {
                        Margin
                    }
                }
                Some(p) if p.picon.as_ref().is_some_and(|f| f.1.unwrap_or(0) >= 216) => Text,
                _ => Margin,
            }
        }
        _ => Margin,
    }
}

/// Converts one body and queues its rows.
fn write_article(
    buf: &mut Buf,
    t: &mut Tracker,
    ref_id: u32,
    root: &xml::Element,
    source: &str,
    file: Option<&str>,
    summaries: &HashMap<u32, Summary>,
) {
    let sum = summaries.get(&ref_id);
    // Placements of items with no DATA record (not on this disc) or marked hidden are left out.
    let placements: Vec<article::Placement> = sum
        .map(|s| {
            s.placements
                .iter()
                .filter(|p| match summaries.get(&p.1) {
                    None => {
                        t.note("article", "placement of an item not on the disc dropped");
                        false
                    }
                    Some(m) => !m.hide,
                })
                .map(|(sec, r, g, seq)| article::Placement {
                    sec: *sec,
                    ref_id: *r,
                    group: g.clone(),
                    sequence: *seq,
                    col: media_column(summaries, *r, g),
                })
                .collect()
        })
        .unwrap_or_default();
    let layout = match sum.map(|s| s.group.as_str()) {
        Some("kids") => article::Layout::Kids,
        Some("sidebar") => article::Layout::Sidebar,
        _ => article::Layout::Encyclopedia,
    };
    let info = article::ItemInfo {
        title: sum.map(|s| s.title.as_slice()),
        alt_title: sum.map_or(&[], |s| s.alt_title.as_slice()),
        placements: &placements,
        layout,
    };
    if sum.is_none() {
        t.note("article", "no DATA record (title unknown)");
    }
    let a = article::convert(t, root, &info);
    let mut html = format!("<article class=\"enc-article\" data-ref=\"{ref_id}\">");
    article::render_blocks(&a.blocks, &|r| media_info(summaries, r), &mut html);
    html.push_str("</article>");
    let words: usize = a.paragraphs.iter().map(|p| p.3.split_whitespace().count()).sum();
    let title_refs = count_title_refs(&a.blocks);
    if a.duplicate_para_ids > 0 {
        t.note("article", "duplicate paragraph ids");
    }
    buf.push(
        "article",
        vec![
            db::i(ref_id),
            db::s(source),
            db::os(file),
            db::s(article::blocks_json(&a.blocks).to_string()),
            db::s(html),
            db::os(sum.and_then(|s| s.outline.clone())),
            db::i(a.paragraphs.len() as i64),
            db::i(a.sections.len() as i64),
            db::i(words as i64),
            db::i(title_refs as i64),
            db::s(layout.name()),
        ],
    );
    let mut seen = HashSet::new();
    for (sid, parent, level, number, title, ty) in &a.sections {
        if !seen.insert(*sid) {
            t.note("article", "duplicate section id");
            continue;
        }
        buf.push(
            "section",
            vec![
                db::i(ref_id),
                db::i(*sid),
                db::oi(*parent),
                db::i(*level),
                db::s(number.clone()),
                db::s(title.clone()),
                db::oi(*ty),
            ],
        );
    }
    let mut seen = HashSet::new();
    for (pid, sec, ord, text, kind) in &a.paragraphs {
        if !seen.insert((*kind, *pid)) {
            continue;
        }
        buf.push(
            "paragraph",
            vec![db::i(ref_id), db::s(*kind), db::i(*pid), db::oi(*sec), db::i(*ord), db::s(text.clone())],
        );
    }
    for (pid, ord, kind, tr, tp, url, text) in &a.links {
        buf.push(
            "link",
            vec![
                db::i(ref_id),
                db::oi(*pid),
                db::i(*ord),
                db::s(*kind),
                db::oi(*tr),
                db::oi(*tp),
                db::os(url.clone()),
                db::s(text.clone()),
            ],
        );
    }
}

fn count_title_refs(v: &[article::Block]) -> usize {
    fn inl(v: &[Inline]) -> usize {
        v.iter()
            .map(|n| match n {
                Inline::TitleRef { .. } => 1,
                Inline::Style(_, c) | Inline::Quote { c, .. } | Inline::Place { c, .. } => inl(c),
                Inline::Link(l) => inl(&l.c),
                _ => 0,
            })
            .sum()
    }
    v.iter()
        .map(|b| match b {
            article::Block::Para { c, .. }
            | article::Block::Intro { c, .. }
            | article::Block::Byline { c, .. }
            | article::Block::Heading { c, .. } => inl(c),
            article::Block::Section { blocks, .. } => count_title_refs(blocks),
            _ => 0,
        })
        .sum()
}

fn load_sidebar(md: &HashMap<String, PathBuf>, name: &str) -> Option<Result<xml::Document>> {
    let p = md.get(name)?;
    Some(
        std::fs::read(p)
            .map_err(anyhow::Error::from)
            .and_then(|b| xml::parse(&sources::decode_xml_bytes(&b))),
    )
}

fn content_pass(
    cx: &mut Ctx,
    s: &Source,
    summaries: &HashMap<u32, Summary>,
    md: &HashMap<String, PathBuf>,
    used_xml: &mut HashSet<String>,
    articles: &mut HashSet<u32>,
) -> Result<()> {
    let acc = run_source(cx, s, |acc, _id, doc| {
        let root = &doc.root;
        let ref_id: u32 = root.attr("refid").and_then(|v| v.parse().ok()).unwrap_or(0);
        if root.child("text").is_some() {
            write_article(&mut acc.buf, &mut acc.t, ref_id, root, "content", None, summaries);
            acc.done.push((ref_id, "article"));
            if summaries.get(&ref_id).is_some_and(|s| !s.xml_files.is_empty()) {
                acc.t.note("article", "body in CONT and an xml file in DATA (CONT used)");
            }
            return;
        }
        acc.t.el_all(root);
        // Empty <content/>: the body, if any, is a sidebar XML file named by the DATA record.
        let files = summaries.get(&ref_id).map(|s| s.xml_files.clone()).unwrap_or_default();
        if files.len() > 1 {
            acc.t.note("article", "several xml files (first used)");
        }
        match files.first() {
            Some(f) => match load_sidebar(md, f) {
                Some(Ok(d)) => {
                    acc.extra.push(("xml".into(), f.clone()));
                    write_article(&mut acc.buf, &mut acc.t2, ref_id, &d.root, "sidebar-xml", Some(f), summaries);
                    if d.root.attr("refid").and_then(|v| v.parse::<u32>().ok()) != Some(ref_id) {
                        acc.t2.note("sidebar", "file refid differs from the item that names it");
                    }
                    acc.done.push((ref_id, "article (sidebar xml)"));
                }
                Some(Err(e)) => acc.errors.push(format!("{f}: {e:#}")),
                None => {
                    acc.t.note("article", "DATA names an xml file that is not on the disc");
                    acc.done.push((ref_id, "empty (xml file missing)"));
                }
            },
            None => acc.done.push((ref_id, "empty (metadata-only item)")),
        }
    })?;
    let mut acc = finish_source(cx, s, acc);
    cx.rep.add_tracker("md", std::mem::take(&mut acc.t2));
    for (k, v) in &acc.extra {
        if k == "xml" {
            used_xml.insert(v.clone());
        }
    }
    for (id, st) in &acc.done {
        if st.starts_with("article") {
            articles.insert(*id);
        }
    }
    Ok(())
}

/// Sidebar XML files that no DATA record names: converted under their own root `refid` when that
/// item has no body yet, otherwise reported as superseded copies.
fn orphan_sidebars(
    cx: &mut Ctx,
    summaries: &HashMap<u32, Summary>,
    md: &HashMap<String, PathBuf>,
    used_xml: &HashSet<String>,
    articles: &mut HashSet<u32>,
) -> Result<()> {
    let mut names: Vec<&String> = md.keys().filter(|k| !used_xml.contains(*k)).collect();
    names.sort();
    let mut buf = Buf::new(&cx.sink);
    let mut t = Tracker::default();
    for n in names {
        let d = match load_sidebar(md, n) {
            Some(Ok(d)) => d,
            Some(Err(e)) => {
                cx.rep.baggage_xml.push((n.clone(), format!("parse error: {e:#}")));
                continue;
            }
            None => continue,
        };
        let rid: Option<u32> = d.root.attr("refid").and_then(|v| v.parse().ok());
        match rid {
            Some(r) if !articles.contains(&r) => {
                t.current = n.clone();
                write_article(&mut buf, &mut t, r, &d.root, "sidebar-xml", Some(n), summaries);
                articles.insert(r);
                let named = summaries.get(&r).is_some_and(|s| s.xml_files.contains(n));
                let why = if named {
                    format!("converted as the body of {r} (named by its DATA record; the item has no CONT record)")
                } else {
                    format!("converted as the body of {r} (by the file's refid; no DATA record names it)")
                };
                cx.rep.baggage_xml.push((n.clone(), why));
            }
            Some(r) => cx.rep.baggage_xml.push((
                n.clone(),
                format!("omitted: superseded copy; item {r} already has a body from the file its DATA record names"),
            )),
            None => cx.rep.baggage_xml.push((n.clone(), "omitted: no refid".into())),
        }
    }
    cx.rep.add_tracker("md", t);
    for n in used_xml {
        cx.rep.baggage_xml.push((n.clone(), "converted (named by its DATA record)".into()));
    }
    Ok(())
}

// ---- dictionaries ----------------------------------------------------------------------------

fn dict_kind(name: &str) -> &'static str {
    match name {
        "DICTA" | "DICT" => "dictionary",
        "THESA" | "THES" => "thesaurus",
        "TRANSL" => "translation",
        _ => "bilingual",
    }
}

/// The English name and identifiers from a `$_EDictSignature2007_$` file.
fn signature_row(t: &mut Tracker, name: &str, root: &xml::Element, entries: usize) -> Vec<rusqlite::types::Value> {
    let mut sig = Tracker::default();
    std::mem::swap(t, &mut sig);
    let v = extras::generic_json(t, root);
    std::mem::swap(t, &mut sig);
    t.merge(sig);
    let txt = |path: &[&str]| -> Option<String> {
        let mut e = root;
        for p in path {
            e = e.child(p)?;
        }
        Some(model::squash(&e.text()))
    };
    vec![
        db::s(name),
        db::s(dict_kind(name)),
        db::os(txt(&["Button", "Name", "A"])),
        db::os(txt(&["UniqTag"])),
        db::os(txt(&["LexFile"])),
        db::os(txt(&["Button", "GUID"])),
        db::os(txt(&["Button", "ParentGUID"])),
        db::i(entries as i64),
        db::s(v.to_string()),
    ]
}

fn dictionaries(cx: &mut Ctx) -> Result<()> {
    let srcs: Vec<Source> = sources::eit_sources(cx.extracted)
        .into_iter()
        .filter(|s| s.label.starts_with("edict/"))
        .collect();
    // Pass 1: every ID GUID → entry ref id (cross-references by GUID only).
    let mut guids: HashMap<String, u32> = HashMap::new();
    for s in &srcs {
        let v = s.par_fold(
            Vec::new,
            |mut acc, id, text| {
                if let (Ok(r), Ok(d)) = (id.parse::<u32>(), xml::parse(text)) {
                    dict::collect_guids(&d.root, r, &mut acc);
                }
                acc
            },
            |mut a, b| {
                a.extend(b);
                a
            },
        )?;
        guids.extend(v);
    }
    for s in &srcs {
        let name = s.label.trim_start_matches("edict/").to_string();
        let g = &guids;
        let acc = run_source(cx, s, |acc, id, doc| {
            let root = &doc.root;
            if root.name == "Signature" {
                let n = s.len() - 1;
                let row = signature_row(&mut acc.t, &name, root, n);
                acc.buf.push("dictionary", row);
                acc.done.push((0, "dictionary signature"));
                return;
            }
            let Ok(ref_id) = id.parse::<u32>() else {
                acc.errors.push(format!("{id}: not a numeric entry name"));
                return;
            };
            let mut dcx = dict::DictCx {
                t: &mut acc.t,
                guids: g,
                audio: Vec::new(),
            };
            let body = dict::convert(&mut dcx, root).unwrap_or(serde_json::Value::Null);
            let audio = std::mem::take(&mut dcx.audio);
            let forms = dict::forms(root);
            let headword = forms
                .iter()
                .find(|f| f.1 == "headword")
                .map(|f| dict::clean_headword(&f.0))
                .unwrap_or_default();
            let variety = root
                .elements()
                .find_map(|e| e.attr("Variety"))
                .map(str::to_string);
            let summary = dict::summary(root);
            let mut html = format!("<div class=\"enc-dict enc-dict-{}\" data-ref=\"{ref_id}\">", dict_kind(&name));
            dict::render_entry(&body, &mut html);
            html.push_str("</div>");
            acc.buf.push(
                "dict_entry",
                vec![
                    db::s(name.clone()),
                    db::i(ref_id),
                    db::s(headword),
                    db::os(variety),
                    db::ne(summary),
                    db::s(body.to_string()),
                    db::s(html),
                ],
            );
            for (f, k) in forms {
                let f = if k == "headword" { dict::clean_headword(&f) } else { f };
                acc.buf.push("dict_form", vec![db::s(f), db::s(name.clone()), db::i(ref_id), db::s(k)]);
            }
            for (f, r) in audio {
                acc.buf.push("dict_audio", vec![db::s(name.clone()), db::i(ref_id), db::s(f), db::oi(r)]);
            }
            acc.done.push((ref_id, "dictionary entry"));
        })?;
        finish_source(cx, s, acc);
    }
    Ok(())
}

// ---- fonts -----------------------------------------------------------------------------------

/// The `AREF/FONTS` files copied to `out/fonts/`: the MS Reference Serif and Sans text fonts the
/// web client's CSS uses, and the Tahoma, Times New Roman and Arial faces that the Flash
/// interactives name as device fonts (the web client hands all of them to Ruffle, web-client.md).
fn is_served_font(name: &str) -> bool {
    let up = name.to_ascii_uppercase();
    let Some(stem) = up.strip_suffix(".TTF") else {
        return false;
    };
    stem.starts_with("REFSER")
        || stem.starts_with("REFSAN")
        || stem.starts_with("TAHOMA")
        || stem.starts_with("TIMES")
        || matches!(stem, "ARIAL" | "ARIALBD" | "ARIALI")
}

/// Copies the fonts of [`is_served_font`] to `out/fonts/`, which the server serves as
/// `/fonts/{file}`. Each file is written to a temporary name and renamed, so a running server
/// never sees a half-written font.
fn copy_fonts(extracted: &Path, dir: &Path) -> Result<usize> {
    let src = extracted.join("iso/AREF/FONTS");
    let Ok(rd) = std::fs::read_dir(&src) else {
        return Ok(0);
    };
    std::fs::create_dir_all(dir)?;
    let mut n = 0;
    for e in rd {
        let e = e?;
        let name = e.file_name().to_string_lossy().to_string();
        if !is_served_font(&name) {
            continue;
        }
        let tmp = dir.join(format!("{name}.partial"));
        std::fs::copy(e.path(), &tmp).with_context(|| format!("copying {name}"))?;
        std::fs::rename(&tmp, dir.join(&name))?;
        n += 1;
        if is_web_font(&name) {
            write_woff2(&e.path(), &dir.join(Path::new(&name).with_extension("woff2")))?;
        }
    }
    Ok(n)
}

/// The faces the web client's CSS names (MS Reference Serif and Sans): these also get a WOFF2 copy.
fn is_web_font(name: &str) -> bool {
    let up = name.to_ascii_uppercase();
    up.ends_with(".TTF") && (up.starts_with("REFSER") || up.starts_with("REFSAN"))
}

/// `src` (a TrueType file) as WOFF2 at `dst`, via a temporary name like [`copy_fonts`].
fn write_woff2(src: &Path, dst: &Path) -> Result<()> {
    let w = woff2::encode(&std::fs::read(src)?).with_context(|| format!("WOFF2 of {}", src.display()))?;
    let tmp = dst.with_extension("woff2.partial");
    std::fs::write(&tmp, w)?;
    std::fs::rename(&tmp, dst)?;
    Ok(())
}

#[cfg(test)]
mod font_tests {
    use super::is_served_font;

    #[test]
    fn served_fonts() {
        for f in ["REFSER.TTF", "refsanbi.ttf", "TAHOMA.TTF", "TAHOMABD.TTF", "TIMESBI.TTF", "ARIAL.TTF", "ARIALBD.TTF", "ARIALI.TTF"] {
            assert!(is_served_font(f), "{f}");
        }
        for f in ["ARIALN.TTF", "MSGEOAR1.TTF", "REFSPCL.TTF", "VERDREF.TTF", "REF_ICON.FON", "TAHOMA.TTF.partial"] {
            assert!(!is_served_font(f), "{f}");
        }
    }
}

// ---- UI sounds -------------------------------------------------------------------------------

/// The interface sounds written to `out/sounds/{name}.wav` from the `WAVE` resources of the
/// application's DLLs (docs/media.md §9): `(file, DLL, resource)`. `startup` is the splash
/// sound; `click` is the button click (the Kids home plays it when a category is clicked); `kids-N`
/// plays when the pointer enters the N-th Kids home category.
fn ui_sound_sources() -> Vec<(String, &'static str, pe_wave::ResName)> {
    use pe_wave::ResName::{Id, Name};
    let mut v = vec![
        ("startup".to_string(), "EE/ENCARTA/ENCSPLSH.DLL", Name("INTROSOUND".into())),
        ("click".to_string(), "EE/ENCARTA/ENCARTAR.DLL", Id(31973)),
    ];
    for n in 1..=10u32 {
        v.push((format!("kids-{n}"), "EE/ENCARTA/ENCARTAR.DLL", Id(31978 + n)));
    }
    v
}

/// Writes the sounds of [`ui_sound_sources`] to `dir` (temporary name, then rename); returns how
/// many. A missing DLL or resource is skipped.
fn write_ui_sounds(extracted: &Path, dir: &Path) -> Result<usize> {
    let mut waves: HashMap<&str, Vec<(pe_wave::ResName, Vec<u8>)>> = HashMap::new();
    let mut n = 0;
    for (file, dll, res) in ui_sound_sources() {
        if !waves.contains_key(dll) {
            let found = match std::fs::read(extracted.join("iso").join(dll)) {
                Ok(d) => pe_wave::wave_resources(&d).with_context(|| format!("resources of {dll}"))?,
                Err(_) => Vec::new(),
            };
            waves.insert(dll, found);
        }
        let Some((_, bytes)) = waves[dll].iter().find(|(r, _)| *r == res) else { continue };
        std::fs::create_dir_all(dir)?;
        let tmp = dir.join(format!("{file}.wav.partial"));
        std::fs::write(&tmp, bytes)?;
        std::fs::rename(&tmp, dir.join(format!("{file}.wav")))?;
        n += 1;
    }
    Ok(n)
}

// ---- small formats ---------------------------------------------------------------------------

fn small_formats(cx: &mut Ctx, articles: &HashSet<u32>) -> Result<()> {
    let mut t = Tracker::default();
    let x = cx.extracted;
    let (ev, terms) = extras::timeline(x, &cx.sink, &mut t)?;
    cx.rep.counts.insert("timeline.records".into(), ev as u64);
    cx.rep.counts.insert("timeline.terms".into(), terms as u64);
    let (q, dead) = extras::mindmaze(x, &cx.sink, &mut t, articles)?;
    cx.rep.counts.insert("mindmaze.questions".into(), q as u64);
    cx.rep.counts.insert("mindmaze.answers_with_missing_article".into(), dead as u64);
    let (ind, vals) = extras::statistics(x, &cx.sink)?;
    cx.rep.counts.insert("stats.indicators".into(), ind as u64);
    cx.rep.counts.insert("stats.values".into(), vals as u64);
    cx.rep.counts.insert("geoquiz.places".into(), extras::geoquiz(x, &cx.sink)? as u64);
    let (pl, links) = extras::place_links(x, &cx.sink)?;
    cx.rep.counts.insert("p2m.places".into(), pl as u64);
    cx.rep.counts.insert("p2m.links".into(), links as u64);
    cx.rep.counts.insert("atlas_media.records".into(), extras::atlas_media(x, &cx.sink, &mut t)? as u64);
    for (n, w) in extras::lexicons(x, &cx.sink)? {
        cx.rep.counts.insert(format!("lexicon.{n}"), w as u64);
    }
    cx.rep.add_tracker("small-formats", t);
    let mut qt = Tracker::default();
    cx.rep.counts.insert("quiz.files".into(), extras::quizzes(x, &cx.sink, &mut qt)? as u64);
    cx.rep.add_tracker("iax/SWQUIZ.EIT", qt.clone());
    cx.rep.add_tracker("iax/KIDS/SWQUIZ.EIT", qt);
    let iax = sources::eit_sources(cx.extracted);
    let mut ts = Vec::new();
    cx.rep.counts.insert("interactive.files".into(), extras::interactives(&iax, &cx.sink, &mut ts)? as u64);
    for (l, t) in ts {
        cx.rep.add_tracker(&l, t);
    }
    Ok(())
}

/// Catalogued ids with no DATA record still get an `item` row.
fn items_without_data(cx: &mut Ctx, summaries: &HashMap<u32, Summary>, titles: &searchx::Titles) {
    let mut b = Buf::new(&cx.sink);
    let mut ids: Vec<(&u32, &IdInfo)> = cx.ids.iter().filter(|(k, _)| !summaries.contains_key(k)).collect();
    ids.sort_by_key(|x| *x.0);
    let mut n = 0;
    for (id, info) in ids {
        if info.data.as_deref().is_some_and(|d| d.to_ascii_lowercase().starts_with("dataf")) {
            // index terms: their DATA record is the index tree itself
            let mut row = vec![db::i(*id), db::s("index-term"), db::s("indx")];
            let title0 = cx.index_titles.get(id).cloned().unwrap_or_default();
            let (sort, disp) = titles.best(*id, &title0).unzip();
            let title = cx.index_titles.get(id).cloned().or(sort.clone());
            row.push(db::os(title.clone()));
            row.push(db::os(title.as_deref().map(render::escaped)));
            row.push(db::os(sort.or(title)));
            row.push(db::os(disp));
            fill_item(&mut row, info);
            b.push("item", row);
        } else {
            n += 1;
            let (sort, disp) = titles.best(*id, "").unzip();
            let mut row = vec![db::i(*id), db::s("other"), rusqlite::types::Value::Null];
            row.push(db::os(sort.clone()));
            row.push(rusqlite::types::Value::Null);
            row.push(db::os(sort));
            row.push(db::os(disp));
            fill_item(&mut row, info);
            b.push("item", row);
        }
    }
    cx.rep.counts.insert("item.without_data_record".into(), n);
}

fn fill_item(row: &mut Vec<rusqlite::types::Value>, info: &IdInfo) {
    use rusqlite::types::Value::Null;
    row.extend([Null, Null]); // full/alt title
    row.push(db::b(info.deluxe));
    row.push(db::b(info.kids));
    row.push(db::os(info.content.clone()));
    row.push(db::os(info.data.clone()));
    row.push(db::os(info.related.clone()));
    while row.len() < 27 {
        row.push(Null);
    }
}

/// Whole-disc checks on known articles (skipped without `extracted/`). When the private VM
/// captures exist (`fixtures/articles/<id>/article.txt`), paragraph text is compared with them.
#[cfg(test)]
mod disc_tests {
    use super::*;
    use encarta_formats::akc::{Library, xml_text};

    fn root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    fn extracted() -> Option<PathBuf> {
        let p = std::env::var_os("ENCARTA_EXTRACTED").map_or(root().join("extracted"), PathBuf::from);
        p.join("iso/EE/ENCARTA/CONTSTD.AKC").exists().then_some(p)
    }

    #[test]
    fn ui_sounds_extract() {
        let Some(x) = extracted() else { return };
        let dir = root().join("target/test-ui-sounds");
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(write_ui_sounds(&x, &dir).unwrap(), 12);
        let startup = std::fs::read(dir.join("startup.wav")).unwrap();
        assert_eq!(&startup[..4], b"RIFF");
        assert_eq!(startup.len(), 445_980);
        assert_eq!(std::fs::read(dir.join("click.wav")).unwrap().len(), 1128);
        assert_eq!(std::fs::read(dir.join("kids-1.wav")).unwrap().len(), 125_480);
    }

    fn convert(lib: &Library, id: u32) -> (data::DataItem, article::Article, String) {
        let mut t = Tracker::default();
        let d = lib.data(id).unwrap().expect("data record");
        let ddoc = xml::parse(xml_text(&d.bytes).unwrap()).unwrap();
        let item = data::convert(&mut t, &ddoc.root);
        let sum = summary_of(&item);
        let c = lib.content(id).unwrap().expect("content record");
        let cdoc = xml::parse(xml_text(&c.bytes).unwrap()).unwrap();
        // As `write_article`: placements of items not on the disc or marked hidden are left out.
        let shown = |r: u32| {
            lib.data(r).unwrap().is_some_and(|d| {
                let doc = xml::parse(xml_text(&d.bytes).unwrap()).unwrap();
                !summary_of(&data::convert(&mut Tracker::default(), &doc.root)).hide
            })
        };
        let placements: Vec<article::Placement> = sum
            .placements
            .iter()
            .filter(|p| !article::is_inline_group(&p.2) || shown(p.1))
            .map(|(sec, r, g, seq)| article::Placement {
                sec: *sec,
                ref_id: *r,
                group: g.clone(),
                sequence: *seq,
                col: article::Column::Margin,
            })
            .collect();
        let a = article::convert(
            &mut t,
            &cdoc.root,
            &article::ItemInfo {
                title: Some(&sum.title),
                alt_title: &sum.alt_title,
                placements: &placements,
                layout: article::Layout::Encyclopedia,
            },
        );
        let mut html = String::new();
        article::render_blocks(&a.blocks, &|_| None, &mut html);
        assert!(!t.notes.contains_key("unmapped inline"), "{:?}", t.notes);
        (item, a, html)
    }

    fn norm(s: &str) -> String {
        model::squash(&s.replace('\u{a0}', " "))
    }

    fn check_fixture(id: u32, a: &article::Article) {
        let p = root().join(format!("fixtures/articles/{id}/article.txt"));
        let Ok(txt) = std::fs::read_to_string(&p) else {
            return;
        };
        let txt = norm(&txt);
        for (pid, _, _, text, _) in &a.paragraphs {
            assert!(txt.contains(&norm(text)), "{id} paragraph {pid} differs from the VM capture: {text}");
        }
    }

    #[test]
    fn zebra_and_greek_art() {
        let Some(x) = extracted() else {
            eprintln!("skipped: no extracted/ tree");
            return;
        };
        let lib = Library::open(&x, Edition::Deluxe).unwrap();

        let (item, a, html) = convert(&lib, 761575094);
        assert_eq!(model::plain(&item.title), "Zebra");
        assert_eq!(item.group, "article");
        assert_eq!(a.sections.len(), 10);
        assert_eq!(a.sections[0].4, "INTRODUCTION");
        // the source uses no-break spaces after the inline title
        assert!(a.paragraphs[0].3.replace('\u{a0}', " ").starts_with("Zebra, member of the horse family"));
        assert!(a.links.iter().any(|l| l.3 == Some(761562654)));
        assert!(html.contains("<span class=\"enc-title-ref\">Zebra</span>,"));
        assert!(html.contains("data-ref=\"461550745\""), "media placed in the introduction");
        match &a.blocks[0] {
            article::Block::Section { type_code, blocks, .. } => {
                assert_eq!(*type_code, Some(4));
                // The VM shows Grévy's Zebra and the Mountain Zebra sound in one cluster.
                assert!(matches!(&blocks[0], article::Block::Cluster(c) if c[0].ref_id == 461550745 && c[1].ref_id == 461517535));
            }
            b => panic!("{b:?}"),
        }
        check_fixture(761575094, &a);

        let (item, a, _) = convert(&lib, 761561691);
        assert!(!model::plain(&item.title).is_empty());
        assert!(a.sections.len() > 5 && a.paragraphs.len() > 20 && !a.links.is_empty());
        check_fixture(761561691, &a);

        for id in [761562654, 761596181] {
            let (_, a, _) = convert(&lib, id);
            check_fixture(id, &a);
        }
    }
    /// The tokens of an article body in reading order: `('s', id)` section, `('p', id)` paragraph,
    /// `('a', ref, cluster)` placed item (`cluster` numbers the box it shares, 0 when alone).
    type Tok = (char, u32, u32);

    /// Reads the tokens from a VM capture's DOM (`fixtures/articles/<id>/article.html`).
    fn vm_tokens(html: &str) -> Vec<Tok> {
        let body = &html[html.find("<BODY").unwrap_or(0)..];
        let body = &body[..body.find("id=ModuleColumn").unwrap_or(body.len())];
        let mut out = Vec::new();
        let mut cluster = 0;
        for tag in body.split('<').skip(1) {
            let head = &tag[..tag.find('>').unwrap_or(tag.len())];
            let name = head.split([' ', '\t', '\n', '\r']).next().unwrap_or("");
            let Some(at) = head.find(" id=").or_else(|| head.find("\nid=")) else {
                continue;
            };
            let id = head[at + 4..].trim_start_matches('"');
            let id = &id[..id.find(['"', ' ', '\r', '\n']).unwrap_or(id.len())];
            let split = id.find(|c: char| c.is_ascii_digit()).unwrap_or(id.len());
            let (kind, num) = id.split_at(split);
            let Ok(n) = num.parse::<u32>() else {
                continue;
            };
            match (name, kind) {
                ("DIV", "s") => {
                    out.push(('s', n, 0));
                    cluster = 0;
                }
                ("P", "p") => {
                    out.push(('p', n, 0));
                    cluster = 0;
                }
                ("DIV", "MediaClusterCont") => cluster = n + 1,
                ("SPAN", "a") if cluster > 0 => out.push(('a', n, cluster)),
                ("DIV" | "TABLE", "a") => {
                    out.push(('a', n, 0));
                    cluster = 0;
                }
                _ => {}
            }
        }
        out
    }

    fn our_tokens(blocks: &[article::Block], out: &mut Vec<Tok>, clusters: &mut u32) {
        use article::Block::*;
        for b in blocks {
            match b {
                Section { id, blocks, .. } => {
                    out.push(('s', *id, 0));
                    our_tokens(blocks, out, clusters);
                }
                Para { id: Some(id), .. } | Intro { id: Some(id), .. } => out.push(('p', *id, 0)),
                Media { ref_id, .. } => out.push(('a', *ref_id, 0)),
                Cluster(items) => {
                    *clusters += 1;
                    out.extend(items.iter().map(|m| ('a', m.ref_id, *clusters)));
                }
                _ => {}
            }
        }
    }

    /// Each placed item's section and the paragraph it precedes (`u32::MAX`: after the last
    /// one), the items in order, and the members of the box each one is in.
    #[allow(clippy::type_complexity)]
    fn positions(toks: &[Tok]) -> (HashMap<u32, (u32, u32)>, Vec<u32>, HashMap<u32, Vec<u32>>) {
        let (mut pos, mut order, mut boxes) = (HashMap::new(), Vec::new(), HashMap::new());
        let mut sec = 0;
        for (i, t) in toks.iter().enumerate() {
            match t.0 {
                's' => sec = t.1,
                'a' if !pos.contains_key(&t.1) => {
                    let next = toks[i + 1..].iter().take_while(|x| x.0 != 's').find(|x| x.0 == 'p');
                    pos.insert(t.1, (sec, next.map_or(u32::MAX, |x| x.1)));
                    order.push(t.1);
                    let mut members: Vec<u32> = if t.2 == 0 {
                        vec![t.1]
                    } else {
                        toks.iter().filter(|x| x.0 == 'a' && x.2 == t.2).map(|x| x.1).collect()
                    };
                    members.sort();
                    boxes.insert(t.1, members);
                }
                _ => {}
            }
        }
        (pos, order, boxes)
    }

    /// Media placement (content-schema §2) against every encyclopedia article captured in the VM:
    /// the paragraph each item precedes, the order of the items and the cluster boxes.
    #[test]
    fn placement_matches_vm_captures() {
        let Some(x) = extracted() else {
            eprintln!("skipped: no extracted/ tree");
            return;
        };
        let Ok(dirs) = std::fs::read_dir(root().join("fixtures/articles")) else {
            eprintln!("skipped: no VM captures");
            return;
        };
        let lib = Library::open(&x, Edition::Deluxe).unwrap();
        let (mut items, mut para, mut pairs, mut ordered, mut boxed) = (0, 0, 0, 0, 0);
        let mut misses = Vec::new();
        let mut ids: Vec<u32> = dirs.filter_map(|d| d.ok()?.file_name().to_str()?.parse().ok()).collect();
        ids.sort();
        for id in ids {
            let Ok(html) = std::fs::read(root().join(format!("fixtures/articles/{id}/article.html"))) else {
                continue;
            };
            let html = String::from_utf8_lossy(&html);
            if html.contains("##### frame") || !lib.content(id).unwrap().is_some_and(|c| !c.bytes.is_empty()) {
                continue;
            }
            let (item, a, _) = convert(&lib, id);
            if item.group != "article" {
                continue;
            }
            let mut ours = Vec::new();
            our_tokens(&a.blocks, &mut ours, &mut 0);
            let (vp, vo, vb) = positions(&vm_tokens(&html));
            let (op, oo, ob) = positions(&ours);
            let common: Vec<u32> = vo.iter().copied().filter(|r| op.contains_key(r)).collect();
            let at: HashMap<u32, usize> = oo.iter().enumerate().map(|(i, r)| (*r, i)).collect();
            pairs += common.len().saturating_sub(1);
            ordered += common.windows(2).filter(|w| at[&w[0]] < at[&w[1]]).count();
            for r in &common {
                items += 1;
                let shared = |b: &HashMap<u32, Vec<u32>>| -> Vec<u32> {
                    b[r].iter().copied().filter(|x| op.contains_key(x) && vp.contains_key(x)).collect()
                };
                if vp[r] == op[r] {
                    para += 1;
                } else {
                    misses.push(format!("{id}: {r} VM {:?}, ours {:?}", vp[r], op[r]));
                }
                if shared(&vb) == shared(&ob) {
                    boxed += 1;
                }
            }
        }
        let pct = |a: usize, b: usize| 100.0 * a as f64 / b.max(1) as f64;
        eprintln!(
            "{items} items: paragraph {:.1} %, order {:.1} %, cluster {:.1} %\n{}",
            pct(para, items),
            pct(ordered, pairs),
            pct(boxed, items),
            misses.join("\n")
        );
        assert!(items > 1600, "{items} items compared");
        assert!(pct(para, items) >= 99.0 && pct(ordered, pairs) >= 99.5 && pct(boxed, items) >= 99.5);
    }
}
