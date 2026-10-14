//! File responses with single-range support (clamped to the file size: PMTiles readers ask for
//! more than small archives hold), HEAD, ETag / Last-Modified validation; and the static trees
//! (`out/atlas`, `out/tours`, the SPA).

use crate::AppState;
use crate::media::mime_for_ext;
use axum::body::Body;
use axum::extract::{Path as UrlPath, Request, State};
use axum::http::{HeaderMap, HeaderValue, Method, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde_json::Value;
use std::io::SeekFrom;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use tokio_util::io::ReaderStream;

pub const NO_CACHE: &str = "no-cache";
pub const DATA_CACHE: &str = "public, max-age=3600";

/// Parses a `Range` header into an inclusive byte range clamped to `len`.
/// `Ok(None)`: no (usable) range, serve everything. `Err(())`: unsatisfiable.
#[allow(clippy::result_unit_err)]
pub fn parse_range(h: &str, len: u64) -> Result<Option<(u64, u64)>, ()> {
    let Some(spec) = h.trim().strip_prefix("bytes=") else { return Ok(None) };
    // Multiple ranges: serve the first one only (a valid 206 for the first part is not
    // multipart, so fall back to the whole file instead).
    if spec.contains(',') {
        return Ok(None);
    }
    let (a, b) = spec.split_once('-').ok_or(())?;
    let (a, b) = (a.trim(), b.trim());
    if len == 0 {
        return Err(());
    }
    let (start, end) = if a.is_empty() {
        let n: u64 = b.parse().map_err(|_| ())?;
        if n == 0 {
            return Err(());
        }
        (len.saturating_sub(n), len - 1)
    } else {
        let s: u64 = a.parse().map_err(|_| ())?;
        let e: u64 = if b.is_empty() { len - 1 } else { b.parse::<u64>().map_err(|_| ())?.min(len - 1) };
        (s, e)
    };
    if start >= len || start > end {
        return Err(());
    }
    Ok(Some((start, end)))
}

fn http_date(t: SystemTime) -> String {
    // RFC 7231 IMF-fixdate, computed without a date crate.
    let secs = t.duration_since(SystemTime::UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let days = (secs / 86400) as i64;
    let (h, m, s) = ((secs % 86400) / 3600, (secs % 3600) / 60, secs % 60);
    let wd = ["Thu", "Fri", "Sat", "Sun", "Mon", "Tue", "Wed"][(days % 7) as usize];
    // Civil from days (Howard Hinnant's algorithm).
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let mo = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(mo <= 2);
    let mon = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"][(mo - 1) as usize];
    format!("{wd}, {d:02} {mon} {y} {h:02}:{m:02}:{s:02} GMT")
}

pub struct FileOpts<'a> {
    pub mime: &'a str,
    pub cache_control: &'static str,
    /// Strong ETag (quoted). Default: size + mtime.
    pub etag: Option<String>,
    pub vary_accept: bool,
}

/// Serves `path` for `method`/`headers`. 404 when it is not a regular file.
pub async fn serve_file(method: &Method, headers: &HeaderMap, path: &Path, o: FileOpts<'_>) -> Response {
    let Ok(meta) = tokio::fs::metadata(path).await else { return StatusCode::NOT_FOUND.into_response() };
    if !meta.is_file() {
        return StatusCode::NOT_FOUND.into_response();
    }
    let len = meta.len();
    let mtime = meta.modified().ok();
    let etag = o.etag.unwrap_or_else(|| {
        format!("\"{:x}-{:x}\"", len, mtime.and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok()).map_or(0, |d| d.as_secs()))
    });
    let mut base = HeaderMap::new();
    base.insert(header::CONTENT_TYPE, HeaderValue::from_str(o.mime).unwrap_or(HeaderValue::from_static("application/octet-stream")));
    base.insert(header::CACHE_CONTROL, HeaderValue::from_static(o.cache_control));
    base.insert(header::ACCEPT_RANGES, HeaderValue::from_static("bytes"));
    base.insert("x-content-type-options", HeaderValue::from_static("nosniff"));
    if let Ok(v) = HeaderValue::from_str(&etag) {
        base.insert(header::ETAG, v);
    }
    if let Some(t) = mtime
        && let Ok(v) = HeaderValue::from_str(&http_date(t)) {
            base.insert(header::LAST_MODIFIED, v);
        }
    if o.vary_accept {
        base.insert(header::VARY, HeaderValue::from_static("Accept"));
    }
    let inm = headers.get(header::IF_NONE_MATCH).and_then(|v| v.to_str().ok());
    let not_modified = match inm {
        Some(v) => v.split(',').any(|t| t.trim() == etag || t.trim() == "*"),
        None => headers
            .get(header::IF_MODIFIED_SINCE)
            .and_then(|v| v.to_str().ok())
            .zip(base.get(header::LAST_MODIFIED).and_then(|v| v.to_str().ok()))
            .is_some_and(|(a, b)| a == b),
    };
    if not_modified {
        let mut r = StatusCode::NOT_MODIFIED.into_response();
        for k in [header::ETAG, header::CACHE_CONTROL, header::LAST_MODIFIED, header::VARY] {
            if let Some(v) = base.get(&k) {
                r.headers_mut().insert(k, v.clone());
            }
        }
        return r;
    }
    // If-Range: only honour the range when the validator still matches.
    let if_range_ok = headers.get(header::IF_RANGE).and_then(|v| v.to_str().ok()).is_none_or(|v| {
        v.trim() == etag || base.get(header::LAST_MODIFIED).and_then(|l| l.to_str().ok()) == Some(v.trim())
    });
    let range = match headers.get(header::RANGE).and_then(|v| v.to_str().ok()).filter(|_| if_range_ok) {
        Some(h) => match parse_range(h, len) {
            Ok(r) => r,
            Err(()) => {
                let mut r = StatusCode::RANGE_NOT_SATISFIABLE.into_response();
                r.headers_mut().insert(header::CONTENT_RANGE, HeaderValue::from_str(&format!("bytes */{len}")).unwrap());
                return r;
            }
        },
        None => None,
    };
    let (status, start, count) = match range {
        Some((s, e)) => (StatusCode::PARTIAL_CONTENT, s, e - s + 1),
        None => (StatusCode::OK, 0, len),
    };
    if let Some((s, e)) = range {
        base.insert(header::CONTENT_RANGE, HeaderValue::from_str(&format!("bytes {s}-{e}/{len}")).unwrap());
    }
    base.insert(header::CONTENT_LENGTH, HeaderValue::from(count));
    let body = if method == Method::HEAD {
        Body::empty()
    } else {
        let Ok(mut f) = tokio::fs::File::open(path).await else { return StatusCode::NOT_FOUND.into_response() };
        if start > 0 && f.seek(SeekFrom::Start(start)).await.is_err() {
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
        Body::from_stream(ReaderStream::with_capacity(f.take(count), 64 * 1024))
    };
    let mut r = Response::new(body);
    *r.status_mut() = status;
    *r.headers_mut() = base;
    r
}

/// The parts of a request the file handlers need (a `Request` is not `Sync`).
pub struct Req {
    pub method: Method,
    pub headers: HeaderMap,
}

impl From<Request> for Req {
    fn from(r: Request) -> Self {
        let (parts, _) = r.into_parts();
        Req { method: parts.method, headers: parts.headers }
    }
}

/// `rel` joined under `root`, refusing `..`, absolute and prefix components.
pub fn safe_join(root: &Path, rel: &str) -> Option<PathBuf> {
    let mut p = root.to_path_buf();
    for c in Path::new(rel.trim_start_matches('/')).components() {
        match c {
            Component::Normal(s) => p.push(s),
            Component::CurDir => {}
            _ => return None,
        }
    }
    Some(p)
}

fn ext(p: &Path) -> &str {
    p.extension().and_then(|e| e.to_str()).unwrap_or("")
}

async fn serve_tree(req: &Req, root: &Path, rel: &str, cache: &'static str) -> Option<Response> {
    let path = safe_join(root, rel)?;
    if !tokio::fs::metadata(&path).await.ok()?.is_file() {
        return None;
    }
    let mime = mime_for_ext(ext(&path)).unwrap_or("application/octet-stream");
    Some(serve_file(&req.method, &req.headers, &path, FileOpts { mime, cache_control: cache, etag: None, vary_accept: false }).await)
}

/// Origin of the request as the client saw it (behind Traefik: X-Forwarded-Proto/Host).
pub fn origin(headers: &HeaderMap) -> String {
    let h = |n: &str| headers.get(n).and_then(|v| v.to_str().ok()).map(|s| s.split(',').next().unwrap_or("").trim().to_string());
    let proto = h("x-forwarded-proto").filter(|s| !s.is_empty()).unwrap_or_else(|| "http".into());
    let host = h("x-forwarded-host").filter(|s| !s.is_empty()).or_else(|| h("host")).unwrap_or_else(|| "localhost".into());
    format!("{proto}://{host}")
}

/// Makes the root-relative URLs of a MapLibre style absolute: `sprite`, `glyphs`, source `url`s
/// and `tiles` (`/atlas/…` → `<origin><mount>/…`, `pmtiles:///atlas/…` → `pmtiles://<origin><mount>/…`).
/// `mount` is where the style was fetched (`/atlas` or `/out/atlas`), so a style read through
/// `/out/atlas/` (the Vite dev proxy's path) points at `/out/atlas/` too.
pub fn absolutize_style(style: &mut Value, origin: &str, mount: &str) {
    let path = |p: &str| -> String {
        match p.strip_prefix("/atlas/") {
            Some(rest) => format!("{mount}/{rest}"),
            None => p.to_string(),
        }
    };
    let fix = |s: &str| -> Option<String> {
        if let Some(rest) = s.strip_prefix("pmtiles://").filter(|r| r.starts_with('/')) {
            Some(format!("pmtiles://{origin}{}", path(rest)))
        } else if s.starts_with('/') && !s.starts_with("//") {
            Some(format!("{origin}{}", path(s)))
        } else {
            None
        }
    };
    let fix_val = |v: &mut Value| {
        if let Some(n) = v.as_str().and_then(fix) {
            *v = Value::String(n);
        }
    };
    if let Some(o) = style.as_object_mut() {
        for k in ["sprite", "glyphs"] {
            if let Some(v) = o.get_mut(k) {
                fix_val(v);
                // MapLibre also accepts an array of {id, url} sprites.
                if let Some(arr) = v.as_array_mut() {
                    for s in arr {
                        if let Some(u) = s.get_mut("url") {
                            fix_val(u);
                        }
                    }
                }
            }
        }
        if let Some(srcs) = o.get_mut("sources").and_then(|s| s.as_object_mut()) {
            for src in srcs.values_mut() {
                if let Some(u) = src.get_mut("url") {
                    fix_val(u);
                }
                if let Some(t) = src.get_mut("tiles").and_then(|t| t.as_array_mut()) {
                    t.iter_mut().for_each(fix_val);
                }
            }
        }
    }
}

async fn atlas_file(st: &AppState, req: &Req, rel: &str, mount: &str) -> Option<Response> {
    let root = st.out.join("atlas");
    if rel.starts_with("styles/") && rel.ends_with(".json") && rel != "styles/index.json" {
        let path = safe_join(&root, rel)?;
        let bytes = tokio::fs::read(&path).await.ok()?;
        let mut v: Value = serde_json::from_slice(&bytes).ok()?;
        absolutize_style(&mut v, &origin(&req.headers), mount);
        let mut r = axum::Json(v).into_response();
        r.headers_mut().insert(header::CACHE_CONTROL, HeaderValue::from_static(DATA_CACHE));
        r.headers_mut().insert(header::VARY, HeaderValue::from_static("Host, X-Forwarded-Host, X-Forwarded-Proto"));
        return Some(r);
    }
    serve_tree(req, &root, rel, DATA_CACHE).await
}

/// `/atlas/{*path}`: a file of `out/atlas` (styles made absolute), else the SPA (client routes
/// such as `/atlas/place/{gid}`).
pub async fn atlas(State(st): State<Arc<AppState>>, UrlPath(rel): UrlPath<String>, req: Request) -> Response {
    let req = Req::from(req);
    match atlas_file(&st, &req, &rel, "/atlas").await {
        Some(r) => r,
        None => spa(&st, &req).await,
    }
}

/// `/out/atlas/{*path}` and `/out/tours/{*path}`: files only (as the Vite dev server serves them).
pub async fn out_atlas(State(st): State<Arc<AppState>>, UrlPath(rel): UrlPath<String>, req: Request) -> Response {
    let req = Req::from(req);
    atlas_file(&st, &req, &rel, "/out/atlas").await.unwrap_or_else(|| StatusCode::NOT_FOUND.into_response())
}

pub async fn out_tours(State(st): State<Arc<AppState>>, UrlPath(rel): UrlPath<String>, req: Request) -> Response {
    let req = Req::from(req);
    serve_tree(&req, &st.out.join("tours"), &rel, DATA_CACHE).await.unwrap_or_else(|| StatusCode::NOT_FOUND.into_response())
}

/// `/out/soundfont/{*path}`: the MIDI synth's General MIDI soundfont (`encarta-convert soundfont`).
pub async fn out_soundfont(State(st): State<Arc<AppState>>, UrlPath(rel): UrlPath<String>, req: Request) -> Response {
    let req = Req::from(req);
    serve_tree(&req, &st.out.join("soundfont"), &rel, DATA_CACHE).await.unwrap_or_else(|| StatusCode::NOT_FOUND.into_response())
}

/// Font files (`.ttf`, `.otf`, `.woff`, `.woff2`) directly in `dir`, sorted; empty when missing.
pub fn font_files(dir: &Path) -> Vec<String> {
    let Ok(rd) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut v: Vec<String> = rd
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| matches!(ext(Path::new(n)).to_ascii_lowercase().as_str(), "ttf" | "otf" | "woff" | "woff2"))
        .collect();
    v.sort();
    v
}

