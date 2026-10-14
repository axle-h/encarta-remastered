//! A tiny synthetic `out/` tree (content database, media manifest + files, search index, atlas
//! and tour files, SPA) written from scratch for the tests. No disc data.

#![allow(dead_code)]

use axum::Router;
use axum::body::Body;
use axum::http::{Request, Response};
use encarta_server::{AppState, Config, router};
use http_body_util::BodyExt;
use rusqlite::{Connection, params};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tower::ServiceExt;

pub const SCHEMA: &str = "
CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT);
CREATE TABLE item (ref_id INTEGER PRIMARY KEY, kind TEXT NOT NULL, source_group TEXT, title TEXT, title_html TEXT, sort_title TEXT, display_title_html TEXT, full_title_html TEXT, alt_title_html TEXT, in_deluxe INTEGER NOT NULL, in_kids INTEGER NOT NULL, content_package TEXT, data_package TEXT, related_package TEXT, media_type TEXT, subgroup TEXT, subtype TEXT, style INTEGER, geocode INTEGER, origin TEXT, revision INTEGER, word_count INTEGER, caption_html TEXT, credit TEXT, credit_html TEXT, preview_html TEXT, meta_json TEXT);
CREATE TABLE article (ref_id INTEGER PRIMARY KEY, source TEXT NOT NULL, source_file TEXT, body_json TEXT NOT NULL, html TEXT NOT NULL, outline_json TEXT, paragraph_count INTEGER, section_count INTEGER, word_count INTEGER, title_ref_count INTEGER, layout TEXT NOT NULL DEFAULT 'encyclopedia');
CREATE TABLE section (ref_id INTEGER, section_id INTEGER, parent_id INTEGER, level INTEGER, number TEXT, title TEXT, type_code INTEGER, PRIMARY KEY(ref_id, section_id));
CREATE TABLE paragraph (ref_id INTEGER, kind TEXT, para_id INTEGER, section_id INTEGER, ordinal INTEGER, text TEXT, PRIMARY KEY(ref_id, kind, para_id));
CREATE TABLE link (ref_id INTEGER, para_id INTEGER, ordinal INTEGER, kind TEXT, target_ref_id INTEGER, target_para INTEGER, url TEXT, text TEXT);
CREATE TABLE assoc (ref_id INTEGER, direction TEXT, ordinal INTEGER, target_ref_id INTEGER, grp TEXT, sec INTEGER, sequence INTEGER, appears INTEGER, par INTEGER, reviewed INTEGER);
CREATE TABLE item_file (ref_id INTEGER, ordinal INTEGER, role TEXT, file_name TEXT, raw_path TEXT, width INTEGER, height INTEGER, src_ref_id INTEGER, disc INTEGER, package_file TEXT);
CREATE TABLE item_child (ref_id INTEGER, ordinal INTEGER, kind TEXT, child_ref_id INTEGER, sequence INTEGER);
CREATE TABLE data_table (ref_id INTEGER PRIMARY KEY, kind TEXT, json TEXT, html TEXT);
CREATE TABLE related_tree (ref_id INTEGER, editions TEXT, tree_json TEXT, node_count INTEGER, PRIMARY KEY(ref_id, editions));
CREATE TABLE index_tree (ref_id INTEGER, editions TEXT, title TEXT, tree_json TEXT, node_count INTEGER, PRIMARY KEY(ref_id, editions));
CREATE TABLE alias (ref_id INTEGER, edition TEXT, collection INTEGER, class_id INTEGER, sort_title TEXT, display_title TEXT, display_html TEXT);
CREATE TABLE dictionary (name TEXT PRIMARY KEY, kind TEXT, title TEXT, uniq_tag TEXT, lex_file TEXT, guid TEXT, parent_guid TEXT, entry_count INTEGER, signature_json TEXT);
CREATE TABLE dict_entry (dictionary TEXT, ref_id INTEGER, headword TEXT, variety TEXT, summary TEXT, body_json TEXT, html TEXT, PRIMARY KEY(dictionary, ref_id));
CREATE TABLE dict_form (form TEXT, dictionary TEXT, ref_id INTEGER, kind TEXT);
CREATE TABLE dict_audio (dictionary TEXT, ref_id INTEGER, file_name TEXT, audio_ref_id INTEGER);
CREATE TABLE timeline_category (bit INTEGER PRIMARY KEY, name TEXT);
CREATE TABLE timeline_region (bit INTEGER PRIMARY KEY, name TEXT);
CREATE TABLE atlas_media (ref_id INTEGER PRIMARY KEY, kind TEXT, title TEXT, title_html TEXT, place_id INTEGER, file TEXT, thumbnail TEXT, paired_ref_id INTEGER, legacy_id INTEGER, credit TEXT, other TEXT);
CREATE TABLE timeline_term (term_id INTEGER PRIMARY KEY, text TEXT, text_html TEXT, ref_ids TEXT);
CREATE TABLE timeline_event (seq INTEGER PRIMARY KEY, ref_id INTEGER, source_table TEXT, kind INTEGER, title TEXT, title_html TEXT, start_year INTEGER, start_month INTEGER, start_day INTEGER, start_circa INTEGER, end_year INTEGER, end_month INTEGER, end_day INTEGER, end_circa INTEGER, span INTEGER, importance INTEGER, category_mask INTEGER, region_mask INTEGER, categories TEXT, regions TEXT, raw_flags INTEGER, display TEXT, label TEXT, date_text TEXT, start_text TEXT, end_text TEXT, prominence INTEGER);
CREATE TABLE mindmaze_question (id INTEGER PRIMARY KEY, category INTEGER, category_label TEXT, level INTEGER, clue TEXT, clue_html TEXT, location INTEGER);
CREATE TABLE mindmaze_answer (question_id INTEGER, ordinal INTEGER, correct INTEGER, text TEXT, text_html TEXT, article_ref_id INTEGER, article_exists INTEGER);
CREATE TABLE stat_country (id INTEGER PRIMARY KEY, name TEXT);
CREATE TABLE stat_category (bit INTEGER PRIMARY KEY, name TEXT);
CREATE TABLE stat_unit (id INTEGER PRIMARY KEY, unit TEXT, alt_unit TEXT, factor REAL, offset REAL);
CREATE TABLE stat_indicator (id INTEGER PRIMARY KEY, name TEXT, category_mask INTEGER, categories TEXT, unit_id INTEGER, content_ref_id INTEGER, source TEXT);
CREATE TABLE stat_value (indicator_id INTEGER, country_id INTEGER, rank INTEGER, value REAL, value_text TEXT, decimals INTEGER, col_b INTEGER, year INTEGER, col_c INTEGER, note TEXT, year_end INTEGER, period TEXT);
CREATE TABLE place_link (place_id INTEGER, ordinal INTEGER, kind INTEGER, kind_label TEXT, content_ref_id INTEGER);
CREATE TABLE lexicon (name TEXT PRIMARY KEY, file TEXT, lcids TEXT, word_count INTEGER, used_by TEXT);
CREATE TABLE lexicon_word (lexicon TEXT, word TEXT);
CREATE TABLE interactive (file_name TEXT, edition TEXT, package TEXT, control_type TEXT, title TEXT, json TEXT, PRIMARY KEY(file_name, edition));
CREATE TABLE quiz (quiz_id INTEGER, edition TEXT, quiz_type TEXT, lang TEXT, json TEXT, PRIMARY KEY(quiz_id, edition));
";

