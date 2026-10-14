//! JSON API handlers (`/api/…`). Shapes are documented in docs/api.md.

use crate::AppState;
use crate::db::Db;
use crate::error::{ApiError, ApiResult, editions, query_objects, row_object};
use crate::media::{disc_name, info_for_name};
use axum::Json;
use axum::extract::{Path, Query, State};
use encarta_search::SearchRequest;
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;
use serde_json::{Map, Value, json};
use std::sync::Arc;

type St = State<Arc<AppState>>;

fn content(st: &AppState) -> ApiResult<&Db> {
    st.content.as_ref().ok_or_else(|| ApiError::unavailable("content database not available"))
}

fn raw(s: Option<String>) -> Option<Box<RawValue>> {
    s.and_then(|s| RawValue::from_string(s).ok())
}

fn truthy(v: &Option<String>) -> Option<bool> {
    v.as_deref().map(|s| !matches!(s, "0" | "false" | "no" | "off"))
}

// ---------------------------------------------------------------- meta

pub async fn meta(State(st): St) -> ApiResult<Json<Value>> {
    let content = match &st.content {
        Some(db) => {
            let rows = db.call(|c| Ok(query_objects(c, "SELECT key, value FROM meta", [])?)).await?;
            let mut m = Map::new();
            for r in rows {
                if let (Some(k), Some(v)) = (r.get("key").and_then(|v| v.as_str()), r.get("value")) {
                    m.insert(k.to_string(), v.clone());
                }
            }
            Value::Object(m)
        }
        None => Value::Null,
    };
    Ok(Json(json!({
        "server": concat!("encarta-server ", env!("CARGO_PKG_VERSION")),
        "content": content,
        "search": st.search.as_ref().map(|s| s.meta.clone()),
        "media_manifest": st.media_manifest,
        "extracted_fallback": st.extracted.is_some(),
        "tours": st.tours,
        "atlas": st.atlas,
        "soundfont": st.soundfont,
        "fonts": st.fonts.iter().map(|f| format!("/fonts/{f}")).collect::<Vec<_>>(),
        "static": st.static_dir.is_some(),
    })))
}

// ---------------------------------------------------------------- items

/// One file of an item by DATA role (first by ordinal), as `<p>_file`, `<p>_width`, `<p>_height`.
fn file_cols(prefix: &str, alias: &str) -> String {
    format!("{alias}.file_name AS {prefix}_file, {alias}.width AS {prefix}_width, {alias}.height AS {prefix}_height")
}

fn assoc_sql() -> &'static str {
    static SQL: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    SQL.get_or_init(|| {
        let one = |role: &str, of: &str| format!("(SELECT rowid FROM item_file WHERE ref_id = {of} AND role = '{role}' ORDER BY ordinal LIMIT 1)");
        // The picture an item shows at full size: its own `image`, else (audio sets, composites)
        // the first child that has one.
        let child_image = "(SELECT f2.rowid FROM item_child ch JOIN item_file f2 ON f2.ref_id = ch.child_ref_id AND f2.role = 'image'
                             WHERE ch.ref_id = a.target_ref_id ORDER BY ch.ordinal, f2.ordinal LIMIT 1)";
        format!(
            "SELECT a.target_ref_id AS ref_id, a.grp AS \"group\", a.sec, a.sequence, a.appears, a.par,
                    i.kind, i.media_type, i.title, i.title_html, i.caption_html,
                    CASE WHEN json_extract(i.meta_json, '$.source_attrs.NoSquish') = '1' THEN 1 END AS no_squish,
                    CASE WHEN a.grp = 'sidebar' THEN i.preview_html END AS preview_html,
                    CASE WHEN a.grp = 'sidebar' THEN json_extract(i.meta_json, '$.sidebar_category') END AS sidebar_category, {}, {}, {}
             FROM assoc a
             LEFT JOIN item i ON i.ref_id = a.target_ref_id
             LEFT JOIN item_file ft ON ft.rowid = {}
             LEFT JOIN item_file fp ON fp.rowid = {}
             LEFT JOIN item_file fi ON fi.rowid = COALESCE({}, {child_image})
             WHERE a.ref_id = ?1 AND a.direction = ?2 ORDER BY a.ordinal LIMIT ?3",
            file_cols("thumb", "ft"),
            file_cols("picon", "fp"),
            file_cols("image", "fi"),
            one("thumb", "a.target_ref_id"),
            one("picon", "a.target_ref_id"),
            one("image", "a.target_ref_id"),
        )
    })
}

/// Moves `<p>_file`/`_width`/`_height` columns into a `FileRef` object `<p>` (null when absent).
pub(crate) fn take_file_ref(o: &mut Map<String, Value>, p: &str) {
    let file = o.remove(&format!("{p}_file")).and_then(|f| f.as_str().map(str::to_string));
    let (w, h) = (o.remove(&format!("{p}_width")), o.remove(&format!("{p}_height")));
    let v = file.map(|f| json!({ "file_name": f, "url": format!("/media/file/{f}"), "width": w, "height": h }));
    o.insert(p.into(), v.unwrap_or(Value::Null));
}

fn assoc_items(c: &Connection, ref_id: i64, direction: &str, limit: i64) -> rusqlite::Result<Vec<Value>> {
    let mut out = query_objects(c, assoc_sql(), params![ref_id, direction, limit])?;
    for v in &mut out {
        let o = v.as_object_mut().unwrap();
        for p in ["thumb", "picon", "image"] {
            take_file_ref(o, p);
        }
        if o.remove("no_squish").is_some() {
            o.insert("no_squish".into(), json!(true));
        }
    }
    Ok(out)
}

fn timeline_event(o: &Map<String, Value>) -> Value {
    let date = |p: &str| {
        let y = o.get(&format!("{p}_year")).cloned().unwrap_or(Value::Null);
        if y.is_null() {
            return Value::Null;
        }
        json!({ "year": y, "month": o.get(&format!("{p}_month")), "day": o.get(&format!("{p}_day")),
                "circa": o.get(&format!("{p}_circa")).and_then(|v| v.as_i64()).is_some_and(|v| v != 0) })
    };
    let list = |k: &str| -> Vec<String> {
        o.get(k).and_then(|v| v.as_str()).map(|s| s.split(", ").filter(|x| !x.is_empty()).map(str::to_string).collect()).unwrap_or_default()
    };
    let mut m = Map::new();
    for k in [
        "seq", "ref_id", "kind", "source_table", "title", "title_html", "span", "importance", "category_mask", "region_mask",
        // Display columns (schema 2, timeline.md §3.2).
        "display", "label", "date_text", "start_text", "end_text", "prominence",
    ] {
        if let Some(v) = o.get(k) {
            m.insert(k.into(), v.clone());
        }
    }
    m.insert("start".into(), date("start"));
    m.insert("end".into(), date("end"));
    m.insert("categories".into(), json!(list("categories")));
    m.insert("regions".into(), json!(list("regions")));
    if let Some(h) = o.get("has_article").and_then(|v| v.as_i64()) {
        m.insert("has_article".into(), json!(h != 0));
    }
    if let Some(t) = o.get("text") {
        m.insert("text".into(), t.clone());
    }
    Value::Object(m)
}

/// `timeline_event` columns plus `has_article` (the event item has a body).
const TIMELINE_COLS: &str = "t.*, EXISTS(SELECT 1 FROM article a WHERE a.ref_id = t.ref_id) AS has_article";