/// `/fonts/{name}`: a font of `out/fonts/` (the disc's MS Reference fonts, when a converter step
/// copies them there; the web client names them first in its font stacks).
pub async fn font(State(st): State<Arc<AppState>>, UrlPath(name): UrlPath<String>, req: Request) -> Response {
    let req = Req::from(req);
    if !st.fonts.iter().any(|f| f.eq_ignore_ascii_case(&name)) {
        return StatusCode::NOT_FOUND.into_response();
    }
    let real = st.fonts.iter().find(|f| f.eq_ignore_ascii_case(&name)).cloned().unwrap_or(name);
    serve_tree(&req, &st.out.join("fonts"), &real, DATA_CACHE).await.unwrap_or_else(|| StatusCode::NOT_FOUND.into_response())
}

/// `/sounds/{name}`: an interface sound of `out/sounds/` (`encarta-convert content`: the startup
/// sound, the click, tick, Kids hover and timeline sounds and the Kids home category sounds, `.wav` only).
pub async fn sound(State(st): State<Arc<AppState>>, UrlPath(name): UrlPath<String>, req: Request) -> Response {
    let req = Req::from(req);
    if !is_sound_name(&name) {
        return StatusCode::NOT_FOUND.into_response();
    }
    serve_tree(&req, &st.out.join("sounds"), &name, DATA_CACHE).await.unwrap_or_else(|| StatusCode::NOT_FOUND.into_response())
}

