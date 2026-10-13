//! `out/media/manifest.sqlite` (schema in docs/media.md §3).
//!
//! `derivative` persists across runs and is what makes the converter resumable: a row is
//! written only after its file has been renamed into place, and a later run trusts a row only if
//! the recipe is unchanged and the file on disk still has the recorded size and hash.
//! The other tables describe the disc and are rebuilt on every run.

use anyhow::Result;
use rusqlite::{Connection, params};
use std::collections::HashMap;
use std::path::Path;

pub const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS file(
    container   TEXT NOT NULL,
    package     TEXT NOT NULL,
    member_path TEXT NOT NULL,
    name        TEXT NOT NULL,
    sha256      TEXT NOT NULL,
    size        INTEGER NOT NULL,
    mime        TEXT NOT NULL,
    kind        TEXT NOT NULL,
    format      TEXT NOT NULL,
    disposition TEXT NOT NULL,
    PRIMARY KEY(container, member_path)
);
CREATE INDEX IF NOT EXISTS file_name ON file(name);
CREATE INDEX IF NOT EXISTS file_sha ON file(sha256);
CREATE TABLE IF NOT EXISTS derivative(
    sha256      TEXT NOT NULL,
    format      TEXT NOT NULL,
    path        TEXT NOT NULL,
    width       INTEGER,
    height      INTEGER,
    duration_ms INTEGER,
    bytes       INTEGER NOT NULL,
    out_sha256  TEXT NOT NULL,
    recipe      TEXT NOT NULL,
    meta        TEXT,
    PRIMARY KEY(sha256, format)
);
CREATE TABLE IF NOT EXISTS catalog_key(
    edition       TEXT NOT NULL,
    tbl           TEXT NOT NULL,
    key           TEXT NOT NULL,
    package_index INTEGER NOT NULL,
    package_file  TEXT,
    local_id      INTEGER NOT NULL,
    container     TEXT,
    member_path   TEXT,
    sha256        TEXT,
    PRIMARY KEY(edition, tbl, key)
);
CREATE INDEX IF NOT EXISTS catalog_key_sha ON catalog_key(sha256);
CREATE TABLE IF NOT EXISTS iax_ref(
    iax_sha256  TEXT NOT NULL,
    element     TEXT NOT NULL,
    ref         TEXT NOT NULL,
    resolution  TEXT NOT NULL,
    container   TEXT,
    member_path TEXT,
    sha256      TEXT
);
CREATE INDEX IF NOT EXISTS iax_ref_iax ON iax_ref(iax_sha256);
CREATE TABLE IF NOT EXISTS member(
    container   TEXT NOT NULL,
    member_path TEXT NOT NULL,
    size        INTEGER NOT NULL,
    format      TEXT,
    status      TEXT NOT NULL,
    owner       TEXT,
    reason      TEXT,
    sha256      TEXT,
    PRIMARY KEY(container, member_path)
);
CREATE TABLE IF NOT EXISTS failure(
    sha256 TEXT NOT NULL,
    format TEXT NOT NULL,
    error  TEXT NOT NULL,
    PRIMARY KEY(sha256, format)
);
CREATE TABLE IF NOT EXISTS run(key TEXT PRIMARY KEY, value TEXT);
";

#[derive(Debug, Clone, PartialEq)]
pub struct DerivRow {
    pub sha256: String,
    pub format: String,
    pub path: String,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub duration_ms: Option<u64>,
    pub bytes: u64,
    pub out_sha256: String,
    pub recipe: String,
    pub meta: Option<String>,
}

pub fn open(path: &Path) -> Result<Connection> {
    let db = Connection::open(path)?;
    db.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;")?;
    db.execute_batch(SCHEMA)?;
    Ok(db)
}

/// All existing derivative rows, keyed by (source sha, format).
pub fn load_derivatives(db: &Connection) -> Result<HashMap<(String, String), DerivRow>> {
    let mut st = db.prepare(
        "SELECT sha256, format, path, width, height, duration_ms, bytes, out_sha256, recipe, meta FROM derivative",
    )?;
    let rows = st.query_map([], |r| {
        Ok(DerivRow {
            sha256: r.get(0)?,
            format: r.get(1)?,
            path: r.get(2)?,
            width: r.get(3)?,
            height: r.get(4)?,
            duration_ms: r.get::<_, Option<i64>>(5)?.map(|v| v as u64),
            bytes: r.get::<_, i64>(6)? as u64,
            out_sha256: r.get(7)?,
            recipe: r.get(8)?,
            meta: r.get(9)?,
        })
    })?;
    let mut out = HashMap::new();
    for row in rows {
        let row = row?;
        out.insert((row.sha256.clone(), row.format.clone()), row);
    }
    Ok(out)
}

pub fn put_derivative(db: &Connection, d: &DerivRow) -> Result<()> {
    db.prepare_cached(
        "INSERT OR REPLACE INTO derivative(sha256, format, path, width, height, duration_ms, bytes, out_sha256, recipe, meta)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
    )?
    .execute(params![
        d.sha256,
        d.format,
        d.path,
        d.width,
        d.height,
        d.duration_ms.map(|v| v as i64),
        d.bytes as i64,
        d.out_sha256,
        d.recipe,
        d.meta
    ])?;
    Ok(())
}

pub fn put_failure(db: &Connection, sha: &str, format: &str, error: &str) -> Result<()> {
    db.prepare_cached("INSERT OR REPLACE INTO failure(sha256, format, error) VALUES (?1, ?2, ?3)")?
        .execute(params![sha, format, error])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_round_trip() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(SCHEMA).unwrap();
        let row = DerivRow {
            sha256: "ab".repeat(32),
            format: "webp".into(),
            path: "d/ab/x.webp".into(),
            width: Some(4),
            height: Some(3),
            duration_ms: None,
            bytes: 10,
            out_sha256: "cd".repeat(32),
            recipe: "r1".into(),
            meta: Some("{}".into()),
        };
        put_derivative(&db, &row).unwrap();
        put_derivative(&db, &row).unwrap();
        let all = load_derivatives(&db).unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[&(row.sha256.clone(), "webp".to_string())], row);
    }
}
