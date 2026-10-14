//! `out/content.sqlite`: schema and a writer thread fed by row batches.
//! The schema is documented in `docs/content-schema.md`; keep the two in step.

use anyhow::{Context, Result};
use rusqlite::types::Value as Sql;
use rusqlite::{Connection, params_from_iter};
use std::collections::HashMap;
use std::path::Path;
use std::sync::mpsc::{Receiver, SyncSender, sync_channel};

pub const SCHEMA_VERSION: &str = "2";

/// `(table, columns)`; rows are inserted positionally in this column order.
pub const TABLES: &[(&str, &str)] = &[
    ("meta", "key TEXT PRIMARY KEY, value TEXT"),
    (
        "item",
        "ref_id INTEGER PRIMARY KEY, kind TEXT NOT NULL, source_group TEXT, title TEXT, title_html TEXT, \
         sort_title TEXT, display_title_html TEXT, full_title_html TEXT, alt_title_html TEXT, \
         in_deluxe INTEGER NOT NULL, in_kids INTEGER NOT NULL, content_package TEXT, data_package TEXT, \
         related_package TEXT, media_type TEXT, subgroup TEXT, subtype TEXT, style INTEGER, geocode INTEGER, \
         origin TEXT, revision INTEGER, word_count INTEGER, caption_html TEXT, credit TEXT, credit_html TEXT, \
         preview_html TEXT, meta_json TEXT",
    ),
    (
        "article",
        "ref_id INTEGER PRIMARY KEY, source TEXT NOT NULL, source_file TEXT, body_json TEXT NOT NULL, \
         html TEXT NOT NULL, outline_json TEXT, paragraph_count INTEGER, section_count INTEGER, \
         word_count INTEGER, title_ref_count INTEGER, layout TEXT",
    ),
    (
        "section",
        "ref_id INTEGER, section_id INTEGER, parent_id INTEGER, level INTEGER, number TEXT, title TEXT, \
         type_code INTEGER, PRIMARY KEY(ref_id, section_id)",
    ),
    (
        "paragraph",
        "ref_id INTEGER, kind TEXT, para_id INTEGER, section_id INTEGER, ordinal INTEGER, text TEXT, \
         PRIMARY KEY(ref_id, kind, para_id)",
    ),
    (
        "link",
        "ref_id INTEGER, para_id INTEGER, ordinal INTEGER, kind TEXT, target_ref_id INTEGER, \
         target_para INTEGER, url TEXT, text TEXT",
    ),
    (
        "assoc",
        "ref_id INTEGER, direction TEXT, ordinal INTEGER, target_ref_id INTEGER, grp TEXT, sec INTEGER, \
         sequence INTEGER, appears INTEGER, par INTEGER, reviewed INTEGER",
    ),
    (
        "item_file",
        "ref_id INTEGER, ordinal INTEGER, role TEXT, file_name TEXT, raw_path TEXT, width INTEGER, \
         height INTEGER, src_ref_id INTEGER, disc INTEGER, package_file TEXT",
    ),
    (
        "item_child",
        "ref_id INTEGER, ordinal INTEGER, kind TEXT, child_ref_id INTEGER, sequence INTEGER",
    ),
    (
        "data_table",
        "ref_id INTEGER PRIMARY KEY, kind TEXT, json TEXT, html TEXT",
    ),
    (
        "related_tree",
        "ref_id INTEGER, editions TEXT, tree_json TEXT, node_count INTEGER, PRIMARY KEY(ref_id, editions)",
    ),
    (
        "index_tree",
        "ref_id INTEGER, editions TEXT, title TEXT, tree_json TEXT, node_count INTEGER, \
         PRIMARY KEY(ref_id, editions)",
    ),
    ("contributor", "ref_id INTEGER PRIMARY KEY, json TEXT"),
    (
        "baggage",
        "key TEXT, edition TEXT, package_file TEXT, catalog_table TEXT, PRIMARY KEY(key, edition)",
    ),
    (
        "alias",
        "ref_id INTEGER, edition TEXT, collection INTEGER, class_id INTEGER, sort_title TEXT, \
         display_title TEXT, display_html TEXT",
    ),
    (
        "search_entry",
        "entry_id INTEGER, edition TEXT, ref_id INTEGER, content_type INTEGER, is_list INTEGER, \
         section INTEGER, para_start INTEGER, para_end INTEGER, dual_para_start INTEGER, \
         dual_para_end INTEGER, dupe_id INTEGER, word_count INTEGER, text TEXT, display_html TEXT, \
         parents TEXT, PRIMARY KEY(edition, entry_id)",
    ),
    (
        "search_word",
        "edition TEXT, word TEXT, word_id INTEGER, alt_id INTEGER, stop_word_type INTEGER",
    ),
    (
        "dictionary",
        "name TEXT PRIMARY KEY, kind TEXT, title TEXT, uniq_tag TEXT, lex_file TEXT, guid TEXT, \
         parent_guid TEXT, entry_count INTEGER, signature_json TEXT",
    ),
    (
        "dict_entry",
        "dictionary TEXT, ref_id INTEGER, headword TEXT, variety TEXT, summary TEXT, body_json TEXT, \
         html TEXT, PRIMARY KEY(dictionary, ref_id)",
    ),
    ("dict_form", "form TEXT, dictionary TEXT, ref_id INTEGER, kind TEXT"),
    (
        "dict_audio",
        "dictionary TEXT, ref_id INTEGER, file_name TEXT, audio_ref_id INTEGER",
    ),
    ("timeline_category", "bit INTEGER PRIMARY KEY, name TEXT"),
    ("timeline_region", "bit INTEGER PRIMARY KEY, name TEXT"),
    (
        "timeline_event",
        "seq INTEGER PRIMARY KEY, ref_id INTEGER, source_table TEXT, kind INTEGER, title TEXT, \
         title_html TEXT, start_year INTEGER, start_month INTEGER, start_day INTEGER, start_circa INTEGER, \
         end_year INTEGER, end_month INTEGER, end_day INTEGER, end_circa INTEGER, span INTEGER, \
         importance INTEGER, category_mask INTEGER, region_mask INTEGER, categories TEXT, regions TEXT, \
         raw_flags INTEGER, display TEXT, label TEXT, date_text TEXT, start_text TEXT, end_text TEXT, \
         prominence INTEGER",
    ),
    (
        "timeline_term",
        "term_id INTEGER PRIMARY KEY, text TEXT, text_html TEXT, ref_ids TEXT",
    ),
    (
        "mindmaze_question",
        "id INTEGER PRIMARY KEY, category INTEGER, category_label TEXT, level INTEGER, clue TEXT, \
         clue_html TEXT, location INTEGER",
    ),
    (
        "mindmaze_answer",
        "question_id INTEGER, ordinal INTEGER, correct INTEGER, text TEXT, text_html TEXT, \
         article_ref_id INTEGER, article_exists INTEGER",
    ),
    ("stat_country", "id INTEGER PRIMARY KEY, name TEXT"),
    ("stat_category", "bit INTEGER PRIMARY KEY, name TEXT"),
    ("stat_qualifier", "id INTEGER PRIMARY KEY, text TEXT"),
    (
        "stat_unit",
        "id INTEGER PRIMARY KEY, unit TEXT, alt_unit TEXT, factor REAL, offset REAL",
    ),
    (
        "stat_indicator",
        "id INTEGER PRIMARY KEY, name TEXT, category_mask INTEGER, categories TEXT, unit_id INTEGER, \
         content_ref_id INTEGER, source TEXT",
    ),
    (
        "stat_value",
        "indicator_id INTEGER, country_id INTEGER, rank INTEGER, value REAL, value_text TEXT, \
         decimals INTEGER, col_b INTEGER, year INTEGER, col_c INTEGER, note TEXT, year_end INTEGER, \
         period TEXT",
    ),
    (
        "geoquiz_level",
        "level INTEGER PRIMARY KEY, questions INTEGER, points TEXT, per_type_unknown TEXT, type_mix TEXT",
    ),
    (
        "geoquiz_place",
        "level INTEGER, ordinal INTEGER, place_type INTEGER, group_id INTEGER, place_id INTEGER, extra INTEGER",
    ),
    (
        "place_link",
        "place_id INTEGER, ordinal INTEGER, kind INTEGER, kind_label TEXT, content_ref_id INTEGER",
    ),
    (
        "atlas_media",
        "ref_id INTEGER PRIMARY KEY, kind TEXT, title TEXT, title_html TEXT, place_id INTEGER, file TEXT, \
         thumbnail TEXT, paired_ref_id INTEGER, legacy_id INTEGER, credit TEXT, other TEXT",
    ),
    (
        "lexicon",
        "name TEXT PRIMARY KEY, file TEXT, lcids TEXT, word_count INTEGER, used_by TEXT",
    ),
    ("lexicon_word", "lexicon TEXT, word TEXT"),
    (
        "interactive",
        "file_name TEXT, edition TEXT, package TEXT, control_type TEXT, title TEXT, json TEXT, \
         PRIMARY KEY(file_name, edition)",
    ),
    (
        "quiz",
        "quiz_id INTEGER, edition TEXT, quiz_type TEXT, lang TEXT, json TEXT, PRIMARY KEY(quiz_id, edition)",
    ),
];