fn is_sound_name(name: &str) -> bool {
    name.strip_suffix(".wav").is_some_and(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-'))
}

/// `/assets/{*path}`: hashed build assets, cached forever.
pub async fn assets(State(st): State<Arc<AppState>>, UrlPath(rel): UrlPath<String>, req: Request) -> Response {
    let req = Req::from(req);
    let Some(dir) = &st.static_dir else { return StatusCode::NOT_FOUND.into_response() };
    serve_tree(&req, &dir.join("assets"), &rel, crate::media::IMMUTABLE).await.unwrap_or_else(|| StatusCode::NOT_FOUND.into_response())
}

/// Everything else: a static file of the SPA, or its `index.html`.
pub async fn fallback(State(st): State<Arc<AppState>>, req: Request) -> Response {
    let path = req.uri().path().to_string();
    let req = Req::from(req);
    if path.starts_with("/api/") || path == "/api" {
        return crate::error::ApiError::not_found(format!("no route {path}")).into_response();
    }
    if path.starts_with("/media/") || path.starts_with("/out/") {
        return StatusCode::NOT_FOUND.into_response();
    }
    if let Some(dir) = &st.static_dir
        && path != "/"
            && let Some(r) = serve_tree(&req, dir, &path, static_cache(&path)).await {
                return r;
            }
    spa(&st, &req).await
}

/// Cache policy of a file of the built SPA outside `assets/`. Ruffle's wasm cores and script
/// chunks carry a content hash in their names (`core.ruffle.<hash>.js`, `<hash>.wasm`), so they
/// are immutable like Vite's assets; everything else (`ruffle.js`, `index.html` …) revalidates.
pub fn static_cache(path: &str) -> &'static str {
    let Some(name) = path.strip_prefix("/ruffle/") else { return NO_CACHE };
    let hashed = name.split('.').any(|part| part.len() >= 16 && part.bytes().all(|b| b.is_ascii_hexdigit()));
    if hashed && !name.contains('/') { crate::media::IMMUTABLE } else { NO_CACHE }
}

