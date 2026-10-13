//! Data tables inside DATA records: `<table>` (statistical tables), `<quickfacts>` (Quick Facts
//! boxes) and `<faf>` (country "Facts and Figures"). Converted to one table model of our own.

use super::inline::{self, Ctx};
use super::model::{self, Inline};
use super::render;
use super::track::Tracker;
use super::xml::Element;
use serde_json::{Map, Value, json};
use std::fmt::Write;

fn cell_inlines(t: &mut Tracker, e: &Element) -> Vec<Inline> {
    t.el(e);
    let mut cx = Ctx {
        t,
        title: None,
        squash: true,
    };
    inline::children(&mut cx, e)
}

fn align(t: &mut Tracker, e: &Element) -> Option<String> {
    t.attr(e, "align").map(str::to_string)
}

/// `<table>`: head (columns, optional column-group rows), body rows, foot (footnotes, source).
pub fn table(t: &mut Tracker, e: &Element) -> Value {
    t.el(e);
    t.attr(e, "revision");
    let mut out = Map::new();
    out.insert("t".into(), "table".into());
    let mut header_rows: Vec<Value> = Vec::new();
    let mut columns: Vec<Value> = Vec::new();
    let mut rows: Vec<Value> = Vec::new();
    let mut footnotes: Vec<Value> = Vec::new();
    let mut source: Option<Value> = None;
    let mut titles: Vec<Value> = Vec::new();
    let mut intro: Vec<Value> = Vec::new();
    for part in e.elements() {
        t.el(part);
        match part.name.as_str() {
            "head" => {
                for h in part.elements() {
                    match h.name.as_str() {
                        "columns" => {
                            t.el(h);
                            for c in h.elements() {
                                match c.name.as_str() {
                                    "column" => {
                                        let a = align(t, c);
                                        let w = t.attr_u32(c, "width");
                                        let label = cell_inlines(t, c);
                                        columns.push(json!({"c": model::inlines_json(&label), "align": a, "width": w}));
                                    }
                                    "colrow" => {
                                        t.el(c);
                                        let mut cells = Vec::new();
                                        for g in c.elements() {
                                            match g.name.as_str() {
                                                "colgroup" => {
                                                    let start = t.attr_u32(g, "start");
                                                    let span = t.attr_u32(g, "span");
                                                    let label = cell_inlines(t, g);
                                                    cells.push(json!({"c": model::inlines_json(&label), "start": start, "span": span}));
                                                }
                                                _ => cells.push(cell(t, g)),
                                            }
                                        }
                                        header_rows.push(Value::Array(cells));
                                    }
                                    _ => t.note("table", &format!("columns/<{}>", c.name)),
                                }
                            }
                        }
                        "title" => {
                            align(t, h);
                            titles.push(model::inlines_json(&cell_inlines(t, h)));
                        }
                        "intro" => {
                            align(t, h);
                            t.attr(h, "id");
                            intro.push(model::inlines_json(&cell_inlines(t, h)));
                        }
                        _ => t.note("table", &format!("head/<{}>", h.name)),
                    }
                }
            }
            "body" => {
                for r in part.elements() {
                    match r.name.as_str() {
                        "row" => {
                            t.el(r);
                            let divider = t.attr(r, "divider") == Some("1");
                            let cells: Vec<Value> = r.elements().map(|c| cell(t, c)).collect();
                            rows.push(json!({"cells": cells, "divider": divider}));
                        }
                        "entry" => rows.push(entry(t, r)),
                        _ => t.note("table", &format!("body/<{}>", r.name)),
                    }
                }
            }
            "foot" => {
                for f in part.elements() {
                    match f.name.as_str() {
                        "footnotes" => {
                            t.el(f);
                            for n in f.elements() {
                                align(t, n);
                                footnotes.push(model::inlines_json(&cell_inlines(t, n)));
                            }
                        }
                        "source" => {
                            align(t, f);
                            source = Some(model::inlines_json(&cell_inlines(t, f)));
                        }
                        _ => t.note("table", &format!("foot/<{}>", f.name)),
                    }
                }
            }
            other => t.note("table", &format!("<{other}>")),
        }
    }
    if !titles.is_empty() {
        out.insert("title".into(), Value::Array(titles));
    }
    if !intro.is_empty() {
        out.insert("intro".into(), Value::Array(intro));
    }
    out.insert("columns".into(), Value::Array(columns));
    if !header_rows.is_empty() {
        out.insert("header_rows".into(), Value::Array(header_rows));
    }
    out.insert("rows".into(), Value::Array(rows));
    if !footnotes.is_empty() {
        out.insert("footnotes".into(), Value::Array(footnotes));
    }
    if let Some(s) = source {
        out.insert("source".into(), s);
    }
    Value::Object(out)
}

