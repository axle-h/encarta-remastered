//! Read-only SQLite access: one connection per blocking-pool thread and database, with prepared
//! statements cached per connection, and a semaphore bounding concurrent queries.

use anyhow::{Context, Result, anyhow};
use rusqlite::{Connection, OpenFlags};
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::Semaphore;

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

thread_local! {
    static CONNS: RefCell<HashMap<u64, Connection>> = RefCell::new(HashMap::new());
}

/// A database opened read-only: the main file plus attached ones (`m` = media manifest).
#[derive(Clone)]
pub struct Db {
    id: u64,
    main: String,
    attach: Vec<(String, String)>,
    sem: Arc<Semaphore>,
}

/// `file:` URI of a path, read-only, optionally `immutable=1` (no locking, no -shm/-wal files:
/// works on read-only volumes; only safe when nothing writes the file).
pub fn uri(path: &Path, immutable: bool) -> String {
    let mut s = String::from("file:");
    for b in path.to_string_lossy().bytes() {
        match b {
            b'%' | b'?' | b'#' | b' ' => s.push_str(&format!("%{b:02X}")),
            _ => s.push(b as char),
        }
    }
    s.push_str("?mode=ro");
    if immutable {
        s.push_str("&immutable=1");
    }
    s
}

fn open_conn(main: &str, attach: &[(String, String)]) -> Result<Connection> {
    let conn = Connection::open_with_flags(
        main,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX | OpenFlags::SQLITE_OPEN_URI,
    )
    .with_context(|| format!("opening {main}"))?;
    conn.busy_timeout(std::time::Duration::from_secs(5))?;
    conn.execute_batch("PRAGMA query_only = 1; PRAGMA cache_size = -16000; PRAGMA mmap_size = 268435456;")?;
    conn.set_prepared_statement_cache_capacity(64);
    for (alias, uri) in attach {
        conn.execute(&format!("ATTACH DATABASE ?1 AS {alias}"), [uri]).with_context(|| format!("attaching {uri}"))?;
    }
    Ok(conn)
}

/// Opens `path` read-only and checks it with `probe`; falls back to `immutable=1` when the
/// normal read-only open fails (read-only volume holding a WAL database).
pub fn probe_uri(path: &Path, probe: &str) -> Result<String> {
    let mut last = None;
    for immutable in [false, true] {
        let u = uri(path, immutable);
        match open_conn(&u, &[]).and_then(|c| c.query_row(probe, [], |_| Ok(())).map_err(Into::into)) {
            Ok(()) => return Ok(u),
            Err(e) => last = Some(e),
        }
    }
    Err(last.unwrap_or_else(|| anyhow!("cannot open {}", path.display())))
}

impl Db {
    pub fn new(main_uri: String, attach: Vec<(String, String)>, max_concurrency: usize) -> Result<Self> {
        open_conn(&main_uri, &attach)?; // fail early
        Ok(Db { id: NEXT_ID.fetch_add(1, Ordering::Relaxed), main: main_uri, attach, sem: Arc::new(Semaphore::new(max_concurrency)) })
    }

    pub fn has(&self, alias: &str) -> bool {
        self.attach.iter().any(|(a, _)| a == alias)
    }

    /// Runs `f` with this thread's connection on the blocking pool.
    pub async fn call<T, F>(&self, f: F) -> Result<T>
    where
        T: Send + 'static,
        F: FnOnce(&Connection) -> Result<T> + Send + 'static,
    {
        let permit = self.sem.clone().acquire_owned().await?;
        let (id, main, attach) = (self.id, self.main.clone(), self.attach.clone());
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            CONNS.with(|cell| {
                let mut map = cell.borrow_mut();
                if let std::collections::hash_map::Entry::Vacant(e) = map.entry(id) {
                    e.insert(open_conn(&main, &attach)?);
                }
                f(&map[&id])
            })
        })
        .await?
    }
}