/// Plain text of an event's body (its paragraphs joined), or None.
fn event_text(c: &Connection, ref_id: i64) -> rusqlite::Result<Option<String>> {
    let mut st = c.prepare_cached("SELECT text FROM paragraph WHERE ref_id = ?1 ORDER BY ordinal, para_id")?;
    let parts: Vec<String> = st.query_map([ref_id], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?;
    Ok((!parts.is_empty()).then(|| parts.join("\n")))
}

#[derive(Deserialize, Default)]
pub struct EditionQuery {
    pub edition: Option<String>,
    pub body: Option<String>,
    pub html: Option<String>,
}

pub async fn item(State(st): St, Path(id): Path<i64>, Query(q): Query<EditionQuery>) -> ApiResult<Json<Value>> {
    let has_m = st.media_manifest;
    let edition = q.edition.unwrap_or_else(|| "deluxe".into());
    let v = content(&st)?
        .call(move |c| {
            let Some(mut o) = c.prepare_cached("SELECT * FROM item WHERE ref_id = ?1")?.query_row([id], row_object).optional()? else {
                // The atlas's own photos and music (MSWORLD.ITR) that have no DATA record.
                if let Some(v) = atlas_media_item(c, has_m, id, &edition)? {
                    return Ok(v);
                }
                return Err(ApiError::not_found(format!("no item {id}")).into());
            };
            let d = o.get("in_deluxe").and_then(|v| v.as_i64()).unwrap_or(0) != 0;
            let k = o.get("in_kids").and_then(|v| v.as_i64()).unwrap_or(0) != 0;
            o.insert("editions".into(), json!(editions(d, k)));
            let has_article: bool = c.prepare_cached("SELECT EXISTS(SELECT 1 FROM article WHERE ref_id = ?1)")?.query_row([id], |r| r.get(0))?;
            o.insert("has_article".into(), json!(has_article));
            let mut files = query_objects(c, "SELECT role, file_name, width, height, src_ref_id, disc FROM item_file WHERE ref_id = ?1 ORDER BY ordinal", [id])?;
            for f in &mut files {
                let o = f.as_object_mut().unwrap();
                let name = o.get("file_name").and_then(|v| v.as_str()).unwrap_or("").to_string();
                o.insert("url".into(), json!(format!("/media/file/{name}")));
                o.insert("media".into(), json!(info_for_name(c, has_m, &name, &edition)?));
            }
            o.insert("files".into(), json!(files));
            o.insert(
                "children".into(),
                json!(query_objects(
                    c,
                    "SELECT ch.kind, ch.child_ref_id AS ref_id, ch.sequence, i.title, i.kind AS item_kind
                     FROM item_child ch LEFT JOIN item i ON i.ref_id = ch.child_ref_id WHERE ch.ref_id = ?1 ORDER BY ch.ordinal",
                    [id]
                )?),
            );
            o.insert("media".into(), json!(assoc_items(c, id, "forward", 1000)?));
            o.insert("appears_in".into(), json!(assoc_items(c, id, "reverse", 500)?));
            // Items whose own forward associations name this one: the quotations of an author, the
            // articles of a contributor, the articles that list a web link. DATA stores no reverse
            // rows for these, so `appears_in` alone would leave such items with no way back.
            o.insert(
                "referenced_by".into(),
                json!(query_objects(
                    c,
                    "SELECT a.ref_id, a.grp AS \"group\", i.kind, i.title, i.title_html, i.media_type
                     FROM assoc a LEFT JOIN item i ON i.ref_id = a.ref_id
                     WHERE a.target_ref_id = ?1 AND a.direction = 'forward' AND a.ref_id != ?1
                     GROUP BY a.ref_id ORDER BY i.sort_title, a.ref_id LIMIT 500",
                    [id]
                )?),
            );
            // Items that list this one among their `children` (a tour stop's tour, a hotspot's stop).
            o.insert(
                "parents".into(),
                json!(query_objects(
                    c,
                    "SELECT ch.ref_id, ch.kind, i.title, i.kind AS item_kind
                     FROM item_child ch LEFT JOIN item i ON i.ref_id = ch.ref_id WHERE ch.child_ref_id = ?1
                     GROUP BY ch.ref_id ORDER BY ch.ref_id LIMIT 50",
                    [id]
                )?),
            );
            let table = c
                .prepare_cached("SELECT kind, json, html FROM data_table WHERE ref_id = ?1")?
                .query_row([id], |r| Ok(json!({ "kind": r.get::<_, Option<String>>(0)?,
                    "json": serde_json::from_str::<Value>(&r.get::<_, String>(1)?).unwrap_or(Value::Null),
                    "html": r.get::<_, Option<String>>(2)? })))
                .optional()?;
            o.insert("data_table".into(), table.unwrap_or(Value::Null));
            let tl = c.prepare_cached(&format!("SELECT {TIMELINE_COLS} FROM timeline_event t WHERE t.ref_id = ?1 LIMIT 1"))?.query_row([id], row_object).optional()?;
            o.insert("timeline".into(), tl.map(|t| timeline_event(&t)).unwrap_or(Value::Null));
            Ok(Value::Object(o))
        })
        .await?;
    Ok(Json(v))
}

fn has_table(c: &Connection, name: &str) -> rusqlite::Result<bool> {
    c.prepare_cached("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1)")?.query_row([name], |r| r.get(0))
}

/// An `atlas_media` row (MSWORLD.ITR: the atlas's photos `i`/`m` and music `w`) in the `/api/item`
/// shape, for the 2,354 that have no DATA record of their own (P2MTBL links to 1,229 of them).
fn atlas_media_item(c: &Connection, has_m: bool, id: i64, edition: &str) -> anyhow::Result<Option<Value>> {
    if !has_table(c, "atlas_media")? {
        return Ok(None);
    }
    let Some(row) = c.prepare_cached("SELECT * FROM atlas_media WHERE ref_id = ?1")?.query_row([id], row_object).optional()? else {
        return Ok(None);
    };
    let s = |k: &str| row.get(k).and_then(|v| v.as_str()).map(str::to_string);
    let audio = s("kind").as_deref() == Some("w");
    let mut files = Vec::new();
    // Catalog names resolve through the manifest's `atlas_file` when the file is on the disc under
    // another name (media.md §7). The 64×64 pushpin (`MSWORLD.EIT/dtn/<stem>.dtn`) stands in for
    // the pictures that are not on the disc at all.
    let stem = s("thumbnail").or_else(|| s("file")).and_then(|f| f.rsplit_once('.').map(|(a, _)| a.to_string()));
    let pin = if audio { None } else { stem.map(|st| format!("{st}.dtn")) };
    let roles: Vec<(&str, Option<String>)> =
        if audio { vec![("audio", s("file"))] } else { vec![("image", s("file")), ("picon", s("thumbnail")), ("thumb", pin)] };
    for (role, name) in roles {
        let Some(name) = name.as_deref().map(str::to_ascii_lowercase) else { continue };
        let media = info_for_name(c, has_m, &name, edition)?;
        // Only files that resolve: a broken picture is worse than none.
        if has_m && media.is_none() {
            continue;
        }
        files.push(json!({ "role": role, "file_name": name, "url": format!("/media/file/{name}"), "media": media }));
    }
    Ok(Some(json!({
        "ref_id": id, "kind": if audio { "audio" } else { "media" }, "source_group": "atlas-media",
        "media_type": if audio { "audio" } else { "image" }, "title": s("title"), "title_html": s("title_html"),
        "credit": s("credit"), "editions": ["deluxe"], "has_article": false, "files": files,
        "meta": { "atlas_media": { "place_id": row.get("place_id"), "paired_ref_id": row.get("paired_ref_id"), "legacy_id": row.get("legacy_id") } },
        "children": [], "media": [], "appears_in": [], "referenced_by": [], "parents": [], "data_table": null, "timeline": null,
    })))
}

/// `GET /api/index-term/{id}`: an index term (DATAF, content-schema §4) with its entries, which a
/// search result expands to. The tree is the edition's (`DLX`, or `KDC` with `edition=kids`).
pub async fn index_term(State(st): St, Path(id): Path<i64>, Query(q): Query<EditionQuery>) -> ApiResult<Json<Value>> {
    let ed = if q.edition.as_deref() == Some("kids") { "KDC" } else { "DLX" };
    let v = content(&st)?
        .call(move |c| {
            let Some(mut o) = c
                .prepare_cached("SELECT ref_id, title, title_html, in_deluxe, in_kids FROM item WHERE ref_id = ?1 AND kind = 'index-term'")?
                .query_row([id], row_object)
                .optional()?
            else {
                return Err(ApiError::not_found(format!("no index term {id}")).into());
            };
            let d = o.remove("in_deluxe").and_then(|v| v.as_i64()).unwrap_or(0) != 0;
            let k = o.remove("in_kids").and_then(|v| v.as_i64()).unwrap_or(0) != 0;
            o.insert("editions".into(), json!(editions(d, k)));
            // The edition's tree, else any (a term carries one tree per edition set).
            let tree: Option<(String, String)> = c
                .prepare_cached(
                    "SELECT editions, tree_json FROM index_tree WHERE ref_id = ?1
                     ORDER BY (',' || editions || ',') LIKE '%,' || ?2 || ',%' DESC, editions LIMIT 1",
                )?
                .query_row(params![id, ed], |r| Ok((r.get(0)?, r.get(1)?)))
                .optional()?;
            let (eds, tree) = tree.unwrap_or_default();
            let tree: Value = serde_json::from_str(&tree).unwrap_or(json!([]));
            // The root node is the term itself; its children are the entries.
            let entries = tree.get(0).and_then(|r| r.get("children")).cloned().unwrap_or(json!([]));
            o.insert("tree_editions".into(), json!(eds));
            o.insert("entries".into(), entries);
            Ok(Value::Object(o))
        })
        .await?;
    Ok(Json(v))
}

#[derive(Serialize)]
pub struct ArticleOut {
    #[serde(flatten)]
    head: Map<String, Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    body: Option<Box<RawValue>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    html: Option<String>,
    outline: Option<Box<RawValue>>,
    sections: Vec<Value>,
    related: Option<Box<RawValue>>,
    related_editions: Option<String>,
    media: Vec<Value>,
    tables: Vec<Value>,
}

pub async fn article(State(st): St, Path(id): Path<i64>, Query(q): Query<EditionQuery>) -> ApiResult<Json<ArticleOut>> {
    let want_body = truthy(&q.body).unwrap_or(true);
    let want_html = truthy(&q.html).unwrap_or(true);
    let tree_ed = if q.edition.as_deref() == Some("kids") { "KDC" } else { "DLX" };
    let out = content(&st)?
        .call(move |c| {
            let row = c
                .prepare_cached(
                    "SELECT a.source, a.source_file, a.paragraph_count, a.section_count, a.word_count,
                            i.title, i.title_html, i.kind, i.alt_title_html, a.layout, i.in_deluxe, i.in_kids,
                            CASE WHEN ?2 THEN a.body_json END, CASE WHEN ?3 THEN a.html END, a.outline_json
                     FROM article a JOIN item i USING (ref_id) WHERE a.ref_id = ?1",
                )?
                .query_row(params![id, want_body, want_html], |r| {
                    let mut head = Map::new();
                    head.insert("ref_id".into(), json!(id));
                    let cols = ["source", "source_file", "paragraph_count", "section_count", "word_count", "title", "title_html", "kind", "alt_title_html", "layout"];
                    for (i, k) in cols.iter().enumerate() {
                        let v = crate::error::value(r.get_ref(i)?, false);
                        if !v.is_null() {
                            head.insert(k.to_string(), v);
                        }
                    }
                    head.insert("editions".into(), json!(editions(r.get::<_, i64>(10)? != 0, r.get::<_, i64>(11)? != 0)));
                    Ok((head, r.get::<_, Option<String>>(12)?, r.get::<_, Option<String>>(13)?, r.get::<_, Option<String>>(14)?))
                })
                .optional()?;
            let Some((head, body, html, outline)) = row else {
                return Err(ApiError::not_found(format!("no article {id}")).into());
            };
            let sections = query_objects(
                c,
                "SELECT section_id AS id, parent_id, level, number, title, type_code AS type FROM section WHERE ref_id = ?1 ORDER BY section_id",
                [id],
            )?;
            let mut trees: Vec<(String, String)> = c
                .prepare_cached("SELECT editions, tree_json FROM related_tree WHERE ref_id = ?1")?
                .query_map([id], |r| Ok((r.get(0)?, r.get(1)?)))?
                .collect::<rusqlite::Result<_>>()?;
            let pick = trees.iter().position(|(e, _)| e.split(',').any(|x| x == tree_ed)).or(if trees.is_empty() { None } else { Some(0) });
            let (related_editions, related) = match pick {
                Some(i) => {
                    let (e, t) = trees.swap_remove(i);
                    (Some(e), raw(Some(t)))
                }
                None => (None, None),
            };
            let tables = query_objects(
                c,
                // `json_json` is parsed into `json` by row_object.
                "SELECT d.ref_id, d.kind, i.title, d.json AS json_json, d.html FROM assoc a JOIN data_table d ON d.ref_id = a.target_ref_id
                 LEFT JOIN item i ON i.ref_id = d.ref_id WHERE a.ref_id = ?1 AND a.direction = 'forward' ORDER BY a.ordinal",
                [id],
            )?;
            Ok(ArticleOut {
                head,
                body: raw(body),
                html,
                outline: raw(outline),
                sections,
                related,
                related_editions,
                media: assoc_items(c, id, "forward", 1000)?,
                tables,
            })
        })
        .await?;
    Ok(Json(out))
}

pub async fn place(State(st): St, Path(gid): Path<i64>) -> ApiResult<Json<Value>> {
    let links = content(&st)?
        .call(move |c| {
            // P2MTBL is shared with other editions: 2,656 of its targets are on no edition of this
            // disc and are left out. Atlas photos and music without a DATA record come from
            // `atlas_media` (MSWORLD.ITR), which `/api/item` serves too.
            if has_table(c, "atlas_media")? {
                return Ok(query_objects(
                    c,
                    "SELECT p.ordinal, p.kind, p.kind_label, p.content_ref_id AS ref_id, COALESCE(i.title, am.title) AS title,
                            COALESCE(i.kind, CASE am.kind WHEN 'w' THEN 'audio' ELSE 'media' END) AS item_kind
                     FROM place_link p LEFT JOIN item i ON i.ref_id = p.content_ref_id LEFT JOIN atlas_media am ON am.ref_id = p.content_ref_id
                     WHERE p.place_id = ?1 AND (i.ref_id IS NOT NULL OR am.ref_id IS NOT NULL) ORDER BY p.ordinal",
                    [gid],
                )?);
            }
            Ok(query_objects(
                c,
                "SELECT p.ordinal, p.kind, p.kind_label, p.content_ref_id AS ref_id, i.title, i.kind AS item_kind
                 FROM place_link p JOIN item i ON i.ref_id = p.content_ref_id WHERE p.place_id = ?1 ORDER BY p.ordinal",
                [gid],
            )?)
        })
        .await?;
    if links.is_empty() {
        return Err(ApiError::not_found(format!("no links for place {gid}")));
    }
    Ok(Json(json!({ "place_id": gid, "links": links })))
}

pub async fn quiz(State(st): St, Path(id): Path<i64>, Query(q): Query<EditionQuery>) -> ApiResult<Json<Value>> {
    let ed = q.edition.unwrap_or_else(|| "deluxe".into());
    let v = content(&st)?
        .call(move |c| {
            Ok(c.prepare_cached(
                "SELECT quiz_id, edition, quiz_type, lang, json FROM quiz WHERE quiz_id = ?1 ORDER BY (edition = ?2) DESC LIMIT 1",
            )?
            .query_row(params![id, ed], |r| {
                Ok(json!({ "quiz_id": r.get::<_, i64>(0)?, "edition": r.get::<_, String>(1)?, "quiz_type": r.get::<_, Option<String>>(2)?,
                    "lang": r.get::<_, Option<String>>(3)?, "quiz": serde_json::from_str::<Value>(&r.get::<_, String>(4)?).unwrap_or(Value::Null) }))
            })
            .optional()?)
        })
        .await?;
    v.map(Json).ok_or_else(|| ApiError::not_found(format!("no quiz {id}")))
}

/// Files an `.iax` references (manifest `iax_ref`, media.md §2.7), with URLs: under the disc tree
/// (`/media/disc/…`, so relative loads work) when the file is in `dswMedia/`, else by hash.
fn iax_refs(c: &Connection, file_name: &str, edition: &str) -> rusqlite::Result<Vec<Value>> {
    let Some(sha) = crate::media::resolve_sha(c, file_name, edition)? else { return Ok(Vec::new()) };
    let mut refs = query_objects(
        c,
        "SELECT r.element, r.ref, r.resolution, r.member_path, r.sha256, f.kind FROM m.iax_ref r
         LEFT JOIN m.file f ON f.rowid = (SELECT rowid FROM m.file WHERE sha256 = r.sha256 LIMIT 1)
         WHERE r.iax_sha256 = ?1 ORDER BY r.element, r.ref",
        [&sha],
    )?;
    for r in &mut refs {
        let o = r.as_object_mut().unwrap();
        let path = o.remove("member_path").and_then(|v| v.as_str().map(str::to_string));
        let sha = o.get("sha256").and_then(|v| v.as_str()).map(str::to_string);
        let url = match (path, sha) {
            (Some(p), _) if p.to_ascii_lowercase().starts_with(crate::media::DiscTree::PREFIX) => Some(format!("/media/disc/{edition}/{p}")),
            (_, Some(s)) => Some(format!("/media/{s}/original")),
            _ => None,
        };
        o.insert("url".into(), json!(url));
    }
    Ok(refs)
}

pub async fn interactive(State(st): St, Path(name): Path<String>, Query(q): Query<EditionQuery>) -> ApiResult<Json<Value>> {
    let ed = q.edition.unwrap_or_else(|| "deluxe".into());
    let key = disc_name(&name);
    let has_m = st.media_manifest;
    let v = content(&st)?
        .call(move |c| {
            let row = c
                .prepare_cached(
                    "SELECT file_name, edition, package, control_type, title, json FROM interactive WHERE lower(file_name) = ?1
                     ORDER BY (edition = ?2) DESC LIMIT 1",
                )?
                .query_row(params![key, ed], |r| {
                    Ok(json!({ "file_name": r.get::<_, String>(0)?, "edition": r.get::<_, String>(1)?, "package": r.get::<_, Option<String>>(2)?,
                        "control_type": r.get::<_, Option<String>>(3)?, "title": r.get::<_, Option<String>>(4)?,
                        "definition": serde_json::from_str::<Value>(&r.get::<_, String>(5)?).unwrap_or(Value::Null) }))
                })
                .optional()?;
            let Some(mut v) = row else { return Ok(None) };
            if has_m {
                let edition = v["edition"].as_str().unwrap_or("deluxe").to_string();
                v["refs"] = json!(iax_refs(c, &key, &edition)?);
            }
            Ok(Some(v))
        })
        .await?;
    v.map(Json).ok_or_else(|| ApiError::not_found(format!("no interactive {name}")))
}

// ---------------------------------------------------------------- dictionary

/// Every dictionary form, lower-cased and sorted, for exact and prefix lookups; and the word wheel
/// (headwords in the reader's order) for `around=`.
pub struct DictIndex {
    names: Vec<(String, String)>, // (name, kind)
    forms: Vec<(String, u16, u64, u8)>,
    originals: Vec<String>,
    /// Headwords sorted by [`wheel_key`]: (key, dictionary, ref, index into `originals`).
    wheel: Vec<(String, u16, u64, u32)>,
}

const FORM_KINDS: [&str; 5] = ["headword", "inflection", "runon", "phrase", "variant"];

/// Sort key of the word wheel: case and accents folded, hyphens and apostrophes ignored, other
/// punctuation as spaces. The reference VM lists "pari-mutuel" after "parietal lobe" and
/// before "paring".
pub fn wheel_key(form: &str) -> String {
    let joined: String = form.chars().filter(|c| !matches!(c, '-' | '\'' | '’' | '‘' | '.' | '‐' | '‑')).collect();
    encarta_search::text::normalize(&joined)
}

/// One lookup result: (dictionary, kind, ref, form, form kind).
type Match = (String, String, u64, String, &'static str);

impl DictIndex {
    pub fn load(c: &Connection) -> rusqlite::Result<Self> {
        let names: Vec<(String, String)> =
            c.prepare("SELECT name, kind FROM dictionary ORDER BY name")?.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<rusqlite::Result<_>>()?;
        let mut rows: Vec<(String, u16, u64, u8, String)> = Vec::new();
        let mut st = c.prepare("SELECT form, dictionary, ref_id, kind FROM dict_form")?;
        let mut q = st.query([])?;
        while let Some(r) = q.next()? {
            let form: String = r.get(0)?;
            let dict: String = r.get(1)?;
            let kind: String = r.get(3)?;
            let Some(d) = names.iter().position(|(n, _)| *n == dict) else { continue };
            let k = FORM_KINDS.iter().position(|x| *x == kind).unwrap_or(0) as u8;
            rows.push((form.to_lowercase(), d as u16, r.get::<_, i64>(2)? as u64, k, form));
        }
        rows.sort_by(|a, b| a.0.cmp(&b.0).then(a.3.cmp(&b.3)).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2)));
        let mut wheel: Vec<(String, u16, u64, u32)> =
            rows.iter().enumerate().filter(|(_, r)| r.3 == 0).map(|(i, r)| (wheel_key(&r.4), r.1, r.2, i as u32)).collect();
        wheel.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| rows[a.3 as usize].4.cmp(&rows[b.3 as usize].4)).then(a.2.cmp(&b.2)));
        wheel.dedup_by(|a, b| a.1 == b.1 && a.2 == b.2 && a.0 == b.0);
        let originals = rows.iter().map(|r| r.4.clone()).collect();
        let forms = rows.into_iter().map(|(l, d, r, k, _)| (l, d, r, k)).collect();
        Ok(DictIndex { names, forms, originals, wheel })
    }

    pub fn len(&self) -> usize {
        self.forms.len()
    }

    pub fn is_empty(&self) -> bool {
        self.forms.is_empty()
    }

    fn wanted(&self, d: u16, dict: Option<&str>, kind: Option<&str>) -> bool {
        let (name, dkind) = &self.names[d as usize];
        dict.is_none_or(|x| x.eq_ignore_ascii_case(name)) && kind.is_none_or(|k| k == dkind)
    }

    fn matched(&self, d: u16, r: u64, form: usize, form_kind: u8) -> Match {
        let (name, dkind) = &self.names[d as usize];
        (name.clone(), dkind.clone(), r, self.originals[form].clone(), FORM_KINDS[form_kind as usize])
    }

    /// Exact (or prefix) matches of `q` among all forms.
    fn lookup(&self, q: &str, prefix: bool, dict: Option<&str>, kind: Option<&str>, limit: usize) -> Vec<Match> {
        let q = q.trim().to_lowercase();
        if q.is_empty() {
            return Vec::new();
        }
        let start = self.forms.partition_point(|f| f.0.as_str() < q.as_str());
        let mut out = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for (i, f) in self.forms[start..].iter().enumerate() {
            let hit = if prefix { f.0.starts_with(&q) } else { f.0 == q };
            if !hit || out.len() >= limit {
                break;
            }
            if self.wanted(f.1, dict, kind) && seen.insert((f.1, f.2)) {
                out.push(self.matched(f.1, f.2, start + i, f.3));
            }
        }
        out
    }

    /// The word wheel around `q`: up to `before` headwords that sort before it (in wheel order)
    /// and up to `after` from its position on.
    fn around(&self, q: &str, dict: Option<&str>, kind: Option<&str>, before: usize, after: usize) -> (Vec<Match>, Vec<Match>) {
        let key = wheel_key(q);
        let pos = self.wheel.partition_point(|w| w.0.as_str() < key.as_str());
        let take = |w: &(String, u16, u64, u32)| self.matched(w.1, w.2, w.3 as usize, 0);
        let mut prev: Vec<Match> =
            self.wheel[..pos].iter().rev().filter(|w| self.wanted(w.1, dict, kind)).take(before).map(take).collect();
        prev.reverse();
        let next = self.wheel[pos..].iter().filter(|w| self.wanted(w.1, dict, kind)).take(after).map(take).collect();
        (prev, next)
    }
}