/// Secondary indexes, created after the bulk load.
pub const INDEXES: &[&str] = &[
    "CREATE INDEX item_kind ON item(kind)",
    "CREATE INDEX item_sort ON item(sort_title)",
    "CREATE INDEX link_from ON link(ref_id)",
    "CREATE INDEX link_to ON link(target_ref_id)",
    "CREATE INDEX assoc_from ON assoc(ref_id)",
    "CREATE INDEX assoc_to ON assoc(target_ref_id)",
    "CREATE INDEX item_file_ref ON item_file(ref_id)",
    "CREATE INDEX item_file_name ON item_file(file_name)",
    "CREATE INDEX item_child_ref ON item_child(ref_id)",
    "CREATE INDEX alias_ref ON alias(ref_id)",
    "CREATE INDEX alias_sort ON alias(sort_title)",
    "CREATE INDEX search_entry_ref ON search_entry(ref_id)",
    "CREATE INDEX search_word_word ON search_word(word)",
    "CREATE INDEX dict_form_form ON dict_form(form)",
    "CREATE INDEX dict_form_ref ON dict_form(ref_id)",
    "CREATE INDEX timeline_event_ref ON timeline_event(ref_id)",
    "CREATE INDEX mindmaze_answer_q ON mindmaze_answer(question_id)",
    "CREATE INDEX stat_value_ind ON stat_value(indicator_id)",
    "CREATE INDEX place_link_place ON place_link(place_id)",
    "CREATE INDEX place_link_content ON place_link(content_ref_id)",
    "CREATE INDEX lexicon_word_lex ON lexicon_word(lexicon, word)",
];

