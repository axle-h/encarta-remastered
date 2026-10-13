//! Lingo literal data held in field text: linear lists `[1, "a", #b]`, property lists
//! `[#key: value, "k": value]`, the empty property list `[:]`, quoted strings (no escapes),
//! `#symbols`, integers, floats, and the constants `VOID`, `TRUE`, `FALSE`, `EMPTY`.
//! This is a data parser, not an interpreter: nothing is evaluated.
//! Spec: `docs/formats/shockwave.md` §2.2 ("Lingo literal syntax").

use anyhow::{Result, bail};
use serde_json::{Map, Value as Json};

#[derive(Debug, Clone, PartialEq)]
pub enum Lingo {
    Void,
    Int(i64),
    Float(f64),
    Str(String),
    Symbol(String),
    List(Vec<Lingo>),
    /// keys keep their order; Lingo allows any value as a key
    PropList(Vec<(Lingo, Lingo)>),
}

impl Lingo {
    /// Property lookup. Symbol keys compare case-insensitively (as in Lingo); string keys exactly.
    pub fn get(&self, key: &str) -> Option<&Lingo> {
        let Lingo::PropList(items) = self else {
            return None;
        };
        items.iter().find_map(|(k, v)| match k {
            Lingo::Symbol(s) if s.eq_ignore_ascii_case(key) => Some(v),
            Lingo::Str(s) if s == key => Some(v),
            _ => None,
        })
    }

    /// `a.get(k1)?.get(k2)?…`
    pub fn path(&self, keys: &[&str]) -> Option<&Lingo> {
        keys.iter().try_fold(self, |v, k| v.get(k))
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Lingo::Str(s) | Lingo::Symbol(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match *self {
            Lingo::Int(i) => Some(i as f64),
            Lingo::Float(f) => Some(f),
            _ => None,
        }
    }

    pub fn as_i64(&self) -> Option<i64> {
        match *self {
            Lingo::Int(i) => Some(i),
            _ => None,
        }
    }

    pub fn items(&self) -> &[Lingo] {
        match self {
            Lingo::List(v) => v,
            _ => &[],
        }
    }

    pub fn props(&self) -> &[(Lingo, Lingo)] {
        match self {
            Lingo::PropList(v) => v,
            _ => &[],
        }
    }

    /// The key text of a property-list key (`#city` → `city`).
    pub fn key_text(&self) -> String {
        match self {
            Lingo::Str(s) | Lingo::Symbol(s) => s.clone(),
            other => other.to_json().to_string(),
        }
    }

    /// JSON view: lists → arrays, property lists → objects (keys without `#`; a repeated key keeps
    /// its first value), symbols → `"#name"`, VOID → null.
    pub fn to_json(&self) -> Json {
        match self {
            Lingo::Void => Json::Null,
            Lingo::Int(i) => Json::from(*i),
            Lingo::Float(f) => serde_json::Number::from_f64(*f).map_or(Json::Null, Json::Number),
            Lingo::Str(s) => Json::String(s.clone()),
            Lingo::Symbol(s) => Json::String(format!("#{s}")),
            Lingo::List(v) => Json::Array(v.iter().map(Lingo::to_json).collect()),
            Lingo::PropList(v) => {
                let mut m = Map::new();
                for (k, val) in v {
                    m.entry(k.key_text()).or_insert_with(|| val.to_json());
                }
                Json::Object(m)
            }
        }
    }
}

/// Parse one literal; trailing whitespace is allowed, anything else after it is an error.
pub fn parse(text: &str) -> Result<Lingo> {
    let mut p = Parser {
        s: text.as_bytes(),
        text,
        i: 0,
    };
    let v = p.value()?;
    p.ws();
    if p.i != p.s.len() {
        bail!(
            "unexpected text after literal at byte {}: {:?}",
            p.i,
            p.snippet()
        );
    }
    Ok(v)
}

struct Parser<'a> {
    s: &'a [u8],
    text: &'a str,
    i: usize,
}