pub async fn dictionaries(State(st): St) -> ApiResult<Json<Value>> {
    let v = content(&st)?
        .call(|c| {
            Ok(query_objects(
                c,
                "SELECT name, kind, title, entry_count, uniq_tag, lex_file, guid, parent_guid, signature_json FROM dictionary ORDER BY name",
                [],
            )?)
        })
        .await?;
    Ok(Json(json!({ "dictionaries": v })))
}

#[derive(Deserialize)]
pub struct LookupQuery {
    pub q: Option<String>,
    pub dict: Option<String>,
    pub kind: Option<String>,
    pub prefix: Option<String>,
    pub limit: Option<usize>,
    pub around: Option<usize>,
}

fn lookup_entries(c: &Connection, hits: Vec<Match>) -> rusqlite::Result<Vec<Value>> {
    let mut st = c.prepare_cached("SELECT headword, summary FROM dict_entry WHERE dictionary = ?1 AND ref_id = ?2")?;
    let mut out = Vec::new();
    for (dict, kind, ref_id, form, form_kind) in hits {
        let (headword, summary): (Option<String>, Option<String>) =
            st.query_row(params![dict, ref_id as i64], |r| Ok((r.get(0)?, r.get(1)?))).optional()?.unwrap_or((None, None));
        out.push(json!({ "dictionary": dict, "kind": kind, "ref_id": ref_id, "headword": headword, "summary": summary,
            "form": form, "form_kind": form_kind, "url": format!("/api/dictionary/{dict}/{ref_id}") }));
    }
    Ok(out)
}

