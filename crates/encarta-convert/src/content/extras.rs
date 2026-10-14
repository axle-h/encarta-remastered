//! The small non-AKC sources: timeline, MindMaze, world statistics, Geography Quiz, place links,
//! the atlas media catalog, spelling/headword lexicons and the SWQUIZ quizzes.

use super::db::{self, Buf, Sink};
use super::model;
use super::render;
use super::searchx::markup;
use super::sources::decode_xml_bytes;
use super::track::Tracker;
use super::xml::{self, Element};
use anyhow::{Context, Result};
use encarta_formats::{itir_catalog, lex, mindmaze, msworld, p2m, timeline};
use serde_json::{Map, Value, json};
use std::collections::HashSet;
use std::path::Path;

fn read(p: &Path) -> Result<Vec<u8>> {
    std::fs::read(p).with_context(|| format!("reading {}", p.display()))
}

fn scan_catalog(dir: &Path) -> Result<Vec<itir_catalog::CatalogEntry>> {
    let c = dir.join("$WW$CTCATALOG");
    itir_catalog::scan(&read(&c.join("BTREE"))?, &read(&c.join("DATA"))?)
}

/// `(plain, html)` of a string with Encarta title markup.
fn text_html(t: &mut Tracker, s: &str) -> (String, String) {
    let v = markup(t, s);
    (model::plain(&v), render::inlines_html(&v))
}

pub fn timeline(extracted: &Path, sink: &Sink, t: &mut Tracker) -> Result<(usize, usize)> {
    let tl = timeline::parse(&read(&extracted.join("iso/EE/ENCARTA/TIMELINE.DAT"))?)?;
    let mut b = Buf::new(sink);
    for c in &tl.categories {
        b.push("timeline_category", vec![db::i(c.bit), db::s(c.name.clone())]);
    }
    for r in &tl.regions {
        b.push("timeline_region", vec![db::i(r.bit), db::s(r.name.clone())]);
    }
    let date = |d: &Option<timeline::Date>| {
        (
            db::oi(d.map(|d| d.year)),
            db::oi(d.map(|d| d.month).filter(|&m| m != 0)),
            db::oi(d.map(|d| d.day).filter(|&x| x != 0)),
            db::oi(d.map(|d| d.circa as i64)),
        )
    };
    for (table, recs) in [("event", &tl.events), ("item", &tl.items)] {
        for r in recs {
            let (plain, html) = text_html(t, &r.title);
            let (sy, sm, sd, sc) = date(&r.start);
            let (ey, em, ed, ec) = date(&r.end);
            b.push(
                "timeline_event",
                vec![
                    db::i(r.seq),
                    db::i(r.ref_id),
                    db::s(table),
                    db::i(r.kind),
                    db::ne(plain.clone()),
                    db::ne(html),
                    sy,
                    sm,
                    sd,
                    sc,
                    ey,
                    em,
                    ed,
                    ec,
                    db::b(r.span),
                    db::i(r.importance),
                    db::i(r.category_mask),
                    db::i(r.region_mask),
                    db::s(tl.category_names(r.category_mask).join(", ")),
                    db::s(tl.region_names(r.region_mask).join(", ")),
                    db::i(r.flags),
                    db::os(timeline_display(r)),
                    db::os(timeline_label(r, &plain)),
                    db::os(timeline_date_text(r)),
                    db::os(r.start.as_ref().map(|d| date_text(d, false))),
                    db::os(r.end.as_ref().map(|d| date_text(d, r.start.is_some_and(|s| s.year < 0) && d.year > 0))),
                    db::i(timeline_prominence(r.importance)),
                ],
            );
        }
    }
    // Term index: texts are the TIMELINE.EIT catalog titles, item k ↔ term k.
    let cat = scan_catalog(&extracted.join("eit/EE/ENCARTA/TIMELINE.EIT"))?;
    let texts: std::collections::HashMap<u32, &str> = cat
        .iter()
        .filter_map(|e| e.properties.first().map(|p| (e.key, p.trim_end_matches('\0'))))
        .collect();
    for (k, term) in tl.terms.iter().enumerate() {
        let text = texts.get(&(k as u32)).copied().unwrap_or_default();
        let (plain, html) = text_html(t, text);
        b.push(
            "timeline_term",
            vec![
                db::i(k as i64),
                db::s(plain),
                db::s(html),
                db::s(serde_json::to_string(&term.ref_ids)?),
            ],
        );
    }
    Ok((tl.events.len() + tl.items.len(), tl.terms.len()))
}

