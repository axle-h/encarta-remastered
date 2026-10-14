//! Media: `/media/file/{name}` (disc names, as in the pre-rendered HTML) and
//! `/media/{sha256}/{variant}` (content-addressed outputs of the media converter), both with
//! Range support through `tower_http::services::ServeFile`.

use crate::AppState;
use crate::error::{ApiError, ApiResult};
use anyhow::Result;
use axum::extract::{Path as UrlPath, Query, Request, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use encarta_formats::catalog::{self, Catalog, Edition, Table};
use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub const IMMUTABLE: &str = "public, max-age=31536000, immutable";

/// MIME type from a file extension (outputs and well-known formats).
pub fn mime_for_ext(ext: &str) -> Option<&'static str> {
    Some(match ext.to_ascii_lowercase().as_str() {
        "jpg" | "jpeg" | "jtn" | "jsm" | "svj" => "image/jpeg",
        "gif" | "gtn" | "gmo" => "image/gif",
        "png" => "image/png",
        "webp" => "image/webp",
        "avif" => "image/avif",
        "bmp" | "dib" | "dtn" | "dfg" => "image/bmp",
        "cur" => "image/x-icon",
        "svg" => "image/svg+xml",
        "wav" => "audio/wav",
        "mp3" => "audio/mpeg",
        "opus" | "ogg" => "audio/ogg",
        "m4a" => "audio/mp4",
        "flac" => "audio/flac",
        "wma" => "audio/x-ms-wma",
        "mid" | "midi" => "audio/midi",
        "swa" => "audio/x-swa",
        "asf" | "wmv" => "video/x-ms-asf",
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        "vtt" => "text/vtt; charset=utf-8",
        "smi" => "application/smil+xml",
        "swf" => "application/x-shockwave-flash",
        "dcr" | "cct" => "application/x-director",
        "xml" | "iax" => "application/xml",
        "json" => "application/json",
        "html" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "txt" => "text/plain; charset=utf-8",
        "glb" => "model/gltf-binary",
        "gltf" => "model/gltf+json",
        "pmtiles" | "bin" => "application/octet-stream",
        "pbf" | "mvt" => "application/x-protobuf",
        "woff2" => "font/woff2",
        "woff" => "font/woff",
        "ttf" => "font/ttf",
        "wasm" => "application/wasm",
        "sqlite" => "application/vnd.sqlite3",
        _ => return None,
    })
}

/// MIME type from the leading bytes (disc files have misleading extensions).
pub fn sniff(b: &[u8]) -> Option<&'static str> {
    let starts = |p: &[u8]| b.starts_with(p);
    Some(if starts(&[0xFF, 0xD8, 0xFF]) {
        "image/jpeg"
    } else if starts(b"GIF8") {
        "image/gif"
    } else if starts(b"\x89PNG") {
        "image/png"
    } else if starts(b"BM") {
        "image/bmp"
    } else if b.len() >= 12 && starts(b"RIFF") && &b[8..12] == b"WAVE" {
        "audio/wav"
    } else if starts(&[0x30, 0x26, 0xB2, 0x75]) {
        "video/x-ms-asf"
    } else if starts(b"ID3") || (b.len() > 1 && b[0] == 0xFF && b[1] & 0xE0 == 0xE0) {
        "audio/mpeg"
    } else if starts(b"FWS") || starts(b"CWS") || starts(b"ZWS") {
        "application/x-shockwave-flash"
    } else if starts(b"MThd") {
        "audio/midi"
    } else if starts(b"XFIR") || starts(b"RIFX") {
        "application/x-director"
    } else if starts(&[0, 0, 2, 0]) || starts(&[0, 0, 1, 0]) {
        "image/x-icon"
    } else if starts(b"<SAMI") || starts(b"<sami") {
        "application/smil+xml"
    } else if starts(b"<?xml") || starts(b"\xEF\xBB\xBF<") || starts(b"<") {
        "application/xml"
    } else {
        return None;
    })
}

