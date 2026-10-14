//! Compatibility rewrites for disc SWFs that misbehave under Ruffle (docs/games.md, "Picture answers").
//!
//! The IAF quiz skins (`IAFPlayer/skins/*.swf`) load an answer picture into an empty clip with
//! `loadMovie` and then give that clip an `onLoad` handler that sets `_width`/`_height` to the
//! size named in the question. Flash Player never runs that handler; Ruffle runs it while the clip
//! is still empty, which scales it to zero and leaves the answer buttons blank. The handler only
//! ever restores the picture's own size (the `w`/`h` of all 120 picture answers on the disc match
//! their JPEGs), so the rewrite renames the property in the script's constant pool, and the
//! handler is never found.
//!
//! The match is narrow: an AVM1 `ConstantPool` in a `DoAction`/`DoInitAction` tag in which
//! `onLoad` directly follows `loadMovie` (a pool lists strings in order of first use, so this is
//! `clip.loadMovie(…); clip.onLoad = …`). Only files under the skins folder are examined.

use flate2::Compression;
use flate2::read::ZlibDecoder;
use flate2::write::ZlibEncoder;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::sync::{Arc, Mutex, OnceLock};

/// Folder (lower case, under `dswMedia/`) whose SWFs are examined.
const SKINS: &str = "shared/players/iafplayer/skins/";
/// Same length as `onLoad`, so no length field changes.
const RENAMED: &[u8] = b"noLoad";

const TAG_DO_ACTION: u16 = 12;
const TAG_DO_INIT_ACTION: u16 = 59;
const ACTION_CONSTANT_POOL: u8 = 0x88;

/// Whether a `dswMedia/…` path (any case) is a candidate for [`patch_swf`].
pub fn applies(disc_path: &str) -> bool {
    let p = disc_path.trim_start_matches('/').replace('\\', "/").to_ascii_lowercase();
    let p = p.strip_prefix("dswmedia/").unwrap_or(&p);
    p.starts_with(SKINS) && p.ends_with(".swf")
}

/// The rewritten SWF, or `None` when nothing matched (or the file is not a readable SWF).
pub fn patch_swf(swf: &[u8]) -> Option<Vec<u8>> {
    if swf.len() < 8 {
        return None;
    }
    let (sig, version) = (&swf[..3], swf[3]);
    let mut body = match sig {
        b"FWS" => swf[8..].to_vec(),
        b"CWS" => {
            let mut v = Vec::new();
            ZlibDecoder::new(&swf[8..]).read_to_end(&mut v).ok()?;
            v
        }
        _ => return None,
    };
    if patch_body(&mut body) == 0 {
        return None;
    }
    let total = u32::try_from(8 + body.len()).ok()?;
    let mut out = Vec::with_capacity(swf.len());
    out.extend_from_slice(if sig == b"CWS" { b"CWS" } else { b"FWS" });
    out.push(version);
    out.extend_from_slice(&total.to_le_bytes());
    if sig == b"CWS" {
        let mut z = ZlibEncoder::new(out, Compression::default());
        z.write_all(&body).ok()?;
        z.finish().ok()
    } else {
        out.extend_from_slice(&body);
        Some(out)
    }
}

/// Patches the uncompressed body (after the 8-byte header) in place; returns the number of renames.
fn patch_body(body: &mut [u8]) -> usize {
    // Frame rect (5-bit field width, four fields, byte aligned), then frame rate and count.
    let Some(&first) = body.first() else { return 0 };
    let nbits = (first >> 3) as usize;
    let mut p = (5 + 4 * nbits).div_ceil(8) + 4;
    let mut renamed = 0;
    while p + 2 <= body.len() {
        let head = u16::from_le_bytes([body[p], body[p + 1]]);
        p += 2;
        let (code, mut len) = (head >> 6, (head & 0x3f) as usize);
        if len == 0x3f {
            let Some(b) = body.get(p..p + 4) else { break };
            len = u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize;
            p += 4;
        }
        let Some(end) = p.checked_add(len).filter(|&e| e <= body.len()) else { break };
        let actions = match code {
            TAG_DO_ACTION => p,
            TAG_DO_INIT_ACTION => p + 2, // sprite id
            0 => break,
            _ => end,
        };
        if actions < end {
            renamed += patch_actions(&mut body[actions..end]);
        }
        p = end;
    }
    renamed
}

/// Walks one action record list (function bodies are inline, so a linear walk sees every pool).
fn patch_actions(a: &mut [u8]) -> usize {
    let mut p = 0;
    let mut renamed = 0;
    while p < a.len() {
        let code = a[p];
        p += 1;
        if code == 0 {
            break;
        }
        if code < 0x80 {
            continue;
        }
        let Some(b) = a.get(p..p + 2) else { break };
        let len = u16::from_le_bytes([b[0], b[1]]) as usize;
        p += 2;
        let Some(end) = p.checked_add(len).filter(|&e| e <= a.len()) else { break };
        if code == ACTION_CONSTANT_POOL {
            renamed += patch_pool(&mut a[p..end]);
        }
        p = end;
    }
    renamed
}

