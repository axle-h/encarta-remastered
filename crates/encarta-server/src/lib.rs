//! Read-only HTTP server for the converted Encarta output. API contract: docs/api.md.
//!
//! Every part of the data directory is optional: a missing database, index or tree turns its
//! endpoints into 404/503 responses, and the server still starts (health check and SPA work
//! with an empty data directory).

pub mod api;
pub mod db;
pub mod error;
pub mod files;
pub mod flash_compat;
pub mod games;
pub mod media;

use axum::Router;
use axum::extract::Request;
use axum::http::{HeaderValue, StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use db::{Db, probe_uri};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;
use tower_http::compression::CompressionLayer;
use tower_http::compression::predicate::Predicate;

#[derive(Debug, Clone)]
pub struct Config {
    /// Converter output (`out/`).
    pub out: PathBuf,
    /// Built web client (`web/dist`).
    pub static_dir: Option<PathBuf>,
    /// Unpacked disc, for the media fallback (off when `None`).
    pub extracted: Option<PathBuf>,
    /// Maximum concurrent SQLite queries per database.
    pub db_concurrency: usize,
}

impl Config {
    pub fn new(out: impl Into<PathBuf>) -> Self {
        Config { out: out.into(), static_dir: None, extracted: None, db_concurrency: 32 }
    }
}

pub struct AppState {
    pub out: PathBuf,
    /// `content.sqlite`, with the media manifest attached as `m` when present.
    pub content: Option<Db>,
    /// Database handle for media lookups (`m.*` tables): `content` when it exists, else an
    /// in-memory database with the manifest attached.
    pub media: Option<Db>,
    pub media_manifest: bool,
    /// `atlas/gazetteer.sqlite`, with `content.sqlite` attached as `c` when present.
    pub gazetteer: Option<Db>,
    pub search: Option<Arc<encarta_search::SearchIndex>>,
    pub extracted: Option<Arc<media::ExtractedFallback>>,
    pub dict: Option<Arc<api::DictIndex>>,
    /// The disc's `dswMedia/` tree (Flash interactives), from the media manifest.
    pub disc_tree: Option<Arc<media::DiscTree>>,
    /// Font files in `out/fonts/` (none unless a converter step puts the disc fonts there).
    pub fonts: Vec<String>,
    pub static_dir: Option<PathBuf>,
    pub tours: bool,
    pub atlas: bool,
}

fn log(msg: impl AsRef<str>) {
    println!("{}", msg.as_ref());
}

impl AppState {
    /// Opens whatever exists under `cfg.out`; never fails.
    pub fn open(cfg: &Config) -> AppState {
        let out = cfg.out.clone();
        let content_path = out.join("content.sqlite");
        let manifest_path = out.join("media/manifest.sqlite");
        let gaz_path = out.join("atlas/gazetteer.sqlite");
        let content_uri = content_path.is_file().then(|| probe_uri(&content_path, "SELECT 1 FROM item LIMIT 1")).and_then(|r| {
            r.map_err(|e| log(format!("warning: content database unusable: {e:#}"))).ok()
        });
        let manifest_uri = manifest_path.is_file().then(|| probe_uri(&manifest_path, "SELECT 1 FROM derivative LIMIT 1")).and_then(|r| {
            r.map_err(|e| log(format!("warning: media manifest unusable: {e:#}"))).ok()
        });
        let attach_m: Vec<(String, String)> = manifest_uri.iter().map(|u| ("m".to_string(), u.clone())).collect();
        let content = content_uri.as_ref().and_then(|u| {
            Db::new(u.clone(), attach_m.clone(), cfg.db_concurrency).map_err(|e| log(format!("warning: content database: {e:#}"))).ok()
        });
        let media_manifest = manifest_uri.is_some() && content.as_ref().is_none_or(|c| c.has("m"));
        let media = if !media_manifest {
            None
        } else if content.is_some() {
            content.clone()
        } else {
            Db::new(":memory:".into(), attach_m.clone(), cfg.db_concurrency).map_err(|e| log(format!("warning: media manifest: {e:#}"))).ok()
        };
        let gazetteer = gaz_path.is_file().then(|| probe_uri(&gaz_path, "SELECT 1 FROM place LIMIT 1")).and_then(|r| {
            r.map_err(|e| log(format!("warning: gazetteer unusable: {e:#}"))).ok()
        });
        let gazetteer = gazetteer.and_then(|u| {
            let attach = content_uri.iter().map(|c| ("c".to_string(), c.clone())).collect();
            Db::new(u, attach, cfg.db_concurrency).map_err(|e| log(format!("warning: gazetteer: {e:#}"))).ok()
        });
        let search_dir = out.join("search");
        let search = if search_dir.join("meta.json").is_file() {
            match encarta_search::SearchIndex::open(&search_dir) {
                Ok(s) => Some(Arc::new(s)),
                Err(e) => {
                    log(format!("warning: search index unusable: {e:#}"));
                    None
                }
            }
        } else {
            None
        };
        let extracted = cfg.extracted.as_ref().and_then(|p| match media::ExtractedFallback::open(p) {
            Ok(f) => Some(Arc::new(f)),
            Err(e) => {
                log(format!("warning: extracted fallback off: {e:#}"));
                None
            }
        });
        let dict = content_uri.as_ref().and_then(|u| {
            let t = Instant::now();
            let r = rusqlite::Connection::open_with_flags(u, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_URI)
                .map_err(anyhow::Error::from)
                .and_then(|c| api::DictIndex::load(&c).map_err(anyhow::Error::from));
            match r {
                Ok(d) => {
                    log(format!("dictionary forms: {} in {:.0} ms", d.len(), t.elapsed().as_secs_f64() * 1e3));
                    Some(Arc::new(d))
                }
                Err(e) => {
                    log(format!("warning: dictionary index: {e:#}"));
                    None
                }
            }
        });
        let disc_tree = manifest_uri.as_ref().and_then(|u| match media::DiscTree::load(u) {
            Ok(t) => Some(Arc::new(t)),
            Err(e) => {
                log(format!("warning: disc tree: {e:#}"));
                None
            }
        });
        let fonts = files::font_files(&out.join("fonts"));
        let static_dir = cfg.static_dir.clone().filter(|d| d.is_dir());
        let st = AppState {
            tours: out.join("tours").is_dir(),
            atlas: out.join("atlas").is_dir(),
            out,
            content,
            media,
            media_manifest,
            gazetteer,
            search,
            extracted,
            dict,
            disc_tree,
            fonts,
            static_dir,
        };
        log(format!(
            "data {}: content={} media_manifest={} search={} gazetteer={} tours={} atlas={} fonts={} extracted_fallback={} static={}",
            st.out.display(),
            st.content.is_some(),
            st.media_manifest,
            st.search.is_some(),
            st.gazetteer.is_some(),
            st.tours,
            st.atlas,
            st.fonts.len(),
            st.extracted.is_some(),
            st.static_dir.as_ref().map_or("none".into(), |d| d.display().to_string()),
        ));
        st
    }
}

/// Compress text and the binaries of [`compressible`]; never ranges, small bodies or media.
#[derive(Clone, Copy)]
struct TextOnly;

impl Predicate for TextOnly {
    fn should_compress<B: http_body::Body>(&self, res: &axum::http::Response<B>) -> bool {
        if res.headers().contains_key(header::CONTENT_RANGE) || res.status() == StatusCode::PARTIAL_CONTENT {
            return false;
        }
        if res.body().size_hint().exact().is_some_and(|n| n < 512) {
            return false;
        }
        let ct = res.headers().get(header::CONTENT_TYPE).and_then(|v| v.to_str().ok()).unwrap_or("");
        compressible(ct)
    }
}

/// Content types worth compressing on the fly: text, plus the large binaries that shrink a lot
/// (Ruffle's 14 MB wasm cores to about a third, the disc fonts, tour glTF and PVS files to a half
/// or less). Pictures, audio and video are already compressed and never are.
pub fn compressible(ct: &str) -> bool {
    let ct = ct.split(';').next().unwrap_or("").trim();
    ct.starts_with("text/")
        || matches!(
            ct,
            "application/json"
                | "application/xml"
                | "image/svg+xml"
                | "application/smil"
                | "application/smil+xml"
                | "model/gltf+json"
                | "application/wasm"
                | "font/ttf"
                | "font/otf"
                | "model/gltf-binary"
                | "application/octet-stream"
        )
}

async fn access_log(req: Request, next: Next) -> Response {
    let t = Instant::now();
    let method = req.method().clone();
    let uri = req.uri().clone();
    let res = next.run(req).await;
    if uri.path() != "/healthz" {
        println!("{} {} {} {:.1}ms", method, uri, res.status().as_u16(), t.elapsed().as_secs_f64() * 1e3);
    }
    res
}

async fn healthz() -> impl IntoResponse {
    ([(header::CACHE_CONTROL, HeaderValue::from_static("no-store"))], "ok")
}

pub fn router(st: Arc<AppState>) -> Router {
    let api = Router::new()
        .route("/meta", get(api::meta))
        .route("/item/{id}", get(api::item))
        .route("/article/{id}", get(api::article))
        .route("/index-term/{id}", get(api::index_term))
        .route("/place/{gid}", get(api::place))
        .route("/quiz/{id}", get(api::quiz))
        .route("/interactive/{name}", get(api::interactive))
        .route("/dictionary", get(api::dictionaries))
        .route("/dictionary/lookup", get(api::dict_lookup))
        .route("/dictionary/entry/{id}", get(api::dict_entry_by_id))
        .route("/dictionary/{dict}/{id}", get(api::dict_entry))
        .route("/timeline", get(api::timeline))
        .route("/timeline/meta", get(api::timeline_meta))
        .route("/mindmaze", get(api::mindmaze))
        .route("/mindmaze/questions", get(api::mindmaze_questions))
        .route("/mindmaze/question/{id}", get(api::mindmaze_question))
        .route("/stats", get(api::stats))
        .route("/stats/indicator/{id}", get(api::stats_indicator))
        .route("/stats/country/{id}", get(api::stats_country))
        .route("/search", get(api::search))
        .route("/suggest", get(api::suggest))
        .route("/spell", get(api::spell))
        .route("/atlas/search", get(api::atlas_search))
        .route("/atlas/place/{entity}", get(api::atlas_place))
        .nest("/games", games::routes())
        .fallback(|uri: axum::extract::OriginalUri| async move { error::ApiError::not_found(format!("no route {}", uri.path())) });
    Router::new()
        .route("/healthz", get(healthz))
        .nest("/api", api)
        .route("/media/file/{*name}", get(media::file))
        .route("/media/{sha}/{variant}", get(media::by_hash))
        // Flash `loadMovie` with variables POSTs: answered like a GET.
        .route("/media/disc/{edition}/{*path}", get(media::disc).post(media::disc))
        .route("/fonts/{name}", get(files::font))
        .route("/atlas/{*path}", get(files::atlas))
        .route("/out/atlas/{*path}", get(files::out_atlas))
        .route("/out/tours/{*path}", get(files::out_tours))
        .route("/assets/{*path}", get(files::assets))
        .fallback(files::fallback)
        .with_state(st)
        .layer(CompressionLayer::new().gzip(true).br(true).compress_when(TextOnly))
        .layer(middleware::from_fn(access_log))
}

#[cfg(test)]
mod tests {
    use super::compressible;

    #[test]
    fn compressible_types() {
        for ct in ["application/json", "text/html; charset=utf-8", "text/javascript", "image/svg+xml", "application/wasm", "font/ttf", "model/gltf-binary", "application/octet-stream"] {
            assert!(compressible(ct), "{ct}");
        }
        for ct in ["image/jpeg", "image/avif", "audio/ogg", "video/mp4", "font/woff2", "application/x-shockwave-flash", ""] {
            assert!(!compressible(ct), "{ct}");
        }
    }
}
