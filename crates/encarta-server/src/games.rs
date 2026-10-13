//! Games API (`/api/games/…`): MindMaze games with answer-paragraph anchors, the SWQUIZ quiz bank
//! grouped by the board games that use it, Geography Quiz games resolved through the gazetteer,
//! the City Climate / Population Chart datasets, match games and the media of any `.iax`.
//! Shapes: docs/api.md "Games"; game rules and data notes: docs/games.md.

use crate::AppState;
use crate::db::Db;
use crate::error::{ApiError, ApiResult, query_objects};
use crate::media::{MediaInfo, media_info};
use axum::extract::{Path, Query, State};
use axum::routing::get;
use axum::{Json, Router};
use rusqlite::{Connection, OptionalExtension, params};
use serde::Deserialize;
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;

type St = State<Arc<AppState>>;

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/", get(catalogue))
        .route("/mindmaze/game", get(mindmaze_game))
        .route("/mindmaze/assets", get(mindmaze_assets))
        .route("/quizzes", get(quiz_games))
        .route("/quiz", get(quiz_batch))
        .route("/geoquiz", get(geoquiz_levels))
        .route("/geoquiz/{level}", get(geoquiz_game))
        .route("/chart/{name}", get(chart))
        .route("/match", get(match_games))
        .route("/interactive/{file}", get(interactive_media))
}

fn content(st: &AppState) -> ApiResult<&Db> {
    st.content.as_ref().ok_or_else(|| ApiError::unavailable("content database not available"))
}

fn edition_of(e: &Option<String>) -> &'static str {
    if e.as_deref() == Some("kids") { "kids" } else { "deluxe" }
}

// ---------------------------------------------------------------- small helpers

/// SplitMix64: a tiny deterministic generator so a `seed` always deals the same game.
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Rng(seed ^ 0x9E37_79B9_7F4A_7C15)
    }
    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    pub fn below(&mut self, n: usize) -> usize {
        (self.next_u64() % n.max(1) as u64) as usize
    }
    pub fn shuffle<T>(&mut self, v: &mut [T]) {
        for i in (1..v.len()).rev() {
            let j = self.below(i + 1);
            v.swap(i, j);
        }
    }
}

fn seed_or_now(seed: Option<u64>) -> u64 {
    seed.unwrap_or_else(|| std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(1, |d| d.as_nanos() as u64))
}

fn id_list(s: &Option<String>) -> Vec<i64> {
    s.as_deref().unwrap_or("").split(',').filter_map(|x| x.trim().parse().ok()).collect()
}

fn strip_tags(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for ch in s.chars() {
        match ch {
            '<' => in_tag = true,
            '>' if in_tag => {
                in_tag = false;
                out.push(' ');
            }
            _ if !in_tag => out.push(ch),
            _ => {}
        }
    }
    out
}

const STOP: &[&str] = &[
    "the", "and", "for", "this", "that", "was", "with", "from", "which", "its", "his", "her", "are", "were", "who", "these",
    "those", "has", "had", "into", "what", "than", "not", "one", "all", "but", "also", "been", "their", "they", "them",
    "most", "known", "called", "following",
];

/// Content words of a text: lower-case alphanumeric runs of 3+ characters, minus common words.
pub fn content_words(s: &str) -> HashSet<String> {
    s.split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.chars().count() >= 3)
        .map(str::to_lowercase)
        .filter(|w| !STOP.contains(&w.as_str()))
        .collect()
}

/// The paragraph of an answer article that answers a MindMaze clue (docs/games.md "Answer anchors").
/// Each paragraph scores one point per clue word and two per answer word it contains; the best score
/// wins, ties going to the paragraph nearest the question's `location` (a paragraph number in the
/// article revision the question was written for). `None` when no paragraph shares a word.
pub fn anchor_paragraph(paras: &[(i64, String)], clue: &str, answer: &str, location: i64) -> Option<i64> {
    let cw = content_words(clue);
    let aw = content_words(answer);
    let mut best: Option<(usize, i64, i64)> = None; // (score, -distance, para)
    for (id, text) in paras {
        let tw = content_words(text);
        let score = cw.intersection(&tw).count() + 2 * aw.intersection(&tw).count();
        if score == 0 {
            continue;
        }
        let key = (score, -(id - location).abs(), *id);
        if best.is_none_or(|b| (key.0, key.1) > (b.0, b.1)) {
            best = Some(key);
        }
    }
    best.map(|b| b.2)
}

/// A media reference shaped like the client's `ItemFile` (`file_name`, `url`, `width`, `height`, `media`).
fn media_ref(name: &str, w: Option<i64>, h: Option<i64>, info: Option<&MediaInfo>) -> Value {
    let url = info
        .and_then(|i| i.variants.get("original").or_else(|| i.variants.values().next()))
        .map(|v| v.url.clone())
        .unwrap_or_else(|| format!("/media/file/{}", name.to_ascii_lowercase()));
    json!({ "file_name": name, "url": url, "width": w, "height": h, "media": info })
}

/// `{"@w": "149", "@h": "99", "value": "x.jpg"}` or `"x.jpg"` or `""` → (name, w, h).
fn file_field(v: Option<&Value>) -> Option<(String, Option<i64>, Option<i64>)> {
    let v = v?;
    let (name, w, h) = match v {
        Value::String(s) => (s.clone(), None, None),
        Value::Object(o) => {
            let num = |k: &str| o.get(k).and_then(|x| x.as_str()).and_then(|s| s.parse::<i64>().ok()).filter(|n| *n > 0);
            (o.get("value").and_then(|x| x.as_str()).unwrap_or("").to_string(), num("@w"), num("@h"))
        }
        _ => return None,
    };
    let name = name.trim().to_string();
    (!name.is_empty()).then_some((name, w, h))
}

fn text_field(v: Option<&Value>) -> String {
    match v {
        Some(Value::String(s)) => s.trim().to_string(),
        Some(Value::Object(o)) => o.get("value").and_then(|x| x.as_str()).unwrap_or("").trim().to_string(),
        _ => String::new(),
    }
}

fn as_list(v: Option<&Value>) -> Vec<&Value> {
    match v {
        Some(Value::Array(a)) => a.iter().collect(),
        Some(Value::Null) | None => Vec::new(),
        Some(Value::String(s)) if s.is_empty() => Vec::new(),
        Some(other) => vec![other],
    }
}

/// The sha256 of the `.iax` at `member` (e.g. `dswMedia/quiz/12/12.iax`), preferring the edition's copy.
fn iax_sha(c: &Connection, member: &str, edition: &str) -> rusqlite::Result<Option<String>> {
    c.prepare_cached(
        "SELECT sha256 FROM m.file WHERE member_path = ?1 ORDER BY ((container LIKE 'EE/KIDS/%') = (?2 = 'kids')) DESC LIMIT 1",
    )?
    .query_row(params![member, edition], |r| r.get(0))
    .optional()
}