/// A pool: u16 count, then NUL-terminated strings.
fn patch_pool(pool: &mut [u8]) -> usize {
    if pool.len() < 2 {
        return 0;
    }
    let count = u16::from_le_bytes([pool[0], pool[1]]) as usize;
    let mut spans = Vec::with_capacity(count);
    let mut p = 2;
    for _ in 0..count {
        let Some(n) = pool[p..].iter().position(|&b| b == 0) else { break };
        spans.push((p, p + n));
        p += n + 1;
    }
    let mut renamed = 0;
    for w in spans.windows(2) {
        let ((a0, a1), (b0, b1)) = (w[0], w[1]);
        if &pool[a0..a1] == b"loadMovie" && &pool[b0..b1] == b"onLoad" {
            pool[b0..b1].copy_from_slice(RENAMED);
            renamed += 1;
        }
    }
    renamed
}

/// Patched bytes by content hash; `None` records "nothing to patch".
type Cache = Mutex<HashMap<String, Option<Arc<Vec<u8>>>>>;

/// [`patch_swf`] memoised by content hash (the skins are requested once per game).
pub fn patched_cached(sha256: &str, read: impl FnOnce() -> Option<Vec<u8>>) -> Option<Arc<Vec<u8>>> {
    static CACHE: OnceLock<Cache> = OnceLock::new();
    let cache = CACHE.get_or_init(Default::default);
    if let Some(hit) = cache.lock().ok()?.get(sha256) {
        return hit.clone();
    }
    let v = read().and_then(|b| patch_swf(&b)).map(Arc::new);
    cache.lock().ok()?.insert(sha256.to_string(), v.clone());
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A synthetic SWF: empty 1-bit rect, one DoInitAction holding a constant pool, End.
    fn swf(pool: &[&str], compressed: bool) -> Vec<u8> {
        let mut cp = (pool.len() as u16).to_le_bytes().to_vec();
        for s in pool {
            cp.extend_from_slice(s.as_bytes());
            cp.push(0);
        }
        let mut actions = vec![0x07]; // a one-byte action before the pool
        actions.push(ACTION_CONSTANT_POOL);
        actions.extend_from_slice(&(cp.len() as u16).to_le_bytes());
        actions.extend_from_slice(&cp);
        actions.push(0);
        let mut tag = 1u16.to_le_bytes().to_vec(); // sprite id
        tag.extend_from_slice(&actions);
        let mut body = vec![0x08, 0x00, 0, 12, 1, 0]; // rect with 1-bit fields (2 bytes), rate, count
        body.extend_from_slice(&((TAG_DO_INIT_ACTION << 6) | 0x3f).to_le_bytes());
        body.extend_from_slice(&(tag.len() as u32).to_le_bytes());
        body.extend_from_slice(&tag);
        body.extend_from_slice(&[0, 0]);
        let mut out = if compressed { b"CWS".to_vec() } else { b"FWS".to_vec() };
        out.push(6);
        out.extend_from_slice(&((8 + body.len()) as u32).to_le_bytes());
        if compressed {
            let mut z = ZlibEncoder::new(out, Compression::default());
            z.write_all(&body).unwrap();
            z.finish().unwrap()
        } else {
            out.extend_from_slice(&body);
            out
        }
    }

    fn body(swf: &[u8]) -> Vec<u8> {
        if &swf[..3] == b"CWS" {
            let mut v = Vec::new();
            ZlibDecoder::new(&swf[8..]).read_to_end(&mut v).unwrap();
            v
        } else {
            swf[8..].to_vec()
        }
    }

    #[test]
    fn renames_onload_after_loadmovie() {
        for compressed in [false, true] {
            let src = swf(&["clip", "loadMovie", "onLoad", "_width"], compressed);
            let out = patch_swf(&src).expect("patched");
            assert_eq!(&out[..3], &src[..3]);
            let (a, b) = (body(&src), body(&out));
            assert_eq!(a.len(), b.len());
            assert_eq!(u32::from_le_bytes(out[4..8].try_into().unwrap()) as usize, 8 + b.len());
            let want = String::from_utf8_lossy(&a).replace("onLoad", "noLoad");
            assert_eq!(String::from_utf8_lossy(&b), want);
        }
    }

    #[test]
    fn leaves_other_onload_alone() {
        assert!(patch_swf(&swf(&["xml", "onLoad", "loadMovie"], true)).is_none());
        assert!(patch_swf(&swf(&["loadMovie", "x", "onLoad"], false)).is_none());
        assert!(patch_swf(b"not a swf").is_none());
    }

    #[test]
    fn applies_only_to_skins() {
        assert!(applies("dswMedia/shared/players/IAFPlayer/skins/BGA_Standard.swf"));
        assert!(applies("DSWMEDIA/Shared/Players/IAFPlayer/Skins/x.SWF"));
        assert!(!applies("dswMedia/shared/players/IAFPlayer/base.swf"));
        assert!(!applies("dswMedia/shared/players/IAFPlayer/skins/x.jpg"));
    }
}