pub const MANIFEST: &str = "
CREATE TABLE file(container TEXT NOT NULL, package TEXT NOT NULL, member_path TEXT NOT NULL, name TEXT NOT NULL, sha256 TEXT NOT NULL, size INTEGER NOT NULL, mime TEXT NOT NULL, kind TEXT NOT NULL, format TEXT NOT NULL, disposition TEXT NOT NULL, PRIMARY KEY(container, member_path));
CREATE INDEX file_name ON file(name);
CREATE INDEX file_sha ON file(sha256);
CREATE TABLE derivative(sha256 TEXT NOT NULL, format TEXT NOT NULL, path TEXT NOT NULL, width INTEGER, height INTEGER, duration_ms INTEGER, bytes INTEGER NOT NULL, out_sha256 TEXT NOT NULL, recipe TEXT NOT NULL, meta TEXT, PRIMARY KEY(sha256, format));
CREATE TABLE catalog_key(edition TEXT NOT NULL, tbl TEXT NOT NULL, key TEXT NOT NULL, package_index INTEGER NOT NULL, package_file TEXT, local_id INTEGER NOT NULL, container TEXT, member_path TEXT, sha256 TEXT, PRIMARY KEY(edition, tbl, key));
CREATE TABLE iax_ref(iax_sha256 TEXT NOT NULL, element TEXT NOT NULL, ref TEXT NOT NULL, resolution TEXT NOT NULL, container TEXT, member_path TEXT, sha256 TEXT);
";

