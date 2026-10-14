//! API, media and static-file tests against the synthetic tree in `common`, served from a
//! read-only copy of `out/`.

mod common;

use axum::body::Body;
use axum::http::{Request, header};
use common::*;
use encarta_server::Config;

struct Ro(Fixture);

impl Drop for Ro {
    fn drop(&mut self) {
        self.0.set_read_only(false);
    }
}

fn fixture() -> Ro {
    let f = Fixture::build();
    f.set_read_only(true);
    Ro(f)
}

#[tokio::test]
async fn content_endpoints() {
    let f = fixture();
    let app = app(&f.0.config());

    let r = get(&app, "/healthz").await;
    assert_eq!(r.status(), 200);

    let (s, meta) = json(&app, "/api/meta").await;
    assert_eq!(s, 200);
    assert_eq!(meta["media_manifest"], true, "{meta}");
    assert!(meta["search"]["item_docs"].as_u64().unwrap() >= 5);

    let (s, item) = json(&app, &format!("/api/item/{PHOTO}")).await;
    assert_eq!(s, 200);
    assert_eq!(item["title"], "Plains Zebra Herd");
    assert_eq!(item["editions"], serde_json::json!(["deluxe"]));
    assert_eq!(item["files"][1]["url"], "/media/file/zz000001.jpg");
    assert_eq!(item["files"][1]["media"]["variants"]["webp"]["url"], format!("/media/{SHA_JPG}/webp"));
    assert_eq!(item["appears_in"][0]["ref_id"], ZEBRA);

    let (s, item) = json(&app, &format!("/api/item/{ZEBRA}")).await;
    assert_eq!(s, 200);
    assert_eq!(item["meta"]["note"], "synthetic");
    assert_eq!(item["has_article"], true);
    assert_eq!(item["media"][0]["thumb"]["url"], "/media/file/zz000001.jtn");
    // Association rows carry the gallery picture and the full image with their sizes.
    assert_eq!(item["media"][0]["picon"]["url"], "/media/file/zz000001.jsm");
    assert_eq!(item["media"][0]["picon"]["width"], 192);
    assert_eq!(item["media"][0]["image"]["width"], 640);
    assert!(item["media"][1]["picon"].is_null());
    assert_eq!(item["children"][0]["title"], "Plains Zebra Herd");
    assert_eq!(json(&app, "/api/item/999999").await.0, 404);

    // The way back from items DATA gives no reverse rows: who names them (forward associations)
    // and whose `children` list them.
    let (_, facts) = json(&app, &format!("/api/item/{FACTS}")).await;
    assert_eq!(facts["referenced_by"][0]["ref_id"], ZEBRA, "{facts}");
    assert_eq!(facts["referenced_by"][0]["group"], "factsandfigures");
    assert_eq!(facts["referenced_by"][0]["title"], "Zebra");
    assert_eq!(facts["appears_in"].as_array().unwrap().len(), 0);
    let (_, photo) = json(&app, &format!("/api/item/{PHOTO}")).await;
    assert_eq!(photo["parents"][0]["ref_id"], ZEBRA, "{photo}");
    assert_eq!(photo["parents"][0]["item_kind"], "article");
    assert_eq!(photo["referenced_by"][0]["ref_id"], ZEBRA);

    let (s, a) = json(&app, &format!("/api/article/{ZEBRA}")).await;
    assert_eq!(s, 200);
    assert_eq!(a["body"][1]["title"], "STRIPES");
    // Schema 2: the page layout, clusters, columns and `title-ref` parts pass through as stored.
    assert_eq!(a["layout"], "encyclopedia");
    assert_eq!(a["body"][0]["blocks"][0]["t"], "cluster");
    assert_eq!(a["body"][0]["blocks"][0]["items"][1]["ref"], FACTS);
    assert_eq!(a["body"][0]["blocks"][1]["c"][0]["alt"][0], "Equus quagga");
    assert_eq!(a["body"][1]["blocks"][0]["col"], "text");
    assert!(a["html"].as_str().unwrap().starts_with("<article"));
    assert_eq!(a["sections"][1]["number"], "II");
    assert_eq!(a["related_editions"], "DLX,STE");
    assert_eq!(a["related"][0]["ref"], HORSE);
    assert_eq!(a["tables"][0]["kind"], "facts");
    assert_eq!(a["tables"][0]["json"]["entries"][0]["value"][0], "Perissodactyla");
    let (_, a) = json(&app, &format!("/api/article/{ZEBRA}?edition=kids&body=0&html=0")).await;
    assert_eq!(a["related_editions"], "KDC");
    assert!(a.get("body").is_none() && a.get("html").is_none());
    assert_eq!(json(&app, &format!("/api/article/{PHOTO}")).await.0, 404);
    let (_, k) = json(&app, &format!("/api/article/{KIDS}?edition=kids")).await;
    assert_eq!(k["layout"], "kids");
    assert_eq!(k["body"][1]["t"], "media-grid");
    let (_, sb) = json(&app, &format!("/api/article/{SIDEBAR}")).await;
    assert_eq!(sb["layout"], "sidebar");
    // A sidebar association carries what its teaser shows; other rows do not.
    let (_, h) = json(&app, &format!("/api/article/{HORSE}")).await;
    assert_eq!(h["media"][0]["sidebar_category"], "WORLD OF SCIENCE");
    assert_eq!(h["media"][0]["preview_html"], "<p class=\"enc-intro\">Why stripes?</p>");
    assert!(a["media"][0].get("preview_html").is_none());

    let (s, p) = json(&app, "/api/place/42").await;
    assert_eq!(s, 200);
    assert_eq!(p["links"][0]["title"], "Zebra");
    assert_eq!(p["links"][1]["kind_label"], "flag");
    assert_eq!(p["links"][2]["kind_label"], "anthem");
    // A target on no edition of this disc is left out; atlas photos and music are titled.
    let links = p["links"].as_array().unwrap();
    assert_eq!(links.len(), 5, "{p}");
    assert!(links.iter().all(|l| l["ref_id"] != 999001));
    assert_eq!(links[3]["title"], "Kenyan Herders");
    assert_eq!(links[3]["item_kind"], "media");
    assert_eq!(links[4]["item_kind"], "audio");
    // … and open as items.
    let (s, am) = json(&app, "/api/item/800").await;
    assert_eq!(s, 200, "{am}");
    assert_eq!(am["kind"], "media");
    assert_eq!(am["credit"], "Synthetic Photo Agency");
    assert_eq!(am["files"][0]["role"], "image");
    assert_eq!(am["files"][0]["url"], "/media/file/zz000001.jpg");
    assert_eq!(am["files"][1]["role"], "picon");
    // The pushpin picture `zz000001.dtn` is not in the manifest, so it is left out.
    assert_eq!(am["files"].as_array().unwrap().len(), 2, "{am}");
    let (_, song) = json(&app, "/api/item/801").await;
    assert_eq!(song["kind"], "audio");
    assert_eq!(song["credit"], "Synthetic Records");
    assert_eq!(song["files"].as_array().unwrap().len(), 0);
    assert_eq!(json(&app, "/api/quiz/7").await.1["quiz"]["q"], 1);
    let (_, ia) = json(&app, "/api/interactive/ZZ.IAX").await;
    assert_eq!(ia["definition"]["@id"], "zz");
    assert_eq!(ia["refs"][0]["ref"], "Slide.jpg");
    assert_eq!(ia["refs"][0]["url"], "/media/disc/deluxe/dswMedia/IAF/a/zz/Slide.jpg");
    assert_eq!(ia["refs"][0]["kind"], "image");

    let (s, e) = json(&app, "/api/nope").await;
    assert_eq!(s, 404);
    assert_eq!(e["error"], "no route /api/nope");
}