impl Parser<'_> {
    fn ws(&mut self) {
        while self.i < self.s.len() {
            match self.s[self.i] {
                b' ' | b'\t' | b'\r' | b'\n' => self.i += 1,
                // `¬` (Lingo line continuation) is not used in data, but tolerate a comment
                b'-' if self.s.get(self.i + 1) == Some(&b'-') => {
                    while self.i < self.s.len() && self.s[self.i] != b'\n' {
                        self.i += 1;
                    }
                }
                _ => break,
            }
        }
    }

    fn peek(&mut self) -> Option<u8> {
        self.ws();
        self.s.get(self.i).copied()
    }

    fn snippet(&self) -> String {
        let end = (self.i + 30).min(self.text.len());
        let mut start = self.i.min(end);
        while !self.text.is_char_boundary(start) {
            start -= 1;
        }
        let mut e = end;
        while !self.text.is_char_boundary(e) {
            e += 1;
        }
        self.text[start..e].to_owned()
    }

    fn value(&mut self) -> Result<Lingo> {
        match self.peek() {
            None => bail!("unexpected end of text"),
            Some(b'[') => self.list(),
            Some(b'"') => Ok(Lingo::Str(self.string()?)),
            Some(b'#') => {
                self.i += 1;
                Ok(Lingo::Symbol(self.ident()?))
            }
            Some(c) if c == b'-' || c == b'+' || c == b'.' || c.is_ascii_digit() => self.number(),
            Some(c) if c.is_ascii_alphabetic() || c == b'_' => {
                let id = self.ident()?;
                Ok(match id.to_ascii_lowercase().as_str() {
                    "void" => Lingo::Void,
                    "true" => Lingo::Int(1),
                    "false" => Lingo::Int(0),
                    "empty" => Lingo::Str(String::new()),
                    _ => bail!("unsupported identifier {id:?} (only data literals are accepted)"),
                })
            }
            Some(_) => bail!(
                "unexpected character at byte {}: {:?}",
                self.i,
                self.snippet()
            ),
        }
    }

    fn ident(&mut self) -> Result<String> {
        let start = self.i;
        while self.i < self.s.len()
            && (self.s[self.i].is_ascii_alphanumeric() || self.s[self.i] == b'_')
        {
            self.i += 1;
        }
        if start == self.i {
            bail!("expected a name at byte {start}");
        }
        Ok(self.text[start..self.i].to_owned())
    }

    fn string(&mut self) -> Result<String> {
        let start = self.i + 1;
        let Some(len) = self.s[start..].iter().position(|&b| b == b'"') else {
            bail!("unterminated string at byte {}", self.i)
        };
        self.i = start + len + 1;
        Ok(self.text[start..start + len].to_owned())
    }

    fn number(&mut self) -> Result<Lingo> {
        let start = self.i;
        if matches!(self.s[self.i], b'-' | b'+') {
            self.i += 1;
        }
        let mut float = false;
        while self.i < self.s.len() {
            match self.s[self.i] {
                b'0'..=b'9' => {}
                b'.' => float = true,
                b'e' | b'E' if self.i > start => {
                    float = true;
                    if matches!(self.s.get(self.i + 1), Some(b'-' | b'+')) {
                        self.i += 1;
                    }
                }
                _ => break,
            }
            self.i += 1;
        }
        let t = &self.text[start..self.i];
        if !float && let Ok(v) = t.parse::<i64>() {
            return Ok(Lingo::Int(v));
        }
        match t.parse::<f64>() {
            Ok(v) => Ok(Lingo::Float(v)),
            Err(_) => bail!("bad number {t:?} at byte {start}"),
        }
    }

    fn list(&mut self) -> Result<Lingo> {
        self.i += 1; // [
        if self.peek() == Some(b']') {
            self.i += 1;
            return Ok(Lingo::List(Vec::new()));
        }
        if self.peek() == Some(b':') {
            self.i += 1;
            if self.peek() != Some(b']') {
                bail!("expected ] after [: at byte {}", self.i);
            }
            self.i += 1;
            return Ok(Lingo::PropList(Vec::new()));
        }
        let mut linear = Vec::new();
        let mut props = Vec::new();
        loop {
            let first = self.value()?;
            if self.peek() == Some(b':') {
                self.i += 1;
                if !linear.is_empty() {
                    bail!("property after linear items at byte {}", self.i);
                }
                props.push((first, self.value()?));
            } else {
                if !props.is_empty() {
                    bail!("linear item after properties at byte {}", self.i);
                }
                linear.push(first);
            }
            match self.peek() {
                Some(b',') => self.i += 1,
                Some(b']') => {
                    self.i += 1;
                    break;
                }
                _ => bail!("expected , or ] at byte {}: {:?}", self.i, self.snippet()),
            }
        }
        Ok(if props.is_empty() {
            Lingo::List(linear)
        } else {
            Lingo::PropList(props)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn nested_property_lists() {
        let v = parse(
            r#"[#Town: "Testville", #Code:#tv, #vals:[1, -2.5, "na"], #x: [#Lo:-30,#hi: 9e1]]"#,
        )
        .unwrap();
        assert_eq!(v.get("town").and_then(Lingo::as_str), Some("Testville"));
        assert_eq!(v.path(&["X", "lo"]).and_then(Lingo::as_i64), Some(-30));
        assert_eq!(
            v.to_json(),
            json!({"Town": "Testville", "Code": "#tv", "vals": [1, -2.5, "na"], "x": {"Lo": -30, "hi": 90.0}})
        );
    }

    #[test]
    fn empty_lists_strings_and_constants() {
        assert_eq!(parse("[]").unwrap(), Lingo::List(vec![]));
        assert_eq!(parse(" [:] \n").unwrap(), Lingo::PropList(vec![]));
        assert_eq!(
            parse(r#""a, b: [c]""#).unwrap(),
            Lingo::Str("a, b: [c]".into())
        );
        assert_eq!(
            parse("[VOID, TRUE]").unwrap(),
            Lingo::List(vec![Lingo::Void, Lingo::Int(1)])
        );
        let v = parse(r#"["123": ["AB", "Town"], "456": ["CD"]]"#).unwrap();
        assert_eq!(v.get("456").unwrap().items().len(), 1);
    }

    #[test]
    fn rejects_code_and_garbage() {
        assert!(parse("[1, 2").is_err());
        assert!(parse("[1, #a: 2]").is_err());
        assert!(parse("[#a: 1, 2]").is_err());
        assert!(parse("put 1").is_err());
        assert!(parse("[1] x").is_err());
        assert!(parse(r#""open"#).is_err());
    }
}