pub async fn dict_lookup(State(st): St, Query(q): Query<LookupQuery>) -> ApiResult<Json<Value>> {
    let Some(idx) = st.dict.clone() else {
        return Err(ApiError::unavailable("dictionary index not available"));
    };
    let query = q.q.clone().unwrap_or_default();
    let limit = q.limit.unwrap_or(50).clamp(1, 500);
    let (dict, kind) = (q.dict.as_deref(), q.kind.as_deref());
    let (before, hits) = match q.around {
        Some(n) => {
            let (b, a) = idx.around(&query, dict, kind, n.min(500), limit);
            (Some(b), a)
        }
        None => (None, idx.lookup(&query, truthy(&q.prefix).unwrap_or(false), dict, kind, limit)),
    };
    let (before, entries) = content(&st)?
        .call(move |c| Ok((before.map(|b| lookup_entries(c, b)).transpose()?, lookup_entries(c, hits)?)))
        .await?;
    let mut out = json!({ "query": query, "entries": entries });
    if let Some(b) = before {
        out["before"] = json!(b);
    }
    Ok(Json(out))
}

fn entry_json(c: &Connection, has_m: bool, dict: Option<&str>, id: i64) -> anyhow::Result<Value> {
    let row = c
        .prepare_cached(
            "SELECT e.dictionary, d.kind, e.ref_id, e.headword, e.variety, e.summary, e.body_json, e.html
             FROM dict_entry e LEFT JOIN dictionary d ON d.name = e.dictionary
             WHERE (?1 IS NULL OR e.dictionary = ?1) AND e.ref_id = ?2 ORDER BY e.dictionary LIMIT 1",
        )?
        .query_row(params![dict.map(str::to_ascii_uppercase), id], row_object)
        .optional()?;
    let Some(mut o) = row else {
        return Err(ApiError::not_found(match dict {
            Some(d) => format!("no entry {d}/{id}"),
            None => format!("no dictionary entry {id}"),
        })
        .into());
    };
    let name = o.get("dictionary").and_then(|v| v.as_str()).unwrap_or("").to_string();
    o.insert("forms".into(), json!(query_objects(c, "SELECT form, kind FROM dict_form WHERE dictionary = ?1 AND ref_id = ?2", params![name, id])?));
    let mut audio = query_objects(c, "SELECT file_name, audio_ref_id FROM dict_audio WHERE dictionary = ?1 AND ref_id = ?2", params![name, id])?;
    for a in &mut audio {
        let o = a.as_object_mut().unwrap();
        let f = o.get("file_name").and_then(|v| v.as_str()).unwrap_or("").to_string();
        o.insert("url".into(), json!(format!("/media/file/{f}")));
        o.insert("media".into(), json!(info_for_name(c, has_m, &f, "deluxe")?));
    }
    o.insert("audio".into(), json!(audio));
    Ok(Value::Object(o))
}

