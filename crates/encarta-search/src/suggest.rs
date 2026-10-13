//! Autocomplete over Encarta's own word wheel (the PFX titles and partial keys in the `alias`
//! table) plus item titles.
//!
//! Files: `suggest.fst` maps every distinct normalised key to `(first entry << 20) | count`;
//! `suggest.bin` holds the entries sorted by key, so the entries of every key prefix form one
//! contiguous slice. A prefix query finds that slice with two fst lookups and picks the best
//! entries by precomputed score; when nothing starts with the prefix, a Levenshtein-1 prefix
//! automaton gives typo-tolerant completions.

use crate::bin_io::{Reader, Writer};
use crate::schema::{CATEGORIES, EDITIONS};
use crate::text::{normalize_with, plain_analyzer};
use anyhow::{Context, Result};
use fst::automaton::Levenshtein;
use fst::{Automaton, IntoStreamer, Map, MapBuilder, Streamer};
use serde::Serialize;
use std::collections::HashMap;
use std::path::Path;

const MAGIC: &[u8; 8] = b"ENCSUG01";
const COUNT_BITS: u64 = 20;

/// One word-wheel entry as given to the builder.
#[derive(Clone, Debug)]
pub struct SuggestInput {
    /// Text the key is computed from (sort title or title).
    pub key_text: String,
    pub display: String,
    pub display_html: Option<String>,
    /// Title of the target item, when it differs from `display`.
    pub title: Option<String>,
    pub ref_id: u64,
    /// Index into [`CATEGORIES`].
    pub category: u8,
    /// Bit mask over [`EDITIONS`].
    pub editions: u8,
    pub score: f32,
}

struct Entry {
    ref_id: u64,
    score: f32,
    category: u8,
    editions: u8,
    display: (u32, u32),
    html: (u32, u32),
    title: (u32, u32),
}

pub struct Suggester {
    map: Map<Vec<u8>>,
    entries: Vec<Entry>,
    strings: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Suggestion {
    pub ref_id: u64,
    pub display: String,
    pub display_html: String,
    /// Title of the item the entry leads to.
    pub title: String,
    pub category: &'static str,
    pub editions: Vec<&'static str>,
    pub score: f32,
    /// True when the completion came from the typo-tolerant fallback.
    pub fuzzy: bool,
    pub url: String,
}

pub fn editions_of(mask: u8) -> Vec<&'static str> {
    EDITIONS.iter().enumerate().filter(|(i, _)| mask & (1 << i) != 0).map(|(_, e)| *e).collect()
}

/// Writes `suggest.fst` and `suggest.bin` into `dir`. Returns (entries, distinct keys).
pub fn build(dir: &Path, inputs: Vec<SuggestInput>) -> Result<(usize, usize)> {
    let mut analyzer = plain_analyzer();
    // Merge duplicates (same key, ref and display) by OR-ing editions and keeping the best score.
    let mut merged: HashMap<(String, u64, String), SuggestInput> = HashMap::new();
    for inp in inputs {
        let key = normalize_with(&mut analyzer, &inp.key_text);
        if key.is_empty() {
            continue;
        }
        let k = (key, inp.ref_id, inp.display.clone());
        match merged.get_mut(&k) {
            Some(e) => {
                e.editions |= inp.editions;
                if inp.score > e.score {
                    e.score = inp.score;
                }
            }
            None => {
                merged.insert(k, inp);
            }
        }
    }
    let mut rows: Vec<((String, u64, String), SuggestInput)> = merged.into_iter().collect();
    rows.sort_by(|a, b| a.0.0.cmp(&b.0.0).then(b.1.score.total_cmp(&a.1.score)).then(a.0.1.cmp(&b.0.1)));

    let mut w = Writer::new(MAGIC);
    w.u32(rows.len() as u32);
    let mut fst = MapBuilder::memory();
    let mut keys = 0;
    let mut i = 0;
    while i < rows.len() {
        let mut j = i;
        while j < rows.len() && rows[j].0.0 == rows[i].0.0 {
            j += 1;
        }
        fst.insert(rows[i].0.0.as_bytes(), ((i as u64) << COUNT_BITS) | ((j - i) as u64).min((1 << COUNT_BITS) - 1))?;
        keys += 1;
        i = j;
    }
    for (_, e) in &rows {
        w.u64(e.ref_id);
        w.f32(e.score);
        w.u8(e.category);
        w.u8(e.editions);
        w.str(&e.display);
        w.str(e.display_html.as_deref().filter(|h| *h != e.display).unwrap_or(""));
        w.str(e.title.as_deref().filter(|t| *t != e.display).unwrap_or(""));
    }
    std::fs::write(dir.join("suggest.fst"), fst.into_inner()?)?;
    std::fs::write(dir.join("suggest.bin"), w.0)?;
    Ok((rows.len(), keys))
}