pub type Row = (&'static str, Vec<Sql>);

/// A cloneable handle that producers use to send row batches to the writer.
#[derive(Clone)]
pub struct Sink {
    tx: SyncSender<Vec<Row>>,
}

impl Sink {
    pub fn send(&self, rows: Vec<Row>) {
        if !rows.is_empty() {
            // The writer only stops early on an error, which `finish` reports.
            let _ = self.tx.send(rows);
        }
    }
}

/// Buffers rows and sends them in batches.
pub struct Buf {
    pub rows: Vec<Row>,
    sink: Sink,
}

impl Buf {
    pub fn new(sink: &Sink) -> Self {
        Buf {
            rows: Vec::new(),
            sink: sink.clone(),
        }
    }

    pub fn push(&mut self, table: &'static str, row: Vec<Sql>) {
        self.rows.push((table, row));
        if self.rows.len() >= 4096 {
            self.flush();
        }
    }

    pub fn flush(&mut self) {
        self.sink.send(std::mem::take(&mut self.rows));
    }
}

impl Drop for Buf {
    fn drop(&mut self) {
        self.flush();
    }
}

pub struct Writer {
    handle: std::thread::JoinHandle<Result<HashMap<&'static str, u64>>>,
    sink: Option<Sink>,
}

fn create(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "PRAGMA journal_mode=OFF; PRAGMA synchronous=OFF; PRAGMA locking_mode=EXCLUSIVE; \
         PRAGMA temp_store=MEMORY; PRAGMA cache_size=-1000000; PRAGMA page_size=8192;",
    )?;
    for (t, cols) in TABLES {
        conn.execute_batch(&format!("CREATE TABLE {t} ({cols});"))?;
    }
    Ok(())
}