/// How the Timeline view draws a record (VM: flag 0x01 is an orange period bar; kind 3 is a
/// striped essay bar whatever its flags; everything else is a point, a square and its title).
pub fn timeline_display(r: &timeline::Record) -> Option<&'static str> {
    match r.kind {
        0 => None,
        3 => Some("essay"),
        _ if r.span => Some("period"),
        _ => Some("point"),
    }
}

/// The boxed upper-case label of a bar ("WORLD WAR II", "HISTORICAL ESSAY: RISE OF PASTORALISM IN ASIA").
pub fn timeline_label(r: &timeline::Record, plain: &str) -> Option<String> {
    match timeline_display(r)? {
        "essay" => Some(format!("HISTORICAL ESSAY: {}", plain.to_uppercase())),
        "period" => Some(plain.to_uppercase()),
        _ => None,
    }
}

/// Title size rank: importance 1 is drawn with the largest title, 5 the smallest (0 = unranked).
pub fn timeline_prominence(importance: u16) -> i64 {
    if (1..=5).contains(&importance) { 6 - importance as i64 } else { 0 }
}

const MONTHS: [&str; 12] = [
    "January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November",
    "December",
];

/// One date as the pop-up prints it (VM 2026-10-14, 23 events). Circa is a trailing "?" on the year ("4? BC"); `ad`
/// prefixes "AD" (used for the end of a range that starts BC); "June 16, 1963". Years before 8000 BC are counted
/// back from 2000 without the circa mark: "120,000 years ago" (118000? BC), "11,000 years ago" (9000? BC), and from
/// a million on with two decimals: "2.50 million years ago", "4.60 billion years ago". 8000 BC stays "8000 BC"
/// (no event lies between 9000 and 8000 BC, so the exact cut is not known).
pub fn date_text(d: &timeline::Date, ad: bool) -> String {
    let q = if d.circa { "?" } else { "" };
    let mag = d.year.unsigned_abs();
    if d.year < -8000 {
        let ago = mag + 2000;
        return if ago >= 1_000_000_000 {
            format!("{:.2} billion years ago", ago as f64 / 1e9)
        } else if ago >= 1_000_000 {
            format!("{:.2} million years ago", ago as f64 / 1e6)
        } else {
            format!("{},{:03} years ago", ago / 1000, ago % 1000)
        };
    }
    let year = if d.year < 0 { format!("{mag}{q} BC") } else if ad { format!("AD {mag}{q}") } else { format!("{mag}{q}") };
    match (d.month, d.day) {
        (m @ 1..=12, day @ 1..=31) => format!("{} {day}, {year}", MONTHS[m as usize - 1]),
        (m @ 1..=12, _) => format!("{} {year}", MONTHS[m as usize - 1]),
        _ => year,
    }
}

/// The pop-up date line: "4? BC - AD 29?" (VM, Jesus Christ), or one date.
pub fn timeline_date_text(r: &timeline::Record) -> Option<String> {
    let s = r.start.as_ref()?;
    Some(match &r.end {
        Some(e) => format!("{} - {}", date_text(s, false), date_text(e, s.year < 0 && e.year > 0)),
        None => date_text(s, false),
    })
}