#[derive(Debug, Clone, Serialize)]
pub struct Variant {
    pub url: String,
    pub mime: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<i64>,
    pub bytes: i64,
    #[serde(skip)]
    pub path: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct MediaInfo {
    pub sha256: String,
    pub kind: Option<String>,
    pub format: Option<String>,
    pub mime: Option<String>,
    pub size: Option<i64>,
    pub variants: BTreeMap<String, Variant>,
}

fn ext_of(p: &str) -> &str {
    p.rsplit('/').next().unwrap_or(p).rsplit('.').next().unwrap_or("")
}

/// Last path segment, lower-case (`loose_media/dsv/T027282A.WMV` → `t027282a.wmv`).
pub fn disc_name(name: &str) -> String {
    name.rsplit(['/', '\\']).next().unwrap_or(name).to_ascii_lowercase()
}

/// sha256 of a disc file name via the manifest (attached as `m`): catalog keys first, then file names.
pub fn resolve_sha(conn: &Connection, name: &str, edition: &str) -> rusqlite::Result<Option<String>> {
    let key = disc_name(name);
    let sha = conn
        .prepare_cached(
            "SELECT sha256 FROM m.catalog_key WHERE edition IN ('deluxe','kids') AND tbl IN ('baggage','generic')
             AND key = ?1 AND sha256 IS NOT NULL ORDER BY (edition = ?2) DESC LIMIT 1",
        )?
        .query_row(rusqlite::params![key, edition], |r| r.get(0))
        .optional()?;
    if sha.is_some() {
        return Ok(sha);
    }
    let sha = conn
        .prepare_cached(
            "SELECT sha256 FROM m.file WHERE name = ?1 ORDER BY ((container LIKE 'EE/KIDS/%') = (?2 = 'kids')) DESC LIMIT 1",
        )?
        .query_row(rusqlite::params![key, edition], |r| r.get(0))
        .optional()?;
    if sha.is_some() {
        return Ok(sha);
    }
    // The atlas's photos and sounds under their MSWORLD.ITR catalog names, found on the disc
    // under other names (media.md §7; table written by `encarta-convert atlas-media`).
    let has: bool = conn
        .prepare_cached("SELECT EXISTS(SELECT 1 FROM m.sqlite_master WHERE type = 'table' AND name = 'atlas_file')")?
        .query_row([], |r| r.get(0))?;
    if !has {
        return Ok(None);
    }
    conn.prepare_cached("SELECT sha256 FROM m.atlas_file WHERE name = ?1 AND sha256 IS NOT NULL")?.query_row([key], |r| r.get(0)).optional()
}

pub fn media_info(conn: &Connection, sha: &str) -> rusqlite::Result<Option<MediaInfo>> {
    let mut variants = BTreeMap::new();
    {
        let mut st = conn.prepare_cached(
            "SELECT format, path, width, height, duration_ms, bytes FROM m.derivative WHERE sha256 = ?1",
        )?;
        let mut rows = st.query([sha])?;
        while let Some(r) = rows.next()? {
            let format: String = r.get(0)?;
            let path: String = r.get(1)?;
            variants.insert(
                format.clone(),
                Variant {
                    url: format!("/media/{sha}/{format}"),
                    mime: mime_for_ext(ext_of(&path)).unwrap_or("application/octet-stream"),
                    width: r.get(2)?,
                    height: r.get(3)?,
                    duration_ms: r.get(4)?,
                    bytes: r.get(5)?,
                    path,
                },
            );
        }
    }
    if variants.is_empty() {
        return Ok(None);
    }
    type FileRow = (Option<String>, Option<String>, Option<String>, Option<i64>);
    let file: Option<FileRow> = conn
        .prepare_cached("SELECT kind, format, mime, size FROM m.file WHERE sha256 = ?1 LIMIT 1")?
        .query_row([sha], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
        .optional()?;
    let (kind, format, mime, size) = file.unwrap_or((None, None, None, None));
    // The manifest's original mime wins over the extension guess.
    if let (Some(m), Some(v)) = (&mime, variants.get_mut("original"))
        && let Some(known) = mime_for_known(m) {
            v.mime = known;
        }
    Ok(Some(MediaInfo { sha256: sha.to_string(), kind, format, mime, size, variants }))
}

fn mime_for_known(m: &str) -> Option<&'static str> {
    [
        "image/jpeg", "image/gif", "image/png", "image/bmp", "audio/wav", "audio/mpeg", "video/x-ms-asf",
        "application/x-shockwave-flash", "audio/midi", "application/xml", "image/x-icon",
    ]
    .into_iter()
    .find(|k| *k == m)
}

/// Media info of a disc file name (None without a manifest or for unknown names).
pub fn info_for_name(conn: &Connection, has_manifest: bool, name: &str, edition: &str) -> rusqlite::Result<Option<MediaInfo>> {
    if !has_manifest {
        return Ok(None);
    }
    match resolve_sha(conn, name, edition)? {
        Some(sha) => media_info(conn, &sha),
        None => Ok(None),
    }
}

/// Fallback without a manifest: the original bytes in the unpacked disc, located through the
/// catalogs (`baggage.ecs` / `generic.ecs`).
pub struct ExtractedFallback {
    catalogs: Vec<(&'static str, Catalog, Vec<PathBuf>)>,
    eit: PathBuf,
    dirs: std::sync::Mutex<HashMap<(usize, u8), Option<PathBuf>>>,
}

impl ExtractedFallback {
    pub fn open(extracted: &Path) -> Result<Self> {
        let mut catalogs = Vec::new();
        let iso = extracted.join("iso");
        let eit = extracted.join("eit");
        for (name, ed) in [("deluxe", Edition::Deluxe), ("kids", Edition::Kids)] {
            let p = ed.paths(extracted);
            if let Ok(cat) = Catalog::open(&p.catalog2, &p.ini) {
                let dirs = p.package_dirs.iter().filter_map(|d| d.strip_prefix(&iso).ok()).map(|r| eit.join(r)).collect();
                catalogs.push((name, cat, dirs));
            }
        }
        anyhow::ensure!(!catalogs.is_empty(), "no catalog under {}", extracted.display());
        Ok(ExtractedFallback { catalogs, eit, dirs: Default::default() })
    }