#[tokio::test]
async fn dictionary_timeline_mindmaze_stats() {
    let f = fixture();
    let app = app(&f.0.config());

    let (_, d) = json(&app, "/api/dictionary").await;
    assert_eq!(d["dictionaries"].as_array().unwrap().len(), 2);
    let (_, l) = json(&app, "/api/dictionary/lookup?q=Zebras").await;
    assert_eq!(l["entries"][0]["headword"], "zebra");
    assert_eq!(l["entries"][0]["form_kind"], "inflection");
    let (_, l) = json(&app, "/api/dictionary/lookup?q=zeb&prefix=1&dict=DICTA").await;
    let heads: Vec<_> = l["entries"].as_array().unwrap().iter().map(|e| e["headword"].as_str().unwrap()).collect();
    assert_eq!(heads, vec!["zebra", "zebu"]);
    // Word wheel: headwords before and after the typed word, hyphens ignored in the order.
    let (_, l) = json(&app, "/api/dictionary/lookup?q=zebra&dict=DICTA&around=1&limit=3").await;
    let words = |v: &serde_json::Value| v.as_array().unwrap().iter().map(|e| e["form"].as_str().unwrap().to_string()).collect::<Vec<_>>();
    assert_eq!(words(&l["before"]), vec!["yak"]);
    assert_eq!(words(&l["entries"]), vec!["zebra", "ze-bra crossing", "zebu"]);
    let (_, l) = json(&app, "/api/dictionary/lookup?q=zebs&dict=DICTA&around=5&limit=1").await;
    assert_eq!(words(&l["before"]), vec!["yak", "zebra", "ze-bra crossing"]);
    assert_eq!(words(&l["entries"]), vec!["zebu"]);
    let (_, l) = json(&app, "/api/dictionary/lookup?q=zebra&dict=DICTA&prefix=1").await;
    assert!(l.get("before").is_none());
    let (s, e) = json(&app, "/api/dictionary/entry/9101").await;
    assert_eq!(s, 200);
    assert_eq!(e["dictionary"], "ENG_FRA");
    assert_eq!(e["summary"], "zèbre");
    assert_eq!(json(&app, "/api/dictionary/entry/123").await.0, 404);
    let (_, l) = json(&app, "/api/dictionary/lookup?q=zebra&kind=bilingual").await;
    assert_eq!(l["entries"][0]["dictionary"], "ENG_FRA");
    let (s, e) = json(&app, "/api/dictionary/DICTA/9001").await;
    assert_eq!(s, 200);
    assert_eq!(e["summary"], "striped wild horse");
    assert_eq!(e["audio"][0]["url"], "/media/file/zz000009.wav");
    assert_eq!(e["forms"].as_array().unwrap().len(), 2);

    let (_, m) = json(&app, "/api/timeline/meta").await;
    assert_eq!(m["count"], 2);
    let (_, t) = json(&app, "/api/timeline?from=1800&to=1900").await;
    assert_eq!(t["total"], 1);
    assert_eq!(t["events"][0]["start"]["year"], 1890);
    assert_eq!(t["events"][0]["regions"][0], "Africa");
    assert_eq!(t["events"][0]["has_article"], true);
    assert_eq!(t["events"][0]["display"], "point");
    assert_eq!(t["events"][0]["date_text"], "May 1, 1890");
    assert_eq!(t["events"][0]["prominence"], 3);
    assert!(t["events"][0].get("label").is_none());
    assert!(t["events"][0].get("text").is_none());
    let (_, t) = json(&app, "/api/timeline?from=-100&to=2000&order=importance&text=1").await;
    // Most prominent first: importance 3 (prominence 3) before importance 5 (prominence 1).
    assert_eq!(t["events"][0]["title"], "Stripes Explained");
    assert_eq!(t["events"][0]["text"], "A naturalist proposes why zebras have stripes.");
    assert_eq!(t["events"][1]["title"], "A Treaty");
    assert_eq!(t["events"][1]["has_article"], false);
    assert_eq!(json(&app, "/api/timeline?order=random").await.0, 400);
    let (_, t) = json(&app, "/api/timeline?category=Political%20History").await;
    assert_eq!(t["events"][0]["title"], "A Treaty");
    assert_eq!(t["events"][0]["start"]["circa"], true);
    assert_eq!(t["events"][0]["display"], "period");
    assert_eq!(t["events"][0]["label"], "A TREATY");
    assert_eq!(t["events"][0]["start_text"], "50? BC");
    assert_eq!(t["events"][0]["end_text"], "40 BC");
    let (_, t) = json(&app, "/api/timeline?region=1,2&from=-100&to=-45").await;
    assert_eq!(t["total"], 1);
    assert_eq!(json(&app, "/api/timeline?category=Nope").await.0, 400);
    // A term's events (topic timelines, centuries, search terms), case-insensitive.
    let (s, t) = json(&app, "/api/timeline?term=stripes").await;
    assert_eq!(s, 200);
    assert_eq!(t["total"], 1, "{t}");
    assert_eq!(t["events"][0]["ref_id"], 500);
    let (_, t) = json(&app, "/api/timeline?term=Treaties&from=-100&to=0").await;
    assert_eq!(t["events"][0]["ref_id"], 501);
    let (_, t) = json(&app, "/api/timeline?term=nothing").await;
    assert_eq!(t["total"], 0);
    // No such term: an event of that exact title.
    let (_, t) = json(&app, "/api/timeline?term=a%20treaty").await;
    assert_eq!(t["total"], 1, "{t}");
    assert_eq!(t["events"][0]["ref_id"], 501);

    let (_, mm) = json(&app, "/api/mindmaze").await;
    assert_eq!(mm["total"], 2);
    let (_, q) = json(&app, "/api/mindmaze/questions?level=2").await;
    assert_eq!(q["questions"][0]["answers"][0]["correct"], true);
    let (_, q1) = json(&app, "/api/mindmaze/questions?limit=2&seed=5").await;
    let (_, q2) = json(&app, "/api/mindmaze/questions?limit=2&seed=5").await;
    assert_eq!(q1, q2);
    assert_eq!(json(&app, "/api/mindmaze/question/1").await.1["clue"], "Striped horse of Africa.");

    let (_, s) = json(&app, "/api/stats").await;
    assert_eq!(s["indicators"][0]["unit"]["unit"], "head");
    let (_, i) = json(&app, "/api/stats/indicator/1").await;
    assert_eq!(i["values"][0]["country"], "Tanzania");
    assert_eq!(i["values"][0]["period"], "2000");
    assert_eq!(i["values"][1]["period"], "1990-2005");
    assert_eq!(i["values"][1]["year_end"], 2005);
    let (_, c) = json(&app, "/api/stats/country/1").await;
    assert_eq!(c["values"][0]["indicator"], "Zebra population");
    assert_eq!(c["values"][0]["period"], "1990-2005");

    let (_, g) = json(&app, "/api/atlas/search?q=nairo").await;
    assert_eq!(g["places"][0]["entity"], 42);
    assert_eq!(g["places"].as_array().unwrap().len(), 3);
    assert_eq!(g["places"][0]["goto_km"], 200.0);
    assert_eq!(g["places"][0]["bbox"], serde_json::json!([36.6, -1.45, 37.1, -1.15]));
    assert_eq!(g["places"][0]["body"], "earth");
    assert_eq!(g["places"][0]["class_name"], "PPL - city");
    assert!(g["places"][1]["bbox"].is_null());
    let (_, g) = json(&app, "/api/atlas/search?q=nairo&body=moon").await;
    assert_eq!(g["places"].as_array().unwrap().len(), 1);
    assert_eq!(g["places"][0]["entity"], 44);
    let (_, g) = json(&app, "/api/atlas/place/42").await;
    assert_eq!(g["links"][0]["title"], "Zebra");
    assert_eq!(g["place"]["classes"][1], 2);
}