pub fn mindmaze(extracted: &Path, sink: &Sink, t: &mut Tracker, articles: &HashSet<u32>) -> Result<(usize, usize)> {
    let d = extracted.join("iso/EE/ENCARTA");
    let qs = mindmaze::parse(&read(&d.join("MINDMAZE.DB"))?, &read(&d.join("MINDMAZE.IDX"))?)?;
    let mut b = Buf::new(sink);
    let mut dead = 0;
    for q in &qs {
        let (plain, html) = text_html(t, &q.clue);
        let cat = q.category();
        b.push(
            "mindmaze_question",
            vec![
                db::i(q.id),
                db::oi(cat.map(|c| c as i64)),
                db::os(cat.map(|c| mindmaze::CATEGORY_LABELS[c])),
                db::oi(q.level().map(|l| l as i64)),
                db::s(plain),
                db::s(html),
                db::i(q.answers[0].location),
            ],
        );
        for (k, a) in q.answers.iter().enumerate() {
            let (plain, html) = text_html(t, &a.text);
            let alive = articles.contains(&a.article_id);
            dead += !alive as usize;
            b.push(
                "mindmaze_answer",
                vec![
                    db::i(q.id),
                    db::i(k as i64),
                    db::b(k == 0),
                    db::s(plain),
                    db::s(html),
                    db::i(a.article_id),
                    db::b(alive),
                ],
            );
        }
    }
    Ok((qs.len(), dead))
}

pub fn statistics(extracted: &Path, sink: &Sink) -> Result<(usize, usize)> {
    let sdb = msworld::parse_sdb(&read(&extracted.join("iso/EE/ENCARTA/MSWORLD.SDB"))?)?;
    let mut b = Buf::new(sink);
    for c in &sdb.countries {
        b.push("stat_country", vec![db::i(c.id), db::s(c.name.clone())]);
    }
    for c in &sdb.categories {
        b.push("stat_category", vec![db::i(c.bit), db::s(c.name.clone())]);
    }
    for q in &sdb.qualifiers {
        b.push("stat_qualifier", vec![db::i(q.id), db::s(q.text.clone())]);
    }
    for u in &sdb.units {
        b.push(
            "stat_unit",
            vec![
                db::i(u.id),
                db::s(u.unit.clone()),
                db::os(u.alt_unit.clone()),
                db::f(u.factor),
                db::f(u.offset),
            ],
        );
    }
    let mut values = 0;
    for ind in &sdb.indicators {
        b.push(
            "stat_indicator",
            vec![
                db::i(ind.id),
                db::s(ind.name.clone()),
                db::i(ind.category_mask),
                db::s(sdb.category_names(ind.category_mask).join(", ")),
                db::i(ind.unit_id),
                db::i(ind.content_id),
                db::s(ind.source.clone()),
            ],
        );
        for (rank, it) in ind.items.iter().enumerate() {
            values += 1;
            b.push(
                "stat_value",
                vec![
                    db::i(ind.id),
                    db::i(it.country_id),
                    db::i(rank as i64 + 1),
                    db::f(it.value),
                    db::s(it.value_text.clone()),
                    db::i(it.decimals),
                    db::i(it.col_b),
                    db::i(it.year),
                    db::i(it.col_c),
                    db::s(it.note.clone()),
                    db::i(it.year_end()),
                    db::s(it.period_text()),
                ],
            );
        }
    }
    b.push("meta", vec![db::s("sdb.revision"), db::s(sdb.revision.clone())]);
    Ok((sdb.indicators.len(), values))
}

pub fn geoquiz(extracted: &Path, sink: &Sink) -> Result<usize> {
    let d = extracted.join("iso/EE/ENCARTA");
    let mut b = Buf::new(sink);
    let mut n = 0;
    for level in 1..=4 {
        let s = msworld::parse_quiz_settings(&read(&d.join(format!("D{level}.TXT")))?)?;
        b.push(
            "geoquiz_level",
            vec![
                db::i(level),
                db::i(s.questions),
                db::s(serde_json::to_string(&s.points)?),
                db::s(serde_json::to_string(&s.per_type_unknown)?),
                db::s(serde_json::to_string(&s.type_mix)?),
            ],
        );
        let places = msworld::parse_quiz_places(&read(&d.join(format!("D{level}PLACES.TXT")))?)?;
        for (k, p) in places.iter().enumerate() {
            n += 1;
            b.push(
                "geoquiz_place",
                vec![
                    db::i(level),
                    db::i(k as i64),
                    db::i(p.kind),
                    db::i(p.group),
                    db::i(p.entity),
                    db::i(p.extra),
                ],
            );
        }
    }
    Ok(n)
}

