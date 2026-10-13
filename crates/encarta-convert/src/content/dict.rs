//! EDICT dictionaries (`EE/ENCARTA/EDICT/*.EIT`): the English dictionary (DICTA), thesaurus
//! (THESA) and the eight bilingual dictionaries. Each file is one entry named by its content id.
//!
//! The entry XML is converted losslessly to a tree of our own JSON: formatting elements become our
//! inline nodes, cross-references become links, and every other element becomes a structural node
//! `{"t":"d","n":"<lower-case name>","a":{attrs},"c":[…]}` that our renderer turns into a `div` or
//! `span` with class `enc-d-<name>`.

use super::entities;
use super::render;
use super::track::Tracker;
use super::xml::{Element, Node};
use serde_json::{Map, Value, json};
use std::collections::HashMap;
use std::fmt::Write;

/// Elements rendered as blocks (the rest are spans).
fn is_block(n: &str) -> bool {
    matches!(
        n,
        "DICTIONARY"
            | "HOMOGRAPH"
            | "HEAD"
            | "BODY"
            | "TAIL"
            | "POS"
            | "SENSES"
            | "SENSE"
            | "PHRASE-SENSE"
            | "ETYMOLOGY"
            | "RUNONS"
            | "ONE-RUNON"
            | "PHRASES"
            | "ONE-PHRASE"
            | "EXAMPLES"
            | "USAGE-NOTE"
            | "ESSAYS"
            | "OTHER-ESSAY"
            | "SYNONYM-ESSAY"
            | "PARA"
            | "CITATIONS"
            | "CITATION"
            | "XREF-GROUP"
            | "THES_ARTICLE"
            | "THESAURUS"
            | "SYNONYMGROUP"
            | "ANTONYMGROUP"
            | "ARTICLE"
            | "INTRO"
            | "ALL-SENSES"
            | "POS-XREFS"
    )
}

/// Build-time paths with no meaning outside Microsoft's network (`\\TKXTICTFS\…`).
fn is_build_path(n: &str) -> bool {
    matches!(n, "WAV_AUDIO_PATH" | "MP3_AUDIO_PATH")
}

pub struct DictCx<'a> {
    pub t: &'a mut Tracker,
    /// GUID (`HOMOGRAPH@ID`, `SENSE@ID` …) → entry ref id, for `Target`-only references.
    pub guids: &'a HashMap<String, u32>,
    pub audio: Vec<(String, Option<u32>)>,
}

fn text_node(s: &str) -> Value {
    Value::String(s.to_string())
}

pub fn convert_children(cx: &mut DictCx, e: &Element) -> Vec<Value> {
    let mut out: Vec<Value> = Vec::new();
    // Whitespace-only text with a line break or tab is source indentation; a lone space is kept.
    for c in &e.children {
        match c {
            Node::Text(s) if s.trim().is_empty() && s.contains(['\n', '\t']) => {}
            Node::Text(s) => push_text(&mut out, s),
            Node::El(x) => {
                if let Some(v) = convert(cx, x) {
                    match v {
                        Value::String(s) => push_text(&mut out, &s),
                        v => out.push(v),
                    }
                }
            }
        }
    }
    out
}

fn push_text(out: &mut Vec<Value>, s: &str) {
    if s.is_empty() {
        return;
    }
    if let Some(Value::String(prev)) = out.last_mut() {
        prev.push_str(s);
    } else {
        out.push(text_node(s));
    }
}

fn styled(cx: &mut DictCx, e: &Element, t: &str) -> Value {
    cx.t.el(e);
    json!({"t": t, "c": convert_children(cx, e)})
}

