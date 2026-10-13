//! Content-addressed store under `out/media/` (docs/media.md §3).
//!
//! - originals: `o/<first two hex digits>/<sha256>.<ext>`
//! - derivatives: `d/<first two hex digits>/<sha256>.<suffix>` (the source's hash, so every
//!   derivative of one source sits next to the others)
//!
//! Files are written to `.tmp/` first and renamed into place, so an existing path is always a
//! complete file. Whether it is the *right* file is checked by hash against the manifest.

use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

pub fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        s.push(DIGITS[(b >> 4) as usize] as char);
        s.push(DIGITS[(b & 15) as usize] as char);
    }
    s
}

pub fn sha256_bytes(b: &[u8]) -> String {
    hex(&Sha256::digest(b))
}

pub fn sha256_file(path: &Path) -> Result<String> {
    let mut f = std::fs::File::open(path).with_context(|| format!("opening {}", path.display()))?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 1 << 16];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(hex(&h.finalize()))
}

pub struct Store {
    pub root: PathBuf,
    tmp: PathBuf,
    counter: AtomicU64,
}

impl Store {
    pub fn open(root: &Path) -> Result<Self> {
        let tmp = root.join(".tmp");
        if tmp.exists() {
            std::fs::remove_dir_all(&tmp).with_context(|| format!("clearing {}", tmp.display()))?;
        }
        std::fs::create_dir_all(&tmp)?;
        Ok(Store { root: root.to_path_buf(), tmp, counter: AtomicU64::new(0) })
    }

    /// Relative path of a stored original.
    pub fn original_rel(sha: &str, ext: &str) -> String {
        format!("o/{}/{sha}.{ext}", &sha[..2])
    }

    /// Relative path of a derivative of the source `sha`.
    pub fn derivative_rel(sha: &str, suffix: &str) -> String {
        format!("d/{}/{sha}.{suffix}", &sha[..2])
    }

    pub fn abs(&self, rel: &str) -> PathBuf {
        self.root.join(rel)
    }

    /// A fresh temporary path (same file system as the store) with the given extension.
    pub fn temp_path(&self, ext: &str) -> PathBuf {
        let n = self.counter.fetch_add(1, Ordering::Relaxed);
        self.tmp.join(format!("{}-{n}.{ext}", std::process::id()))
    }

    /// Moves a finished temporary file to `rel`, creating parent directories.
    pub fn commit(&self, tmp: &Path, rel: &str) -> Result<PathBuf> {
        let dst = self.abs(rel);
        if let Some(parent) = dst.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::rename(tmp, &dst).with_context(|| format!("renaming into {}", dst.display()))?;
        Ok(dst)
    }

    /// Writes bytes to `rel` atomically.
    #[cfg(test)]
    pub fn write(&self, rel: &str, bytes: &[u8]) -> Result<PathBuf> {
        let tmp = self.temp_path("part");
        std::fs::write(&tmp, bytes).with_context(|| format!("writing {}", tmp.display()))?;
        self.commit(&tmp, rel)
    }

    /// True when `rel` exists with this size and hash.
    pub fn verify(&self, rel: &str, size: u64, sha: &str) -> bool {
        let p = self.abs(rel);
        match std::fs::metadata(&p) {
            Ok(m) if m.len() == size => sha256_file(&p).is_ok_and(|h| h == sha),
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_addressing() {
        // SHA-256 of "abc" (FIPS 180-2 test vector).
        let h = sha256_bytes(b"abc");
        assert_eq!(h, "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
        assert_eq!(Store::original_rel(&h, "jpg"), format!("o/ba/{h}.jpg"));
        assert_eq!(Store::derivative_rel(&h, "webp"), format!("d/ba/{h}.webp"));
    }

    #[test]
    fn write_verify_and_skip() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/agent-convert-media/unit-store")
            .join(format!("{}", std::process::id()));
        let store = Store::open(&root).unwrap();
        let data = b"synthetic media bytes";
        let sha = sha256_bytes(data);
        let rel = Store::original_rel(&sha, "bin");
        assert!(!store.verify(&rel, data.len() as u64, &sha));
        let p = store.write(&rel, data).unwrap();
        assert_eq!(sha256_file(&p).unwrap(), sha);
        assert!(store.verify(&rel, data.len() as u64, &sha));
        // A file with the right name but wrong bytes is not accepted.
        std::fs::write(&p, b"synthetic media bytez").unwrap();
        assert!(!store.verify(&rel, data.len() as u64, &sha));
        std::fs::remove_dir_all(&root).unwrap();
    }
}