pub async fn dict_entry(State(st): St, Path((dict, id)): Path<(String, i64)>) -> ApiResult<Json<Value>> {
    let has_m = st.media_manifest;
    Ok(Json(content(&st)?.call(move |c| entry_json(c, has_m, Some(&dict), id)).await?))
}

/// `GET /api/dictionary/entry/{id}`: ref ids are unique across the dictionaries.
pub async fn dict_entry_by_id(State(st): St, Path(id): Path<i64>) -> ApiResult<Json<Value>> {
    let has_m = st.media_manifest;
    Ok(Json(content(&st)?.call(move |c| entry_json(c, has_m, None, id)).await?))
}

// ---------------------------------------------------------------- timeline

pub async fn timeline_meta(State(st): St) -> ApiResult<Json<Value>> {
    let v = content(&st)?
        .call(|c| {
            let cats = query_objects(c, "SELECT bit, name FROM timeline_category ORDER BY bit", [])?;
            let regs = query_objects(c, "SELECT bit, name FROM timeline_region ORDER BY bit", [])?;
            let (n, lo, hi): (i64, Option<i64>, Option<i64>) =
                c.query_row("SELECT COUNT(*), MIN(start_year), MAX(COALESCE(end_year, start_year)) FROM timeline_event", [], |r| {
                    Ok((r.get(0)?, r.get(1)?, r.get(2)?))
                })?;
            Ok(json!({ "categories": cats, "regions": regs, "count": n, "min_year": lo, "max_year": hi }))
        })
        .await?;
    Ok(Json(v))
}