    /// Path of the original file for `name` (preferring `edition`).
    pub fn find(&self, name: &str, edition: &str) -> Option<PathBuf> {
        let key = disc_name(name);
        let mut order: Vec<usize> = (0..self.catalogs.len()).collect();
        order.sort_by_key(|&i| self.catalogs[i].0 != edition);
        for i in order {
            let (_, cat, dirs) = &self.catalogs[i];
            for r in cat.resolve_key(&key) {
                let dir = {
                    let mut cache = self.dirs.lock().unwrap();
                    cache
                        .entry((i, r.package))
                        .or_insert_with(|| r.file.as_deref().and_then(|f| catalog::find_file_ci(dirs, f)).filter(|p| p.is_dir()))
                        .clone()
                };
                let Some(dir) = dir else { continue };
                let member = dir.join(catalog::member_path(r.table, &key));
                if member.is_file() {
                    return Some(member);
                }
                // Member names keep the disc's case in a few containers.
                let sub = if r.table == Table::Baggage { dir.join("baggage") } else { dir.clone() };
                if let Some(p) = catalog::find_file_ci(&[sub], &key).filter(|p| p.is_file()) {
                    return Some(p);
                }
            }
        }
        let _ = &self.eit;
        None
    }
}

#[derive(Deserialize, Default)]
pub struct FileQuery {
    pub edition: Option<String>,
    pub original: Option<String>,
}

fn accepts(headers: &HeaderMap, mime: &str) -> bool {
    headers.get(header::ACCEPT).and_then(|v| v.to_str().ok()).is_some_and(|a| a.split(',').any(|p| p.trim().starts_with(mime)))
}

/// Serves `path` with Range/HEAD/conditional support and long-lived caching.
async fn serve_path(req: Request, path: PathBuf, mime: &str, etag: Option<String>, vary_accept: bool) -> ApiResult<Response> {
    let req = crate::files::Req::from(req);
    let r = crate::files::serve_file(&req.method, &req.headers, &path, crate::files::FileOpts {
        mime,
        cache_control: IMMUTABLE,
        etag,
        vary_accept,
    })
    .await;
    if r.status() == StatusCode::NOT_FOUND {
        return Err(ApiError::not_found("media file missing on disk"));
    }
    Ok(r)
}

/// `GET /media/file/{*name}`
pub async fn file(
    State(st): State<Arc<AppState>>,
    UrlPath(name): UrlPath<String>,
    Query(q): Query<FileQuery>,
    req: Request,
) -> ApiResult<Response> {
    let edition = q.edition.clone().unwrap_or_else(|| "deluxe".into());
    if let Some(db) = &st.media {
        let (n, e) = (name.clone(), edition.clone());
        let info = db.call(move |c| Ok(match resolve_sha(c, &n, &e)? { Some(sha) => media_info(c, &sha)?, None => None })).await?;
        if let Some(info) = info {
            let Some(orig) = info.variants.get("original") else {
                return Err(ApiError::not_found("no original"));
            };
            let mut pick = orig;
            let mut vary = false;
            let image = orig.mime.starts_with("image/") && !matches!(orig.mime, "image/x-icon");
            if image && q.original.is_none() {
                vary = true;
                for (fmt, mime) in [("avif", "image/avif"), ("webp", "image/webp")] {
                    if let Some(v) = info.variants.get(fmt)
                        && accepts(req.headers(), mime) && v.bytes < pick.bytes && v.width == orig.width {
                            pick = v;
                        }
                }
            }
            let fmt = info.variants.iter().find(|(_, v)| std::ptr::eq(*v, pick)).map(|(k, _)| k.clone()).unwrap_or_default();
            let path = st.out.join("media").join(&pick.path);
            let etag = format!("\"{}-{fmt}\"", info.sha256);
            return serve_path(req, path, pick.mime, Some(etag), vary).await;
        }
    }
    if let Some(fb) = st.extracted.clone() {
        let (n, e) = (name.clone(), edition);
        let found = tokio::task::spawn_blocking(move || {
            let p = fb.find(&n, &e)?;
            let mut head = [0u8; 16];
            let len = std::fs::File::open(&p).and_then(|mut f| f.read(&mut head)).unwrap_or(0);
            let mime = sniff(&head[..len]).or_else(|| mime_for_ext(ext_of(&n))).unwrap_or("application/octet-stream");
            Some((p, mime))
        })
        .await
        .map_err(|e| ApiError::from(anyhow::Error::from(e)))?;
        if let Some((p, mime)) = found {
            return serve_path(req, p, mime, None, false).await;
        }
    }
    Err(ApiError::not_found(format!("no media file {name}")))
}

/// `GET /media/{sha256}/{variant}`
pub async fn by_hash(State(st): State<Arc<AppState>>, UrlPath((sha, variant)): UrlPath<(String, String)>, req: Request) -> ApiResult<Response> {
    if sha.len() != 64 || !sha.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(ApiError::not_found("not a sha256"));
    }
    let Some(db) = &st.media else {
        return Err(ApiError::not_found("no media manifest"));
    };
    let s = sha.to_ascii_lowercase();
    let v = variant.clone();
    let row: Option<String> = db
        .call(move |c| {
            Ok(c.prepare_cached("SELECT path FROM m.derivative WHERE sha256 = ?1 AND format = ?2")?
                .query_row([&s, &v], |r| r.get(0))
                .optional()?)
        })
        .await?;
    let Some(path) = row else {
        return Err(ApiError::not_found(format!("no {variant} for {sha}")));
    };
    let mime = mime_for_ext(ext_of(&path)).unwrap_or("application/octet-stream");
    serve_path(req, st.out.join("media").join(&path), mime, Some(format!("\"{sha}-{variant}\"")), false).await
}

/// The disc's `dswMedia/` folders by path (Flash interactives load their player, skins and media
/// by relative URL): lower-case member path → sha256, per edition (Kids containers are `EE/KIDS/…`).
pub struct DiscTree {
    paths: HashMap<(bool, String), String>,
}

impl DiscTree {
    pub const PREFIX: &'static str = "dswmedia/";