#[tokio::test]
async fn search_suggest_spell() {
    let f = fixture();
    let app = app(&f.0.config());

    let (s, r) = json(&app, "/api/search?q=zebra").await;
    assert_eq!(s, 200);
    assert_eq!(r["hits"][0]["ref_id"], ZEBRA, "{r}");
    assert_eq!(r["hits"][0]["url"], "/item/100"); // exact title: no deep link
    assert!(r["facets"]["kind"]["article"].as_u64().unwrap() >= 1);
    let (_, r) = json(&app, "/api/search?q=biting%20flies").await;
    assert_eq!(r["hits"][0]["url"], "/item/100#p2");
    assert_eq!(r["hits"][0]["para"]["section_title"], "STRIPES");
    assert!(r["hits"][0]["snippet_html"].as_str().unwrap().contains("<b>biting</b>"));
    let (_, r) = json(&app, "/api/search?q=zebra&kind=media").await;
    assert!(r["hits"].as_array().unwrap().iter().all(|h| h["category"] == "media"));
    let photo = r["hits"].as_array().unwrap().iter().find(|h| h["ref_id"] == PHOTO).expect("photo hit");
    assert_eq!(photo["media_type"], "image");
    assert_eq!(photo["thumb"]["url"], "/media/file/zz000001.jtn");
    // The last word is a prefix; a title that starts with the query comes first, without a deep link.
    let (_, r) = json(&app, "/api/search?q=plains%20zeb").await;
    assert_eq!(r["mode"], "all", "{r}");
    assert_eq!(r["hits"][0]["ref_id"], PHOTO, "{r}");
    assert_eq!(r["hits"][0]["url"], "/item/200");
    let (_, r) = json(&app, "/api/search?q=bit").await;
    assert_eq!(r["hits"][0]["ref_id"], ZEBRA, "{r}"); // "biting" in paragraph 2
    assert_eq!(r["hits"][0]["url"], "/item/100#p2");
    let (_, r) = json(&app, "/api/search?q=zebra&edition=kids").await;
    assert_eq!(r["hits"][0]["ref_id"], KIDS);
    let (_, r) = json(&app, "/api/search?q=zebra&kind=dictionary").await;
    assert_eq!(r["hits"][0]["url"], "/dictionary/DICTA/9001");
    let (_, r) = json(&app, "/api/search?q=zebra%20xylophone").await;
    assert_eq!(r["mode"], "any");
    let (_, r) = json(&app, "/api/search?q=strpes").await;
    assert_eq!(r["did_you_mean"], "stripes");

    let (_, s) = json(&app, "/api/suggest?q=ze").await;
    assert_eq!(s["suggestions"][0]["ref_id"], ZEBRA, "{s}");
    let (_, s) = json(&app, "/api/suggest?q=equus").await;
    assert_eq!(s["suggestions"][0]["display"], "Equus quagga");
    assert_eq!(s["suggestions"][0]["title"], "Zebra");
    let (_, s) = json(&app, "/api/suggest?q=zebras&edition=kids").await;
    assert_eq!(s["suggestions"][0]["ref_id"], KIDS);

    let (_, sp) = json(&app, "/api/spell?q=zebr").await;
    assert_eq!(sp["corrected"], "zebra");
    let (_, sp) = json(&app, "/api/spell?q=horse").await;
    assert!(sp["corrected"].is_null());
}