async fn spa(st: &AppState, req: &Req) -> Response {
    if !matches!(req.method, Method::GET | Method::HEAD) {
        return StatusCode::METHOD_NOT_ALLOWED.into_response();
    }
    match &st.static_dir {
        Some(dir) => {
            let index = dir.join("index.html");
            let r = serve_file(&req.method, &req.headers, &index, FileOpts {
                mime: "text/html; charset=utf-8",
                cache_control: NO_CACHE,
                etag: None,
                vary_accept: false,
            })
            .await;
            if r.status() == StatusCode::NOT_FOUND {
                (StatusCode::NOT_FOUND, "web client not built (no index.html)").into_response()
            } else {
                r
            }
        }
        None => (StatusCode::NOT_FOUND, "web client not built").into_response(),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn sound_names() {
        assert!(super::is_sound_name("kids-10.wav") && super::is_sound_name("kids-hover.wav") && super::is_sound_name("tick.wav"));
        for n in ["", ".wav", "../x.wav", "a/b.wav", "click.wav.partial", "Click.wav", "click.mp3"] {
            assert!(!super::is_sound_name(n), "{n}");
        }
    }

    use super::*;

    #[test]
    fn ranges() {
        assert_eq!(parse_range("bytes=0-16383", 1000), Ok(Some((0, 999))));
        assert_eq!(parse_range("bytes=10-", 1000), Ok(Some((10, 999))));
        assert_eq!(parse_range("bytes=-100", 1000), Ok(Some((900, 999))));
        assert_eq!(parse_range("bytes=-5000", 1000), Ok(Some((0, 999))));
        assert_eq!(parse_range("bytes=1000-", 1000), Err(()));
        assert_eq!(parse_range("bytes=5-2", 1000), Err(()));
        assert_eq!(parse_range("bytes=0-1,5-6", 1000), Ok(None));
        assert_eq!(parse_range("items=0-1", 1000), Ok(None));
    }

    #[test]
    fn ruffle_hashed_files_are_immutable() {
        assert_eq!(static_cache("/ruffle/core.ruffle.16d61072ac7fdc4d5013.js"), crate::media::IMMUTABLE);
        assert_eq!(static_cache("/ruffle/172de42e6619eac8f371.wasm"), crate::media::IMMUTABLE);
        assert_eq!(static_cache("/ruffle/ruffle.js"), NO_CACHE);
        assert_eq!(static_cache("/ruffle/LICENSE_MIT"), NO_CACHE);
        assert_eq!(static_cache("/tours.html"), NO_CACHE);
        assert_eq!(static_cache("/172de42e6619eac8f371.wasm"), NO_CACHE);
    }

    #[test]
    fn dates() {
        assert_eq!(http_date(SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(784111777)), "Sun, 06 Nov 1994 08:49:37 GMT");
    }

    #[test]
    fn styles() {
        let mut v = serde_json::json!({"sprite": "/atlas/sprites/sprite", "glyphs": "/atlas/glyphs/{fontstack}/{range}.pbf",
            "sources": {"e": {"url": "pmtiles:///atlas/vector/earth.pmtiles"}, "x": {"tiles": ["/a/{z}.png", "https://x/y"]}}});
        let orig = v.clone();
        absolutize_style(&mut v, "https://enc.example", "/atlas");
        assert_eq!(v["sprite"], "https://enc.example/atlas/sprites/sprite");
        assert_eq!(v["glyphs"], "https://enc.example/atlas/glyphs/{fontstack}/{range}.pbf");
        assert_eq!(v["sources"]["e"]["url"], "pmtiles://https://enc.example/atlas/vector/earth.pmtiles");
        assert_eq!(v["sources"]["x"]["tiles"][0], "https://enc.example/a/{z}.png");
        assert_eq!(v["sources"]["x"]["tiles"][1], "https://x/y");
        // Fetched through /out/atlas/: the URLs keep that prefix.
        let mut v = orig;
        absolutize_style(&mut v, "http://127.0.0.1:5191", "/out/atlas");
        assert_eq!(v["sprite"], "http://127.0.0.1:5191/out/atlas/sprites/sprite");
        assert_eq!(v["sources"]["e"]["url"], "pmtiles://http://127.0.0.1:5191/out/atlas/vector/earth.pmtiles");
        assert_eq!(v["sources"]["x"]["tiles"][0], "http://127.0.0.1:5191/a/{z}.png");
        assert!(safe_join(Path::new("/r"), "../etc/passwd").is_none());
        assert_eq!(safe_join(Path::new("/r"), "/a/b.json"), Some(PathBuf::from("/r/a/b.json")));
    }
}