pub fn place_links(extracted: &Path, sink: &Sink) -> Result<(usize, usize)> {
    let p = p2m::parse(&read(&extracted.join("iso/EE/ENCARTA/P2MTBL.DAT"))?)?;
    let kids = read(&extracted.join("iso/EE/KIDS/P2MTBL.DAT"))?;
    let same = kids == read(&extracted.join("iso/EE/ENCARTA/P2MTBL.DAT"))?;
    let mut b = Buf::new(sink);
    let mut links = 0;
    for pl in &p.places {
        for (k, l) in pl.links.iter().enumerate() {
            links += 1;
            b.push(
                "place_link",
                vec![
                    db::i(pl.place_id),
                    db::i(k as i64),
                    db::i(l.kind),
                    db::s(p2m::kind_label(l.kind)),
                    db::i(l.content_id),
                ],
            );
        }
    }
    b.push("meta", vec![db::s("p2m.kids_identical"), db::s(same.to_string())]);
    Ok((p.places.len(), links))
}

pub fn atlas_media(extracted: &Path, sink: &Sink, t: &mut Tracker) -> Result<usize> {
    let cat = scan_catalog(&extracted.join("eit/EE/ENCARTA/MSWORLD.ITR"))?;
    let recs = msworld::media_records(&cat)?;
    let mut b = Buf::new(sink);
    for r in &recs {
        let (plain, html) = text_html(t, r.title.trim_end_matches('\0'));
        b.push(
            "atlas_media",
            vec![
                db::i(r.key),
                db::s(r.kind.clone()),
                db::s(plain),
                db::s(html),
                db::oi(r.place_id),
                db::os(r.file.clone().map(|f| f.to_ascii_lowercase())),
                db::os(r.thumbnail.clone().map(|f| f.to_ascii_lowercase())),
                db::oi(r.paired),
                db::oi(r.legacy_id),
                db::os(r.credit.clone()),
                db::ne(r.other.join(";")),
            ],
        );
    }
    Ok(recs.len())
}

/// Every `*.LEX` word list (they are small enough to store whole).
pub fn lexicons(extracted: &Path, sink: &Sink) -> Result<Vec<(String, usize)>> {
    let mut b = Buf::new(sink);
    let mut out = Vec::new();
    let files = [
        ("EESPLA01", "iso/EE/ENCARTA/EESPLA01.LEX", "search speller (Deluxe)"),
        ("EESPLA01_KIDS", "iso/EE/KIDS/EESPLA01.LEX", "search speller (Kids)"),
    ];
    let mut list: Vec<(String, std::path::PathBuf, String)> = files
        .iter()
        .map(|(n, p, u)| (n.to_string(), extracted.join(p), u.to_string()))
        .collect();
    let edict = extracted.join("iso/EE/ENCARTA/EDICT");
    let mut names: Vec<_> = std::fs::read_dir(&edict)?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("lex")))
        .collect();
    names.sort();
    for p in names {
        let stem = p.file_stem().unwrap().to_string_lossy().to_ascii_uppercase();
        list.push((stem.clone(), p, format!("EDICT lexicon {stem}")));
    }
    for (name, path, used) in list {
        let l = lex::parse(&read(&path)?)?;
        let mut words = l.words()?;
        words.sort();
        b.push(
            "lexicon",
            vec![
                db::s(name.clone()),
                db::s(path.strip_prefix(extracted).unwrap_or(&path).display().to_string()),
                db::s(
                    l.header
                        .lcids
                        .iter()
                        .map(|c| format!("{c:#06x}"))
                        .collect::<Vec<_>>()
                        .join(","),
                ),
                db::i(words.len() as i64),
                db::s(used),
            ],
        );
        for w in &words {
            b.push("lexicon_word", vec![db::s(name.clone()), db::s(w.clone())]);
        }
        out.push((name, words.len()));
    }
    Ok(out)
}

// ---- SWQUIZ quizzes (`dswMedia/quiz/<n>/<n>.iax`) -----------------------------------------

/// Quiz text fields hold HTML fragments wrapped in `<html>…</html>` (escaped in the XML).
fn quiz_text(s: &str) -> String {
    let s = s.trim();
    let s = s.strip_prefix("<html>").unwrap_or(s);
    let s = s.strip_suffix("</html>").unwrap_or(s);
    s.to_string()
}