fn range(uri: &str, r: &str) -> Request<Body> {
    Request::get(uri).header(header::RANGE, r).body(Body::empty()).unwrap()
}

#[tokio::test]
async fn media_and_ranges() {
    let f = fixture();
    let app = app(&f.0.config());
    let full = jpeg(5000);

    let res = get(&app, "/media/file/zz000001.jpg").await;
    assert_eq!(res.status(), 200);
    assert_eq!(res.headers()[header::CONTENT_TYPE], "image/jpeg");
    assert_eq!(res.headers()[header::CACHE_CONTROL], "public, max-age=31536000, immutable");
    assert_eq!(res.headers()[header::CONTENT_LENGTH], "5000");
    let etag = res.headers()[header::ETAG].to_str().unwrap().to_string();
    assert_eq!(bytes(res).await, full);

    let res = send(&app, range("/media/file/zz000001.jpg", "bytes=100-199")).await;
    assert_eq!(res.status(), 206);
    assert_eq!(res.headers()[header::CONTENT_RANGE], "bytes 100-199/5000");
    assert_eq!(bytes(res).await, &full[100..200]);
    // Suffix range, open range, and an end past the file: clamped, not 416.
    let res = send(&app, range("/media/file/zz000001.jpg", "bytes=-10")).await;
    assert_eq!(res.headers()[header::CONTENT_RANGE], "bytes 4990-4999/5000");
    let res = send(&app, range("/media/file/ZZ000001.JPG", "bytes=4000-16383")).await;
    assert_eq!(res.status(), 206);
    assert_eq!(res.headers()[header::CONTENT_RANGE], "bytes 4000-4999/5000");
    assert_eq!(bytes(res).await, &full[4000..]);
    let res = send(&app, range("/media/file/zz000001.jpg", "bytes=6000-")).await;
    assert_eq!(res.status(), 416);
    assert_eq!(res.headers()[header::CONTENT_RANGE], "bytes */5000");

    let res = send(&app, Request::get("/media/file/zz000001.jpg").header(header::IF_NONE_MATCH, &etag).body(Body::empty()).unwrap()).await;
    assert_eq!(res.status(), 304);
    let res = send(&app, Request::head("/media/file/zz000001.jpg").body(Body::empty()).unwrap()).await;
    assert_eq!(res.status(), 200);
    assert_eq!(res.headers()[header::CONTENT_LENGTH], "5000");
    assert!(bytes(res).await.is_empty());

    // Accept negotiation picks the smaller WebP; original=1 opts out.
    let neg = Request::get("/media/file/zz000001.jpg").header(header::ACCEPT, "image/avif,image/webp,*/*").body(Body::empty()).unwrap();
    let res = send(&app, neg).await;
    assert_eq!(res.headers()[header::CONTENT_TYPE], "image/webp");
    assert_eq!(res.headers()[header::VARY], "Accept");
    let neg = Request::get("/media/file/zz000001.jpg?original=1").header(header::ACCEPT, "image/webp").body(Body::empty()).unwrap();
    assert_eq!(send(&app, neg).await.headers()[header::CONTENT_TYPE], "image/jpeg");

    // Paths with folders resolve by their last segment; content-addressed variants.
    assert_eq!(get(&app, "/media/file/loose/dir/zz000001.jtn").await.status(), 200);
    let res = get(&app, &format!("/media/{SHA_JPG}/webp")).await;
    assert_eq!(res.headers()[header::CONTENT_TYPE], "image/webp");
    assert_eq!(bytes(res).await.len(), 16);
    assert_eq!(get(&app, &format!("/media/{SHA_JPG}/avif")).await.status(), 404);
    assert_eq!(get(&app, "/media/nothex/original").await.status(), 404);
    assert_eq!(get(&app, "/media/file/missing.jpg").await.status(), 404);
}