#[derive(Deserialize)]
pub struct TimelineQuery {
    pub from: Option<i64>,
    pub to: Option<i64>,
    pub category: Option<String>,
    pub region: Option<String>,
    pub q: Option<String>,
    pub min_importance: Option<i64>,
    pub max_importance: Option<i64>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
    /// `date` (default) or `importance` (most important first, then by date).
    pub order: Option<String>,
    /// Include each event's body text.
    pub text: Option<String>,
    /// Only the events a timeline term lists (case-insensitive exact term text).
    pub term: Option<String>,
}

fn mask_of(c: &Connection, table: &str, spec: &Option<String>) -> ApiResult<i64> {
    let Some(spec) = spec.as_deref().filter(|s| !s.is_empty()) else { return Ok(0) };
    let mut m = 0i64;
    for tok in spec.split(',').map(str::trim).filter(|t| !t.is_empty()) {
        if let Ok(b) = tok.parse::<i64>() {
            m |= b;
            continue;
        }
        let bit: Option<i64> = c
            .prepare_cached(&format!("SELECT bit FROM {table} WHERE lower(name) = lower(?1)"))?
            .query_row([tok], |r| r.get(0))
            .optional()?;
        m |= bit.ok_or_else(|| ApiError::bad_request(format!("unknown {table} {tok:?}")))?;
    }
    Ok(m)
}

pub async fn timeline(State(st): St, Query(q): Query<TimelineQuery>) -> ApiResult<Json<Value>> {
    let order = match q.order.as_deref().unwrap_or("date") {
        "date" => "start_year, start_month, start_day, seq",
        // `prominence` = 6 − importance: importance 1 is the most prominent (VM, timeline.md §3.2).
        "importance" => "prominence DESC, start_year, start_month, start_day, seq",
        o => return Err(ApiError::bad_request(format!("unknown order {o:?} (date, importance)"))),
    };
    let with_text = truthy(&q.text).unwrap_or(false);
    let v = content(&st)?
        .call(move |c| {
            let cat = mask_of(c, "timeline_category", &q.category)?;
            let reg = mask_of(c, "timeline_region", &q.region)?;
            let like = q.q.as_deref().filter(|s| !s.is_empty()).map(|s| format!("%{s}%"));
            // `term`: the events a TIMELINE.DAT term lists (timeline.md §5): a topic timeline
            // ("Aztec Empire"), a century, a search term. Older databases have no term table.
            let term = q.term.as_deref().map(str::trim).filter(|s| !s.is_empty()).map(str::to_string);
            let has_terms: bool =
                c.prepare_cached("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'timeline_term')")?.query_row([], |r| r.get(0))?;
            if term.is_some() && !has_terms {
                return Ok(json!({ "total": 0, "offset": 0, "events": [] }));
            }
            let term_filter = if has_terms {
                // A topic item that is an event title with no term of its own finds that event.
                "AND (?8 IS NULL OR t.ref_id IN (SELECT j.value FROM json_each((SELECT ref_ids FROM timeline_term WHERE text = ?8 COLLATE NOCASE LIMIT 1)) j)
                      OR (NOT EXISTS (SELECT 1 FROM timeline_term WHERE text = ?8 COLLATE NOCASE) AND t.title = ?8 COLLATE NOCASE AND t.source_table != 'item'))"
            } else {
                "AND ?8 IS NULL"
            };
            let filter = format!(
                "WHERE (?1 IS NULL OR COALESCE(end_year, start_year) >= ?1) AND (?2 IS NULL OR start_year <= ?2)
                AND (?3 = 0 OR category_mask & ?3 != 0) AND (?4 = 0 OR region_mask & ?4 != 0) AND (?5 IS NULL OR title LIKE ?5)
                AND (?6 IS NULL OR importance >= ?6) AND (?7 IS NULL OR importance <= ?7) {term_filter}"
            );
            let p = params![q.from, q.to, cat, reg, like, q.min_importance, q.max_importance, term];
            let total: i64 = c.prepare_cached(&format!("SELECT COUNT(*) FROM timeline_event t {filter}"))?.query_row(p, |r| r.get(0))?;
            let limit = q.limit.unwrap_or(500).clamp(1, 20_000);
            let offset = q.offset.unwrap_or(0).max(0);
            let mut st = c.prepare_cached(&format!(
                "SELECT {TIMELINE_COLS} FROM timeline_event t {filter} ORDER BY {order} LIMIT ?9 OFFSET ?10"
            ))?;
            let rows = st.query_map(params![q.from, q.to, cat, reg, like, q.min_importance, q.max_importance, term, limit, offset], row_object)?;
            let mut rows: Vec<Map<String, Value>> = rows.collect::<rusqlite::Result<_>>()?;
            if with_text {
                for o in &mut rows {
                    if let Some(id) = o.get("ref_id").and_then(|v| v.as_i64())
                        && let Some(t) = event_text(c, id)? {
                            o.insert("text".into(), json!(t));
                        }
                }
            }
            let events: Vec<Value> = rows.iter().map(timeline_event).collect();
            Ok(json!({ "total": total, "offset": offset, "events": events }))
        })
        .await?;
    Ok(Json(v))
}

// ---------------------------------------------------------------- mindmaze

pub async fn mindmaze(State(st): St) -> ApiResult<Json<Value>> {
    let v = content(&st)?
        .call(|c| {
            let total: i64 = c.query_row("SELECT COUNT(*) FROM mindmaze_question", [], |r| r.get(0))?;
            let cats = query_objects(
                c,
                "SELECT category, category_label AS label, COUNT(*) AS questions FROM mindmaze_question GROUP BY category ORDER BY category",
                [],
            )?;
            let levels = query_objects(c, "SELECT level, COUNT(*) AS questions FROM mindmaze_question GROUP BY level ORDER BY level", [])?;
            Ok(json!({ "total": total, "categories": cats, "levels": levels }))
        })
        .await?;
    Ok(Json(v))
}

fn with_answers(c: &Connection, mut qs: Vec<Value>) -> rusqlite::Result<Vec<Value>> {
    for q in &mut qs {
        let o = q.as_object_mut().unwrap();
        let id = o.get("id").and_then(|v| v.as_i64()).unwrap_or(0);
        let mut answers = query_objects(
            c,
            "SELECT ordinal, correct, text, text_html, article_ref_id, article_exists FROM mindmaze_answer WHERE question_id = ?1 ORDER BY ordinal",
            [id],
        )?;
        for a in &mut answers {
            let a = a.as_object_mut().unwrap();
            for k in ["correct", "article_exists"] {
                if let Some(v) = a.get(k).and_then(|v| v.as_i64()) {
                    a.insert(k.into(), json!(v != 0));
                }
            }
        }
        o.insert("answers".into(), json!(answers));
    }
    Ok(qs)
}