pub fn convert(cx: &mut DictCx, e: &Element) -> Option<Value> {
    let n = e.name.as_str();
    Some(match n {
        "ITAL" => styled(cx, e, "em"),
        "BOLD" | "B" => styled(cx, e, "strong"),
        "SUP" => styled(cx, e, "sup"),
        "SUB" => styled(cx, e, "sub"),
        "BR" => {
            cx.t.el(e);
            json!({"t": "br"})
        }
        "DBLBR" => {
            cx.t.el(e);
            json!({"t": "d", "n": "dblbr", "c": [{"t": "br"}, {"t": "br"}]})
        }
        "entity" => {
            cx.t.el(e);
            let tag = cx.t.attr(e, "tag").unwrap_or("");
            match entities::entity_owned(tag) {
                Some(s) => Value::String(s),
                None => {
                    cx.t.note("unknown entity", tag);
                    return None;
                }
            }
        }
        _ if is_build_path(n) => {
            cx.t.el(e);
            cx.t.note("dropped", "dictionary build-machine audio path");
            return None;
        }
        "FG_AUDIO_WAV" => {
            cx.t.el(e);
            let file = super::data::baggage_key(&e.text());
            json!({"t": "d", "n": "audio", "file": file})
        }
        _ => {
            cx.t.el_all(e);
            let target_ref = e.attr("TargetRefID").and_then(|v| v.parse::<u32>().ok());
            let target = e.attr("Target");
            let media_ref = if n == "Media" {
                e.attr("RefID").and_then(|v| v.parse::<u32>().ok())
            } else {
                None
            };
            if n == "AUDIO_PRON" {
                let f = e
                    .child("FG_AUDIO_WAV")
                    .map(|x| super::data::baggage_key(&x.text()))
                    .unwrap_or_default();
                cx.audio.push((f, e.attr("RefID").and_then(|v| v.parse().ok())));
            }
            let resolved = target_ref
                .or(media_ref)
                .or_else(|| target.and_then(|g| cx.guids.get(g).copied()));
            if target.is_some() && resolved.is_none() {
                cx.t.note("dictionary", "unresolved Target GUID");
            }
            let mut m = Map::new();
            m.insert("t".into(), "d".into());
            m.insert("n".into(), n.to_ascii_lowercase().into());
            if !e.attrs.is_empty() {
                let mut a = Map::new();
                for (k, v) in &e.attrs {
                    a.insert(k.clone(), v.clone().into());
                }
                m.insert("a".into(), Value::Object(a));
            }
            if let Some(r) = resolved {
                m.insert("ref".into(), r.into());
            }
            m.insert("c".into(), Value::Array(convert_children(cx, e)));
            Value::Object(m)
        }
    })
}

pub fn render(v: &[Value], out: &mut String) {
    for n in v {
        render_node(n, out);
    }
}

fn render_node(n: &Value, out: &mut String) {
    match n {
        Value::String(s) => render::esc(s, out),
        Value::Object(m) => {
            let c = m.get("c").and_then(Value::as_array).map_or(&[][..], |v| v);
            match m.get("t").and_then(Value::as_str) {
                Some("em") => wrap("em", "", c, out),
                Some("strong") => wrap("strong", "", c, out),
                Some("sup") => wrap("sup", "", c, out),
                Some("sub") => wrap("sub", "", c, out),
                Some("br") => out.push_str("<br>"),
                Some("d") => {
                    let name = m.get("n").and_then(Value::as_str).unwrap_or("x");
                    let upper = name.to_ascii_uppercase();
                    if name == "audio" {
                        let f = m.get("file").and_then(Value::as_str).unwrap_or("");
                        let _ = write!(
                            out,
                            "<button class=\"enc-d-audio\" data-file=\"{0}\" data-src=\"/media/file/{0}\">\u{1F50A}</button>",
                            render::escaped(f)
                        );
                        return;
                    }
                    if let Some(r) = m.get("ref").and_then(Value::as_u64) {
                        let _ = write!(
                            out,
                            "<a class=\"enc-d-{name} enc-d-link\" href=\"{}\" data-ref=\"{r}\">",
                            render::item_href(r as u32, None)
                        );
                        render(c, out);
                        out.push_str("</a>");
                        return;
                    }
                    let tag = if is_block(&upper) { "div" } else { "span" };
                    let _ = write!(out, "<{tag} class=\"enc-d-{name}\">");
                    render(c, out);
                    let _ = write!(out, "</{tag}>");
                }
                _ => render(c, out),
            }
        }
        _ => {}
    }
}

fn wrap(tag: &str, class: &str, c: &[Value], out: &mut String) {
    if class.is_empty() {
        let _ = write!(out, "<{tag}>");
    } else {
        let _ = write!(out, "<{tag} class=\"{class}\">");
    }
    render(c, out);
    let _ = write!(out, "</{tag}>");
}

/// Renders an entry; a bilingual entry's `Headword` (stored last in the source) is shown first.
pub fn render_entry(v: &Value, out: &mut String) {
    let c = v.get("c").and_then(Value::as_array);
    match (v.get("n").and_then(Value::as_str), c) {
        (Some("article"), Some(c)) => {
            out.push_str("<div class=\"enc-d-article\">");
            let is_head = |n: &Value| n.get("n").and_then(Value::as_str) == Some("headword");
            for n in c.iter().filter(|n| is_head(n)) {
                render_node(n, out);
            }
            for n in c.iter().filter(|n| !is_head(n)) {
                render_node(n, out);
            }
            out.push_str("</div>");
        }
        _ => render_node(v, out),
    }
}