fn succ(prefix: &[u8]) -> Option<Vec<u8>> {
    let mut v = prefix.to_vec();
    while let Some(last) = v.pop() {
        if last < 0xFF {
            v.push(last + 1);
            return Some(v);
        }
    }
    None
}

impl Suggester {
    pub fn open(dir: &Path) -> Result<Self> {
        let map = Map::new(std::fs::read(dir.join("suggest.fst")).context("suggest.fst")?)?;
        let bytes = std::fs::read(dir.join("suggest.bin")).context("suggest.bin")?;
        let mut r = Reader::new(&bytes, MAGIC)?;
        let n = r.u32()? as usize;
        let mut entries = Vec::with_capacity(n);
        let mut strings = String::new();
        let push = |s: &str, strings: &mut String| {
            let off = strings.len() as u32;
            strings.push_str(s);
            (off, s.len() as u32)
        };
        for _ in 0..n {
            let ref_id = r.u64()?;
            let score = r.f32()?;
            let category = r.u8()?;
            let editions = r.u8()?;
            let display = push(r.str()?, &mut strings);
            let html = push(r.str()?, &mut strings);
            let title = push(r.str()?, &mut strings);
            entries.push(Entry { ref_id, score, category, editions, display, html, title });
        }
        Ok(Suggester { map, entries, strings })
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    fn s(&self, (off, len): (u32, u32)) -> &str {
        &self.strings[off as usize..(off + len) as usize]
    }

    /// Index range of the entries whose key starts with `prefix`.
    fn prefix_range(&self, prefix: &[u8]) -> (usize, usize) {
        let first_at_or_after = |k: &[u8]| -> usize {
            let mut st = self.map.range().ge(k).into_stream();
            st.next().map_or(self.entries.len(), |(_, v)| (v >> COUNT_BITS) as usize)
        };
        let lo = first_at_or_after(prefix);
        let hi = succ(prefix).map_or(self.entries.len(), |s| first_at_or_after(&s));
        (lo, hi.max(lo))
    }

    /// Completions of `query`, best first. `editions` / `category` filter (bit mask, category index).
    pub fn suggest(&self, query: &str, limit: usize, editions: Option<u8>, category: Option<u8>) -> Vec<Suggestion> {
        let mut key = normalize_with(&mut plain_analyzer(), query);
        if key.is_empty() || limit == 0 {
            return Vec::new();
        }
        if query.ends_with(char::is_whitespace) {
            key.push(' ');
        }
        let ok = |e: &Entry| editions.is_none_or(|m| e.editions & m != 0) && category.is_none_or(|c| e.category == c);
        let (lo, hi) = self.prefix_range(key.as_bytes());
        let exact = self.map.get(key.trim_end().as_bytes()).map(|v| (v >> COUNT_BITS) as usize..((v >> COUNT_BITS) + (v & ((1 << COUNT_BITS) - 1))) as usize);
        let mut cands: Vec<(f32, usize)> = (lo..hi)
            .filter(|&i| ok(&self.entries[i]))
            .map(|i| {
                let e = &self.entries[i];
                let bonus = if exact.as_ref().is_some_and(|r| r.contains(&i)) { 4.0 } else { 1.0 };
                // Prefer completions that need less typing ("zeb" → "Zebra" before "Zebulon Pike").
                let typed = (key.len() as f32 / (e.display.1.max(1) as f32)).min(1.0);
                (e.score * bonus * (1.0 + 0.5 * typed), i)
            })
            .collect();
        let mut fuzzy = false;
        if cands.is_empty() && key.trim_end().len() >= 4 {
            fuzzy = true;
            cands = self.fuzzy_candidates(key.trim_end(), &ok);
        }
        self.pick(cands, limit, fuzzy)
    }

    fn fuzzy_candidates(&self, key: &str, ok: &dyn Fn(&Entry) -> bool) -> Vec<(f32, usize)> {
        let Ok(lev) = Levenshtein::new(key, 1) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        let mut st = self.map.search(lev.starts_with()).into_stream();
        let mut keys = 0;
        while let Some((_, v)) = st.next() {
            let start = (v >> COUNT_BITS) as usize;
            let n = (v & ((1 << COUNT_BITS) - 1)) as usize;
            out.extend((start..start + n).filter(|&i| ok(&self.entries[i])).map(|i| (self.entries[i].score * 0.5, i)));
            keys += 1;
            if keys >= 20_000 {
                break;
            }
        }
        out
    }

    fn pick(&self, mut cands: Vec<(f32, usize)>, limit: usize, fuzzy: bool) -> Vec<Suggestion> {
        let by_score = |a: &(f32, usize), b: &(f32, usize)| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1));
        // Most prefixes have few entries; for big ones, sort only the head first.
        let head = (limit * 8).max(64);
        if cands.len() > head {
            cands.select_nth_unstable_by(head, by_score);
            let (top, _) = cands.split_at_mut(head);
            top.sort_unstable_by(by_score);
        } else {
            cands.sort_unstable_by(by_score);
        }
        let mut seen = std::collections::HashSet::new();
        let mut out = Vec::new();
        let mut take = |cands: &[(f32, usize)], out: &mut Vec<Suggestion>| {
            for &(score, i) in cands {
                if out.len() >= limit {
                    break;
                }
                let e = &self.entries[i];
                if !seen.insert(e.ref_id) {
                    continue;
                }
                let display = self.s(e.display).to_string();
                let html = self.s(e.html);
                let title = self.s(e.title);
                out.push(Suggestion {
                    ref_id: e.ref_id,
                    display_html: if html.is_empty() { escape(&display) } else { html.to_string() },
                    title: if title.is_empty() { display.clone() } else { title.to_string() },
                    display,
                    category: CATEGORIES.get(e.category as usize).copied().unwrap_or("other"),
                    editions: editions_of(e.editions),
                    score,
                    fuzzy,
                    url: format!("/item/{}", e.ref_id),
                });
            }
        };
        let n = cands.len().min(head);
        take(&cands[..n], &mut out);
        if out.len() < limit && cands.len() > head {
            let rest = &mut cands[head..];
            rest.sort_unstable_by(by_score);
            take(rest, &mut out);
        }
        out
    }
}

