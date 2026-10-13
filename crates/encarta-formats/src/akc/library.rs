//! Content id → package → record: the catalog (`search-catalog-misc.md` §3) joined to AKC files.

use super::AkcFile;
use crate::catalog::{Catalog, Edition, Resolved, Table, find_file_ci};
use anyhow::{Context, Result, bail};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// One edition's catalog plus lazily opened AKC packages.
pub struct Library {
    catalog: Catalog,
    dirs: Vec<PathBuf>,
    open: Mutex<HashMap<u8, Arc<AkcFile>>>,
}

/// A decoded record and where it came from.
#[derive(Debug, Clone)]
pub struct Located {
    pub table: Table,
    pub package: u8,
    pub file: String,
    pub local_id: u32,
    pub bytes: Vec<u8>,
}

impl Library {
    /// Opens the Deluxe (`CATALOG.DLX`) or Kids (`CATALOG.KDC`) catalog under an extracted tree
    /// (`<extracted>/iso`, `<extracted>/eit`).
    pub fn open(extracted: &Path, edition: Edition) -> Result<Library> {
        let p = edition.paths(extracted);
        let catalog = Catalog::open(&p.catalog2, &p.ini)?;
        Ok(Self::new(catalog, p.package_dirs))
    }

    /// Uses `catalog`, finding package files in `dirs` (case-insensitively).
    pub fn new(catalog: Catalog, dirs: Vec<PathBuf>) -> Library {
        Library {
            catalog,
            dirs,
            open: Mutex::new(HashMap::new()),
        }
    }

    pub fn catalog(&self) -> &Catalog {
        &self.catalog
    }

    /// The AKC file for package slot `package`, opened once and shared.
    pub fn package(&self, package: u8) -> Result<Arc<AkcFile>> {
        if let Some(f) = self.open.lock().unwrap().get(&package) {
            return Ok(f.clone());
        }
        let name = self
            .catalog
            .package_file(package)
            .with_context(|| format!("package slot {package} has no file"))?;
        if !name.to_ascii_lowercase().ends_with(".akc") {
            bail!("package {package} ({name}) is not an AKC file");
        }
        let path = find_file_ci(&self.dirs, name)
            .with_context(|| format!("{name} not found in {:?}", self.dirs))?;
        let f = Arc::new(AkcFile::open(&path)?);
        Ok(self
            .open
            .lock()
            .unwrap()
            .entry(package)
            .or_insert(f)
            .clone())
    }

    /// Where `ref_id` lives in a numeric table (content, data or relart).
    pub fn locate(&self, ref_id: u32, table: Table) -> Option<Resolved> {
        self.catalog
            .resolve_id(ref_id)
            .into_iter()
            .find(|r| r.table == table)
    }

    fn fetch(&self, r: Resolved) -> Result<Located> {
        let f = self.package(r.package)?;
        let bytes = f.get(r.local_id)?.with_context(|| {
            format!(
                "catalog points at record {} of {:?}, which does not exist",
                r.local_id, r.file
            )
        })?;
        Ok(Located {
            table: r.table,
            package: r.package,
            file: r.file.unwrap_or_default(),
            local_id: r.local_id,
            bytes,
        })
    }

    /// Decodes the record of `ref_id` in `table` (`None` if the catalog has no such entry).
    pub fn get(&self, ref_id: u32, table: Table) -> Result<Option<Located>> {
        self.locate(ref_id, table)
            .map(|r| self.fetch(r))
            .transpose()
    }

    /// Article body XML (`content.ecn` → `Cont*.akc`).
    pub fn content(&self, ref_id: u32) -> Result<Option<Located>> {
        self.get(ref_id, Table::Content)
    }

    /// Item metadata XML (`data.ecn` → `Data*.akc`).
    pub fn data(&self, ref_id: u32) -> Result<Option<Located>> {
        self.get(ref_id, Table::Data)
    }

    /// Related-articles XML (`relart.ecn` → `Rel*.akc`).
    pub fn related(&self, ref_id: u32) -> Result<Option<Located>> {
        self.get(ref_id, Table::Relart)
    }

    /// A string-keyed AKC record (`generic.ecs`, e.g. `list/expert.xml` → DataESK).
    pub fn get_key(&self, key: &str) -> Result<Option<Located>> {
        let r = self.catalog.resolve_key(key).into_iter().find(|r| {
            r.file
                .as_deref()
                .is_some_and(|f| f.to_ascii_lowercase().ends_with(".akc"))
        });
        r.map(|r| self.fetch(r)).transpose()
    }
}