fn cell(t: &mut Tracker, c: &Element) -> Value {
    if c.name != "cell" {
        t.note("table", &format!("row/<{}>", c.name));
    }
    let a = align(t, c);
    let span = t.attr_u32(c, "colspan").filter(|&s| s != 1);
    let v = cell_inlines(t, c);
    let mut m = Map::new();
    m.insert("c".into(), model::inlines_json(&v));
    if let Some(a) = a {
        m.insert("align".into(), a.into());
    }
    if let Some(s) = span {
        m.insert("colspan".into(), s.into());
    }
    Value::Object(m)
}

/// A facts entry: `name`, `value`, nested entries and stat/map links.
fn entry(t: &mut Tracker, e: &Element) -> Value {
    t.el(e);
    let quick = t.attr(e, "quick") == Some("1");
    let mut m = Map::new();
    let mut values: Vec<Value> = Vec::new();
    let mut children: Vec<Value> = Vec::new();
    let mut links: Vec<Value> = Vec::new();
    for c in e.elements() {
        match c.name.as_str() {
            "name" => {
                align(t, c);
                m.insert("name".into(), model::inlines_json(&cell_inlines(t, c)));
            }
            "value" => {
                align(t, c);
                values.push(model::inlines_json(&cell_inlines(t, c)));
            }
            "entry" => children.push(entry(t, c)),
            "links" => {
                t.el(c);
                let mut cx = Ctx {
                    t,
                    title: None,
                    squash: true,
                };
                for l in inline::children(&mut cx, c) {
                    if let Inline::Link(l) = l {
                        links.push(json!({"url": l.url, "ref": l.ref_id, "c": model::inlines_json(&l.c)}));
                    }
                }
            }
            other => t.note("facts", &format!("entry/<{other}>")),
        }
    }
    if quick {
        m.insert("quick".into(), true.into());
    }
    // Most entries have one value; a few have none or a trailing empty one.
    values.retain(|v| v.as_array().is_some_and(|a| !a.is_empty()));
    match values.len() {
        0 => {}
        1 => {
            m.insert("value".into(), values.pop().unwrap());
        }
        _ => {
            m.insert("values".into(), Value::Array(values));
        }
    }
    if !links.is_empty() {
        m.insert("links".into(), Value::Array(links));
    }
    if !children.is_empty() {
        m.insert("entries".into(), Value::Array(children));
    }
    Value::Object(m)
}

/// `<quickfacts>`: a head title/intro and a body of entries.
pub fn quickfacts(t: &mut Tracker, e: &Element) -> Value {
    let mut v = table(t, e);
    v["t"] = "facts".into();
    // quickfacts bodies are entries, not rows
    let rows = v["rows"].take();
    v.as_object_mut().unwrap().remove("rows");
    v.as_object_mut().unwrap().remove("columns");
    v["entries"] = rows;
    v
}

/// `<faf spectype="faf">`: titled sections of entries.
pub fn faf(t: &mut Tracker, e: &Element) -> Value {
    t.el(e);
    t.attr(e, "revision");
    t.attr(e, "spectype");
    let mut sections = Vec::new();
    for s in e.elements() {
        t.el(s);
        let mut title = Value::Null;
        let mut entries = Vec::new();
        for c in s.elements() {
            match c.name.as_str() {
                "title" => title = model::inlines_json(&cell_inlines(t, c)),
                "entry" => entries.push(entry(t, c)),
                other => t.note("facts", &format!("section/<{other}>")),
            }
        }
        sections.push(json!({"title": title, "entries": entries}));
    }
    json!({"t": "facts", "sections": sections})
}

// ---- HTML ----------------------------------------------------------------------------------

