//! Spelling suggestions from the disc's own lexicons (`lexicon_word`), ranked by how often each
//! word occurs in the article text.
//!
//! Files: `spell.fst` maps every normalised word to its index in `spell.bin`, which lists the
//! lexicon names and, per word, its display form, corpus frequency and lexicon bit mask.
//! Candidates come from a Levenshtein automaton over the fst (distance 1 for short words, 2 for
//! longer ones) plus adjacent transpositions, ranked by optimal-string-alignment distance and
//! frequency.

use crate::bin_io::{Reader, Writer};
use crate::text::{for_each_word, plain_analyzer};
use anyhow::{Context, Result};
use fst::automaton::Levenshtein;
use fst::{IntoStreamer, Map, MapBuilder, Streamer};
use serde::Serialize;
use std::collections::{BTreeMap, HashMap};
use std::path::Path;

const MAGIC: &[u8; 8] = b"ENCSPL01";

pub struct Speller {
    map: Map<Vec<u8>>,
    lexicons: Vec<String>,
    words: Vec<Word>,
}

struct Word {
    display: String,
    freq: u32,
    mask: u16,
}

#[derive(Debug, Clone, Serialize)]
pub struct WordSuggestion {
    pub word: String,
    pub distance: u32,
    pub freq: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct WordCheck {
    pub word: String,
    pub known: bool,
    pub suggestions: Vec<WordSuggestion>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SpellResult {
    pub query: String,
    /// The query with every unknown word replaced by its best suggestion; `None` when every
    /// word is known or nothing better was found.
    pub corrected: Option<String>,
    pub words: Vec<WordCheck>,
}

/// Builds the spell files from `(lexicon name, words)` lists and corpus word counts
/// (plain words → occurrences). Returns the number of distinct words.
pub fn build(dir: &Path, lexicons: &[(String, Vec<String>)], counts: &HashMap<String, u32>) -> Result<usize> {
    let mut analyzer = plain_analyzer();
    // key → (display, mask)
    let mut words: BTreeMap<String, (String, u16)> = BTreeMap::new();
    for (bit, (_, list)) in lexicons.iter().enumerate().take(16) {
        for w in list {
            let mut toks = Vec::new();
            for_each_word(&mut analyzer, w, |t| toks.push(t.to_string()));
            let [key] = toks.as_slice() else { continue };
            if key.len() < 2 || key.chars().all(|c| c.is_ascii_digit()) {
                continue;
            }
            let e = words.entry(key.clone()).or_insert_with(|| (w.clone(), 0));
            e.1 |= 1 << bit;
            // Prefer a lower-case display form ("paris" the word over "Paris" the name only when both exist).
            if w.chars().next().is_some_and(char::is_lowercase) && !e.0.chars().next().is_some_and(char::is_lowercase) {
                e.0 = w.clone();
            }
        }
    }
    let mut fst = MapBuilder::memory();
    let mut w = Writer::new(MAGIC);
    w.u32(lexicons.len().min(16) as u32);
    for (name, _) in lexicons.iter().take(16) {
        w.str(name);
    }
    w.u32(words.len() as u32);
    for (i, (key, (display, mask))) in words.iter().enumerate() {
        fst.insert(key.as_bytes(), i as u64)?;
        w.str(display);
        w.u32(counts.get(key).copied().unwrap_or(0));
        w.u16(*mask);
    }
    std::fs::write(dir.join("spell.fst"), fst.into_inner()?)?;
    std::fs::write(dir.join("spell.bin"), w.0)?;
    Ok(words.len())
}

/// Optimal string alignment distance (Levenshtein plus adjacent transpositions), on chars.
pub fn osa_distance(a: &str, b: &str) -> u32 {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let (n, m) = (a.len(), b.len());
    let mut d = vec![vec![0u32; m + 1]; n + 1];
    for (i, row) in d.iter_mut().enumerate() {
        row[0] = i as u32;
    }
    for (j, v) in d[0].iter_mut().enumerate() {
        *v = j as u32;
    }
    for i in 1..=n {
        for j in 1..=m {
            let cost = u32::from(a[i - 1] != b[j - 1]);
            let mut v = (d[i - 1][j] + 1).min(d[i][j - 1] + 1).min(d[i - 1][j - 1] + cost);
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                v = v.min(d[i - 2][j - 2] + 1);
            }
            d[i][j] = v;
        }
    }
    d[n][m]
}

impl Speller {
    pub fn open(dir: &Path) -> Result<Self> {
        let map = Map::new(std::fs::read(dir.join("spell.fst")).context("spell.fst")?)?;
        let bytes = std::fs::read(dir.join("spell.bin")).context("spell.bin")?;
        let mut r = Reader::new(&bytes, MAGIC)?;
        let nl = r.u32()? as usize;
        let lexicons = (0..nl).map(|_| r.str().map(str::to_string)).collect::<Result<Vec<_>>>()?;
        let n = r.u32()? as usize;
        let mut words = Vec::with_capacity(n);
        for _ in 0..n {
            let display = r.str()?.to_string();
            let freq = r.u32()?;
            let mask = r.u16()?;
            words.push(Word { display, freq, mask });
        }
        Ok(Speller { map, lexicons, words })
    }

    pub fn lexicons(&self) -> &[String] {
        &self.lexicons
    }

    pub fn len(&self) -> usize {
        self.words.len()
    }

    pub fn is_empty(&self) -> bool {
        self.words.is_empty()
    }