    pub fn load(manifest_uri: &str) -> rusqlite::Result<Self> {
        let c = Connection::open_with_flags(manifest_uri, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_URI)?;
        let mut st = c.prepare("SELECT container, member_path, sha256 FROM file WHERE member_path LIKE 'dswMedia/%'")?;
        let mut paths = HashMap::new();
        let mut rows = st.query([])?;
        while let Some(r) = rows.next()? {
            let container: String = r.get(0)?;
            let path: String = r.get(1)?;
            paths.entry((container.starts_with("EE/KIDS/"), path.to_ascii_lowercase())).or_insert(r.get(2)?);
        }
        Ok(DiscTree { paths })
    }

    pub fn len(&self) -> usize {
        self.paths.len()
    }

    pub fn is_empty(&self) -> bool {
        self.paths.is_empty()
    }

    /// sha256 of `dswMedia/…` (any case), preferring the edition's copy.
    pub fn find(&self, path: &str, kids: bool) -> Option<&str> {
        let p = path.trim_start_matches('/').replace('\\', "/").to_ascii_lowercase();
        [kids, !kids].into_iter().find_map(|k| self.paths.get(&(k, p.clone()))).map(String::as_str)
    }
}

/// `GET /media/disc/{edition}/{*path}`: a file of the disc's `dswMedia/` tree by path, so that a
/// Flash player loads its siblings with relative URLs. An `.iax` that is not in the tree (they
/// live in `baggage/`) is resolved by name.
pub async fn disc(State(st): State<Arc<AppState>>, UrlPath((edition, path)): UrlPath<(String, String)>, req: Request) -> ApiResult<Response> {
    let (Some(db), Some(tree)) = (&st.media, st.disc_tree.clone()) else {
        return Err(ApiError::not_found("no media manifest"));
    };
    let kids = edition == "kids";
    let sha = tree.find(&path, kids).map(str::to_string);
    let name = disc_name(&path);
    let ed = if kids { "kids" } else { "deluxe" }.to_string();
    let row = db
        .call(move |c| {
            let sha = match sha {
                Some(s) => Some(s),
                None if name.ends_with(".iax") => resolve_sha(c, &name, &ed)?,
                None => None,
            };
            let Some(sha) = sha else { return Ok(None) };
            Ok(media_info(c, &sha)?.and_then(|i| i.variants.get("original").map(|v| (i.sha256.clone(), v.path.clone(), v.mime))))
        })
        .await?;
    let Some((sha, p, mime)) = row else {
        return Err(ApiError::not_found(format!("no disc file {path}")));
    };
    let file = st.out.join("media").join(p);
    if crate::flash_compat::applies(&path) {
        let (s, f) = (sha.clone(), file.clone());
        let patched = tokio::task::spawn_blocking(move || crate::flash_compat::patched_cached(&s, || std::fs::read(f).ok()))
            .await
            .ok()
            .flatten();
        if let Some(bytes) = patched {
            return Ok(serve_bytes(&req, bytes, mime, &format!("\"{sha}-ruffle1\"")));
        }
    }
    serve_path(req, file, mime, Some(format!("\"{sha}-original\"")), false).await
}

/// Serves an in-memory body (GET/HEAD, `If-None-Match`; no ranges) with long-lived caching.
fn serve_bytes(req: &Request, bytes: Arc<Vec<u8>>, mime: &str, etag: &str) -> Response {
    use axum::http::{HeaderValue, Method};
    let inm = req.headers().get(header::IF_NONE_MATCH).and_then(|v| v.to_str().ok());
    let fresh = inm.is_some_and(|v| v.split(',').any(|t| t.trim() == etag || t.trim() == "*"));
    let mut r = if fresh {
        StatusCode::NOT_MODIFIED.into_response()
    } else if req.method() == Method::HEAD {
        let mut r = StatusCode::OK.into_response();
        r.headers_mut().insert(header::CONTENT_LENGTH, HeaderValue::from(bytes.len()));
        r
    } else {
        (StatusCode::OK, bytes.as_ref().clone()).into_response()
    };
    let h = r.headers_mut();
    if !fresh {
        h.insert(header::CONTENT_TYPE, HeaderValue::from_str(mime).unwrap_or(HeaderValue::from_static("application/octet-stream")));
        h.insert("x-content-type-options", HeaderValue::from_static("nosniff"));
    }
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static(IMMUTABLE));
    if let Ok(v) = HeaderValue::from_str(etag) {
        h.insert(header::ETAG, v);
    }
    r
}
