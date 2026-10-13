//! Harvest of the "Find" search exports (`FINDDLX.CAB`): PFX titles and aliases, IdXE result
//! records (section titles, synonyms, headwords with paragraph ranges) and the IdxW vocabulary.

use super::db::{self, Buf, Sink};
use super::inline::{self, Ctx};
use super::model::{self, Inline};
use super::render;
use super::track::Tracker;
use super::xml;
use anyhow::{Context, Result};
use encarta_formats::search::{FindFiles, IdxE, IdxW, Pfx};
use std::collections::HashMap;
use std::path::Path;

/// Title markup in the search files (`<it>…</it>`, `<entity tag=…/>`) → inline nodes.
pub fn markup(t: &mut Tracker, s: &str) -> Vec<Inline> {
    if !s.contains(['<', '&']) {
        return vec![Inline::Text(s.to_string())];
    }
    let wrapped = format!("<t>{s}</t>");
    let doc = xml::parse(&wrapped).or_else(|_| {
        // A bare `&` in a title: escape ampersands that do not start a reference.
        let fixed = s.replace("&amp;", "\u{0}").replace(['&', '\u{0}'], "&amp;");
        xml::parse(&format!("<t>{fixed}</t>"))
    });
    match doc {
        Ok(d) => {
            let mut cx = Ctx {
                t,
                title: None,
                squash: false,
            };
            inline::children(&mut cx, &d.root)
        }
        Err(_) => {
            t.note("search markup", "unparsable title kept as text");
            vec![Inline::Text(s.to_string())]
        }
    }
}

pub struct Titles {
    /// ref id → every full-title entry `(sort title, display HTML, plain display)` (PFX collection 0).
    pub primary: HashMap<u32, Vec<(String, String, String)>>,
}

impl Titles {
    /// The entry whose display text is the item's title, else the first one (`(sort, html)`).
    pub fn best(&self, ref_id: u32, title: &str) -> Option<(String, String)> {
        let v = self.primary.get(&ref_id)?;
        let t = model::squash(title);
        v.iter()
            .find(|e| model::squash(&e.2) == t)
            .or_else(|| v.iter().find(|e| model::squash(&e.2).eq_ignore_ascii_case(&t)))
            .or_else(|| v.first())
            .map(|e| (e.0.clone(), e.1.clone()))
    }
}

pub struct Stats {
    pub pfx: [usize; 2],
    pub idxe: usize,
    pub idxe_failures: u64,
    pub words: usize,
}

/// Loads one edition's search files and writes `alias`, `search_entry` and `search_word` rows.
pub fn harvest(
    dir: &Path,
    tla: &str,
    edition: &str,
    sink: &Sink,
    t: &mut Tracker,
    titles: &mut Titles,
) -> Result<Stats> {
    let ff = FindFiles::locate(dir, tla)?;
    let mut buf = Buf::new(sink);
    let pfx_bytes = std::fs::read(&ff.pfx).with_context(|| format!("reading {}", ff.pfx.display()))?;
    let pfx = Pfx::parse(&pfx_bytes)?;
    let mut counts = [0usize; 2];
    for (ci, col) in pfx.collections.iter().enumerate() {
        if ci < 2 {
            counts[ci] = col.entries.len();
        }
        for e in &col.entries {
            let inl = markup(t, &e.display_title);
            let html = render::inlines_html(&inl);
            if ci == 0 {
                titles
                    .primary
                    .entry(e.ref_id)
                    .or_default()
                    .push((e.sort_title.clone(), html.clone(), model::plain(&inl)));
            }
            buf.push(
                "alias",
                vec![
                    db::i(e.ref_id),
                    db::s(edition),
                    db::i(ci as i64),
                    db::i(e.class_id),
                    db::s(e.sort_title.clone()),
                    db::s(model::plain(&inl)),
                    db::s(html),
                ],
            );
        }
    }
    let idxe_bytes = std::fs::read(&ff.idxe)?;
    let idxe = IdxE::new(&idxe_bytes)?;
    let mut walk = idxe.walk();
    let mut n = 0;
    for r in walk.by_ref() {
        n += 1;
        let inl = markup(t, &r.display_text);
        let (ps, pe) = r.para_range().map_or((None, None), |(a, b)| (Some(a), Some(b)));
        let dual = (r.dual_para != 0).then_some(r.dual_para);
        buf.push(
            "search_entry",
            vec![
                db::i(r.entry_id),
                db::s(edition),
                db::i(r.ref_id),
                db::i(r.content_type()),
                db::b(r.is_list()),
                db::oi(r.section()),
                db::oi(ps),
                db::oi(pe),
                db::oi(dual.map(|d| d & 0xFFFF)),
                db::oi(dual.map(|d| d >> 16)),
                db::oi(r.dupe_id),
                db::i(r.word_count),
                db::s(model::plain(&inl)),
                db::s(render::inlines_html(&inl)),
                db::ne(
                    r.parents
                        .iter()
                        .map(|p| p.to_string())
                        .collect::<Vec<_>>()
                        .join(","),
                ),
            ],
        );
    }
    let failures = walk.failures;
    let idxw_bytes = std::fs::read(&ff.idxw)?;
    let idxw = IdxW::new(&idxw_bytes)?;
    let (words, _) = idxw.walk()?;
    for w in &words {
        buf.push(
            "search_word",
            vec![
                db::s(edition),
                db::s(w.key.clone()),
                db::i(w.word_id),
                db::i(w.alt_id),
                db::i(w.stop_word_type),
            ],
        );
    }
    Ok(Stats {
        pfx: counts,
        idxe: n,
        idxe_failures: failures,
        words: words.len(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn title_markup() {
        let mut t = Tracker::default();
        let v = markup(&mut t, "Caf<entity tag=\"eacute\"/> <it>Life</it> & more");
        assert_eq!(render::inlines_html(&v), "Café <em>Life</em> &amp; more");
    }
}