fn column_count(cols: &str) -> usize {
    // Split on commas that are not inside parentheses (PRIMARY KEY(a, b)).
    let mut depth = 0;
    let mut n = 1;
    for ch in cols.chars() {
        match ch {
            '(' => depth += 1,
            ')' => depth -= 1,
            ',' if depth == 0 => n += 1,
            _ => {}
        }
    }
    n - cols.matches("PRIMARY KEY(").count()
}

fn writer_loop(conn: Connection, rx: Receiver<Vec<Row>>) -> Result<HashMap<&'static str, u64>> {
    let sql: HashMap<&'static str, String> = TABLES
        .iter()
        .map(|(t, cols)| {
            let n = column_count(cols);
            let qs = vec!["?"; n].join(",");
            (*t, format!("INSERT INTO {t} VALUES ({qs})"))
        })
        .collect();
    let mut counts: HashMap<&'static str, u64> = HashMap::new();
    conn.execute_batch("BEGIN")?;
    for batch in rx {
        for (table, row) in batch {
            let q = sql
                .get(table)
                .with_context(|| format!("no table {table}"))?;
            let mut st = conn.prepare_cached(q)?;
            st.execute(params_from_iter(row.iter()))
                .with_context(|| format!("inserting into {table}: {row:?}"))?;
            *counts.entry(table).or_default() += 1;
        }
    }
    conn.execute_batch("COMMIT")?;
    for ix in INDEXES {
        conn.execute_batch(ix)?;
    }
    conn.execute_batch("ANALYZE")?;
    Ok(counts)
}

impl Writer {
    pub fn start(path: &Path) -> Result<Writer> {
        if path.exists() {
            std::fs::remove_file(path)?;
        }
        let conn = Connection::open(path)?;
        create(&conn)?;
        let (tx, rx) = sync_channel::<Vec<Row>>(256);
        let handle = std::thread::spawn(move || writer_loop(conn, rx));
        Ok(Writer {
            handle,
            sink: Some(Sink { tx }),
        })
    }

    pub fn sink(&self) -> &Sink {
        self.sink.as_ref().unwrap()
    }

    /// Closes the channel, waits for the writer, returns row counts per table.
    pub fn finish(mut self) -> Result<HashMap<&'static str, u64>> {
        drop(self.sink.take());
        self.handle
            .join()
            .map_err(|_| anyhow::anyhow!("writer thread panicked"))?
    }
}

// Small constructors for SQL values.
pub fn i(v: impl Into<i64>) -> Sql {
    Sql::Integer(v.into())
}
pub fn oi<T: Into<i64>>(v: Option<T>) -> Sql {
    v.map_or(Sql::Null, |x| Sql::Integer(x.into()))
}
pub fn s(v: impl Into<String>) -> Sql {
    Sql::Text(v.into())
}
pub fn os(v: Option<impl Into<String>>) -> Sql {
    v.map_or(Sql::Null, |x| Sql::Text(x.into()))
}
/// Empty strings become NULL.
pub fn ne(v: impl Into<String>) -> Sql {
    let v = v.into();
    if v.is_empty() { Sql::Null } else { Sql::Text(v) }
}
pub fn f(v: f64) -> Sql {
    Sql::Real(v)
}
pub fn b(v: bool) -> Sql {
    Sql::Integer(v as i64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn column_counts() {
        assert_eq!(column_count("a, b, PRIMARY KEY(a, b)"), 2);
        assert_eq!(column_count("key TEXT PRIMARY KEY, value TEXT"), 2);
        // next to the test binary (inside the target directory), not in the RAM-backed /tmp
        let dir = std::env::current_exe().unwrap().parent().unwrap().join(format!("enc-db-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("t.sqlite");
        let w = Writer::start(&p).unwrap();
        w.sink().send(vec![("meta", vec![s("k"), s("v")])]);
        let c = w.finish().unwrap();
        assert_eq!(c["meta"], 1);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