/// Every file an `.iax` references, as resolved by the media converter (`iax_ref`): ref → MediaInfo.
fn iax_media(c: &Connection, iax: &str) -> rusqlite::Result<BTreeMap<String, MediaInfo>> {
    let mut refs: Vec<(String, String)> = Vec::new();
    {
        let mut st = c.prepare_cached("SELECT DISTINCT ref, sha256 FROM m.iax_ref WHERE iax_sha256 = ?1 AND sha256 IS NOT NULL")?;
        let rows = st.query_map([iax], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        for r in rows {
            refs.push(r?);
        }
    }
    let mut out = BTreeMap::new();
    let mut cache: HashMap<String, Option<MediaInfo>> = HashMap::new();
    for (name, sha) in refs {
        let info = match cache.get(&sha) {
            Some(i) => i.clone(),
            None => {
                let i = media_info(c, &sha)?;
                cache.insert(sha.clone(), i.clone());
                i
            }
        };
        if let Some(i) = info {
            out.insert(name, i);
        }
    }
    Ok(out)
}

fn lookup<'a>(media: &'a BTreeMap<String, MediaInfo>, name: &str) -> Option<&'a MediaInfo> {
    media.get(name).or_else(|| media.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v))
}

// ---------------------------------------------------------------- catalogue

pub async fn catalogue(State(st): St) -> ApiResult<Json<Value>> {
    let charts: Vec<Value> = [("city_climate", "City Climate"), ("population", "Population Chart")]
        .into_iter()
        .filter(|(id, _)| st.out.join(format!("media/interactives/{id}.json")).is_file())
        .map(|(id, title)| json!({ "id": id, "title": title }))
        .collect();
    let gaz = st.gazetteer.is_some();
    let Some(db) = &st.content else {
        return Ok(Json(json!({ "mindmaze": null, "quiz": null, "geoquiz": null, "charts": charts, "match_games": 0 })));
    };
    let v = db
        .call(move |c| {
            let total: i64 = c.query_row("SELECT COUNT(*) FROM mindmaze_question", [], |r| r.get(0)).unwrap_or(0);
            let cats = query_objects(
                c,
                "SELECT category, category_label AS label, COUNT(*) AS questions FROM mindmaze_question GROUP BY category ORDER BY category",
                [],
            )
            .unwrap_or_default();
            let types = query_objects(c, "SELECT quiz_type AS type, COUNT(DISTINCT quiz_id) AS quizzes FROM quiz GROUP BY quiz_type", [])
                .unwrap_or_default();
            let quiz_total: i64 = c.query_row("SELECT COUNT(DISTINCT quiz_id) FROM quiz", [], |r| r.get(0)).unwrap_or(0);
            let levels = geoquiz_level_rows(c, "").unwrap_or_default();
            let matches: i64 = c
                .query_row("SELECT COUNT(DISTINCT lower(file_name)) FROM interactive WHERE control_type = 'MatchGame'", [], |r| r.get(0))
                .unwrap_or(0);
            Ok(json!({
                "mindmaze": if total > 0 { json!({ "total": total, "categories": cats }) } else { Value::Null },
                "quiz": json!({ "total": quiz_total, "types": types }),
                "geoquiz": if gaz && !levels.is_empty() { json!({ "levels": levels }) } else { Value::Null },
                "match_games": matches,
            }))
        })
        .await?;
    let mut v = v;
    v["charts"] = json!(charts);
    Ok(Json(v))
}

// ---------------------------------------------------------------- MindMaze

#[derive(Deserialize)]
pub struct MazeGameQuery {
    pub level: Option<i64>,
    /// Comma-separated category numbers (0–8); all when absent.
    pub categories: Option<String>,
    pub count: Option<usize>,
    pub seed: Option<u64>,
    /// Question ids the player has already seen (skipped while enough others remain).
    pub exclude: Option<String>,
}

pub async fn mindmaze_game(State(st): St, Query(q): Query<MazeGameQuery>) -> ApiResult<Json<Value>> {
    let level = q.level.unwrap_or(1).clamp(1, 4);
    let cats = id_list(&q.categories);
    let count = q.count.unwrap_or(30).clamp(1, 200);
    let seed = seed_or_now(q.seed);
    let exclude: HashSet<i64> = id_list(&q.exclude).into_iter().collect();
    let v = content(&st)?
        .call(move |c| {
            let mut ids: Vec<(i64, i64)> = Vec::new();
            {
                let mut s = c.prepare_cached("SELECT id, category FROM mindmaze_question WHERE level = ?1 ORDER BY id")?;
                let rows = s.query_map([level], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)))?;
                for r in rows {
                    let (id, cat) = r?;
                    if cats.is_empty() || cats.contains(&cat) {
                        ids.push((id, cat));
                    }
                }
            }
            let mut rng = Rng::new(seed);
            let (mut fresh, mut seen): (Vec<_>, Vec<_>) = ids.into_iter().partition(|(id, _)| !exclude.contains(id));
            rng.shuffle(&mut fresh);
            rng.shuffle(&mut seen);
            fresh.extend(seen);
            fresh.truncate(count);
            let mut out = Vec::with_capacity(fresh.len());
            for (id, _) in fresh {
                if let Some(qv) = maze_question(c, id)? {
                    out.push(qv);
                }
            }
            Ok(json!({ "level": level, "seed": seed, "questions": out }))
        })
        .await?;
    Ok(Json(v))
}

fn maze_question(c: &Connection, id: i64) -> rusqlite::Result<Option<Value>> {
    let Some(mut q) = query_objects(
        c,
        "SELECT id, category, category_label, level, clue, clue_html, location FROM mindmaze_question WHERE id = ?1",
        [id],
    )?
    .into_iter()
    .next() else {
        return Ok(None);
    };
    let clue = strip_tags(q.get("clue_html").and_then(|v| v.as_str()).unwrap_or(""));
    let location = q.get("location").and_then(|v| v.as_i64()).unwrap_or(1);
    let mut answers = query_objects(
        c,
        "SELECT a.ordinal, a.correct, a.text, a.text_html, a.article_ref_id, a.article_exists, i.title AS article_title
         FROM mindmaze_answer a LEFT JOIN item i ON i.ref_id = a.article_ref_id
         WHERE a.question_id = ?1 ORDER BY a.ordinal",
        [id],
    )?;
    for a in &mut answers {
        let o = a.as_object_mut().unwrap();
        for k in ["correct", "article_exists"] {
            if let Some(v) = o.get(k).and_then(|v| v.as_i64()) {
                o.insert(k.into(), json!(v != 0));
            }
        }
        if o.get("correct") == Some(&json!(true)) && o.get("article_exists") == Some(&json!(true)) {
            let rid = o.get("article_ref_id").and_then(|v| v.as_i64()).unwrap_or(0);
            let paras: Vec<(i64, String)> = c
                .prepare_cached("SELECT para_id, text FROM paragraph WHERE ref_id = ?1 AND kind = 'p' ORDER BY para_id")?
                .query_map([rid], |r| Ok((r.get(0)?, r.get(1)?)))?
                .collect::<rusqlite::Result<_>>()?;
            let answer = strip_tags(o.get("text_html").and_then(|v| v.as_str()).unwrap_or(""));
            if let Some(p) = anchor_paragraph(&paras, &clue, &answer, location) {
                o.insert("para".into(), json!(p));
            }
        }
    }
    q.as_object_mut().unwrap().insert("answers".into(), json!(answers));
    Ok(Some(q))
}