#[tokio::test]
async fn static_trees_and_spa() {
    let f = fixture();
    let app = app(&f.0.config());

    // Styles: root-relative URLs made absolute for the client's origin.
    let req = Request::get("/atlas/styles/default.json")
        .header(header::HOST, "encarta.example")
        .header("x-forwarded-proto", "https")
        .body(Body::empty())
        .unwrap();
    let style: serde_json::Value = serde_json::from_slice(&bytes(send(&app, req).await).await).unwrap();
    assert_eq!(style["sprite"], "https://encarta.example/atlas/sprites/sprite");
    assert_eq!(style["sources"]["earth"]["url"], "pmtiles://https://encarta.example/atlas/vector/earth.pmtiles");
    // Through /out/atlas/ the URLs keep that prefix (the Vite dev proxy forwards only /out).
    let req = Request::get("/out/atlas/styles/default.json").header(header::HOST, "127.0.0.1:5173").body(Body::empty()).unwrap();
    let style: serde_json::Value = serde_json::from_slice(&bytes(send(&app, req).await).await).unwrap();
    assert_eq!(style["sprite"], "http://127.0.0.1:5173/out/atlas/sprites/sprite");
    assert_eq!(style["sources"]["earth"]["url"], "pmtiles://http://127.0.0.1:5173/out/atlas/vector/earth.pmtiles");
    // PMTiles reads 16 KiB from a 1000-byte archive: 206 with the clamped range.
    let res = send(&app, range("/atlas/vector/earth.pmtiles", "bytes=0-16383")).await;
    assert_eq!(res.status(), 206);
    assert_eq!(res.headers()[header::CONTENT_RANGE], "bytes 0-999/1000");
    assert_eq!(bytes(res).await.len(), 1000);
    let res = send(&app, range("/out/atlas/vector/earth.pmtiles", "bytes=10-19")).await;
    assert_eq!(bytes(res).await, (10..20u8).collect::<Vec<_>>());
    // A client route under /atlas/ gets the SPA.
    let res = get(&app, "/atlas/place/42").await;
    assert_eq!(res.status(), 200);
    assert!(String::from_utf8(bytes(res).await).unwrap().contains("<div id=root>"));

    assert_eq!(json(&app, "/out/tours/0001/scene.json").await.1["id"], "0001");
    assert_eq!(get(&app, "/out/tours/nope.json").await.status(), 404);
    assert_eq!(get(&app, "/out/content.sqlite").await.status(), 404);
    assert_eq!(get(&app, "/out/tours/../content.sqlite").await.status(), 404);

    let res = get(&app, "/assets/app-1a2b3c.js").await;
    assert_eq!(res.headers()[header::CACHE_CONTROL], "public, max-age=31536000, immutable");
    assert_eq!(get(&app, "/assets/missing.js").await.status(), 404);
    let res = get(&app, "/favicon.svg").await;
    assert_eq!(res.headers()[header::CACHE_CONTROL], "no-cache");
    for p in ["/", "/item/100", "/dictionary/DICTA/9001"] {
        let res = get(&app, p).await;
        assert_eq!(res.status(), 200, "{p}");
        assert_eq!(res.headers()[header::CONTENT_TYPE], "text/html; charset=utf-8");
        assert_eq!(res.headers()[header::CACHE_CONTROL], "no-cache");
    }
    // Compression for JSON, never for ranges.
    let req = Request::get("/api/stats").header(header::ACCEPT_ENCODING, "gzip").body(Body::empty()).unwrap();
    let req2 = Request::get(format!("/api/article/{ZEBRA}")).header(header::ACCEPT_ENCODING, "br").body(Body::empty()).unwrap();
    assert_eq!(send(&app, req2).await.headers()[header::CONTENT_ENCODING], "br");
    let _ = send(&app, req).await;
    let req = Request::get("/atlas/vector/earth.pmtiles").header(header::ACCEPT_ENCODING, "gzip").header(header::RANGE, "bytes=0-9").body(Body::empty()).unwrap();
    assert!(send(&app, req).await.headers().get(header::CONTENT_ENCODING).is_none());
}