pub const ZEBRA: i64 = 100;
pub const HORSE: i64 = 101;
pub const PHOTO: i64 = 200;
pub const FACTS: i64 = 300;
pub const KIDS: i64 = 400;
pub const EVENT: i64 = 500;
pub const SIDEBAR: i64 = 600;
pub const INDEX_TERM: i64 = 210000001;
pub const SHA_JPG: &str = "aa00000000000000000000000000000000000000000000000000000000000001";
pub const SHA_THUMB: &str = "bb00000000000000000000000000000000000000000000000000000000000002";
pub const SHA_IAX: &str = "cc00000000000000000000000000000000000000000000000000000000000003";
pub const SHA_SWF: &str = "dd00000000000000000000000000000000000000000000000000000000000004";
/// A synthetic `.iax` (the interactive descriptor dialect, invented content).
pub const IAX: &str = "<iai id=\"zz\" basecontroltype=\"SlideShowControl\" lang=\"A\"><slides><slide><image>Slide.jpg</image></slide></slides></iai>";

/// A fake JPEG of `n` bytes (SOI marker, then a counter pattern).
pub fn jpeg(n: usize) -> Vec<u8> {
    let mut v = vec![0xFF, 0xD8, 0xFF, 0xE0];
    v.extend((0..n - 4).map(|i| (i % 251) as u8));
    v
}