/// The MindMaze artwork, sounds and music (`MINDMAZE.EIT/baggage`), by lower-case file name.
pub async fn mindmaze_assets(State(st): St) -> ApiResult<Json<Value>> {
    if !st.media_manifest {
        return Err(ApiError::unavailable("media manifest not available"));
    }
    let db = st.media.as_ref().ok_or_else(|| ApiError::unavailable("media manifest not available"))?;
    let v = db
        .call(|c| {
            let rows: Vec<(String, String)> = c
                .prepare_cached("SELECT lower(name), sha256 FROM m.file WHERE container LIKE '%/MINDMAZE.EIT' ORDER BY name")?
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
                .collect::<rusqlite::Result<_>>()?;
            let mut m = Map::new();
            for (name, sha) in rows {
                if let Some(info) = media_info(c, &sha)? {
                    m.insert(name, json!(info));
                }
            }
            Ok(json!({ "assets": m }))
        })
        .await?;
    Ok(Json(v))
}

// ---------------------------------------------------------------- quizzes

#[derive(Deserialize)]
pub struct EdQuery {
    pub edition: Option<String>,
}

/// One playable item (an `interactive` item whose DATA `item` file is this `.iax`).
fn launch_items(c: &Connection, file: &str) -> rusqlite::Result<Vec<Value>> {
    let mut items = query_objects(
        c,
        "SELECT i.ref_id, i.title, i.caption_html, i.in_deluxe, i.in_kids, i.subgroup,
                (SELECT t.file_name FROM item_file t WHERE t.ref_id = i.ref_id AND t.role = 'thumb' ORDER BY t.ordinal LIMIT 1) AS thumb,
                (SELECT t.file_name FROM item_file t WHERE t.ref_id = i.ref_id AND t.role = 'picon' ORDER BY t.ordinal LIMIT 1) AS picon
         FROM item_file f JOIN item i ON i.ref_id = f.ref_id
         WHERE f.role = 'item' AND lower(f.file_name) = lower(?1) ORDER BY i.ref_id",
        [file],
    )?;
    for it in &mut items {
        let o = it.as_object_mut().unwrap();
        for k in ["thumb", "picon"] {
            if let Some(f) = o.get(k).and_then(|v| v.as_str()).map(str::to_string) {
                o.insert(k.into(), json!(format!("/media/file/{f}")));
            }
        }
        for k in ["in_deluxe", "in_kids"] {
            if let Some(v) = o.get(k).and_then(|v| v.as_i64()) {
                o.insert(k.into(), json!(v != 0));
            }
        }
    }
    Ok(items)
}

fn display_title(t: &str) -> String {
    let t = t.trim();
    for p in ["Adult: ", "Kids: ", "Kids - ", "Kids:"] {
        if let Some(r) = t.strip_prefix(p) {
            return r.trim().to_string();
        }
    }
    t.to_string()
}