#[derive(Deserialize)]
pub struct MazeQuery {
    pub category: Option<i64>,
    pub level: Option<i64>,
    pub limit: Option<i64>,
    pub random: Option<String>,
    pub seed: Option<i64>,
}

pub async fn mindmaze_questions(State(st): St, Query(q): Query<MazeQuery>) -> ApiResult<Json<Value>> {
    let random = truthy(&q.random).unwrap_or(false) || q.seed.is_some();
    let seed = q.seed.unwrap_or_else(|| std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(1, |d| d.as_nanos() as i64)) & 0x7fff_ffff;
    let v = content(&st)?
        .call(move |c| {
            let order = if random { "((id * 2654435761 + ?4) % 4294967291)" } else { "id + 0 * ?4" };
            let qs = query_objects(
                c,
                &format!(
                    "SELECT id, category, category_label, level, clue, clue_html, location FROM mindmaze_question
                     WHERE (?1 IS NULL OR category = ?1) AND (?2 IS NULL OR level = ?2) ORDER BY {order} LIMIT ?3"
                ),
                params![q.category, q.level, q.limit.unwrap_or(10).clamp(1, 1000), seed],
            )?;
            Ok(json!({ "questions": with_answers(c, qs)? }))
        })
        .await?;
    Ok(Json(v))
}

pub async fn mindmaze_question(State(st): St, Path(id): Path<i64>) -> ApiResult<Json<Value>> {
    let v = content(&st)?
        .call(move |c| {
            let qs = query_objects(
                c,
                "SELECT id, category, category_label, level, clue, clue_html, location FROM mindmaze_question WHERE id = ?1",
                [id],
            )?;
            Ok(with_answers(c, qs)?.into_iter().next())
        })
        .await?;
    v.map(Json).ok_or_else(|| ApiError::not_found(format!("no question {id}")))
}

// ---------------------------------------------------------------- statistics

const INDICATOR_SQL: &str = "SELECT s.id, s.name, s.category_mask, s.categories, s.content_ref_id, s.source,
        u.id AS unit_id, u.unit, u.alt_unit, u.factor, u.offset
    FROM stat_indicator s LEFT JOIN stat_unit u ON u.id = s.unit_id";

fn indicator(o: Map<String, Value>) -> Value {
    let mut o = o;
    let unit = o.remove("unit_id").map(|id| {
        json!({ "id": id, "unit": o.remove("unit"), "alt_unit": o.remove("alt_unit"), "factor": o.remove("factor"), "offset": o.remove("offset") })
    });
    for k in ["unit", "alt_unit", "factor", "offset"] {
        o.remove(k);
    }
    let cats: Vec<String> =
        o.get("categories").and_then(|v| v.as_str()).map(|s| s.split(", ").filter(|x| !x.is_empty()).map(str::to_string).collect()).unwrap_or_default();
    o.insert("categories".into(), json!(cats));
    o.insert("unit".into(), unit.unwrap_or(Value::Null));
    Value::Object(o)
}

pub async fn stats(State(st): St) -> ApiResult<Json<Value>> {
    let v = content(&st)?
        .call(|c| {
            let cats = query_objects(c, "SELECT bit, name FROM stat_category ORDER BY bit", [])?;
            let mut st = c.prepare_cached(&format!("{INDICATOR_SQL} ORDER BY s.name"))?;
            let inds: Vec<Value> = st.query_map([], row_object)?.map(|r| r.map(indicator)).collect::<rusqlite::Result<_>>()?;
            let countries = query_objects(c, "SELECT id, name FROM stat_country ORDER BY name", [])?;
            Ok(json!({ "categories": cats, "indicators": inds, "countries": countries }))
        })
        .await?;
    Ok(Json(v))
}

pub async fn stats_indicator(State(st): St, Path(id): Path<i64>) -> ApiResult<Json<Value>> {
    let v = content(&st)?
        .call(move |c| {
            let Some(ind) = c.prepare_cached(&format!("{INDICATOR_SQL} WHERE s.id = ?1"))?.query_row([id], row_object).optional()? else {
                return Err(ApiError::not_found(format!("no indicator {id}")).into());
            };
            let values = query_objects(
                c,
                "SELECT v.country_id, c.name AS country, v.rank, v.value, v.value_text, v.decimals, v.year, v.year_end, v.period, v.note
                 FROM stat_value v LEFT JOIN stat_country c ON c.id = v.country_id WHERE v.indicator_id = ?1 ORDER BY v.rank, c.name",
                [id],
            )?;
            Ok(json!({ "indicator": indicator(ind), "values": values }))
        })
        .await?;
    Ok(Json(v))
}

pub async fn stats_country(State(st): St, Path(id): Path<i64>) -> ApiResult<Json<Value>> {
    let v = content(&st)?
        .call(move |c| {
            let Some(country) = c.prepare_cached("SELECT id, name FROM stat_country WHERE id = ?1")?.query_row([id], row_object).optional()? else {
                return Err(ApiError::not_found(format!("no country {id}")).into());
            };
            let values = query_objects(
                c,
                "SELECT v.indicator_id, s.name AS indicator, v.rank, v.value, v.value_text, v.decimals, v.year, v.year_end, v.period, v.note
                 FROM stat_value v JOIN stat_indicator s ON s.id = v.indicator_id WHERE v.country_id = ?1 ORDER BY s.name",
                [id],
            )?;
            Ok(json!({ "country": country, "values": values }))
        })
        .await?;
    Ok(Json(v))
}

// ---------------------------------------------------------------- search

#[derive(Deserialize)]
pub struct SearchQuery {
    pub q: Option<String>,
    pub kind: Option<String>,
    pub edition: Option<String>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
    pub snippets: Option<String>,
    pub lexicon: Option<String>,
}

fn search_index(st: &AppState) -> ApiResult<Arc<encarta_search::SearchIndex>> {
    st.search.clone().ok_or_else(|| ApiError::unavailable("search index not available (run `encarta-convert search`)"))
}

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> anyhow::Result<T> + Send + 'static) -> ApiResult<T> {
    Ok(tokio::task::spawn_blocking(f).await.map_err(anyhow::Error::from)??)
}

pub async fn search(State(st): St, Query(q): Query<SearchQuery>) -> ApiResult<Json<Value>> {
    let idx = search_index(&st)?;
    let req = SearchRequest {
        q: q.q.unwrap_or_default(),
        kind: q.kind,
        edition: q.edition,
        limit: q.limit.unwrap_or(20),
        offset: q.offset.unwrap_or(0).min(10_000),
        snippets: truthy(&q.snippets).unwrap_or(true),
    };
    let res = blocking(move || idx.search(&req)).await?;
    let mut out = serde_json::to_value(&res).map_err(anyhow::Error::from)?;
    // Hits get the item's media type and thumbnail from the content database (not in the index).
    if let Some(db) = st.content.clone() {
        let refs: Vec<i64> = res.hits.iter().filter(|h| h.dictionary.is_none()).map(|h| h.ref_id as i64).collect();
        let extra = db
            .call(move |c| {
                let mut st = c.prepare_cached(&format!(
                    "SELECT i.media_type, {} FROM item i
                     LEFT JOIN item_file ft ON ft.rowid = (SELECT rowid FROM item_file WHERE ref_id = i.ref_id AND role = 'thumb' ORDER BY ordinal LIMIT 1)
                     WHERE i.ref_id = ?1",
                    file_cols("thumb", "ft")
                ))?;
                let mut m = std::collections::HashMap::new();
                for id in refs {
                    if let Some(o) = st.query_row([id], row_object).optional()? {
                        m.insert(id, o);
                    }
                }
                Ok(m)
            })
            .await?;
        if let Some(hits) = out.get_mut("hits").and_then(|h| h.as_array_mut()) {
            for h in hits {
                let Some(id) = h.get("ref_id").and_then(|v| v.as_i64()) else { continue };
                let Some(mut o) = extra.get(&id).cloned() else { continue };
                take_file_ref(&mut o, "thumb");
                let hm = h.as_object_mut().unwrap();
                if let Some(t) = o.remove("media_type") {
                    hm.insert("media_type".into(), t);
                }
                hm.insert("thumb".into(), o.remove("thumb").unwrap_or(Value::Null));
            }
        }
    }
    Ok(Json(out))
}

