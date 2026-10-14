//! Query side: opens `out/search/` and answers search, suggest and spell requests.

use crate::build::FORMAT_VERSION;
use crate::schema::{CATEGORIES, DOC_ITEM, DOC_PARA, EDITIONS, Fields, category_code, fields_of, register_analyzers};
use crate::spell::{SpellResult, Speller};
use crate::suggest::{Suggester, Suggestion, escape};
use crate::text::{normalize, stem_analyzer, truncate_words};
use anyhow::{Context, Result, bail};
use serde::Serialize;
use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use tantivy::collector::{Collector, Count, SegmentCollector, TopDocs};
use tantivy::columnar::ColumnValues;
use tantivy::query::{
    BooleanQuery, BoostQuery, ConstScoreQuery, Occur, PhrasePrefixQuery, PhraseQuery, Query, QueryParser, RegexQuery, TermQuery, TermSetQuery,
};
use tantivy::schema::{IndexRecordOption, Value};
use tantivy::snippet::SnippetGenerator;
use tantivy::tokenizer::TokenStream;
use tantivy::{DocAddress, DocId, Index, IndexReader, ReloadPolicy, Score, SegmentOrdinal, SegmentReader, TantivyDocument, Term};

pub struct SearchIndex {
    index: Index,
    reader: IndexReader,
    f: Fields,
    suggester: Option<Suggester>,
    speller: Option<Speller>,
    pub meta: serde_json::Value,
}

