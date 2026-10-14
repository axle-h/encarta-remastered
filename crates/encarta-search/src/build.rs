//! Index builder: typed documents in, `out/search/` files out.
//!
//! Layout of the output directory:
//! - `index/` — the tantivy index (schema in [`crate::schema`]);
//! - `suggest.fst`, `suggest.bin` — autocomplete ([`crate::suggest`]);
//! - `spell.fst`, `spell.bin` — spelling ([`crate::spell`]);
//! - `meta.json` — format version and build statistics.

use crate::schema::{DOC_ITEM, DOC_PARA, EDITIONS, Fields, build_schema, category_code, register_analyzers};
use crate::suggest::SuggestInput;
use crate::text::{for_each_word, normalize_with, plain_analyzer};
use anyhow::{Context, Result, bail};
use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Instant;
use tantivy::tokenizer::TextAnalyzer;
use tantivy::{Index, IndexWriter, TantivyDocument};

/// Bumped whenever the index layout changes; the reader refuses other versions.
pub const FORMAT_VERSION: u32 = 1;

/// One searchable item.
#[derive(Debug, Clone, Default)]
pub struct ItemDoc {
    pub ref_id: u64,
    /// One of [`crate::schema::CATEGORIES`].
    pub category: &'static str,
    /// The content database's `item.kind` (or the dictionary kind for dictionary entries).
    pub item_kind: String,
    /// Bit mask over [`EDITIONS`].
    pub editions: u8,
    /// Dictionary name (`DICTA`, `THESA`, `ENG_FRA` …) for dictionary entries.
    pub dictionary: Option<String>,
    pub title: String,
    pub title_html: Option<String>,
    /// Alternative names (PFX sort and display titles, dictionary forms), searched as text.
    pub aliases: Vec<String>,
    /// Other whole titles of the item (besides `title`) that count as an exact title match.
    pub keys: Vec<String>,
    /// Section titles and essay headings.
    pub headings: Vec<String>,
    pub body: String,
    /// Short plain text shown when no paragraph matches (first paragraph, caption, definition).
    pub summary: String,
    /// Static ranking prior (category weight × popularity), multiplies the BM25 score.
    pub prior: f64,
}

/// One article paragraph (or intro, by-line, essay heading).
#[derive(Debug, Clone, Default)]
pub struct ParaDoc {
    pub ref_id: u64,
    pub category: &'static str,
    pub item_kind: String,
    pub editions: u8,
    /// `p`, `intro`, `byline` or `heading` (separate id spaces, anchors `#p`, `#i`, `#b`, `#h`).
    pub kind: String,
    pub para_id: u64,
    pub section_id: Option<u64>,
    pub section_title: Option<String>,
    /// Position in the article (0-based or 1-based); earlier paragraphs win ties for deep links.
    pub ordinal: u64,
    pub text: String,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct BuildStats {
    pub format_version: u32,
    pub item_docs: u64,
    pub para_docs: u64,
    pub items_by_category: std::collections::BTreeMap<String, u64>,
    pub suggest_entries: usize,
    pub suggest_keys: usize,
    /// DATAF index terms used as aliases, and the (term, item) aliases they added.
    pub index_terms: u64,
    pub index_term_aliases: u64,
    pub spell_words: usize,
    pub index_bytes: u64,
    pub total_bytes: u64,
    pub segments: usize,
    pub seconds_indexing: f64,
    pub seconds_merging: f64,
    pub seconds_total: f64,
}

pub struct IndexBuilder {
    dir: PathBuf,
    writer: IndexWriter,
    index: Index,
    f: Fields,
    analyzer: TextAnalyzer,
    counts: HashMap<String, u32>,
    suggestions: Vec<SuggestInput>,
    stats: BuildStats,
    started: Instant,
}

pub struct BuildOptions {
    pub threads: usize,
    pub heap_bytes: usize,
    /// Merge the index down to one segment at the end (smaller, faster queries).
    pub merge: bool,
}

impl Default for BuildOptions {
    fn default() -> Self {
        let threads = std::thread::available_parallelism().map_or(4, |n| n.get()).clamp(1, 8);
        BuildOptions { threads, heap_bytes: (threads * 192) << 20, merge: true }
    }
}

fn dir_size(p: &Path) -> u64 {
    let Ok(rd) = std::fs::read_dir(p) else { return 0 };
    rd.flatten()
        .map(|e| match e.metadata() {
            Ok(m) if m.is_dir() => dir_size(&e.path()),
            Ok(m) => m.len(),
            Err(_) => 0,
        })
        .sum()
}

impl IndexBuilder {
    /// Starts a fresh build in `dir` (which must not exist or be empty).
    pub fn create(dir: &Path, opts: &BuildOptions) -> Result<Self> {
        if dir.exists() && std::fs::read_dir(dir)?.next().is_some() {
            bail!("{} is not empty", dir.display());
        }
        let index_dir = dir.join("index");
        std::fs::create_dir_all(&index_dir)?;
        let (schema, f) = build_schema();
        let index = Index::create_in_dir(&index_dir, schema)?;
        register_analyzers(&index);
        let writer: IndexWriter = index.writer_with_num_threads(opts.threads, opts.heap_bytes)?;
        if opts.merge {
            // One explicit merge at the end; background merges would race with it.
            writer.set_merge_policy(Box::new(tantivy::indexer::NoMergePolicy));
        }
        Ok(IndexBuilder {
            dir: dir.to_path_buf(),
            writer,
            index,
            f,
            analyzer: plain_analyzer(),
            counts: HashMap::new(),
            suggestions: Vec::new(),
            stats: BuildStats { format_version: FORMAT_VERSION, ..Default::default() },
            started: Instant::now(),
        })
    }