/// The board games (`BGAControl`) and the quiz-bank groups each uses; quizzes no game uses form a last group.
pub async fn quiz_games(State(st): St, Query(q): Query<EdQuery>) -> ApiResult<Json<Value>> {
    let ed = edition_of(&q.edition);
    let v = content(&st)?
        .call(move |c| {
            let rows: Vec<(String, String, Option<String>, String)> = c
                .prepare_cached(
                    "SELECT file_name, edition, title, json FROM interactive WHERE control_type = 'BGAControl'
                     ORDER BY lower(file_name), (edition = ?1) DESC",
                )?
                .query_map([ed], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
                .collect::<rusqlite::Result<_>>()?;
            let known: HashSet<i64> = c.prepare_cached("SELECT DISTINCT quiz_id FROM quiz")?.query_map([], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?;
            let mut seen_files = HashSet::new();
            let mut used = HashSet::new();
            let mut games = Vec::new();
            for (file, edition, title, js) in rows {
                if !seen_files.insert(file.to_ascii_lowercase()) {
                    continue;
                }
                let d: Value = serde_json::from_str(&js).unwrap_or(Value::Null);
                let mut groups = Vec::new();
                for g in as_list(d.get("questiongroups").and_then(|x| x.get("qgroup"))) {
                    let ids: Vec<i64> = g
                        .get("@questions")
                        .and_then(|x| x.as_str())
                        .unwrap_or("")
                        .split(',')
                        .filter_map(|s| s.trim().parse().ok())
                        .filter(|id| known.contains(id))
                        .collect();
                    if ids.is_empty() {
                        continue;
                    }
                    used.extend(ids.iter().copied());
                    groups.push(json!({ "id": g.get("@id"), "name": g.get("@fname"), "questions": ids }));
                }
                let raw_title = title.unwrap_or_default();
                games.push(json!({
                    "file_name": file, "edition": edition, "title": display_title(&raw_title), "iax_title": raw_title,
                    "groups": groups, "items": launch_items(c, &file)?,
                }));
            }
            games.sort_by(|a, b| a["title"].as_str().cmp(&b["title"].as_str()));
            let mut rest: Vec<i64> = known.difference(&used).copied().collect();
            rest.sort_unstable();
            let types = query_objects(c, "SELECT quiz_type AS type, COUNT(DISTINCT quiz_id) AS quizzes FROM quiz GROUP BY quiz_type", [])?;
            Ok(json!({ "total": known.len(), "types": types, "games": games, "ungrouped": rest }))
        })
        .await?;
    Ok(Json(v))
}

#[derive(Deserialize)]
pub struct QuizBatchQuery {
    pub ids: Option<String>,
    pub edition: Option<String>,
}

/// Quizzes by id (up to 200), normalised, with every image and sound resolved in the quiz's own folder.
pub async fn quiz_batch(State(st): St, Query(q): Query<QuizBatchQuery>) -> ApiResult<Json<Value>> {
    let ids = id_list(&q.ids);
    if ids.is_empty() {
        return Err(ApiError::bad_request("ids= is required"));
    }
    if ids.len() > 200 {
        return Err(ApiError::bad_request("at most 200 ids"));
    }
    let ed = edition_of(&q.edition);
    let has_m = st.media_manifest;
    let v = content(&st)?
        .call(move |c| {
            let mut out = Vec::new();
            for id in ids {
                let row: Option<(String, String)> = c
                    .prepare_cached("SELECT edition, json FROM quiz WHERE quiz_id = ?1 ORDER BY (edition = ?2) DESC LIMIT 1")?
                    .query_row(params![id, ed], |r| Ok((r.get(0)?, r.get(1)?)))
                    .optional()?;
                let Some((edition, js)) = row else { continue };
                let d: Value = serde_json::from_str(&js).unwrap_or(Value::Null);
                let media = if has_m {
                    match iax_sha(c, &format!("dswMedia/quiz/{id}/{id}.iax"), &edition)? {
                        Some(sha) => iax_media(c, &sha)?,
                        None => BTreeMap::new(),
                    }
                } else {
                    BTreeMap::new()
                };
                out.push(normalise_quiz(id, &edition, &d, &media));
            }
            Ok(json!({ "quizzes": out }))
        })
        .await?;
    Ok(Json(v))
}

fn quiz_part(v: Option<&Value>, media: &BTreeMap<String, MediaInfo>) -> Value {
    let o = v.and_then(|x| x.as_object());
    let get = |k: &str| o.and_then(|o| o.get(k));
    let file = |k: &str| file_field(get(k)).map(|(n, w, h)| media_ref(&n, w, h, lookup(media, &n)));
    json!({ "html": text_field(get("text")), "image": file("image"), "audio": file("audio") })
}

pub fn normalise_quiz(id: i64, edition: &str, d: &Value, media: &BTreeMap<String, MediaInfo>) -> Value {
    let qtype = d.get("@type").and_then(|x| x.as_str()).unwrap_or("multichoice");
    let answers_node = d.get("answers");
    let mut answers = Vec::new();
    let mut accept = Vec::new();
    for a in as_list(answers_node.and_then(|x| x.get("answer"))) {
        match a {
            Value::String(s) => accept.push(json!(s.trim())),
            Value::Object(_) => {
                let mut part = quiz_part(Some(a), media);
                part["id"] = a.get("@id").cloned().unwrap_or(Value::Null);
                answers.push(part);
            }
            _ => {}
        }
    }
    let correct = answers_node.and_then(|x| x.get("@correct")).cloned().unwrap_or(Value::Null);
    json!({
        "id": id, "edition": edition, "type": qtype,
        "question": quiz_part(d.get("question"), media),
        "answers": answers, "correct": correct, "accept": accept,
        "reward": quiz_part(d.get("reward"), media),
        "credits": text_field(d.get("credits")), "reward_credits": text_field(d.get("rewardcredits")),
    })
}

// ---------------------------------------------------------------- Geography Quiz

/// Difficulty names as the Geography Quiz dialog shows them (fixtures/geoquiz/01-open.png).
pub const GEOQUIZ_LEVEL_NAMES: [(&str, &str); 4] =
    [("Navigator", "easy"), ("Explorer", "not so easy"), ("Globetrotter", "difficult"), ("World Class", "most difficult")];

fn geoquiz_level_rows(c: &Connection, schema: &str) -> rusqlite::Result<Vec<Value>> {
    let mut rows = query_objects(
        c,
        &format!(
            "SELECT level, questions, points AS points_json, per_type_unknown AS per_type_json, type_mix AS type_mix_json
             FROM {schema}geoquiz_level ORDER BY level"
        ),
        [],
    )?;
    for r in &mut rows {
        let o = r.as_object_mut().unwrap();
        if let Some(lv) = o.get("level").and_then(|v| v.as_i64())
            && let Some((name, hint)) = GEOQUIZ_LEVEL_NAMES.get((lv - 1).max(0) as usize)
        {
            o.insert("name".into(), json!(name));
            o.insert("difficulty".into(), json!(hint));
        }
    }
    Ok(rows)
}

fn gazetteer(st: &AppState) -> ApiResult<&Db> {
    st.gazetteer.as_ref().ok_or_else(|| ApiError::unavailable("atlas gazetteer not available"))
}

fn geoquiz_levels_from(c: &Connection, has_c: bool) -> rusqlite::Result<Vec<Value>> {
    if has_c { geoquiz_level_rows(c, "c.") } else { Ok(Vec::new()) }
}

pub async fn geoquiz_levels(State(st): St) -> ApiResult<Json<Value>> {
    let db = gazetteer(&st)?;
    let has_c = db.has("c");
    let v = db
        .call(move |c| {
            let mut levels = geoquiz_levels_from(c, has_c)?;
            let counts = query_objects(c, "SELECT level, kind, COUNT(*) AS n FROM quiz_place GROUP BY level, kind", [])?;
            for l in &mut levels {
                let lv = l["level"].as_i64();
                let pool: Vec<i64> =
                    (0..3).map(|k| counts.iter().filter(|r| r["level"].as_i64() == lv && r["kind"].as_i64() == Some(k)).map(|r| r["n"].as_i64().unwrap_or(0)).sum()).collect();
                l["pool"] = json!(pool);
            }
            Ok(json!({ "levels": levels }))
        })
        .await?;
    Ok(Json(v))
}

#[derive(Deserialize)]
pub struct SeedQuery {
    pub seed: Option<u64>,
    pub count: Option<usize>,
}

/// How many questions of each place type a game asks: `mix` shares of `n`, largest remainders first.
pub fn split_counts(mix: &[f64], n: usize) -> Vec<usize> {
    let total: f64 = mix.iter().sum::<f64>().max(1e-9);
    let exact: Vec<f64> = mix.iter().map(|m| m / total * n as f64).collect();
    let mut counts: Vec<usize> = exact.iter().map(|x| x.floor() as usize).collect();
    let mut order: Vec<usize> = (0..mix.len()).collect();
    order.sort_by(|&a, &b| (exact[b] - exact[b].floor()).total_cmp(&(exact[a] - exact[a].floor())).then(a.cmp(&b)));
    let mut left = n.saturating_sub(counts.iter().sum());
    for i in order.into_iter().cycle().take(mix.len() * 2) {
        if left == 0 {
            break;
        }
        if mix[i] > 0.0 {
            counts[i] += 1;
            left -= 1;
        }
    }
    counts
}

const PLACE_KINDS: [&str; 3] = ["country", "place", "continent"];

pub async fn geoquiz_game(State(st): St, Path(level): Path<i64>, Query(q): Query<SeedQuery>) -> ApiResult<Json<Value>> {
    if !(1..=4).contains(&level) {
        return Err(ApiError::not_found(format!("no Geography Quiz level {level}")));
    }
    let db = gazetteer(&st)?;
    let has_c = db.has("c");
    let seed = seed_or_now(q.seed);
    let v = db
        .call(move |c| {
            let lv = geoquiz_levels_from(c, has_c)?.into_iter().find(|l| l["level"].as_i64() == Some(level));
            let mix: Vec<f64> = lv
                .as_ref()
                .and_then(|l| l["type_mix"].as_array().cloned())
                .map(|a| a.iter().filter_map(|x| x.as_f64()).collect())
                .unwrap_or_else(|| vec![0.7, 0.2, 0.1]);
            let n = q.count.or_else(|| lv.as_ref().and_then(|l| l["questions"].as_u64()).map(|x| x as usize)).unwrap_or(20).clamp(1, 100);
            let mut pools: Vec<Vec<Value>> = vec![Vec::new(), Vec::new(), Vec::new()];
            let places = query_objects(
                c,
                "SELECT q.kind, q.region, r.name AS region_name, p.entity, p.name, p.feature_type, p.context, p.class_name,
                        p.lat, p.lon, p.west, p.south, p.east, p.north, p.goto_km
                 FROM quiz_place q JOIN place_full p ON p.entity = q.entity LEFT JOIN place r ON r.entity = q.region
                 WHERE q.level = ?1 AND p.lat IS NOT NULL ORDER BY q.rowid",
                [level],
            )?;
            for mut p in places {
                let o = p.as_object_mut().unwrap();
                let mut kind = o.get("kind").and_then(|v| v.as_i64()).unwrap_or(1).clamp(0, 2);
                // D4 lists the city Vadodara as a country (msworld.md §2): ask it as a place.
                if kind == 0 && o.get("feature_type").and_then(|v| v.as_str()).is_some_and(|f| f != "country") {
                    kind = 1;
                }
                o.insert("kind".into(), json!(kind));
                o.insert("kind_label".into(), json!(PLACE_KINDS[kind as usize]));
                pools[kind as usize].push(p);
            }
            let mut rng = Rng::new(seed);
            let mut want = split_counts(&mix, n);
            let mut picked: Vec<Value> = Vec::new();
            for (k, pool) in pools.iter_mut().enumerate() {
                rng.shuffle(pool);
                let take = want[k].min(pool.len());
                picked.extend(pool.drain(..take));
                want[k] -= take;
            }
            // A pool too small for its share: fill from the others, in type order.
            let mut short: usize = want.iter().sum();
            for (k, pool) in pools.iter_mut().enumerate() {
                if short == 0 || mix.get(k).copied().unwrap_or(0.0) <= 0.0 {
                    continue;
                }
                let take = short.min(pool.len());
                picked.extend(pool.drain(..take));
                short -= take;
            }
            rng.shuffle(&mut picked);
            Ok(json!({ "level": level, "seed": seed, "settings": lv, "places": picked }))
        })
        .await?;
    Ok(Json(v))
}

// ---------------------------------------------------------------- charts

pub async fn chart(State(st): St, Path(name): Path<String>) -> ApiResult<Json<Value>> {
    let name = name.trim_end_matches(".json");
    if !matches!(name, "city_climate" | "population") {
        return Err(ApiError::not_found(format!("no chart {name}")));
    }
    let path = st.out.join(format!("media/interactives/{name}.json"));
    let bytes = tokio::fs::read(&path).await.map_err(|_| ApiError::not_found(format!("chart dataset {name} not converted")))?;
    let mut v: Value = serde_json::from_slice(&bytes).map_err(|e| ApiError(axum::http::StatusCode::INTERNAL_SERVER_ERROR, format!("{name}.json: {e}")))?;
    let narration = v.pointer("/narration/file").and_then(|x| x.as_str()).map(str::to_ascii_lowercase);
    if let (Some(file), Some(db), true) = (narration, st.media.clone(), st.media_manifest) {
        let info = db
            .call(move |c| {
                let sha: Option<String> =
                    c.prepare_cached("SELECT sha256 FROM m.file WHERE name = ?1 LIMIT 1")?.query_row([&file], |r| r.get(0)).optional()?;
                Ok(match sha {
                    Some(s) => media_info(c, &s)?,
                    None => None,
                })
            })
            .await?;
        if let Some(n) = v.get_mut("narration").and_then(|n| n.as_object_mut()) {
            n.insert("media".into(), json!(info));
        }
    }
    Ok(Json(v))
}

// ---------------------------------------------------------------- match games and iax media

pub async fn match_games(State(st): St, Query(q): Query<EdQuery>) -> ApiResult<Json<Value>> {
    let ed = edition_of(&q.edition);
    let v = content(&st)?
        .call(move |c| {
            let rows: Vec<(String, String, Option<String>)> = c
                .prepare_cached(
                    "SELECT file_name, edition, title FROM interactive WHERE control_type = 'MatchGame'
                     ORDER BY lower(file_name), (edition = ?1) DESC",
                )?
                .query_map([ed], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
                .collect::<rusqlite::Result<_>>()?;
            let mut seen = HashSet::new();
            let mut games = Vec::new();
            for (file, edition, title) in rows {
                if !seen.insert(file.to_ascii_lowercase()) {
                    continue;
                }
                let raw = title.unwrap_or_default();
                let audience = if raw.starts_with("Adult") { "adult" } else if raw.starts_with("Kids") || edition == "kids" { "kids" } else { "adult" };
                games.push(json!({ "file_name": file, "edition": edition, "title": display_title(&raw), "iax_title": raw,
                    "audience": audience, "items": launch_items(c, &file)? }));
            }
            games.sort_by(|a, b| a["title"].as_str().cmp(&b["title"].as_str()));
            Ok(json!({ "games": games }))
        })
        .await?;
    Ok(Json(v))
}

/// An interactive's definition plus every file it references, resolved (`media`: ref → MediaInfo).
pub async fn interactive_media(State(st): St, Path(file): Path<String>, Query(q): Query<EdQuery>) -> ApiResult<Json<Value>> {
    let ed = edition_of(&q.edition);
    let key = crate::media::disc_name(&file);
    let has_m = st.media_manifest;
    let v = content(&st)?
        .call(move |c| {
            let row = c
                .prepare_cached(
                    "SELECT file_name, edition, package, control_type, title, json FROM interactive WHERE lower(file_name) = ?1
                     ORDER BY (edition = ?2) DESC LIMIT 1",
                )?
                .query_row(params![key, ed], |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, Option<String>>(2)?,
                        r.get::<_, Option<String>>(3)?,
                        r.get::<_, Option<String>>(4)?,
                        r.get::<_, String>(5)?,
                    ))
                })
                .optional()?;
            let Some((file_name, edition, package, control, title, js)) = row else {
                return Err(ApiError::not_found(format!("no interactive {key}")).into());
            };
            let media = if has_m {
                // The descriptor sits in its IAF folder (`dswMedia/IAF/a/<id>/<id>.iax`) in the edition's package.
                let sha: Option<String> = c
                    .prepare_cached(
                        "SELECT sha256 FROM m.file WHERE name = ?1
                         ORDER BY ((container LIKE 'EE/KIDS/%') = (?2 = 'kids')) DESC, (container LIKE '%' || ?3) DESC LIMIT 1",
                    )?
                    .query_row(params![file_name.to_ascii_lowercase(), edition, package.clone().unwrap_or_default()], |r| r.get(0))
                    .optional()?;
                match sha {
                    Some(s) => iax_media(c, &s)?,
                    None => BTreeMap::new(),
                }
            } else {
                BTreeMap::new()
            };
            Ok(json!({
                "file_name": file_name, "edition": edition, "package": package, "control_type": control,
                "title": title.as_deref().map(display_title), "iax_title": title,
                "definition": serde_json::from_str::<Value>(&js).unwrap_or(Value::Null),
                "media": media, "items": launch_items(c, &file_name)?,
            }))
        })
        .await?;
    Ok(Json(v))
}