#[tokio::test]
async fn empty_data_dir_still_serves_health_and_spa() {
    let dir = tempfile::tempdir().unwrap();
    let web = dir.path().join("web");
    spa(&web);
    let mut cfg = Config::new(dir.path().join("data"));
    cfg.static_dir = Some(web);
    let app = app(&cfg);
    assert_eq!(get(&app, "/healthz").await.status(), 200);
    assert_eq!(get(&app, "/item/1").await.status(), 200);
    assert_eq!(json(&app, "/api/item/1").await.0, 503);
    assert_eq!(json(&app, "/api/search?q=x").await.0, 503);
    assert_eq!(json(&app, "/api/meta").await.0, 200);
    assert_eq!(get(&app, "/media/file/x.jpg").await.status(), 404);
    assert_eq!(get(&app, "/out/atlas/x.pmtiles").await.status(), 404);
    // Without a SPA either: health still fine, others 404.
    let app = common::app(&Config::new(dir.path().join("nothing")));
    assert_eq!(get(&app, "/healthz").await.status(), 200);
    assert_eq!(get(&app, "/").await.status(), 404);
}

#[tokio::test]
async fn media_manifest_without_content_db() {
    let f = Fixture::build();
    std::fs::remove_file(f.out.join("content.sqlite")).unwrap();
    let app = app(&f.config());
    assert_eq!(get(&app, "/media/file/zz000001.jpg").await.status(), 200);
    assert_eq!(json(&app, "/api/item/100").await.0, 503);
}