    fn common(&self, doc: &mut TantivyDocument, doc_type: u64, ref_id: u64, category: &str, kind: &str, editions: u8) {
        let f = &self.f;
        doc.add_u64(f.doc_type, doc_type);
        doc.add_u64(f.ref_id, ref_id);
        doc.add_text(f.category, category);
        doc.add_u64(f.cat_code, category_code(category).unwrap_or(category_code("other").unwrap()));
        doc.add_text(f.item_kind, kind);
        for (i, e) in EDITIONS.iter().enumerate() {
            if editions & (1 << i) != 0 {
                doc.add_text(f.edition, e);
            }
        }
        doc.add_u64(f.edition_mask, editions as u64);
    }

    pub fn add_item(&mut self, d: &ItemDoc) -> Result<()> {
        let f = self.f;
        let mut doc = TantivyDocument::default();
        self.common(&mut doc, DOC_ITEM, d.ref_id, d.category, &d.item_kind, d.editions);
        if let Some(dict) = &d.dictionary {
            doc.add_text(f.dictionary, dict);
        }
        doc.add_text(f.title, &d.title);
        if let Some(h) = d.title_html.as_ref().filter(|h| **h != d.title) {
            doc.add_text(f.title_html, h);
        }
        let mut keys: Vec<String> = std::iter::once(&d.title)
            .chain(&d.keys)
            .map(|t| normalize_with(&mut self.analyzer, t))
            .filter(|k| !k.is_empty())
            .collect();
        keys.sort();
        keys.dedup();
        for k in keys {
            doc.add_text(f.title_key, k);
        }
        for a in &d.aliases {
            doc.add_text(f.aliases, a);
        }
        for h in &d.headings {
            doc.add_text(f.headings, h);
        }
        if !d.body.is_empty() {
            doc.add_text(f.body, &d.body);
        }
        if !d.summary.is_empty() {
            doc.add_text(f.summary, &d.summary);
        }
        doc.add_f64(f.prior, d.prior);
        self.writer.add_document(doc)?;
        self.stats.item_docs += 1;
        *self.stats.items_by_category.entry(d.category.to_string()).or_default() += 1;
        // Title words count towards the spelling frequencies too.
        let counts = &mut self.counts;
        for_each_word(&mut self.analyzer, &d.title, |w| *counts.entry(w.to_string()).or_default() += 1);
        Ok(())
    }

    pub fn add_para(&mut self, p: &ParaDoc) -> Result<()> {
        let f = self.f;
        let mut doc = TantivyDocument::default();
        self.common(&mut doc, DOC_PARA, p.ref_id, p.category, &p.item_kind, p.editions);
        doc.add_text(f.para_kind, &p.kind);
        doc.add_u64(f.para_id, p.para_id);
        if let Some(s) = p.section_id {
            doc.add_u64(f.section_id, s);
        }
        if let Some(t) = &p.section_title {
            doc.add_text(f.section_title, t);
        }
        doc.add_text(f.para_text, &p.text);
        doc.add_f64(f.prior, 1.0 / (1.0 + 0.03 * p.ordinal as f64));
        self.writer.add_document(doc)?;
        self.stats.para_docs += 1;
        let counts = &mut self.counts;
        for_each_word(&mut self.analyzer, &p.text, |w| {
            if let Some(c) = counts.get_mut(w) {
                *c += 1;
            } else {
                counts.insert(w.to_string(), 1);
            }
        });
        Ok(())
    }

    pub fn set_index_terms(&mut self, terms: u64, aliases: u64) {
        self.stats.index_terms = terms;
        self.stats.index_term_aliases = aliases;
    }

    pub fn stats(&self) -> &BuildStats {
        &self.stats
    }

    pub fn add_suggestion(&mut self, s: SuggestInput) {
        self.suggestions.push(s);
    }

    /// Commits the index, merges it, writes the suggest and spell files and `meta.json`.
    pub fn finish(mut self, opts: &BuildOptions, lexicons: &[(String, Vec<String>)]) -> Result<BuildStats> {
        self.writer.commit().context("commit")?;
        self.stats.seconds_indexing = self.started.elapsed().as_secs_f64();
        let t = Instant::now();
        let mut writer = self.writer;
        if opts.merge {
            let ids = self.index.searchable_segment_ids()?;
            if ids.len() > 1 {
                writer.merge(&ids).wait().context("merge")?;
            }
        }
        writer.wait_merging_threads()?;
        self.stats.seconds_merging = t.elapsed().as_secs_f64();
        self.stats.segments = self.index.searchable_segment_ids()?.len();

        let (n, k) = crate::suggest::build(&self.dir, std::mem::take(&mut self.suggestions))?;
        self.stats.suggest_entries = n;
        self.stats.suggest_keys = k;
        self.stats.spell_words = crate::spell::build(&self.dir, lexicons, &self.counts)?;
        self.stats.index_bytes = dir_size(&self.dir.join("index"));
        self.stats.seconds_total = self.started.elapsed().as_secs_f64();
        std::fs::write(self.dir.join("meta.json"), serde_json::to_vec_pretty(&self.stats)?)?;
        self.stats.total_bytes = dir_size(&self.dir);
        std::fs::write(self.dir.join("meta.json"), serde_json::to_vec_pretty(&self.stats)?)?;
        Ok(self.stats)
    }
}