// ---------------------------------------------------------------- tests (synthetic data only)

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AppState, Config, router};
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    #[test]
    fn rng_is_deterministic_and_shuffle_permutes() {
        let mut a = Rng::new(42);
        let mut b = Rng::new(42);
        assert_eq!(a.next_u64(), b.next_u64());
        let mut v: Vec<u32> = (0..50).collect();
        Rng::new(7).shuffle(&mut v);
        let mut s = v.clone();
        s.sort_unstable();
        assert_eq!(s, (0..50).collect::<Vec<_>>());
        assert_ne!(v, s);
    }

    #[test]
    fn split_counts_follows_mix() {
        assert_eq!(split_counts(&[0.7, 0.2, 0.1], 20), vec![14, 4, 2]);
        assert_eq!(split_counts(&[0.6, 0.35, 0.05], 20), vec![12, 7, 1]);
        assert_eq!(split_counts(&[0.5, 0.5, 0.0], 20), vec![10, 10, 0]);
        assert_eq!(split_counts(&[1.0, 1.0, 1.0], 10).iter().sum::<usize>(), 10);
        assert_eq!(split_counts(&[0.4, 0.6, 0.0], 7), vec![3, 4, 0]);
    }

    #[test]
    fn anchor_prefers_matching_paragraph_then_location() {
        let paras = vec![
            (1, "Zorblax is a made-up land of hills.".to_string()),
            (2, "The Quibble river floods the plains every spring.".to_string()),
            (3, "Farmers fear the Quibble river floods.".to_string()),
            (4, "Nothing relevant here at all.".to_string()),
        ];
        // Both 2 and 3 score the same; location 3 breaks the tie.
        assert_eq!(anchor_paragraph(&paras, "This river floods the plains of Zorblax", "Quibble", 3), Some(2));
        assert_eq!(anchor_paragraph(&paras, "Which river floods?", "Quibble", 3), Some(3));
        assert_eq!(anchor_paragraph(&paras, "Which river floods?", "Quibble", 1), Some(2));
        assert_eq!(anchor_paragraph(&paras, "xyzzy", "plugh", 1), None);
    }

    #[test]
    fn quiz_normalisation() {
        let mc = json!({"@type": "multichoice", "answers": {"@correct": "2", "answer": [
            {"@id": "1", "text": "Alpha", "image": ""}, {"@id": "2", "text": "Beta", "image": {"@w": "10", "@h": "5", "value": "b.jpg"}}]},
            "question": {"text": "Pick <i>one</i>", "image": "", "audio": ""}, "reward": {"text": "Beta it is"}, "credits": "Me"});
        let n = normalise_quiz(9, "deluxe", &mc, &BTreeMap::new());
        assert_eq!(n["type"], "multichoice");
        assert_eq!(n["correct"], "2");
        assert_eq!(n["answers"][1]["image"]["file_name"], "b.jpg");
        assert_eq!(n["answers"][1]["image"]["width"], 10);
        assert_eq!(n["answers"][0]["image"], Value::Null);
        assert_eq!(n["question"]["html"], "Pick <i>one</i>");
        let ta = json!({"@type": "textAnswer", "answers": {"answer": "gamma"}, "question": {"text": "?"}});
        let n = normalise_quiz(10, "kids", &ta, &BTreeMap::new());
        assert_eq!(n["accept"], json!(["gamma"]));
        let ta2 = json!({"@type": "textAnswer", "answers": {"answer": ["x", "y "]}});
        assert_eq!(normalise_quiz(11, "kids", &ta2, &BTreeMap::new())["accept"], json!(["x", "y"]));
    }

    // --- an out/ tree of synthetic data -------------------------------------------------------

    /// Scratch space under the cargo target directory (never the system temp dir).
    fn scratch(prefix: &str) -> tempfile::TempDir {
        let base = std::env::var_os("CARGO_TARGET_DIR")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| std::path::PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../target")))
            .join("tmp");
        std::fs::create_dir_all(&base).unwrap();
        tempfile::Builder::new().prefix(prefix).tempdir_in(base).unwrap()
    }

    fn build_out() -> tempfile::TempDir {
        let dir = scratch("games-test");
        let out = dir.path();
        std::fs::create_dir_all(out.join("media/interactives")).unwrap();
        std::fs::create_dir_all(out.join("atlas")).unwrap();
        let c = Connection::open(out.join("content.sqlite")).unwrap();
        c.execute_batch(
            "CREATE TABLE item (ref_id INTEGER PRIMARY KEY, kind TEXT, title TEXT, caption_html TEXT, in_deluxe INTEGER, in_kids INTEGER, subgroup TEXT);
             CREATE TABLE item_file (ref_id INTEGER, ordinal INTEGER, role TEXT, file_name TEXT);
             CREATE TABLE mindmaze_question (id INTEGER PRIMARY KEY, category INTEGER, category_label TEXT, level INTEGER, clue TEXT, clue_html TEXT, location INTEGER);
             CREATE TABLE mindmaze_answer (question_id INTEGER, ordinal INTEGER, correct INTEGER, text TEXT, text_html TEXT, article_ref_id INTEGER, article_exists INTEGER);
             CREATE TABLE paragraph (ref_id INTEGER, kind TEXT, para_id INTEGER, section_id INTEGER, ordinal INTEGER, text TEXT);
             CREATE TABLE quiz (quiz_id INTEGER, edition TEXT, quiz_type TEXT, lang TEXT, json TEXT);
             CREATE TABLE interactive (file_name TEXT, edition TEXT, package TEXT, control_type TEXT, title TEXT, json TEXT);
             CREATE TABLE geoquiz_level (level INTEGER PRIMARY KEY, questions INTEGER, points TEXT, per_type_unknown TEXT, type_mix TEXT);
             INSERT INTO item VALUES (100, 'article', 'Glimmer River', NULL, 1, 0, NULL), (101, 'article', 'Moon Cheese', NULL, 1, 0, NULL),
                                     (500, 'interactive', 'Pet Quiz', 'Test yourself', 0, 1, 'GAM'), (501, 'interactive', 'Match It', NULL, 1, 0, 'MAG');
             INSERT INTO item_file VALUES (500, 1, 'item', 'aaaa0001.iax'), (500, 2, 'thumb', 'aaaa0001.gtn'), (501, 1, 'item', 'bbbb0002.iax');
             INSERT INTO paragraph VALUES (100, 'p', 1, 1, 1, 'The Glimmer River is long.'), (100, 'p', 2, 1, 2, 'It floods the Plain of Ooze every spring.');",
        )
        .unwrap();
        for id in 1..=12 {
            let (cat, level) = (id % 3, if id <= 8 { 1 } else { 2 });
            c.execute("INSERT INTO mindmaze_question VALUES (?1, ?2, 'Cat', ?3, 'Which river floods the Plain of Ooze?', 'Which river floods the Plain of Ooze?', 7)", params![id, cat, level]).unwrap();
            for ord in 1..=4 {
                c.execute(
                    "INSERT INTO mindmaze_answer VALUES (?1, ?2, ?3, ?4, ?4, ?5, ?6)",
                    params![id, ord, (ord == 1) as i64, if ord == 1 { "Glimmer" } else { "Other" }, if ord == 1 { 100 } else { 999 }, (ord == 1) as i64],
                )
                .unwrap();
            }
        }
        let q1 = json!({"@type": "multichoice", "answers": {"@correct": "1", "answer": [{"@id": "1", "text": "Yes"}, {"@id": "2", "text": "No"}]},
            "question": {"text": "Is it?", "image": {"@w": "20", "@h": "10", "value": "pic.jpg"}}, "reward": {"text": "It is."}});
        let q2 = json!({"@type": "textAnswer", "answers": {"answer": ["cat", "a cat"]}, "question": {"text": "Meow?"}});
        c.execute("INSERT INTO quiz VALUES (1, 'deluxe', 'multichoice', 'A', ?1), (1, 'kids', 'multichoice', 'A', ?1), (2, 'kids', 'textAnswer', 'A', ?2), (3, 'kids', 'textAnswer', 'A', ?2)", params![q1.to_string(), q2.to_string()]).unwrap();
        let bga = json!({"questiongroups": {"qgroup": [{"@id": "1", "@fname": "Pets", "@questions": "1,2,77"}]}});
        let mg = json!({"slides": {"slide": {"standardslide": {"items": {"item": []}}}}});
        c.execute(
            "INSERT INTO interactive VALUES ('aaaa0001.iax', 'kids', 'SWKDC.EIT', 'BGAControl', 'Kids: Pet Quiz', ?1),
                                            ('bbbb0002.iax', 'deluxe', 'SWDLX02.EIT', 'MatchGame', 'Adult: Match It', ?2)",
            params![bga.to_string(), mg.to_string()],
        )
        .unwrap();
        c.execute_batch(
            "INSERT INTO geoquiz_level VALUES (1, 4, '[5,4,3,2,0]', '[0,0,0]', '[0.5,0.5,0.0]'), (2, 20, '[10,7,4,2,0]', '[1,1,1]', '[0.6,0.35,0.05]');",
        )
        .unwrap();
        drop(c);

        let m = Connection::open(out.join("media/manifest.sqlite")).unwrap();
        m.execute_batch(
            "CREATE TABLE file (container TEXT, package TEXT, member_path TEXT, name TEXT, sha256 TEXT, size INTEGER, mime TEXT, kind TEXT, format TEXT, disposition TEXT);
             CREATE TABLE derivative (sha256 TEXT, format TEXT, path TEXT, width INTEGER, height INTEGER, duration_ms INTEGER, bytes INTEGER, out_sha256 TEXT, recipe TEXT, meta TEXT);
             CREATE TABLE catalog_key (edition TEXT, tbl TEXT, key TEXT, package_index INTEGER, package_file TEXT, local_id INTEGER, container TEXT, member_path TEXT, sha256 TEXT);
             CREATE TABLE iax_ref (iax_sha256 TEXT, element TEXT, ref TEXT, resolution TEXT, container TEXT, member_path TEXT, sha256 TEXT);
             INSERT INTO file VALUES ('EE/ENCARTA/SWQUIZ.EIT', 'p', 'dswMedia/quiz/1/1.iax', '1.iax', 'q1iax', 10, 'application/xml', 'interactive', 'xml', 'kept'),
                                     ('EE/ENCARTA/SWQUIZ.EIT', 'p', 'dswMedia/quiz/1/pic.jpg', 'pic.jpg', 'picsha', 10, 'image/jpeg', 'image', 'jpeg', 'kept'),
                                     ('EE/ENCARTA/MINDMAZE.EIT', 'p', 'baggage/knock.wav', 'knock.wav', 'knocksha', 10, 'audio/wav', 'audio', 'wav', 'converted'),
                                     ('EE/ENCARTA/SWALLDSK.EIT', 'p', 'dswMedia/ia/clim_CCL/clim_in.swa', 'clim_in.swa', 'swasha', 10, 'audio/x-swa', 'audio', 'swa', 'converted'),
                                     ('CONTENT/SWDLX02.EIT', 'p', 'dswMedia/IAF/a/bbbb0002/bbbb0002.iax', 'bbbb0002.iax', 'mgiax', 10, 'application/xml', 'interactive', 'xml', 'kept');
             INSERT INTO derivative VALUES ('picsha', 'original', 'o/pi/picsha.jpg', 20, 10, NULL, 10, 'x', 'copy', NULL),
                                           ('knocksha', 'opus', 'd/kn/knocksha.opus', NULL, NULL, 300, 5, 'x', 'ffmpeg', NULL),
                                           ('swasha', 'mp3', 'd/sw/swasha.mp3', NULL, NULL, 8700, 5, 'x', 'strip', NULL),
                                           ('q1iax', 'original', 'o/q1/q1iax.iax', NULL, NULL, NULL, 10, 'x', 'copy', NULL);
             INSERT INTO iax_ref VALUES ('q1iax', 'image', 'pic.jpg', 'relative', 'EE/ENCARTA/SWQUIZ.EIT', 'dswMedia/quiz/1/pic.jpg', 'picsha'),
                                        ('mgiax', 'image', 'pic.jpg', 'iaf-folder', 'EE/ENCARTA/SWQUIZ.EIT', 'x', 'picsha');",
        )
        .unwrap();
        drop(m);

        let g = Connection::open(out.join("atlas/gazetteer.sqlite")).unwrap();
        g.execute_batch(
            "CREATE TABLE place (entity INTEGER PRIMARY KEY, name TEXT, lat REAL, lon REAL, west REAL, south REAL, east REAL, north REAL, goto_km REAL, ft TEXT);
             CREATE VIEW place_full AS SELECT p.*, p.ft AS feature_type, '' AS context, '' AS class_name FROM place p;
             CREATE TABLE quiz_place (level INTEGER, kind INTEGER, region INTEGER, entity INTEGER);
             INSERT INTO place VALUES (1, 'Ruritania', 10, 20, 9, 9, 21, 11, 500, 'country'), (2, 'Grand Fenwick', 30, 40, 29, 29, 41, 31, 300, 'country'),
                                      (3, 'Zenda', 11, 21, 11, 11, 21, 21, 50, 'Sovereign Capital City'), (4, 'Strelsau', 12, 22, 12, 12, 22, 22, 50, 'city'),
                                      (5, 'Odd City', 0, 0, 0, 0, 0, 0, 10, 'city'), (9, 'Imaginaria', 0, 0, -1, -1, 1, 1, 9000, 'continent');
             INSERT INTO quiz_place VALUES (1, 0, 9, 1), (1, 0, 9, 2), (1, 0, 9, 5), (1, 1, 9, 3), (1, 1, 9, 4), (1, 2, 9, 9);",
        )
        .unwrap();
        drop(g);

        std::fs::write(
            out.join("media/interactives/city_climate.json"),
            json!({"id": "clim_CCL", "title": "City Climate", "narration": {"file": "clim_in.swa", "duration_secs": 8.7}, "cities": []}).to_string(),
        )
        .unwrap();
        dir
    }

    async fn get(app: &Router, uri: &str) -> (StatusCode, Value) {
        let res = app.clone().oneshot(Request::get(uri).body(Body::empty()).unwrap()).await.unwrap();
        let status = res.status();
        let body = res.into_body().collect().await.unwrap().to_bytes();
        (status, serde_json::from_slice(&body).unwrap_or(Value::Null))
    }

    #[tokio::test]
    async fn endpoints_on_synthetic_data() {
        let dir = build_out();
        let st = Arc::new(AppState::open(&Config::new(dir.path())));
        let app = router(st);

        let (s, v) = get(&app, "/api/games").await;
        assert_eq!(s, StatusCode::OK, "{v}");
        assert_eq!(v["mindmaze"]["total"], 12);
        assert_eq!(v["charts"][0]["id"], "city_climate");
        assert_eq!(v["match_games"], 1);
        assert_eq!(v["geoquiz"]["levels"][0]["name"], "Navigator");

        let (s, v) = get(&app, "/api/games/mindmaze/game?level=1&categories=1,2&count=3&seed=5").await;
        assert_eq!(s, StatusCode::OK, "{v}");
        let qs = v["questions"].as_array().unwrap();
        assert_eq!(qs.len(), 3);
        assert!(qs.iter().all(|q| q["level"] == 1 && q["category"] != 0));
        let correct = qs[0]["answers"].as_array().unwrap().iter().find(|a| a["correct"] == true).unwrap().clone();
        assert_eq!(correct["para"], 2);
        assert_eq!(correct["article_title"], "Glimmer River");
        let (_, again) = get(&app, "/api/games/mindmaze/game?level=1&categories=1,2&count=3&seed=5").await;
        assert_eq!(again["questions"], v["questions"]);
        // Excluded ids come last.
        let (_, ex) = get(&app, "/api/games/mindmaze/game?level=1&count=8&seed=1&exclude=1,2,3,4,5,6").await;
        let ids: Vec<i64> = ex["questions"].as_array().unwrap().iter().map(|q| q["id"].as_i64().unwrap()).collect();
        assert!(ids[..2].iter().all(|id| *id > 6), "{ids:?}");

        let (s, v) = get(&app, "/api/games/mindmaze/assets").await;
        assert_eq!(s, StatusCode::OK);
        assert_eq!(v["assets"]["knock.wav"]["variants"]["opus"]["url"], "/media/knocksha/opus");

        let (_, v) = get(&app, "/api/games/quizzes?edition=kids").await;
        assert_eq!(v["games"][0]["title"], "Pet Quiz");
        assert_eq!(v["games"][0]["groups"][0]["questions"], json!([1, 2]));
        assert_eq!(v["games"][0]["items"][0]["ref_id"], 500);
        assert_eq!(v["games"][0]["items"][0]["thumb"], "/media/file/aaaa0001.gtn");
        assert_eq!(v["ungrouped"], json!([3]));

        let (s, v) = get(&app, "/api/games/quiz?ids=1,2,404").await;
        assert_eq!(s, StatusCode::OK);
        let qz = v["quizzes"].as_array().unwrap();
        assert_eq!(qz.len(), 2);
        assert_eq!(qz[0]["question"]["image"]["url"], "/media/picsha/original");
        assert_eq!(qz[1]["accept"], json!(["cat", "a cat"]));
        assert_eq!(get(&app, "/api/games/quiz").await.0, StatusCode::BAD_REQUEST);

        let (s, v) = get(&app, "/api/games/geoquiz").await;
        assert_eq!(s, StatusCode::OK, "{v}");
        assert_eq!(v["levels"][0]["pool"], json!([3, 2, 1]));
        let (s, v) = get(&app, "/api/games/geoquiz/1?seed=3").await;
        assert_eq!(s, StatusCode::OK, "{v}");
        let places = v["places"].as_array().unwrap();
        assert_eq!(places.len(), 4);
        // The non-country in the country pool is asked as a place.
        assert!(places.iter().all(|p| p["entity"] != 5 || p["kind"] == 1));
        assert!(places.iter().all(|p| p["kind"] != 2), "mix gives continents no share");
        assert_eq!(places[0]["region_name"], "Imaginaria");
        assert_eq!(get(&app, "/api/games/geoquiz/9").await.0, StatusCode::NOT_FOUND);

        let (s, v) = get(&app, "/api/games/chart/city_climate").await;
        assert_eq!(s, StatusCode::OK);
        assert_eq!(v["narration"]["media"]["variants"]["mp3"]["url"], "/media/swasha/mp3");
        assert_eq!(get(&app, "/api/games/chart/population").await.0, StatusCode::NOT_FOUND);
        assert_eq!(get(&app, "/api/games/chart/secrets").await.0, StatusCode::NOT_FOUND);

        let (_, v) = get(&app, "/api/games/match").await;
        assert_eq!(v["games"][0]["title"], "Match It");
        assert_eq!(v["games"][0]["audience"], "adult");
        let (s, v) = get(&app, "/api/games/interactive/BBBB0002.iax").await;
        assert_eq!(s, StatusCode::OK, "{v}");
        assert_eq!(v["control_type"], "MatchGame");
        assert_eq!(v["media"]["pic.jpg"]["variants"]["original"]["url"], "/media/picsha/original");
        assert_eq!(get(&app, "/api/games/interactive/nope.iax").await.0, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn empty_data_dir() {
        let dir = scratch("games-empty");
        let app = router(Arc::new(AppState::open(&Config::new(dir.path()))));
        let (s, v) = get(&app, "/api/games").await;
        assert_eq!(s, StatusCode::OK);
        assert_eq!(v["mindmaze"], Value::Null);
        assert_eq!(get(&app, "/api/games/geoquiz/1").await.0, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(get(&app, "/api/games/quizzes").await.0, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(get(&app, "/api/games/mindmaze/assets").await.0, StatusCode::SERVICE_UNAVAILABLE);
    }
}