/// Inline JSON (as produced by `model::inlines_json`) back to HTML. Tables are rendered from
/// their JSON so that the stored JSON is the single source.
fn inl(v: &Value, out: &mut String) {
    if let Some(a) = v.as_array() {
        for n in a {
            match n {
                Value::String(s) => render::esc(s, out),
                _ => {
                    if let Some(i) = inline_from_json(n) {
                        render::inline(&i, out);
                    }
                }
            }
        }
    }
}

/// Rebuilds an [`Inline`] from its JSON (the subset tables use).
pub fn inline_from_json(v: &Value) -> Option<Inline> {
    use super::model::{Link, LinkKind, Style};
    if let Some(s) = v.as_str() {
        return Some(Inline::Text(s.to_string()));
    }
    let c = || -> Vec<Inline> {
        v["c"]
            .as_array()
            .map(|a| a.iter().filter_map(inline_from_json).collect())
            .unwrap_or_default()
    };
    let s = |k: &str| v[k].as_str().unwrap_or("").to_string();
    Some(match v["t"].as_str()? {
        "em" => Inline::Style(Style::Em, c()),
        "strong" => Inline::Style(Style::Strong, c()),
        "smallcaps" => Inline::Style(Style::SmallCaps, c()),
        "sub" => Inline::Style(Style::Sub, c()),
        "sup" => Inline::Style(Style::Sup, c()),
        "u" => Inline::Style(Style::Underline, c()),
        "br" => Inline::Break,
        "link" => {
            let kind = match v["kind"].as_str()? {
                "web" => LinkKind::Web,
                "see-also" => LinkKind::SeeAlso,
                "see" => LinkKind::See,
                _ => LinkKind::Qv,
            };
            Inline::Link(Link {
                kind,
                ref_id: v["ref"].as_u64().map(|x| x as u32),
                para: v["para"].as_u64().map(|x| x as u32),
                url: v["url"].as_str().map(str::to_string),
                c: c(),
            })
        }
        "frac" => Inline::Frac(s("num"), s("den")),
        "stack" => Inline::Stack(s("top"), s("bottom")),
        "radical" => Inline::Radical(s("c")),
        "overline" => Inline::Overline(s("c")),
        "vector" => Inline::Vector(s("c")),
        "title-ref" => Inline::TitleRef(c()),
        _ => Inline::Unknown {
            name: s("t"),
            c: c(),
        },
    })
}

pub fn render_html(v: &Value) -> String {
    let mut out = String::new();
    match v["t"].as_str() {
        Some("table") => render_table(v, &mut out),
        Some("facts") => render_facts(v, &mut out),
        _ => {}
    }
    out
}

fn render_table(v: &Value, out: &mut String) {
    out.push_str("<table class=\"enc-table\">");
    if let Some(ts) = v["title"].as_array() {
        out.push_str("<caption>");
        for t in ts {
            inl(t, out);
        }
        out.push_str("</caption>");
    }
    out.push_str("<thead>");
    if let Some(hr) = v["header_rows"].as_array() {
        for r in hr {
            out.push_str("<tr>");
            for c in r.as_array().into_iter().flatten() {
                let span = c["span"].as_u64().unwrap_or(1);
                let _ = write!(out, "<th colspan=\"{span}\">");
                inl(&c["c"], out);
                out.push_str("</th>");
            }
            out.push_str("</tr>");
        }
    }
    if let Some(cols) = v["columns"].as_array().filter(|c| !c.is_empty()) {
        out.push_str("<tr>");
        for c in cols {
            let a = c["align"].as_str().unwrap_or("left");
            let _ = write!(out, "<th class=\"enc-align-{a}\">");
            inl(&c["c"], out);
            out.push_str("</th>");
        }
        out.push_str("</tr>");
    }
    out.push_str("</thead><tbody>");
    for r in v["rows"].as_array().into_iter().flatten() {
        let div = r["divider"].as_bool() == Some(true);
        out.push_str(if div { "<tr class=\"enc-divider\">" } else { "<tr>" });
        for c in r["cells"].as_array().into_iter().flatten() {
            let a = c["align"].as_str().unwrap_or("left");
            match c["colspan"].as_u64() {
                Some(s) => {
                    let _ = write!(out, "<td class=\"enc-align-{a}\" colspan=\"{s}\">");
                }
                None => {
                    let _ = write!(out, "<td class=\"enc-align-{a}\">");
                }
            }
            inl(&c["c"], out);
            out.push_str("</td>");
        }
        out.push_str("</tr>");
    }
    out.push_str("</tbody>");
    let notes = v["footnotes"].as_array();
    if notes.is_some() || !v["source"].is_null() {
        out.push_str("<tfoot><tr><td colspan=\"99\">");
        for n in notes.into_iter().flatten() {
            out.push_str("<p class=\"enc-footnote\">");
            inl(n, out);
            out.push_str("</p>");
        }
        if !v["source"].is_null() {
            out.push_str("<p class=\"enc-source\">");
            inl(&v["source"], out);
            out.push_str("</p>");
        }
        out.push_str("</td></tr></tfoot>");
    }
    out.push_str("</table>");
}