    /// Bit mask for lexicon names; `None` (or no known names) means every lexicon.
    pub fn mask_for(&self, names: &[&str]) -> u16 {
        let m = names
            .iter()
            .filter_map(|n| self.lexicons.iter().position(|l| l.eq_ignore_ascii_case(n)))
            .fold(0u16, |m, i| m | (1 << i));
        if m == 0 { u16::MAX } else { m }
    }

    fn lookup(&self, key: &str, mask: u16) -> Option<&Word> {
        let w = &self.words[self.map.get(key.as_bytes())? as usize];
        (w.mask & mask != 0).then_some(w)
    }

    /// Whether `word` (one plain word) is in the selected lexicons.
    pub fn is_known(&self, word: &str, mask: u16) -> bool {
        self.lookup(word, mask).is_some()
    }

    /// Corrections for one plain (normalised) word, best first.
    pub fn suggest_word(&self, word: &str, mask: u16, limit: usize) -> Vec<WordSuggestion> {
        let len = word.chars().count();
        if len < 2 {
            return Vec::new();
        }
        let max_d = if len <= 4 { 1 } else { 2 };
        let mut found: HashMap<u64, String> = HashMap::new();
        let lev = Levenshtein::new(word, max_d).or_else(|_| Levenshtein::new(word, 1));
        if let Ok(lev) = lev {
            let mut st = self.map.search(lev).into_stream();
            while let Some((k, v)) = st.next() {
                found.insert(v, String::from_utf8_lossy(k).into_owned());
                if found.len() > 5000 {
                    break;
                }
            }
        }
        // Adjacent transpositions ("rbea" → "bear") are one edit for the user but two for Levenshtein.
        let chars: Vec<char> = word.chars().collect();
        for i in 0..chars.len().saturating_sub(1) {
            let mut c = chars.clone();
            c.swap(i, i + 1);
            let s: String = c.into_iter().collect();
            if let Some(v) = self.map.get(s.as_bytes()) {
                found.insert(v, s);
            }
        }
        let first = word.chars().next();
        let mut out: Vec<(f32, WordSuggestion)> = found
            .into_iter()
            .filter_map(|(v, key)| {
                let w = &self.words[v as usize];
                if w.mask & mask == 0 {
                    return None;
                }
                let d = osa_distance(word, &key);
                if d == 0 || d > max_d {
                    return None;
                }
                let same_first = key.chars().next() == first;
                let score = (1.0 + w.freq as f32).ln() - 4.0 * d as f32 + if same_first { 1.0 } else { 0.0 };
                Some((score, WordSuggestion { word: w.display.clone(), distance: d, freq: w.freq }))
            })
            .collect();
        out.sort_by(|a, b| b.0.total_cmp(&a.0).then_with(|| a.1.word.cmp(&b.1.word)));
        out.into_iter().take(limit).map(|(_, s)| s).collect()
    }

    /// Checks every word of `query` and builds a corrected query.
    pub fn check_query(&self, query: &str, mask: u16, limit: usize) -> SpellResult {
        let mut analyzer = plain_analyzer();
        let mut toks = Vec::new();
        for_each_word(&mut analyzer, query, |t| toks.push(t.to_string()));
        let mut changed = false;
        let mut corrected = Vec::new();
        let mut words = Vec::new();
        for t in toks {
            let numeric = t.chars().any(|c| c.is_ascii_digit());
            let known = numeric || t.chars().count() < 2 || self.is_known(&t, mask);
            let suggestions = if known { Vec::new() } else { self.suggest_word(&t, mask, limit) };
            match suggestions.first() {
                Some(s) if !known => {
                    changed = true;
                    corrected.push(s.word.to_lowercase());
                }
                _ => corrected.push(t.clone()),
            }
            words.push(WordCheck { word: t, known, suggestions });
        }
        SpellResult { query: query.to_string(), corrected: changed.then(|| corrected.join(" ")), words }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distances() {
        assert_eq!(osa_distance("zebr", "zebra"), 1);
        assert_eq!(osa_distance("rbea", "bear"), 2);
        assert_eq!(osa_distance("ebar", "bear"), 1);
        assert_eq!(osa_distance("", "ab"), 2);
    }

    #[test]
    fn suggests() {
        let dir = tempfile::tempdir().unwrap();
        let lex = vec![
            ("MAIN".to_string(), ["zebra", "zebu", "Zebras", "bear", "revolution", "Paris"].map(String::from).to_vec()),
            ("KIDS".to_string(), vec!["zebra".to_string()]),
        ];
        let counts: HashMap<String, u32> =
            [("zebra", 500), ("zebu", 20), ("zebras", 100), ("bear", 50), ("revolution", 900)]
                .into_iter()
                .map(|(k, v)| (k.to_string(), v))
                .collect();
        build(dir.path(), &lex, &counts).unwrap();
        let s = Speller::open(dir.path()).unwrap();
        let all = s.mask_for(&[]);
        assert_eq!(s.suggest_word("zebr", all, 3)[0].word, "zebra");
        assert_eq!(s.suggest_word("ebar", all, 3)[0].word, "bear");
        let r = s.check_query("french revolutoin zebr", all, 3);
        assert_eq!(r.corrected.as_deref(), Some("french revolution zebra"));
        assert!(!r.words[0].known);
        assert!(s.is_known("paris", all));
        let kids = s.mask_for(&["kids"]);
        assert!(!s.is_known("bear", kids));
        assert!(s.check_query("zebra", all, 3).corrected.is_none());
    }
}