/// Generic element → JSON (`@attr` keys, child elements by name, repeated children as arrays,
/// leaf text as strings with any `<html>…</html>` wrapper removed).
pub fn generic_json(t: &mut Tracker, e: &Element) -> Value {
    t.el_all(e);
    let kids: Vec<&Element> = e.elements().collect();
    if kids.is_empty() {
        let txt = e.text();
        let mut m = Map::new();
        for (k, v) in &e.attrs {
            m.insert(format!("@{k}"), v.clone().into());
        }
        if m.is_empty() {
            return quiz_text(&txt).into();
        }
        m.insert("value".into(), quiz_text(&txt).into());
        return Value::Object(m);
    }
    let mut m = Map::new();
    for (k, v) in &e.attrs {
        m.insert(format!("@{k}"), v.clone().into());
    }
    for c in kids {
        let v = generic_json(t, c);
        match m.get_mut(&c.name) {
            Some(Value::Array(a)) => a.push(v),
            Some(old) => {
                let prev = old.take();
                *old = json!([prev, v]);
            }
            None => {
                m.insert(c.name.clone(), v);
            }
        }
    }
    Value::Object(m)
}

/// SWQUIZ quiz definitions (identical in Deluxe and Kids): stored as JSON in the quiz's own
/// vocabulary (question/answers/reward with HTML text fragments and image/audio file names).
pub fn quizzes(extracted: &Path, sink: &Sink, t: &mut Tracker) -> Result<usize> {
    let mut b = Buf::new(sink);
    let mut n = 0;
    for (edition, dir) in [
        ("deluxe", "eit/EE/ENCARTA/SWQUIZ.EIT/dswMedia/quiz"),
        ("kids", "eit/EE/KIDS/SWQUIZ.EIT/dswMedia/quiz"),
    ] {
        let Ok(rd) = std::fs::read_dir(extracted.join(dir)) else {
            continue;
        };
        let mut dirs: Vec<_> = rd.flatten().map(|e| e.path()).collect();
        dirs.sort();
        for d in dirs {
            let Some(id) = d.file_name().and_then(|x| x.to_str()).and_then(|x| x.parse::<u32>().ok()) else {
                continue;
            };
            let p = d.join(format!("{id}.iax"));
            let Ok(bytes) = std::fs::read(&p) else {
                t.note("quiz", "directory without .iax");
                continue;
            };
            let doc = xml::parse(&decode_xml_bytes(&bytes))?;
            let root = &doc.root;
            let ty = root.attr("type").unwrap_or("").to_string();
            let lang = root.attr("lang").unwrap_or("").to_string();
            let v = generic_json(t, root);
            n += 1;
            b.push(
                "quiz",
                vec![db::i(id), db::s(edition), db::s(ty), db::s(lang), db::s(v.to_string())],
            );
        }
    }
    Ok(n)
}