#[tokio::test]
async fn disc_tree_and_fonts() {
    let f = fixture();
    let app = app(&f.0.config());

    // The dswMedia tree by path, any case; an .iax in an IAF folder resolves to the baggage file.
    let res = get(&app, "/media/disc/deluxe/dswmedia/iaf/a/zz/SLIDE.JPG").await;
    assert_eq!(res.status(), 200);
    assert_eq!(res.headers()[header::CONTENT_TYPE], "image/jpeg");
    assert_eq!(bytes(res).await, jpeg(5000));
    let res = get(&app, "/media/disc/kids/dswMedia/shared/players/IAFPlayer/base.swf").await;
    assert_eq!(res.headers()[header::CONTENT_TYPE], "application/x-shockwave-flash");
    let res = get(&app, "/media/disc/deluxe/dswMedia/IAF/a/zz/zz.iax").await;
    assert_eq!(res.status(), 200);
    assert_eq!(String::from_utf8(bytes(res).await).unwrap(), IAX);
    // Flash's loadMovie with variables POSTs.
    let res = send(&app, Request::post("/media/disc/deluxe/dswMedia/shared/players/IAFPlayer/base.swf").body(Body::from("a=1")).unwrap()).await;
    assert_eq!(res.status(), 200);
    assert_eq!(get(&app, "/media/disc/deluxe/dswMedia/nope.swf").await.status(), 404);

    let (_, meta) = json(&app, "/api/meta").await;
    assert_eq!(meta["fonts"], serde_json::json!(["/fonts/REFSAN.TTF"]));
    let res = get(&app, "/fonts/refsan.ttf").await;
    assert_eq!(res.status(), 200);
    assert_eq!(res.headers()[header::CONTENT_TYPE], "font/ttf");
    assert_eq!(get(&app, "/fonts/other.ttf").await.status(), 404);
}