fn render_entries(es: &[Value], out: &mut String) {
    out.push_str("<dl class=\"enc-facts-list\">");
    for e in es {
        let q = e["quick"].as_bool() == Some(true);
        let _ = write!(out, "<dt{}>", if q { " class=\"enc-quick\"" } else { "" });
        inl(&e["name"], out);
        out.push_str("</dt><dd>");
        if !e["value"].is_null() {
            inl(&e["value"], out);
        }
        for x in e["values"].as_array().into_iter().flatten() {
            inl(x, out);
            out.push_str("<br>");
        }
        for l in e["links"].as_array().into_iter().flatten() {
            out.push_str(" <a class=\"enc-link enc-app\" data-app-url=\"");
            render::esc(l["url"].as_str().unwrap_or(""), out);
            out.push_str("\">");
            inl(&l["c"], out);
            out.push_str("</a>");
        }
        if let Some(sub) = e["entries"].as_array() {
            render_entries(sub, out);
        }
        out.push_str("</dd>");
    }
    out.push_str("</dl>");
}

fn render_facts(v: &Value, out: &mut String) {
    out.push_str("<div class=\"enc-facts\">");
    if let Some(ts) = v["title"].as_array() {
        out.push_str("<h3 class=\"enc-facts-title\">");
        for t in ts {
            inl(t, out);
        }
        out.push_str("</h3>");
    }
    for i in v["intro"].as_array().into_iter().flatten() {
        out.push_str("<p class=\"enc-intro\">");
        inl(i, out);
        out.push_str("</p>");
    }
    if let Some(es) = v["entries"].as_array() {
        render_entries(es, out);
    }
    for s in v["sections"].as_array().into_iter().flatten() {
        out.push_str("<section class=\"enc-facts-section\"><h3>");
        inl(&s["title"], out);
        out.push_str("</h3>");
        render_entries(s["entries"].as_array().map_or(&[][..], |v| v), out);
        out.push_str("</section>");
    }
    out.push_str("</div>");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::xml;

    #[test]
    fn statistical_table() {
        let d = xml::parse(r#"<table revision="100"><head><columns><column align="left" width="100">Year</column><column align="right" width="65">Pop<sup>1</sup></column></columns></head><body><row><cell colspan="1" align="left">1990</cell><cell colspan="2" align="right">5</cell></row></body><foot><footnotes><footnote align="left">1 est.</footnote></footnotes><source align="left">UN</source></foot></table>"#).unwrap();
        let mut t = Tracker::default();
        let v = table(&mut t, &d.root);
        assert_eq!(v["rows"][0]["cells"][1]["colspan"], 2);
        let h = render_html(&v);
        assert!(h.contains("<th class=\"enc-align-right\">Pop<sup>1</sup></th>"));
        assert!(h.contains("<p class=\"enc-source\">UN</p>"));
    }

    #[test]
    fn facts_entries() {
        let d = xml::parse(r#"<faf revision="100" spectype="faf"><section><title>Basic</title><entry quick="1"><name>Area</name><value>652 sq km</value><entry><name></name><value>251 sq mi</value><links><xr type="url"><x id="msencarta://view=Map/eid=3"></x>See this on a map</xr></links></entry></entry></section></faf>"#).unwrap();
        let mut t = Tracker::default();
        let v = faf(&mut t, &d.root);
        assert_eq!(v["sections"][0]["entries"][0]["quick"], true);
        assert_eq!(v["sections"][0]["entries"][0]["entries"][0]["links"][0]["url"], "msencarta://view=Map/eid=3");
        assert!(render_html(&v).contains("data-app-url=\"msencarta://view=Map/eid=3\""));
    }
}