/// Interactive definitions (`*.iax` in `Sw*.EIT`, root `<iai>`): slide shows, labelled diagrams,
/// games. Stored as generic JSON (texts are HTML fragments); the files themselves are shipped by
/// the media converter for the player.
pub fn interactives(srcs: &[super::sources::Source], sink: &Sink, trackers: &mut Vec<(String, Tracker)>) -> Result<usize> {
    let mut b = Buf::new(sink);
    let mut n = 0;
    for s in srcs {
        if !s.label.starts_with("iax/") || s.label.ends_with("SWQUIZ.EIT") {
            continue;
        }
        let edition = if s.label.starts_with("iax/KIDS/") { "kids" } else { "deluxe" };
        let package = s.label.rsplit('/').next().unwrap_or("").to_string();
        let mut t = Tracker::default();
        if let super::sources::SourceKind::Files(files) = &s.kind {
            for (name, p) in files {
                let doc = xml::parse(&decode_xml_bytes(&read(p)?))?;
                let r = &doc.root;
                let v = generic_json(&mut t, r);
                let title = r.attr("friendlyname").map(str::to_string);
                b.push(
                    "interactive",
                    vec![
                        db::s(name.to_ascii_lowercase()),
                        db::s(edition),
                        db::s(package.clone()),
                        db::os(r.attr("basecontroltype")),
                        db::os(title),
                        db::s(v.to_string()),
                    ],
                );
                n += 1;
            }
        }
        trackers.push((s.label.clone(), t));
    }
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date(year: i64, month: u16, day: u16, circa: bool) -> timeline::Date {
        timeline::Date { year, month, day, circa, thousands: false, raw_year: year.unsigned_abs() as u32 }
    }

    fn rec(kind: u16, span: bool, start: timeline::Date, end: Option<timeline::Date>) -> timeline::Record {
        timeline::Record {
            flags: 0,
            title: "Synthetic Era".into(),
            title_offset: 0,
            title_len: 0,
            start: Some(start),
            end,
            span,
            importance: 2,
            category_mask: 1,
            region_mask: 1,
            seq: 1,
            ref_id: 1,
            kind,
        }
    }

    #[test]
    fn timeline_strings() {
        let r = rec(1, false, date(-4, 0, 0, true), Some(date(29, 0, 0, true)));
        assert_eq!(timeline_date_text(&r).as_deref(), Some("4? BC - AD 29?"));
        assert_eq!(timeline_display(&r), Some("point"));
        let r = rec(1, true, date(1939, 9, 1, false), Some(date(1945, 9, 2, false)));
        assert_eq!(timeline_date_text(&r).as_deref(), Some("September 1, 1939 - September 2, 1945"));
        assert_eq!(timeline_label(&r, "Synthetic Era").as_deref(), Some("SYNTHETIC ERA"));
        let r = rec(3, false, date(-3000, 0, 0, false), None);
        assert_eq!(timeline_label(&r, "Rise of X").as_deref(), Some("HISTORICAL ESSAY: RISE OF X"));
        assert_eq!(date_text(&date(-4_600_000_000, 0, 0, false), false), "4.60 billion years ago");
        assert_eq!(date_text(&date(-2_500_000, 0, 0, false), false), "2.50 million years ago");
        assert_eq!(date_text(&date(-118_000, 0, 0, true), false), "120,000 years ago");
        assert_eq!(date_text(&date(-9_000, 0, 0, true), false), "11,000 years ago");
        assert_eq!(date_text(&date(-8_000, 0, 0, false), false), "8000 BC");
        assert_eq!(date_text(&date(1963, 6, 16, false), false), "June 16, 1963");
        assert_eq!(timeline_prominence(1), 5);
        assert_eq!(timeline_prominence(0), 0);
    }
}

// ---- audio records whose sound is inside an interactive ----------------------------------

/// Pairs `(ref_id, file)` from an interactive's definition: a slide's `audiorefid` is the content
/// id of its sound in hex (`29D02BC1` = 701508545) next to the file the player loads (`audiofile`).
pub fn iax_audio_refs(def: &Value, out: &mut Vec<(i64, String)>) {
    match def {
        Value::Object(o) => {
            let id = o.iter().find(|(k, _)| k.eq_ignore_ascii_case("audiorefid")).and_then(|(_, v)| v.as_str());
            let file = o.iter().find(|(k, _)| k.eq_ignore_ascii_case("audiofile")).and_then(|(_, v)| match v {
                Value::String(s) => Some(s.as_str()),
                Value::Object(f) => f.get("value").and_then(Value::as_str),
                _ => None,
            });
            if let (Some(id), Some(file)) = (id, file)
                && id.len() == 8
                && let Ok(n) = i64::from_str_radix(id, 16)
                && !file.trim().is_empty()
            {
                out.push((n, file.trim().to_string()));
            }
            o.values().for_each(|v| iax_audio_refs(v, out));
        }
        Value::Array(a) => a.iter().for_each(|v| iax_audio_refs(v, out)),
        _ => {}
    }
}