fn content(path: &Path) {
    let c = Connection::open(path).unwrap();
    c.execute_batch(SCHEMA).unwrap();
    let item = |id: i64, kind: &str, title: &str, deluxe: i64, kids: i64, caption: Option<&str>, meta: Option<&str>| {
        c.execute(
            "INSERT INTO item (ref_id, kind, source_group, title, title_html, sort_title, in_deluxe, in_kids, caption_html, meta_json, media_type)
             VALUES (?1, ?2, ?2, ?3, ?3, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![id, kind, title, deluxe, kids, caption, meta, if kind == "media" { Some("image") } else { None }],
        )
        .unwrap();
    };
    item(ZEBRA, "article", "Zebra", 1, 0, None, Some(r#"{"note":"synthetic"}"#));
    item(HORSE, "article", "Horse", 1, 0, None, None);
    item(PHOTO, "media", "Plains Zebra Herd", 1, 0, Some("A herd of <em>striped</em> grazers."), None);
    item(FACTS, "facts", "Zebra", 1, 0, None, None);
    item(KIDS, "article", "Zebras for Kids", 0, 1, None, None);
    item(EVENT, "timeline-event", "Stripes Explained", 1, 0, None, None);
    // An index term (DATAF): a search alias of its targets and an expandable search result.
    item(INDEX_TERM, "index-term", "Hippotigris", 1, 0, None, None);
    c.execute(
        "INSERT INTO index_tree VALUES (?1, 'STE,STC,DLX,STD', 'Hippotigris', ?2, 3)",
        params![INDEX_TERM, r#"[{"title":["Hippotigris"],"children":[{"group":"article","ref":100,"para":2,"pn":1,"title":["zebra"]},{"group":"article","ref":101,"title":["Horse"]}]}]"#],
    )
    .unwrap();
    // Schema 2 body: `title-ref` parts, a margin cluster and a text-column picture.
    let body = r#"[{"t":"section","id":1,"kind":"introduction","level":1,"number":["I"],"title":["INTRODUCTION"],"blocks":[{"t":"cluster","col":"margin","items":[{"ref":200,"group":"media","seq":1},{"ref":300,"group":"factsandfigures","seq":2}]},{"t":"p","id":1,"c":[{"t":"title-ref","title":["Zebra"],"alt":["Equus quagga"],"c":["Zebra"]},", a striped grazing animal."]}]},{"t":"section","id":2,"kind":"major","level":1,"number":["II"],"title":"STRIPES","blocks":[{"t":"media","ref":200,"group":"media","seq":3,"col":"text"},{"t":"p","id":2,"c":["The stripes may confuse biting flies."]}]}]"#;
    let html = r#"<article class="enc-article" data-ref="100"><p id="p1">Zebra, a striped grazing animal.</p><img src="/media/file/zz000001.jtn"></article>"#;
    c.execute(
        "INSERT INTO article VALUES (?1, 'content', NULL, ?2, ?3, '[{\"id\":1,\"t\":\"p\"}]', 2, 2, 12, 1, 'encyclopedia')",
        params![ZEBRA, body, html],
    )
    .unwrap();
    // The Kids article ends with its tile grid (`media-grid`).
    let kids_body = r#"[{"t":"p","id":1,"c":["Zebras live in Africa and eat grass."]},{"t":"media-grid","col":"grid","items":[{"ref":200,"group":"media","seq":1}]}]"#;
    for (id, t, body, layout) in [
        (HORSE, "Horse, a hoofed animal ridden by people.", "[]", "encyclopedia"),
        (KIDS, "Zebras live in Africa and eat grass.", kids_body, "kids"),
    ] {
        c.execute("INSERT INTO article VALUES (?1, 'content', NULL, ?2, '<article></article>', NULL, 1, 0, 5, 0, ?3)", params![id, body, layout]).unwrap();
        c.execute("INSERT INTO paragraph VALUES (?1, 'p', 1, NULL, 1, ?2)", params![id, t]).unwrap();
    }
    c.execute("INSERT INTO article VALUES (?1, 'content', NULL, '[]', '<article></article>', NULL, 1, 0, 5, 0, 'encyclopedia')", [EVENT]).unwrap();
    // A sidebar: its own layout, previewed in the Horse article's teaser.
    item(SIDEBAR, "sidebar", "Stripes Through History", 1, 0, None, Some(r#"{"sidebar_category":"WORLD OF SCIENCE"}"#));
    c.execute("UPDATE item SET preview_html = '<p class=\"enc-intro\">Why stripes?</p>' WHERE ref_id = ?1", [SIDEBAR]).unwrap();
    c.execute(
        "INSERT INTO article VALUES (?1, 'sidebar-xml', 'zz.xml', '[{\"t\":\"intro\",\"id\":1,\"c\":[\"Why stripes?\"]}]', '<article></article>', NULL, 1, 0, 2, 0, 'sidebar')",
        [SIDEBAR],
    )
    .unwrap();
    c.execute("INSERT INTO paragraph VALUES (?1, 'p', 1, NULL, 1, 'A naturalist proposes why zebras have stripes.')", [EVENT]).unwrap();
    c.execute_batch(
        "INSERT INTO section VALUES (100, 1, NULL, 1, 'I', 'INTRODUCTION', 4), (100, 2, NULL, 1, 'II', 'STRIPES', 5);
         INSERT INTO paragraph VALUES (100, 'p', 1, 1, 1, 'Zebra, a striped grazing animal.'),
                                      (100, 'p', 2, 2, 2, 'The stripes may confuse biting flies that carry disease.');
         INSERT INTO link VALUES (101, 1, 0, 'qv', 100, NULL, NULL, 'zebra'), (400, 1, 0, 'qv', 100, NULL, NULL, 'zebra');
         INSERT INTO assoc VALUES (100, 'forward', 0, 200, 'media', 1, 1, NULL, NULL, 0),
                                  (100, 'forward', 1, 300, 'factsandfigures', 1, 2, NULL, NULL, 0),
                                  (200, 'reverse', 0, 100, 'article', NULL, 1, NULL, NULL, 0),
                                  (101, 'forward', 0, 600, 'sidebar', 1, 1, NULL, NULL, 0);
         INSERT INTO item_file VALUES (200, 0, 'thumb', 'zz000001.jtn', NULL, 64, 64, NULL, NULL, NULL),
                                      (200, 1, 'image', 'zz000001.jpg', NULL, 640, 480, NULL, NULL, NULL),
                                      (200, 2, 'picon', 'zz000001.jsm', NULL, 192, 144, NULL, NULL, NULL);
         INSERT INTO item_child VALUES (100, 0, 'media', 200, 1);
         INSERT INTO data_table VALUES (300, 'facts', '{\"entries\":[{\"name\":[\"Order\"],\"value\":[\"Perissodactyla\"]}]}', '<div class=\"enc-facts\"></div>');
         INSERT INTO related_tree VALUES (100, 'DLX,STE', '[{\"title\":[\"Horse\"],\"ref\":101}]', 1), (100, 'KDC', '[{\"title\":[\"Kids horse\"],\"ref\":101}]', 1);
         INSERT INTO alias VALUES (100, 'deluxe', 0, 1, 'Zebra', 'Zebra', 'Zebra'),
                                  (100, 'deluxe', 0, 1, 'Equus quagga', 'Equus quagga', '<em>Equus quagga</em>'),
                                  (101, 'deluxe', 0, 1, 'Horse', 'Horse', 'Horse'),
                                  (200, 'deluxe', 1, 256, 'Zebra Herd', 'Plains Zebra Herd', 'Plains Zebra Herd'),
                                  (400, 'kids', 0, 1, 'Zebras for Kids', 'Zebras for Kids', 'Zebras for Kids');
         INSERT INTO dictionary VALUES ('DICTA', 'dictionary', 'English', 'DA', 'DICTA', 'G1', 'G0', 2, '{}'),
                                       ('ENG_FRA', 'bilingual', 'English-French', 'BAF', 'BILAF', 'G2', 'G9', 1, '{}');
         INSERT INTO dict_entry VALUES ('DICTA', 9001, 'zebra', 'US', 'striped wild horse', '{\"c\":[]}', '<div class=\"enc-dict\">zebra</div>'),
                                       ('DICTA', 9002, 'zebu', 'US', 'humped ox', '{}', '<div>zebu</div>'),
                                       ('DICTA', 9003, 'yak', 'US', 'shaggy ox', '{}', '<div>yak</div>'),
                                       ('DICTA', 9004, 'ze-bra crossing', 'UK', 'a road crossing', '{}', '<div>crossing</div>'),
                                       ('ENG_FRA', 9101, 'zebra', NULL, 'zèbre', '{}', '<div>zèbre</div>');
         INSERT INTO dict_form VALUES ('zebra', 'DICTA', 9001, 'headword'), ('zebras', 'DICTA', 9001, 'inflection'),
                                      ('zebu', 'DICTA', 9002, 'headword'), ('Zebra', 'ENG_FRA', 9101, 'headword'),
                                      ('yak', 'DICTA', 9003, 'headword'), ('yaks', 'DICTA', 9003, 'inflection'),
                                      ('ze-bra crossing', 'DICTA', 9004, 'headword');
         INSERT INTO dict_audio VALUES ('DICTA', 9001, 'zz000009.wav', 77);
         INSERT INTO timeline_category VALUES (1, 'Science and Technology'), (2, 'Political History');
         INSERT INTO timeline_region VALUES (1, 'Europe'), (2, 'Africa');
         INSERT INTO timeline_event VALUES (1, 500, 'event', 1, 'Stripes Explained', 'Stripes Explained', 1890, 5, 1, 0, NULL, NULL, NULL, NULL, 0, 3, 1, 2, 'Science and Technology', 'Africa', 0,
                                               'point', NULL, 'May 1, 1890', 'May 1, 1890', NULL, 3),
                                           (2, 501, 'event', 1, 'A Treaty', 'A Treaty', -50, NULL, NULL, 1, -40, NULL, NULL, 0, 10, 5, 2, 1, 'Political History', 'Europe', 3,
                                               'period', 'A TREATY', '50? BC - 40 BC', '50? BC', '40 BC', 1);
         INSERT INTO timeline_term VALUES (0, 'Stripes', 'Stripes', '[500]'), (1, 'Treaties', 'Treaties', '[501]');
         INSERT INTO mindmaze_question VALUES (1, 0, 'Life science', 1, 'Striped horse of Africa.', 'Striped horse of Africa.', 1),
                                              (2, 0, 'Life science', 2, 'Animal ridden by knights.', 'Animal ridden by knights.', 1);
         INSERT INTO mindmaze_answer VALUES (1, 0, 1, 'Zebra', 'Zebra', 100, 1), (1, 1, 0, 'Horse', 'Horse', 101, 1),
                                            (2, 0, 1, 'Horse', 'Horse', 101, 1), (2, 1, 0, 'Zebra', 'Zebra', 100, 1);
         INSERT INTO stat_country VALUES (1, 'Kenya'), (2, 'Tanzania');
         INSERT INTO stat_category VALUES (1, 'Basic Indicators');
         INSERT INTO stat_unit VALUES (7, 'head', NULL, 0, 0);
         INSERT INTO stat_indicator VALUES (1, 'Zebra population', 1, 'Basic Indicators', 7, NULL, 'Synthetic survey');
         INSERT INTO stat_value VALUES (1, 2, 1, 900.0, '900', 0, 0, 2000, 0, NULL, 2000, '2000'), (1, 1, 2, 800.0, '800', 0, 0, 1990, 15, NULL, 2005, '1990-2005');
         INSERT INTO place_link VALUES (42, 0, 1, 'article', 100), (42, 1, 14, 'flag', 200), (42, 2, 15, 'anthem', 200),
                                       (42, 3, 1, 'article', 999001), (42, 4, 5, 'photo:people', 800), (42, 5, 9, 'music', 801);
         INSERT INTO atlas_media VALUES (800, 'i', 'Kenyan Herders', 'Kenyan Herders', 42, 'ZZ000001.JPG', 'zz000001.jtn', NULL, 7, 'Synthetic Photo Agency', NULL),
                                        (801, 'w', 'Kenya: Herding Song', 'Kenya: Herding Song', 42, 'zz000801.wav', NULL, NULL, NULL, 'Synthetic Records', NULL),
                                        (802, 'i', 'Herders at Dusk', 'Herders at Dusk', 42, 'yy000002.jpg', 'yy000002.jsm', NULL, NULL, 'Synthetic Photo Agency', NULL);
         INSERT INTO lexicon VALUES ('EESPLA01', 'x', '0x0409', 3, 'speller'), ('DICTA', 'y', '0x0409', 3, 'dict');
         INSERT INTO lexicon_word VALUES ('EESPLA01', 'Kenya'), ('DICTA', 'zebra'), ('DICTA', 'zebu'), ('DICTA', 'stripes'), ('DICTA', 'horse');
         INSERT INTO quiz VALUES (7, 'deluxe', 'mc', 'en', '{\"q\":1}');
         INSERT INTO interactive VALUES ('zz.iax', 'deluxe', 'SW.EIT', 'slideshow', 'Slides', '{\"@id\":\"zz\"}');
         INSERT INTO meta VALUES ('schema_version', '2');",
    )
    .unwrap();
}

fn manifest(out: &Path) {
    let media = out.join("media");
    std::fs::create_dir_all(media.join("o/aa")).unwrap();
    std::fs::create_dir_all(media.join("d/aa")).unwrap();
    std::fs::create_dir_all(media.join("o/bb")).unwrap();
    std::fs::create_dir_all(media.join("d/bb")).unwrap();
    std::fs::write(media.join(format!("o/aa/{SHA_JPG}.jpg")), jpeg(5000)).unwrap();
    std::fs::write(media.join(format!("d/aa/{SHA_JPG}.webp")), b"RIFF0000WEBPVP8 ").unwrap(); // 16 bytes
    std::fs::write(media.join(format!("o/bb/{SHA_THUMB}.jpg")), jpeg(300)).unwrap();
    let c = Connection::open(media.join("manifest.sqlite")).unwrap();
    c.execute_batch("PRAGMA journal_mode = WAL;").unwrap();
    c.execute_batch(MANIFEST).unwrap();
    for (sha, name, size) in [(SHA_JPG, "zz000001.jpg", 5000), (SHA_THUMB, "zz000001.jtn", 300)] {
        c.execute(
            "INSERT INTO file VALUES ('CONTENT/MD.EIT', 'MD.EIT', ?1, ?2, ?3, ?4, 'image/jpeg', 'image', 'jpeg', 'converted')",
            params![format!("baggage/{name}"), name, sha, size],
        )
        .unwrap();
        c.execute(
            "INSERT INTO catalog_key VALUES ('deluxe', 'baggage', ?1, 1, 'MD.EIT', 1, 'CONTENT/MD.EIT', ?2, ?3)",
            params![name, format!("baggage/{name}"), sha],
        )
        .unwrap();
        let p = &sha[..2];
        c.execute(
            "INSERT INTO derivative VALUES (?1, 'original', ?2, 640, 480, NULL, ?3, ?1, 'copy', NULL)",
            params![sha, format!("o/{p}/{sha}.jpg"), size],
        )
        .unwrap();
    }
    c.execute(
        "INSERT INTO derivative VALUES (?1, 'webp', ?2, 640, 480, NULL, 16, 'x', 'libwebp', NULL)",
        params![SHA_JPG, format!("d/aa/{SHA_JPG}.webp")],
    )
    .unwrap();
    // A Flash interactive: the `.iax` in baggage, the shared player and a slide in its IAF folder.
    std::fs::create_dir_all(media.join("o/cc")).unwrap();
    std::fs::create_dir_all(media.join("o/dd")).unwrap();
    std::fs::write(media.join(format!("o/cc/{SHA_IAX}.xml")), IAX).unwrap();
    std::fs::write(media.join(format!("o/dd/{SHA_SWF}.swf")), b"FWS\x06synthetic movie").unwrap();
    c.execute_batch(&format!(
        "INSERT INTO file VALUES ('CONTENT/SWDLX01.EIT', 'SWDLX01.EIT', 'baggage/zz.iax', 'zz.iax', '{SHA_IAX}', {n}, 'application/xml', 'interactive', 'xml', 'kept'),
                                 ('CONTENT/SWDLX01.EIT', 'SWDLX01.EIT', 'dswMedia/IAF/a/zz/Slide.jpg', 'slide.jpg', '{SHA_JPG}', 5000, 'image/jpeg', 'image', 'jpeg', 'converted'),
                                 ('EE/ENCARTA/SWSHARED.EIT', 'SWSHARED.EIT', 'dswMedia/shared/players/IAFPlayer/base.swf', 'base.swf', '{SHA_SWF}', 19, 'application/x-shockwave-flash', 'flash', 'swf', 'kept'),
                                 ('EE/KIDS/SWSHARED.EIT', 'SWSHARED.EIT', 'dswMedia/shared/players/IAFPlayer/base.swf', 'base.swf', '{SHA_SWF}', 19, 'application/x-shockwave-flash', 'flash', 'swf', 'kept');
         INSERT INTO catalog_key VALUES ('deluxe', 'baggage', 'zz.iax', 2, 'SWDLX01.EIT', 1, 'CONTENT/SWDLX01.EIT', 'baggage/zz.iax', '{SHA_IAX}');
         INSERT INTO derivative VALUES ('{SHA_IAX}', 'original', 'o/cc/{SHA_IAX}.xml', NULL, NULL, NULL, {n}, '{SHA_IAX}', 'copy', NULL),
                                       ('{SHA_SWF}', 'original', 'o/dd/{SHA_SWF}.swf', NULL, NULL, NULL, 19, '{SHA_SWF}', 'copy', NULL);
         INSERT INTO iax_ref VALUES ('{SHA_IAX}', 'image', 'Slide.jpg', 'iaf-folder', 'CONTENT/SWDLX01.EIT', 'dswMedia/IAF/a/zz/Slide.jpg', '{SHA_JPG}');
         CREATE TABLE atlas_file(name TEXT PRIMARY KEY, role TEXT NOT NULL, ref_id INTEGER NOT NULL, records INTEGER NOT NULL, linked INTEGER NOT NULL,
                                 sha256 TEXT, resolution TEXT NOT NULL, via TEXT, score REAL);
         INSERT INTO atlas_file VALUES ('yy000002.jpg', 'image', 802, 1, 0, '{SHA_JPG}', 'crop', 'zz000001.jpg', 0.97),
                                       ('zz000801.wav', 'audio', 801, 1, 1, NULL, 'absent', NULL, NULL);",
        n = IAX.len()
    ))
    .unwrap();
    drop(c); // checkpoints and removes -wal/-shm
}

fn atlas_and_tours(out: &Path) {
    let atlas = out.join("atlas");
    std::fs::create_dir_all(atlas.join("styles")).unwrap();
    std::fs::create_dir_all(atlas.join("vector")).unwrap();
    std::fs::write(
        atlas.join("styles/default.json"),
        r#"{"version":8,"sprite":"/atlas/sprites/sprite","glyphs":"/atlas/glyphs/{fontstack}/{range}.pbf",
            "sources":{"earth":{"type":"vector","url":"pmtiles:///atlas/vector/earth.pmtiles"}},"layers":[]}"#,
    )
    .unwrap();
    std::fs::write(atlas.join("vector/earth.pmtiles"), (0..1000u32).map(|i| i as u8).collect::<Vec<_>>()).unwrap();
    let g = Connection::open(atlas.join("gazetteer.sqlite")).unwrap();
    g.execute_batch(
        "CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT);
         CREATE TABLE place (entity INTEGER PRIMARY KEY, name TEXT, importance REAL, lat REAL, lon REAL, classes TEXT,
                             body TEXT NOT NULL DEFAULT 'earth', goto_km REAL, west REAL, south REAL, east REAL, north REAL);
         CREATE VIEW place_full AS SELECT p.*, 'City' AS feature_type, 'Kenya' AS context,
                                          CASE WHEN p.body = 'moon' THEN 'MOON - crater' ELSE 'PPL - city' END AS class_name FROM place p;
         CREATE TABLE place_name (id INTEGER PRIMARY KEY, entity INTEGER, name TEXT, kind TEXT);
         CREATE VIRTUAL TABLE place_fts USING fts5(name, entity UNINDEXED, tokenize = 'unicode61 remove_diacritics 2');
         CREATE TABLE place_link (entity INTEGER, kind INTEGER, kind_label TEXT, content_id INTEGER);
         INSERT INTO place VALUES (42, 'Nairobi', 9.0, -1.29, 36.82, '[1,2]', 'earth', 200, 36.6, -1.45, 37.1, -1.15),
                                  (43, 'Nairobi River', 1.0, -1.3, 36.9, '[3]', 'earth', NULL, NULL, NULL, NULL, NULL),
                                  (44, 'Nairobi Crater', 0.5, 10.0, 20.0, '[]', 'moon', 900, NULL, NULL, NULL, NULL);
         INSERT INTO place_name VALUES (1, 42, 'Nairobi', 'display'), (2, 43, 'Nairobi River', 'display'), (3, 44, 'Nairobi Crater', 'display');
         INSERT INTO place_fts (name, entity) VALUES ('Nairobi', 42), ('Nairobi River', 43), ('Nairobi Crater', 44);
         INSERT INTO place_link VALUES (42, 1, 'article', 100);",
    )
    .unwrap();
    g.execute(
        "INSERT INTO meta VALUES ('search_sql', ?1)",
        ["WITH hits AS (SELECT entity, max(lower(name) = lower(?2)) AS exact, min(rank) AS r FROM place_fts WHERE place_fts MATCH ?1 GROUP BY entity)
          SELECT p.entity, p.name, p.feature_type, p.context, p.lat, p.lon FROM hits h JOIN place_full p ON p.entity = h.entity
          ORDER BY h.exact DESC, p.importance DESC, h.r LIMIT 20"],
    )
    .unwrap();
    std::fs::create_dir_all(out.join("fonts")).unwrap();
    std::fs::write(out.join("fonts/REFSAN.TTF"), b"\x00\x01\x00\x00synthetic font").unwrap();
    std::fs::create_dir_all(out.join("tours/0001")).unwrap();
    std::fs::write(out.join("tours/index.json"), r#"{"format":"encarta-tours-index","tours":[]}"#).unwrap();
    std::fs::write(out.join("tours/0001/scene.json"), r#"{"id":"0001"}"#).unwrap();
    std::fs::create_dir_all(out.join("soundfont/samples")).unwrap();
    std::fs::write(out.join("soundfont/index.json"), r#"{"name":"Synthetic","presets":[]}"#).unwrap();
    std::fs::write(out.join("soundfont/samples/0.flac"), b"fLaC").unwrap();
}

pub fn spa(dir: &Path) {
    std::fs::create_dir_all(dir.join("assets")).unwrap();
    std::fs::write(dir.join("index.html"), "<!doctype html><title>Encarta</title><div id=root></div>").unwrap();
    std::fs::write(dir.join("assets/app-1a2b3c.js"), "console.log('encarta');".repeat(100)).unwrap();
    std::fs::write(dir.join("favicon.svg"), "<svg xmlns='http://www.w3.org/2000/svg'/>").unwrap();
}

pub struct Fixture {
    pub dir: tempfile::TempDir,
    pub out: PathBuf,
    pub web: PathBuf,
}

impl Fixture {
    /// The whole synthetic tree, search index included.
    pub fn build() -> Fixture {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("out");
        std::fs::create_dir_all(&out).unwrap();
        content(&out.join("content.sqlite"));
        manifest(&out);
        atlas_and_tours(&out);
        let opts = encarta_search::BuildOptions { threads: 1, heap_bytes: 20 << 20, merge: true };
        encarta_search::content_db::build_from_content_db(&out.join("content.sqlite"), &out.join("search"), &opts).unwrap();
        let web = dir.path().join("web");
        spa(&web);
        Fixture { dir, out, web }
    }

    pub fn config(&self) -> Config {
        let mut c = Config::new(&self.out);
        c.static_dir = Some(self.web.clone());
        c
    }

    /// Makes the whole tree read-only (as on a readOnly volume).
    pub fn set_read_only(&self, ro: bool) {
        fn walk(p: &Path, ro: bool) {
            use std::os::unix::fs::PermissionsExt;
            let meta = std::fs::metadata(p).unwrap();
            if meta.is_dir() {
                if !ro {
                    std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o755)).unwrap();
                }
                for e in std::fs::read_dir(p).unwrap() {
                    walk(&e.unwrap().path(), ro);
                }
                if ro {
                    std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o555)).unwrap();
                }
            } else {
                std::fs::set_permissions(p, std::fs::Permissions::from_mode(if ro { 0o444 } else { 0o644 })).unwrap();
            }
        }
        walk(&self.out, ro);
    }
}

pub fn app(cfg: &Config) -> Router {
    router(Arc::new(AppState::open(cfg)))
}

pub async fn send(app: &Router, req: Request<Body>) -> Response<Body> {
    app.clone().oneshot(req).await.unwrap()
}

pub async fn get(app: &Router, uri: &str) -> Response<Body> {
    send(app, Request::get(uri).body(Body::empty()).unwrap()).await
}

pub async fn bytes(res: Response<Body>) -> Vec<u8> {
    res.into_body().collect().await.unwrap().to_bytes().to_vec()
}

pub async fn json(app: &Router, uri: &str) -> (u16, serde_json::Value) {
    let res = get(app, uri).await;
    let status = res.status().as_u16();
    let b = bytes(res).await;
    (status, serde_json::from_slice(&b).unwrap_or_else(|_| serde_json::Value::String(String::from_utf8_lossy(&b).into())))
}
