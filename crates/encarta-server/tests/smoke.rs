//! Smoke test against the user's real converted output (`ENCARTA_OUT`, default `../../out`);
//! skipped when it is absent. Checks the Zebra article (761575094) end to end.

mod common;

use axum::body::Body;
use axum::http::{Request, header};
use common::*;
use encarta_server::Config;
use std::path::PathBuf;

const ZEBRA: u64 = 761575094;

fn out_dir() -> Option<PathBuf> {
    let p = std::env::var_os("ENCARTA_OUT").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../out"));
    p.join("content.sqlite").is_file().then_some(p)
}

#[tokio::test]
async fn zebra_end_to_end() {
    let Some(out) = out_dir() else {
        eprintln!("skipped: no out/content.sqlite");
        return;
    };
    let app = app(&Config::new(out.clone()));

    let (s, a) = json(&app, &format!("/api/article/{ZEBRA}")).await;
    assert_eq!(s, 200);
    assert_eq!(a["title"], "Zebra");
    assert!(a["html"].as_str().unwrap().contains("enc-article"));
    assert!(a["sections"].as_array().unwrap().len() >= 5);
    assert!(a["related"].is_array());
    assert!(a["tables"].as_array().unwrap().iter().any(|t| t["kind"] == "facts"));

    let (s, i) = json(&app, &format!("/api/item/{ZEBRA}")).await;
    assert_eq!(s, 200);
    assert!(i["media"].as_array().unwrap().len() >= 5);

    if out.join("search/meta.json").is_file() {
        let (_, r) = json(&app, "/api/search?q=zebra").await;
        assert_eq!(r["hits"][0]["ref_id"], ZEBRA, "{r}");
        let (_, r) = json(&app, "/api/suggest?q=zeb").await;
        assert!(r["suggestions"].as_array().unwrap().iter().take(3).any(|s| s["ref_id"] == ZEBRA), "{r}");
        let (_, r) = json(&app, "/api/spell?q=zebr").await;
        assert_eq!(r["corrected"], "zebra");
    }
    if out.join("media/manifest.sqlite").is_file() {
        // The article's first picture (Grévy's zebra) by its disc name, with a range.
        let req = Request::get("/media/file/t790545a.jpg").header(header::RANGE, "bytes=0-1023").body(Body::empty()).unwrap();
        let res = send(&app, req).await;
        assert_eq!(res.status(), 206);
        assert_eq!(res.headers()[header::CONTENT_TYPE], "image/jpeg");
        let b = bytes(res).await;
        assert_eq!(b.len(), 1024);
        assert_eq!(&b[..3], &[0xFF, 0xD8, 0xFF]);
        assert_eq!(get(&app, "/media/file/t790545a.jtn").await.status(), 200);
    }
}