/// `audio` items with no sound file of their own whose sound an interactive names (the slides of
/// a narrated slide show, the clips of a sound game): adds their `audio` file (raw path
/// `iax:<definition>`) to `item_file`. Runs on the finished database; returns the items fixed.
pub fn interactive_audio(conn: &rusqlite::Connection) -> Result<usize> {
    let stubs: std::collections::HashSet<i64> = conn
        .prepare(
            "SELECT ref_id FROM item i WHERE kind = 'audio'
             AND NOT EXISTS (SELECT 1 FROM item_file f WHERE f.ref_id = i.ref_id AND f.role = 'audio')",
        )?
        .query_map([], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    let mut found: std::collections::BTreeMap<i64, (String, String)> = Default::default();
    let mut st = conn.prepare("SELECT file_name, json FROM interactive ORDER BY edition = 'deluxe' DESC, file_name")?;
    for row in st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))? {
        let (iax, j) = row?;
        let mut refs = Vec::new();
        iax_audio_refs(&serde_json::from_str(&j).unwrap_or(Value::Null), &mut refs);
        for (id, file) in refs {
            if stubs.contains(&id) {
                found.entry(id).or_insert((file.to_ascii_lowercase(), iax.clone()));
            }
        }
    }
    let mut ins = conn.prepare(
        "INSERT INTO item_file (ref_id, ordinal, role, file_name, raw_path)
         VALUES (?1, (SELECT COALESCE(MAX(ordinal), -1) + 1 FROM item_file WHERE ref_id = ?1), 'audio', ?2, ?3)",
    )?;
    for (id, (file, iax)) in &found {
        ins.execute(rusqlite::params![id, file, format!("iax:{iax}")])?;
    }
    Ok(found.len())
}

#[cfg(test)]
mod iax_audio_tests {
    use super::*;

    #[test]
    fn finds_slide_sounds_by_hex_id() {
        let def = json!({ "slides": { "slide": [
            { "standardslide": { "audiofile": "000A3F0B.mp3", "audiorefid": "29D02BC1", "imagerefid": "00000001" } },
            { "standardslide": { "audiofile": { "value": "B.mp3" }, "audioRefId": "0000000A" } },
            { "standardslide": { "audiofile": "", "audiorefid": "0000000B" } },
            { "standardslide": { "audiofile": "c.mp3", "audiorefid": "xyz" } } ] } });
        let mut out = Vec::new();
        iax_audio_refs(&def, &mut out);
        assert_eq!(out, vec![(701508545, "000A3F0B.mp3".to_string()), (10, "B.mp3".to_string())]);
    }

    #[test]
    fn adds_the_file_to_stub_items_only() {
        let c = rusqlite::Connection::open_in_memory().unwrap();
        c.execute_batch(
            "CREATE TABLE item (ref_id INTEGER PRIMARY KEY, kind TEXT);
             CREATE TABLE item_file (ref_id INTEGER, ordinal INTEGER, role TEXT, file_name TEXT, raw_path TEXT, width INTEGER, height INTEGER, src_ref_id INTEGER, disc INTEGER, package_file TEXT);
             CREATE TABLE interactive (file_name TEXT, edition TEXT, package TEXT, control_type TEXT, title TEXT, json TEXT);
             INSERT INTO item VALUES (10, 'audio'), (11, 'audio'), (12, 'media');
             INSERT INTO item_file (ref_id, ordinal, role, file_name) VALUES (10, 0, 'thumb', 'audio.jtn'), (11, 0, 'audio', 'own.wma');
             INSERT INTO interactive VALUES ('x.iax', 'deluxe', 'SW', 'slideshow', 'X',
               '{\"s\":[{\"audiofile\":\"A.MP3\",\"audiorefid\":\"0000000A\"},{\"audiofile\":\"b.mp3\",\"audiorefid\":\"0000000B\"},{\"audiofile\":\"c.mp3\",\"audiorefid\":\"0000000C\"}]}');",
        )
        .unwrap();
        assert_eq!(interactive_audio(&c).unwrap(), 1);
        let rows: Vec<(i64, i64, String, String)> = c
            .prepare("SELECT ref_id, ordinal, file_name, raw_path FROM item_file WHERE role = 'audio' AND raw_path IS NOT NULL")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        assert_eq!(rows, vec![(10, 1, "a.mp3".to_string(), "iax:x.iax".to_string())]);
    }
}