/// A one-line summary: the quick definitions (dictionary), the first synonyms (thesaurus) or
/// the start of the translation (bilingual).
pub fn summary(root: &Element) -> String {
    let mut quick = Vec::new();
    let mut defs = Vec::new();
    let mut syn = Vec::new();
    root.walk(&mut |x, _| match x.name.as_str() {
        "QUICK-DEFINITION" => quick.push(super::model::squash(&x.text())),
        "DEF" if defs.len() < 3 => defs.push(super::model::squash(&x.text())),
        "SYNONYM" if syn.len() < 8 => syn.push(super::model::squash(&x.text())),
        _ => {}
    });
    if !quick.is_empty() {
        return quick.join("; ");
    }
    if !defs.is_empty() {
        return defs.join("; ");
    }
    if !syn.is_empty() {
        return syn.join(", ");
    }
    if root.name == "ARTICLE" {
        let mut s = String::new();
        for n in &root.children {
            match n {
                Node::Text(t) => s.push_str(t),
                Node::El(e) if e.name != "Headword" => {
                    s.push_str(&e.text());
                    s.push(' ');
                }
                _ => {}
            }
        }
        let s = super::model::squash(&s);
        return s.chars().take(200).collect();
    }
    String::new()
}

/// Collects every `ID` GUID in an entry (for resolving `Target`-only cross-references).
pub fn collect_guids(e: &Element, ref_id: u32, out: &mut Vec<(String, u32)>) {
    e.walk(&mut |x, _| {
        if let Some(g) = x.attr("ID") {
            out.push((g.to_string(), ref_id));
        }
    });
}

/// Headword forms of an entry for lookup: `(form, kind)`.
pub fn forms(root: &Element) -> Vec<(String, &'static str)> {
    let mut out = Vec::new();
    let strip = |s: &str| super::model::squash(&s.replace('·', ""));
    root.walk(&mut |x, _| {
        let kind = match x.name.as_str() {
            "DisplayHeadword" => Some("headword"),
            "Headword" => Some("headword"),
            "ONE-INFLECTION" => Some("inflection"),
            "RUNON" => Some("runon"),
            "PHRASE-HEAD" => Some("phrase"),
            "VARIANT" | "ALSO-VARIANT" => Some("variant"),
            _ => None,
        };
        if let Some(k) = kind {
            let text = match x.child("ENTRY") {
                Some(en) => en.text(),
                None => x.text(),
            };
            let f = strip(&text);
            if !f.is_empty() {
                out.push((f, k));
            }
        }
    });
    // Thesaurus head word
    if root.name == "THES_ARTICLE"
        && let Some(h) = root.child("THESAURUS").and_then(|t| t.child("HEAD"))
    {
        out.push((super::model::squash(&h.text()), "headword"));
    }
    out.sort();
    out.dedup();
    out
}

/// Strips the bilingual homograph marker: `borne  (2)` → `borne`.
pub fn clean_headword(s: &str) -> String {
    let s = super::model::squash(s);
    match s.rfind(" (") {
        Some(i) if s.ends_with(')') && s[i + 2..s.len() - 1].bytes().all(|b| b.is_ascii_digit()) => {
            s[..i].to_string()
        }
        _ => s,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::xml;

    #[test]
    fn converts_entry_and_resolves_guid() {
        let d = xml::parse(r#"<DICTIONARY><HOMOGRAPH ID="G1" Variety="US"><HEAD><HEADWORD><ENTRY>ab·la·tive</ENTRY><DisplayHeadword>ablative</DisplayHeadword></HEADWORD></HEAD><BODY><SENSE ID="G2"><DEF>see <XREF Target="G9">case</XREF> <ITAL>x</ITAL></DEF></SENSE></BODY><AUDIO_PRON RefID="5"><FG_AUDIO_WAV>msencdata::baggage/5.wav</FG_AUDIO_WAV><WAV_AUDIO_PATH>\\X\a.WAV</WAV_AUDIO_PATH></AUDIO_PRON></HOMOGRAPH></DICTIONARY>"#).unwrap();
        let mut t = Tracker::default();
        let mut g = HashMap::new();
        g.insert("G9".to_string(), 77u32);
        let mut cx = DictCx {
            t: &mut t,
            guids: &g,
            audio: vec![],
        };
        let v = convert_children(&mut cx, &d.root);
        assert_eq!(cx.audio, vec![("5.wav".to_string(), Some(5))]);
        let mut h = String::new();
        render(&v, &mut h);
        assert!(h.contains("<a class=\"enc-d-xref enc-d-link\" href=\"/item/77\" data-ref=\"77\">case</a>"));
        assert!(h.contains("data-file=\"5.wav\""));
        assert!(!h.contains("TKX") && !h.contains("a.WAV"));
        assert_eq!(forms(&d.root), vec![("ablative".to_string(), "headword")]);
        assert_eq!(clean_headword("borne  (2)"), "borne");
    }
}
