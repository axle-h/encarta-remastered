//! API errors (`{"error": "…"}`) and small JSON helpers.

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use rusqlite::types::ValueRef;
use serde_json::{Map, Value, json};

#[derive(Debug)]
pub struct ApiError(pub StatusCode, pub String);

pub type ApiResult<T> = Result<T, ApiError>;

impl ApiError {
    pub fn not_found(what: impl Into<String>) -> Self {
        ApiError(StatusCode::NOT_FOUND, what.into())
    }
    pub fn bad_request(what: impl Into<String>) -> Self {
        ApiError(StatusCode::BAD_REQUEST, what.into())
    }
    pub fn unavailable(what: impl Into<String>) -> Self {
        ApiError(StatusCode::SERVICE_UNAVAILABLE, what.into())
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, Json(json!({ "error": self.1 }))).into_response()
    }
}

impl From<anyhow::Error> for ApiError {
    fn from(e: anyhow::Error) -> Self {
        match e.downcast_ref::<ApiError>() {
            Some(a) => ApiError(a.0, a.1.clone()),
            None => {
                println!("error: {e:#}");
                ApiError(StatusCode::INTERNAL_SERVER_ERROR, format!("{e:#}"))
            }
        }
    }
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.0, self.1)
    }
}

impl std::error::Error for ApiError {}

impl From<rusqlite::Error> for ApiError {
    fn from(e: rusqlite::Error) -> Self {
        anyhow::Error::from(e).into()
    }
}

/// One SQLite value as JSON. Text columns whose name ends in `_json` are parsed.
pub fn value(v: ValueRef<'_>, parse: bool) -> Value {
    match v {
        ValueRef::Null => Value::Null,
        ValueRef::Integer(i) => json!(i),
        ValueRef::Real(f) => json!(f),
        ValueRef::Text(t) => {
            let s = String::from_utf8_lossy(t);
            if parse { serde_json::from_str(&s).unwrap_or(Value::String(s.into_owned())) } else { Value::String(s.into_owned()) }
        }
        ValueRef::Blob(b) => json!(b.len()),
    }
}

/// A row as a JSON object: NULL columns omitted, `x_json` columns parsed into `x`.
pub fn row_object(row: &rusqlite::Row<'_>) -> rusqlite::Result<Map<String, Value>> {
    let stmt = row.as_ref();
    let mut m = Map::new();
    for i in 0..stmt.column_count() {
        let name = stmt.column_name(i)?;
        let v = row.get_ref(i)?;
        if matches!(v, ValueRef::Null) {
            continue;
        }
        match name.strip_suffix("_json") {
            Some(base) => m.insert(base.to_string(), value(v, true)),
            None => m.insert(name.to_string(), value(v, false)),
        };
    }
    Ok(m)
}

/// Runs `sql` and returns every row as an object.
pub fn query_objects(conn: &rusqlite::Connection, sql: &str, params: impl rusqlite::Params) -> rusqlite::Result<Vec<Value>> {
    let mut st = conn.prepare_cached(sql)?;
    let rows = st.query_map(params, |r| row_object(r).map(Value::Object))?;
    rows.collect()
}

pub fn editions(deluxe: bool, kids: bool) -> Vec<&'static str> {
    let mut v = Vec::new();
    if deluxe {
        v.push("deluxe");
    }
    if kids {
        v.push("kids");
    }
    v
}