pub(crate) fn escape(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => o.push_str("&amp;"),
            '<' => o.push_str("&lt;"),
            '>' => o.push_str("&gt;"),
            '"' => o.push_str("&quot;"),
            _ => o.push(c),
        }
    }
    o
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inp(key: &str, ref_id: u64, score: f32) -> SuggestInput {
        SuggestInput {
            key_text: key.into(),
            display: key.into(),
            display_html: None,
            title: None,
            ref_id,
            category: 0,
            editions: 1,
            score,
        }
    }

    #[test]
    fn prefix_and_fuzzy() {
        let dir = tempfile::tempdir().unwrap();
        build(
            dir.path(),
            vec![
                inp("Zebra", 1, 5.0),
                inp("Zebra mussel", 2, 2.0),
                inp("Zebu", 3, 1.0),
                inp("Zebrafish", 4, 3.0),
                inp("Equus zebra", 1, 1.0),
                inp("Paris", 5, 9.0),
            ],
        )
        .unwrap();
        let s = Suggester::open(dir.path()).unwrap();
        let got: Vec<u64> = s.suggest("zeb", 10, None, None).iter().map(|x| x.ref_id).collect();
        assert_eq!(got, vec![1, 4, 2, 3]);
        assert_eq!(s.suggest("zebra ", 10, None, None)[0].ref_id, 2);
        assert_eq!(s.suggest("Zébra", 1, None, None)[0].ref_id, 1);
        let f = s.suggest("zrbra", 10, None, None);
        assert!(f[0].fuzzy && f[0].ref_id == 1);
        assert!(s.suggest("zeb", 10, Some(2), None).is_empty());
        assert!(s.suggest("", 10, None, None).is_empty());
    }
}