pub async fn suggest(State(st): St, Query(q): Query<SearchQuery>) -> ApiResult<Json<Value>> {
    let idx = search_index(&st)?;
    let query = q.q.unwrap_or_default();
    let limit = q.limit.unwrap_or(10).clamp(1, 100);
    let qq = query.clone();
    let s = blocking(move || Ok(idx.suggest(&qq, limit, q.edition.as_deref(), q.kind.as_deref()))).await?;
    Ok(Json(json!({ "query": query, "suggestions": s })))
}

pub async fn spell(State(st): St, Query(q): Query<SearchQuery>) -> ApiResult<Json<encarta_search::SpellResult>> {
    let idx = search_index(&st)?;
    let query = q.q.unwrap_or_default();
    let limit = q.limit.unwrap_or(5).clamp(1, 50);
    let r = blocking(move || {
        let lex: Vec<&str> = q.lexicon.as_deref().map(|l| l.split(',').map(str::trim).filter(|s| !s.is_empty()).collect()).unwrap_or_default();
        idx.spell(&query, &lex, q.edition.as_deref(), limit).ok_or_else(|| anyhow::Error::from(ApiError::unavailable("no spelling data")))
    })
    .await?;
    Ok(Json(r))
}

// ---------------------------------------------------------------- atlas gazetteer

fn gazetteer(st: &AppState) -> ApiResult<&Db> {
    st.gazetteer.as_ref().ok_or_else(|| ApiError::unavailable("atlas gazetteer not available"))
}

/// FTS5 query: every word as a quoted prefix term (all must match).
pub fn fts_query(q: &str) -> String {
    q.split(|c: char| !c.is_alphanumeric() && c != '\'' && c != '’')
        .filter(|w| !w.is_empty())
        .map(|w| format!("\"{}\"*", w.replace('"', "\"\"")))
        .collect::<Vec<_>>()
        .join(" ")
}

#[derive(Deserialize)]
pub struct GazQuery {
    pub q: Option<String>,
    pub limit: Option<i64>,
    /// `earth` or `moon`: only places on that body.
    pub body: Option<String>,
}

pub async fn atlas_search(State(st): St, Query(q): Query<GazQuery>) -> ApiResult<Json<Value>> {
    let db = gazetteer(&st)?;
    let text = q.q.unwrap_or_default();
    let fts = fts_query(&text);
    let limit = q.limit.unwrap_or(20).clamp(1, 200);
    let body = q.body.map(|b| b.to_ascii_lowercase()).filter(|b| !b.is_empty());
    if fts.is_empty() {
        return Ok(Json(json!({ "query": text, "places": [] })));
    }
    let t = text.clone();
    let places = db
        .call(move |c| {
            let sql: String = c.query_row("SELECT value FROM meta WHERE key = 'search_sql'", [], |r| r.get(0))?;
            let trimmed = sql.trim_end();
            let sql = match trimmed.rfind("LIMIT") {
                Some(i) => format!("{} LIMIT ?3", &trimmed[..i]),
                None => format!("SELECT * FROM ({trimmed}) LIMIT ?3"),
            };
            // The gazetteer's own ranking first; a body filter looks further down that list.
            let inner = if body.is_some() { 5000 } else { limit };
            let hits = query_objects(c, &sql, params![fts, t, inner])?;
            let mut extra = c.prepare_cached("SELECT goto_km, west, south, east, north, body, class_name FROM place_full WHERE entity = ?1")?;
            let mut out = Vec::new();
            for mut h in hits {
                let Some(entity) = h.get("entity").and_then(|v| v.as_i64()) else { continue };
                let row = extra
                    .query_row([entity], |r| {
                        Ok((r.get::<_, Option<f64>>(0)?, [r.get::<_, Option<f64>>(1)?, r.get(2)?, r.get(3)?, r.get(4)?], r.get::<_, Option<String>>(5)?, r.get::<_, Option<String>>(6)?))
                    })
                    .optional()?;
                let Some((goto_km, bbox, pbody, class_name)) = row else { continue };
                if body.as_deref().is_some_and(|b| pbody.as_deref() != Some(b)) {
                    continue;
                }
                let o = h.as_object_mut().unwrap();
                o.insert("goto_km".into(), json!(goto_km));
                o.insert("bbox".into(), if bbox.iter().all(|x| x.is_some()) { json!(bbox) } else { Value::Null });
                o.insert("body".into(), json!(pbody));
                o.insert("class_name".into(), json!(class_name));
                out.push(h);
                if out.len() as i64 >= limit {
                    break;
                }
            }
            Ok(out)
        })
        .await?;
    Ok(Json(json!({ "query": text, "places": places })))
}

pub async fn atlas_place(State(st): St, Path(entity): Path<i64>) -> ApiResult<Json<Value>> {
    let db = gazetteer(&st)?;
    let with_content = db.has("c");
    let v = db
        .call(move |c| {
            let Some(mut place) = c.prepare_cached("SELECT * FROM place_full WHERE entity = ?1")?.query_row([entity], row_object).optional()? else {
                return Err(ApiError::not_found(format!("no place {entity}")).into());
            };
            if let Some(cl) = place.remove("classes").and_then(|v| v.as_str().and_then(|s| serde_json::from_str::<Value>(s).ok())) {
                place.insert("classes".into(), cl);
            }
            let names = query_objects(c, "SELECT * FROM place_name WHERE entity = ?1", [entity])?;
            let links = if with_content {
                query_objects(
                    c,
                    "SELECT l.kind, l.kind_label, l.content_id AS ref_id, i.title, i.kind AS item_kind
                     FROM place_link l LEFT JOIN c.item i ON i.ref_id = l.content_id WHERE l.entity = ?1",
                    [entity],
                )?
            } else {
                query_objects(c, "SELECT kind, kind_label, content_id AS ref_id FROM place_link WHERE entity = ?1", [entity])?
            };
            Ok(json!({ "place": place, "names": names, "links": links }))
        })
        .await?;
    Ok(Json(v))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wheel_order_ignores_hyphens_and_case() {
        // The VM lists: parietal lobe, pari-mutuel, paring, Paris.
        let mut v = vec!["Paris", "paring", "pari-mutuel", "parietal lobe"];
        v.sort_by_key(|w| wheel_key(w));
        assert_eq!(v, vec!["parietal lobe", "pari-mutuel", "paring", "Paris"]);
        assert_eq!(wheel_key("Grévy’s"), "grevys");
    }
}