#[derive(Debug, Clone, Default)]
pub struct SearchRequest {
    pub q: String,
    /// A category (`article`, `media`, `dictionary`, `timeline`, `map`, `quotation`, `sidebar`,
    /// `web`, `other`) or an exact item kind (`video`, `yearbook-article` …).
    pub kind: Option<String>,
    /// `deluxe` or `kids`.
    pub edition: Option<String>,
    pub limit: usize,
    pub offset: usize,
    /// Find the best paragraph of each hit and build snippets.
    pub snippets: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ParaHit {
    pub kind: String,
    pub id: u64,
    pub section_id: Option<u64>,
    pub section_title: Option<String>,
    /// `#p12`, `#i3`, `#h4` or `#b1`, the element id in the article HTML.
    pub anchor: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Hit {
    pub ref_id: u64,
    pub category: String,
    pub kind: String,
    pub editions: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dictionary: Option<String>,
    pub title: String,
    pub title_html: String,
    pub score: f32,
    /// Client URL: `/item/<id>[#p<para>]` or `/dictionary/<dict>/<id>`.
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub para: Option<ParaHit>,
    /// HTML (escaped text with `<b>` around matched words).
    pub snippet_html: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SearchResponse {
    pub query: String,
    /// `all` when every word had to match, `any` when the search fell back to any word.
    pub mode: &'static str,
    pub total: usize,
    pub offset: usize,
    pub hits: Vec<Hit>,
    /// Hit counts per category and per edition, for the query without the kind/edition filters.
    pub facets: Facets,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub did_you_mean: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Facets {
    pub kind: BTreeMap<String, u64>,
    pub edition: BTreeMap<String, u64>,
}

/// Counts hits per category code and edition bit.
struct FacetCounter;

#[derive(Default)]
struct FacetFruit {
    cats: [u64; CATEGORIES.len()],
    eds: [u64; EDITIONS.len()],
}

struct FacetSeg {
    cat: std::sync::Arc<dyn ColumnValues<u64>>,
    ed: std::sync::Arc<dyn ColumnValues<u64>>,
    fruit: FacetFruit,
}

impl Collector for FacetCounter {
    type Fruit = FacetFruit;
    type Child = FacetSeg;
    fn for_segment(&self, _: SegmentOrdinal, r: &SegmentReader) -> tantivy::Result<FacetSeg> {
        let other = category_code("other").unwrap();
        Ok(FacetSeg {
            cat: r.fast_fields().u64("cat_code")?.first_or_default_col(other),
            ed: r.fast_fields().u64("edition_mask")?.first_or_default_col(0),
            fruit: FacetFruit::default(),
        })
    }
    fn requires_scoring(&self) -> bool {
        false
    }
    fn merge_fruits(&self, fruits: Vec<FacetFruit>) -> tantivy::Result<FacetFruit> {
        let mut out = FacetFruit::default();
        for f in fruits {
            for (a, b) in out.cats.iter_mut().zip(f.cats) {
                *a += b;
            }
            for (a, b) in out.eds.iter_mut().zip(f.eds) {
                *a += b;
            }
        }
        Ok(out)
    }
}

impl SegmentCollector for FacetSeg {
    type Fruit = FacetFruit;
    fn collect(&mut self, doc: DocId, _: Score) {
        let c = self.cat.get_val(doc) as usize;
        if let Some(n) = self.fruit.cats.get_mut(c) {
            *n += 1;
        }
        let m = self.ed.get_val(doc);
        for (i, n) in self.fruit.eds.iter_mut().enumerate() {
            if m & (1 << i) != 0 {
                *n += 1;
            }
        }
    }
    fn harvest(self) -> FacetFruit {
        self.fruit
    }
}

/// Keeps the best-scoring document per `ref_id`.
struct BestPerRef;

struct BestSeg {
    seg: SegmentOrdinal,
    refs: std::sync::Arc<dyn ColumnValues<u64>>,
    prior: std::sync::Arc<dyn ColumnValues<f64>>,
    best: HashMap<u64, (Score, DocAddress)>,
}

impl Collector for BestPerRef {
    type Fruit = HashMap<u64, (Score, DocAddress)>;
    type Child = BestSeg;
    fn for_segment(&self, seg: SegmentOrdinal, r: &SegmentReader) -> tantivy::Result<BestSeg> {
        Ok(BestSeg {
            seg,
            refs: r.fast_fields().u64("ref_id")?.first_or_default_col(0),
            prior: r.fast_fields().f64("prior")?.first_or_default_col(1.0),
            best: HashMap::new(),
        })
    }
    fn requires_scoring(&self) -> bool {
        true
    }
    fn merge_fruits(&self, fruits: Vec<Self::Fruit>) -> tantivy::Result<Self::Fruit> {
        let mut out: Self::Fruit = HashMap::new();
        for f in fruits {
            for (k, v) in f {
                let e = out.entry(k).or_insert(v);
                if v.0 > e.0 {
                    *e = v;
                }
            }
        }
        Ok(out)
    }
}

impl SegmentCollector for BestSeg {
    type Fruit = HashMap<u64, (Score, DocAddress)>;
    fn collect(&mut self, doc: DocId, score: Score) {
        let r = self.refs.get_val(doc);
        let score = score * self.prior.get_val(doc) as f32;
        let addr = DocAddress::new(self.seg, doc);
        let e = self.best.entry(r).or_insert((score, addr));
        if score > e.0 {
            *e = (score, addr);
        }
    }
    fn harvest(self) -> Self::Fruit {
        self.best
    }
}

fn anchor_prefix(kind: &str) -> &'static str {
    match kind {
        "intro" => "i",
        "byline" => "b",
        "heading" => "h",
        _ => "p",
    }
}

/// Added to the text score when the whole query equals a title or whole-title alias.
const EXACT_TITLE_SCORE: f32 = 30.0;

/// Added when a title (or whole-title alias) starts with the query, its last word typed in part
/// ("french rev" → "French Revolution").
const PREFIX_TITLE_SCORE: f32 = 20.0;

/// Most indexed terms a partly typed last word expands to, per field (the most frequent win).
const PREFIX_EXPANSIONS: usize = 24;

/// The query split for prefix matching: the words before the last, and the last word as typed
/// (plain: lower-case, accent-folded, not stemmed) when it is to be read as a prefix.
#[derive(Debug, PartialEq)]
pub struct QuerySplit {
    pub head: String,
    pub last: String,
    pub prefix: Option<String>,
}

/// The last word is a prefix unless the query ends with white space or a quote, or the word is a
/// single character.
pub fn split_query(q: &str) -> QuerySplit {
    let t = q.trim_start();
    let ends_open = !t.is_empty() && !t.ends_with(char::is_whitespace) && !t.ends_with('"');
    let cut = t.trim_end().rfind(char::is_whitespace).map_or(0, |i| i + 1);
    let (head, last) = t.trim_end().split_at(cut);
    let plain = normalize(last);
    let prefix = (ends_open && !last.contains(['"', ':', '*']) && plain.chars().count() >= 2 && !plain.contains(' ')).then_some(plain);
    QuerySplit { head: head.trim().to_string(), last: last.to_string(), prefix }
}

/// Escapes ASCII punctuation for a tantivy regex.
fn regex_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 4);
    for c in s.chars() {
        if c.is_ascii_punctuation() {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

fn const_q(q: Box<dyn Query>) -> Box<dyn Query> {
    Box::new(ConstScoreQuery::new(q, 0.0))
}

impl SearchIndex {
    /// Opens `dir` (an `out/search/` directory). Missing suggest/spell files are tolerated.
    pub fn open(dir: &Path) -> Result<Self> {
        let meta: serde_json::Value = serde_json::from_slice(
            &std::fs::read(dir.join("meta.json")).with_context(|| format!("{}/meta.json", dir.display()))?,
        )?;
        let v = meta.get("format_version").and_then(|v| v.as_u64()).unwrap_or(0);
        if v != FORMAT_VERSION as u64 {
            bail!("search index format {v}, this build reads {FORMAT_VERSION}: rebuild with `encarta-convert search`");
        }
        let index = Index::open(crate::ro_dir::ReadOnlyDir::open(&dir.join("index"))?)?;
        register_analyzers(&index);
        let f = fields_of(&index.schema())?;
        let reader = index.reader_builder().reload_policy(ReloadPolicy::Manual).try_into()?;
        Ok(SearchIndex {
            index,
            reader,
            f,
            suggester: Suggester::open(dir).ok(),
            speller: Speller::open(dir).ok(),
            meta,
        })
    }

    pub fn suggester(&self) -> Option<&Suggester> {
        self.suggester.as_ref()
    }

    pub fn speller(&self) -> Option<&Speller> {
        self.speller.as_ref()
    }

    pub fn num_docs(&self) -> u64 {
        self.reader.searcher().num_docs()
    }

    /// Stemmed terms of `text` in `field`.
    fn terms(&self, field: tantivy::schema::Field, text: &str) -> Vec<Term> {
        let mut a = stem_analyzer();
        let mut st = a.token_stream(text);
        let mut out = Vec::new();
        while st.advance() {
            out.push(Term::from_field_text(field, &st.token().text));
        }
        out
    }

    fn phrase(&self, field: tantivy::schema::Field, text: &str, slop: u32, boost: f32) -> Option<Box<dyn Query>> {
        let terms = self.terms(field, text);
        if terms.len() < 2 {
            return None;
        }
        let mut p = PhraseQuery::new(terms);
        p.set_slop(slop);
        Some(Box::new(BoostQuery::new(Box::new(p), boost)))
    }

    /// The indexed terms of `field` that start with `prefix`, most frequent first (at most
    /// [`PREFIX_EXPANSIONS`]).
    fn expand_prefix(&self, searcher: &tantivy::Searcher, field: tantivy::schema::Field, prefix: &str) -> Vec<Term> {
        let mut df: HashMap<Vec<u8>, u64> = HashMap::new();
        for seg in searcher.segment_readers() {
            let Ok(inv) = seg.inverted_index(field) else { continue };
            let Ok(mut st) = inv.terms().range().ge(prefix.as_bytes()).into_stream() else { continue };
            let mut scanned = 0;
            while st.advance() && scanned < 20_000 {
                if !st.key().starts_with(prefix.as_bytes()) {
                    break;
                }
                scanned += 1;
                *df.entry(st.key().to_vec()).or_default() += st.value().doc_freq as u64;
            }
        }
        let mut v: Vec<(Vec<u8>, u64)> = df.into_iter().collect();
        v.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        v.truncate(PREFIX_EXPANSIONS);
        v.into_iter().filter_map(|(k, _)| String::from_utf8(k).ok()).map(|t| Term::from_field_text(field, &t)).collect()
    }

    /// Words of `q` over `fields` (with boosts): every word must match in conjunction mode, any
    /// word otherwise; a partly typed last word ([`split_query`]) also matches the indexed terms
    /// it starts.
    fn words_query(&self, searcher: &tantivy::Searcher, fields: &[(tantivy::schema::Field, f32)], q: &str, conjunction: bool) -> Box<dyn Query> {
        let mut parser = QueryParser::for_index(&self.index, fields.iter().map(|f| f.0).collect());
        for (f, b) in fields {
            parser.set_field_boost(*f, *b);
        }
        if conjunction {
            parser.set_conjunction_by_default();
        }
        let split = split_query(q);
        let Some(prefix) = split.prefix else {
            return parser.parse_query_lenient(q).0;
        };
        let occur = if conjunction { Occur::Must } else { Occur::Should };
        let mut clauses: Vec<(Occur, Box<dyn Query>)> = Vec::new();
        if !split.head.is_empty() {
            clauses.push((occur, parser.parse_query_lenient(&split.head).0));
        }
        let mut last: Vec<(Occur, Box<dyn Query>)> = vec![(Occur::Should, parser.parse_query_lenient(&split.last).0)];
        for (f, b) in fields {
            for t in self.expand_prefix(searcher, *f, &prefix) {
                let tq = TermQuery::new(t, IndexRecordOption::WithFreqs);
                last.push((Occur::Should, Box::new(BoostQuery::new(Box::new(tq), *b))));
            }
        }
        clauses.push((occur, Box::new(BooleanQuery::new(last))));
        Box::new(BooleanQuery::new(clauses))
    }

    /// The text part of the item query (no filters).
    fn item_text_query(&self, searcher: &tantivy::Searcher, q: &str, conjunction: bool) -> Box<dyn Query> {
        let f = &self.f;
        let fields = [(f.title, 4.0), (f.aliases, 2.5), (f.headings, 1.5), (f.body, 1.0)];
        let parsed = self.words_query(searcher, &fields, q, conjunction);
        let mut clauses: Vec<(Occur, Box<dyn Query>)> = vec![(Occur::Must, parsed)];
        let key = normalize(q);
        if !key.is_empty() {
            let exact = TermQuery::new(Term::from_field_text(f.title_key, &key), IndexRecordOption::Basic);
            // A constant: BM25 on this multi-valued key field would punish items with many aliases.
            clauses.push((Occur::Should, Box::new(ConstScoreQuery::new(Box::new(exact), EXACT_TITLE_SCORE))));
        }
        let split = split_query(q);
        if split.prefix.is_some() && !key.is_empty() {
            if let Ok(starts) = RegexQuery::from_pattern(&format!("{}.+", regex_escape(&key)), f.title_key) {
                clauses.push((Occur::Should, Box::new(ConstScoreQuery::new(Box::new(starts), PREFIX_TITLE_SCORE))));
            }
            let mut terms = self.terms(f.title, &split.head);
            if !terms.is_empty() {
                terms.push(Term::from_field_text(f.title, split.prefix.as_deref().unwrap_or("")));
                let mut pp = PhrasePrefixQuery::new(terms);
                pp.set_max_expansions(PREFIX_EXPANSIONS as u32);
                clauses.push((Occur::Should, Box::new(BoostQuery::new(Box::new(pp), 4.0))));
            }
        } else if let Some(p) = self.phrase(f.title, q, 0, 4.0) {
            clauses.push((Occur::Should, p));
        }
        if let Some(p) = self.phrase(f.body, q, 1, 1.5) {
            clauses.push((Occur::Should, p));
        }
        Box::new(BooleanQuery::new(clauses))
    }

    fn filters(&self, req: &SearchRequest) -> Vec<(Occur, Box<dyn Query>)> {
        let f = &self.f;
        let mut out = Vec::new();
        if let Some(k) = req.kind.as_deref().filter(|k| !k.is_empty() && *k != "all") {
            let q: Box<dyn Query> = match category_code(k) {
                Some(c) => Box::new(TermQuery::new(Term::from_field_u64(f.cat_code, c), IndexRecordOption::Basic)),
                None => Box::new(TermQuery::new(Term::from_field_text(f.item_kind, k), IndexRecordOption::Basic)),
            };
            out.push((Occur::Must, const_q(q)));
        }
        if let Some(e) = req.edition.as_deref().filter(|e| !e.is_empty() && *e != "all") {
            let q = TermQuery::new(Term::from_field_text(f.edition, e), IndexRecordOption::Basic);
            out.push((Occur::Must, const_q(Box::new(q))));
        } else if req.edition.as_deref() != Some("all") {
            // No edition asked for: Premium search, which has no Kids-only items (`edition=all` keeps them).
            let has = |e: &str| -> Box<dyn Query> {
                Box::new(TermQuery::new(Term::from_field_text(f.edition, e), IndexRecordOption::Basic))
            };
            let kids_only = BooleanQuery::new(vec![(Occur::Must, has("kids")), (Occur::MustNot, has("deluxe"))]);
            out.push((Occur::MustNot, const_q(Box::new(kids_only))));
        }
        out
    }

    fn doc_type(&self, t: u64) -> (Occur, Box<dyn Query>) {
        let q = TermQuery::new(Term::from_field_u64(self.f.doc_type, t), IndexRecordOption::Basic);
        (Occur::Must, const_q(Box::new(q)))
    }

    pub fn search(&self, req: &SearchRequest) -> Result<SearchResponse> {
        let searcher = self.reader.searcher();
        let limit = req.limit.clamp(1, 200);
        let q = req.q.trim();
        if q.is_empty() {
            return Ok(SearchResponse {
                query: req.q.clone(),
                mode: "all",
                total: 0,
                offset: req.offset,
                hits: Vec::new(),
                facets: Facets::default(),
                did_you_mean: None,
            });
        }
        let filters = self.filters(req);
        let mut mode = "all";
        let mut result = None;
        for conjunction in [true, false] {
            let text = self.item_text_query(&searcher, q, conjunction);
            let mut unfiltered = vec![self.doc_type(DOC_ITEM), (Occur::Must, text.box_clone())];
            let facet_query = BooleanQuery::new(std::mem::take(&mut unfiltered));
            let facets = searcher.search(&facet_query, &FacetCounter)?;
            let total_unfiltered: u64 = facets.cats.iter().sum();
            if total_unfiltered == 0 && conjunction && q.split_whitespace().count() > 1 {
                continue;
            }
            let mut clauses = vec![self.doc_type(DOC_ITEM), (Occur::Must, text.box_clone())];
            clauses.extend(filters.iter().map(|(o, q)| (*o, q.box_clone())));
            let main = BooleanQuery::new(clauses);
            let (docs, total) = searcher.search(&main, &(top_collector(limit, req.offset), Count))?;
            mode = if conjunction { "all" } else { "any" };
            result = Some((text, docs, total, facets));
            break;
        }
        let Some((text_query, docs, total, facets)) = result else {
            bail!("unreachable: disjunctive pass always runs");
        };
        let f = &self.f;
        let mut hits = Vec::with_capacity(docs.len());
        for (score, addr) in &docs {
            let d: TantivyDocument = searcher.doc(*addr)?;
            let s = |field| d.get_first(field).and_then(|v| v.as_str()).map(str::to_string);
            let ref_id = d.get_first(f.ref_id).and_then(|v| v.as_u64()).unwrap_or(0);
            let title = s(f.title).unwrap_or_default();
            let dictionary = s(f.dictionary);
            let url = match &dictionary {
                Some(dict) => format!("/dictionary/{dict}/{ref_id}"),
                None => format!("/item/{ref_id}"),
            };
            hits.push(Hit {
                ref_id,
                category: s(f.category).unwrap_or_default(),
                kind: s(f.item_kind).unwrap_or_default(),
                editions: d.get_all(f.edition).filter_map(|v| v.as_str().map(str::to_string)).collect(),
                dictionary,
                title_html: s(f.title_html).unwrap_or_else(|| escape(&title)),
                title,
                score: *score,
                url,
                para: None,
                snippet_html: s(f.summary).unwrap_or_default(),
            });
        }
        if req.snippets && !hits.is_empty() {
            self.add_snippets(&searcher, q, text_query.as_ref(), &mut hits)?;
        }
        let mut out_facets = Facets::default();
        for (i, c) in CATEGORIES.iter().enumerate() {
            if facets.cats[i] > 0 {
                out_facets.kind.insert(c.to_string(), facets.cats[i]);
            }
        }
        for (i, e) in EDITIONS.iter().enumerate() {
            out_facets.edition.insert(e.to_string(), facets.eds[i]);
        }
        let did_you_mean = if total < 5 {
            self.speller.as_ref().and_then(|sp| {
                let mask = sp.mask_for(default_lexicons(req.edition.as_deref()));
                sp.check_query(q, mask, 1).corrected
            })
        } else {
            None
        };
        Ok(SearchResponse { query: req.q.clone(), mode, total, offset: req.offset, hits, facets: out_facets, did_you_mean })
    }

    /// Best paragraph per hit (deep link + highlighted snippet); summaries for the others.
    fn add_snippets(
        &self,
        searcher: &tantivy::Searcher,
        q: &str,
        item_query: &dyn Query,
        hits: &mut [Hit],
    ) -> Result<()> {
        let f = &self.f;
        let para_text = self.words_query(searcher, &[(f.para_text, 1.0)], q, false);
        let mut text_clauses: Vec<(Occur, Box<dyn Query>)> = vec![(Occur::Must, para_text)];
        if let Some(p) = self.phrase(f.para_text, q, 1, 2.0) {
            text_clauses.push((Occur::Should, p));
        }
        let text_q: Box<dyn Query> = Box::new(BooleanQuery::new(text_clauses));
        let refs: Vec<Term> = hits.iter().filter(|h| h.dictionary.is_none()).map(|h| Term::from_field_u64(f.ref_id, h.ref_id)).collect();
        let best = if refs.is_empty() {
            HashMap::new()
        } else {
            let pq = BooleanQuery::new(vec![
                self.doc_type(DOC_PARA),
                (Occur::Must, const_q(Box::new(TermSetQuery::new(refs)))),
                (Occur::Must, text_q.box_clone()),
            ]);
            searcher.search(&pq, &BestPerRef)?
        };
        // Hits whose title is the query (or starts with it, the last word typed in part) get
        // their opening text, not a deep link.
        let key = normalize(q);
        let all_refs: Vec<Term> = hits.iter().map(|h| Term::from_field_u64(f.ref_id, h.ref_id)).collect();
        let exact: std::collections::HashSet<u64> = if key.is_empty() {
            Default::default()
        } else {
            let mut title: Vec<(Occur, Box<dyn Query>)> =
                vec![(Occur::Should, Box::new(TermQuery::new(Term::from_field_text(f.title_key, &key), IndexRecordOption::Basic)))];
            if split_query(q).prefix.is_some()
                && let Ok(starts) = RegexQuery::from_pattern(&format!("{}.+", regex_escape(&key)), f.title_key) {
                    title.push((Occur::Should, Box::new(starts)));
                }
            let eq = BooleanQuery::new(vec![
                self.doc_type(DOC_ITEM),
                (Occur::Must, const_q(Box::new(TermSetQuery::new(all_refs)))),
                (Occur::Must, Box::new(BooleanQuery::new(title))),
            ]);
            searcher.search(&eq, &BestPerRef)?.into_keys().collect()
        };
        let mut para_gen = SnippetGenerator::create(searcher, text_q.as_ref(), f.para_text)?;
        para_gen.set_max_num_chars(220);
        let mut summary_gen = SnippetGenerator::create(searcher, item_query, f.body)?;
        summary_gen.set_max_num_chars(220);
        for h in hits.iter_mut() {
            if let Some((_, addr)) = best.get(&h.ref_id).filter(|_| h.dictionary.is_none() && !exact.contains(&h.ref_id)) {
                let d: TantivyDocument = searcher.doc(*addr)?;
                let kind = d.get_first(f.para_kind).and_then(|v| v.as_str()).unwrap_or("p").to_string();
                let id = d.get_first(f.para_id).and_then(|v| v.as_u64()).unwrap_or(0);
                let text = d.get_first(f.para_text).and_then(|v| v.as_str()).unwrap_or("");
                let snip = para_gen.snippet(text);
                h.snippet_html =
                    if snip.is_empty() { escape(&truncate_words(text, 220)) } else { snip.to_html() };
                let anchor = format!("#{}{id}", anchor_prefix(&kind));
                h.url.push_str(&anchor);
                h.para = Some(ParaHit {
                    kind,
                    id,
                    section_id: d.get_first(f.section_id).and_then(|v| v.as_u64()),
                    section_title: d.get_first(f.section_title).and_then(|v| v.as_str()).map(str::to_string),
                    anchor,
                });
            } else {
                let snip = summary_gen.snippet(&h.snippet_html);
                h.snippet_html =
                    if snip.is_empty() { escape(&truncate_words(&h.snippet_html, 220)) } else { snip.to_html() };
            }
        }
        Ok(())
    }

    /// Score explanation of the item document `ref_id` for query `q` (debugging aid).
    pub fn explain(&self, q: &str, ref_id: u64) -> Result<String> {
        let searcher = self.reader.searcher();
        let query = BooleanQuery::new(vec![self.doc_type(DOC_ITEM), (Occur::Must, self.item_text_query(&searcher, q, true))]);
        let find = BooleanQuery::new(vec![
            self.doc_type(DOC_ITEM),
            (Occur::Must, Box::new(TermQuery::new(Term::from_field_u64(self.f.ref_id, ref_id), IndexRecordOption::Basic))),
        ]);
        let docs = searcher.search(&find, &TopDocs::with_limit(1).order_by_score())?;
        let Some((_, addr)) = docs.first() else { bail!("{ref_id} not indexed") };
        Ok(query.explain(&searcher, *addr)?.to_pretty_json())
    }

    pub fn suggest(&self, q: &str, limit: usize, edition: Option<&str>, kind: Option<&str>) -> Vec<Suggestion> {
        let Some(s) = &self.suggester else { return Vec::new() };
        let ed = edition.and_then(|e| EDITIONS.iter().position(|x| *x == e)).map(|i| 1u8 << i);
        let cat = kind.and_then(category_code).map(|c| c as u8);
        s.suggest(q, limit, ed, cat)
    }

    /// Spelling check of a query. `lexicons` are lexicon names (default: [`default_lexicons`]).
    pub fn spell(&self, q: &str, lexicons: &[&str], edition: Option<&str>, limit: usize) -> Option<SpellResult> {
        let sp = self.speller.as_ref()?;
        let mask = if !lexicons.is_empty() { sp.mask_for(lexicons) } else { sp.mask_for(default_lexicons(edition)) };
        Some(sp.check_query(q, mask, limit))
    }
}

/// English word lists used for spelling: the search spellers (which hold mostly names and rare
/// words, not "zebra") plus the dictionary and thesaurus headword lists.
pub fn default_lexicons(edition: Option<&str>) -> &'static [&'static str] {
    match edition {
        Some("kids") => &["EESPLA01_KIDS", "DICTA"],
        _ => &["EESPLA01", "EESPLA01_KIDS", "DICTA", "THESA"],
    }
}

/// Top documents by BM25 × the `prior` fast field.
fn top_collector(limit: usize, offset: usize) -> impl Collector<Fruit = Vec<(Score, DocAddress)>> {
    TopDocs::with_limit(limit).and_offset(offset).tweak_score(move |seg: &SegmentReader| {
        let prior = seg.fast_fields().f64("prior").map(|c| c.first_or_default_col(1.0)).ok();
        move |doc: DocId, score: Score| match &prior {
            Some(p) => score * p.get_val(doc) as f32,
            None => score,
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_the_last_word_as_a_prefix() {
        assert_eq!(
            split_query("French Rev"),
            QuerySplit { head: "French".into(), last: "Rev".into(), prefix: Some("rev".into()) }
        );
        assert_eq!(split_query("zébr").prefix.as_deref(), Some("zebr"));
        // A finished word (trailing space), a quoted phrase or a single letter stays a word.
        assert_eq!(split_query("french rev ").prefix, None);
        assert_eq!(split_query("\"french rev\"").prefix, None);
        assert_eq!(split_query("french r").prefix, None);
        assert_eq!(split_query("").prefix, None);
        assert_eq!(regex_escape("a.b c"), "a\\.b c");
    }

    fn test_index() -> (tempfile::TempDir, SearchIndex) {
        use crate::build::{BuildOptions, IndexBuilder, ItemDoc};
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("s");
        let opts = BuildOptions { threads: 1, heap_bytes: 50 << 20, merge: false };
        let mut b = IndexBuilder::create(&root, &opts).unwrap();
        for (id, title, ed, body) in [
            (1, "Rome (Italy)", 1u8, "capital of Italy"),
            (2, "Rome (Georgia)", 1, "city in Georgia"),
            (3, "University of Rome", 1, "rome university rome rome"),
            (4, "Rome", 2, "kids rome"),
            (5, "Zebra", 1, "striped"),
            (6, "Zebras", 2, "kids zebras"),
        ] {
            b.add_item(&ItemDoc {
                ref_id: id,
                category: "article",
                item_kind: "article".into(),
                editions: ed,
                title: title.into(),
                body: body.into(),
                prior: 1.0,
                ..Default::default()
            })
            .unwrap();
        }
        b.finish(&opts, &[]).unwrap();
        let idx = SearchIndex::open(&root).unwrap();
        (dir, idx)
    }

    fn titles(idx: &SearchIndex, q: &str, edition: Option<&str>) -> Vec<String> {
        let req = SearchRequest { q: q.into(), edition: edition.map(str::to_string), limit: 10, ..Default::default() };
        idx.search(&req).unwrap().hits.into_iter().map(|h| h.title).collect()
    }

    #[test]
    fn default_search_has_no_kids_only_items() {
        let (_d, idx) = test_index();
        assert_eq!(titles(&idx, "zebra", None), ["Zebra"]);
        assert_eq!(titles(&idx, "zebra", Some("kids")), ["Zebras"]);
        assert_eq!(titles(&idx, "zebra", Some("all")).len(), 2);
        assert!(!titles(&idx, "rome", None).contains(&"Rome".to_string()));
        assert_eq!(titles(&idx, "rome", Some("kids")), ["Rome"]);
    }

    #[test]
    fn parenthetical_titles_are_exact_matches() {
        let (_d, idx) = test_index();
        let t = titles(&idx, "rome", None);
        assert_eq!(t.len(), 3);
        assert_eq!(t[2], "University of Rome", "{t:?}");
    }
}
