//! DATA records (`Data{Std,Dlx,Kdc}.akc` `<data>`): per-item metadata → [`DataItem`].

use super::article::{self, Block};
use super::inline::{self, Ctx};
use super::model::{self, Inline};
use super::tables;
use super::track::Tracker;
use super::xml::Element;
use serde_json::{Map, Value, json};

#[derive(Debug, Clone)]
pub struct FileRef {
    /// `ticon`, `picon`, `thumb`, `image`, `audio`, `video`, `cc`, `xml`, `item`, `surround`, …
    pub role: String,
    /// Baggage key (lower-case file name), joinable with `baggage.ecs` and the media manifest.
    pub name: String,
    /// The path exactly as written in the record.
    pub raw: String,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub src_id: Option<u32>,
    pub disc: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct Assoc {
    pub forward: bool,
    pub target: u32,
    pub group: String,
    pub sec: Option<u32>,
    pub sequence: Option<i64>,
    pub appears: Option<u32>,
    pub par: Option<u32>,
    pub reviewed: bool,
}

#[derive(Debug, Clone, Default)]
pub struct DataItem {
    pub ref_id: u32,
    pub group: String,
    /// Root attributes, original names and string values.
    pub attrs: Vec<(String, String)>,
    pub title: Vec<Inline>,
    pub full_title: Vec<Inline>,
    pub alt_title: Vec<Inline>,
    pub caption: Vec<Inline>,
    pub credit: Vec<Inline>,
    pub preview: Vec<Block>,
    pub files: Vec<FileRef>,
    pub assocs: Vec<Assoc>,
    pub outline: Option<Value>,
    /// `children`: `(kind, ref id, sequence)` (`article`, `media`, `hotspot`, `stop`, `label`, `site`, `assoc`).
    pub children: Vec<(String, u32, Option<i64>)>,
    /// Everything else, already in our JSON vocabulary (group-specific fields and tables).
    pub meta: Map<String, Value>,
    /// Pre-rendered HTML of a table/facts block, if the record has one.
    pub table_html: Option<String>,
}

impl DataItem {
    pub fn attr(&self, k: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(a, _)| a == k)
            .map(|(_, v)| v.as_str())
    }
}

/// Strips `msencdata::baggage/` and lower-cases: the baggage key.
pub fn baggage_key(raw: &str) -> String {
    let s = raw.trim();
    let s = s
        .strip_prefix("msencdata::baggage/")
        .or_else(|| s.strip_prefix("msencdata::"))
        .unwrap_or(s);
    s.to_ascii_lowercase()
}

fn inl(t: &mut Tracker, e: &Element) -> Vec<Inline> {
    inline::title(t, e)
}

fn inl_json(t: &mut Tracker, e: &Element) -> Value {
    model::inlines_json(&inl(t, e))
}

/// Leaf text fields: source element → our key.
fn leaf_key(name: &str) -> Option<&'static str> {
    Some(match name {
        "theme" => "theme",
        "context" => "context",
        "attributee" => "attributee",
        "date" => "date",
        "born" => "born",
        "died" => "died",
        "flourished" => "flourished",
        "designation" => "designation",
        "nationality" => "nationality",
        "id" => "person_id",
        "type" => "expert_type",
        "credentials" => "credentials",
        "info" => "info",
        "adjective" => "adjective",
        "area" => "area",
        "url" => "url",
        "urlparams" => "url_params",
        "jtitle" => "japanese_title",
        _ => return None,
    })
}

fn add_field(meta: &mut Map<String, Value>, k: &str, v: Value) {
    match meta.get_mut(k) {
        None => {
            meta.insert(k.to_string(), v);
        }
        Some(Value::Array(a)) if k.ends_with('s') => a.push(v),
        Some(old) => {
            let prev = old.take();
            *old = json!([prev, v]);
        }
    }
}

fn name_parts(t: &mut Tracker, e: &Element) -> Value {
    t.el(e);
    let mut m = Map::new();
    for c in e.elements() {
        t.el(c);
        let k = match c.name.as_str() {
            "first" | "middle" | "last" | "sir" => c.name.clone(),
            other => {
                t.note("data", &format!("name/<{other}>"));
                other.to_string()
            }
        };
        let s = model::squash(&c.text());
        if !s.is_empty() {
            m.insert(k, s.into());
        }
    }
    Value::Object(m)
}

fn outline(t: &mut Tracker, e: &Element) -> Value {
    t.el(e);
    let mut items = Vec::new();
    for c in e.elements() {
        match c.name.as_str() {
            "pkey" => {
                t.el(c);
                items.push(json!({"t": "p", "id": t.attr_u32(c, "id")}));
            }
            "sec" => {
                t.el(c);
                let id = t.attr_u32(c, "id");
                let ty = t.attr_u32(c, "type");
                let title = t.attr(c, "title").map(model::squash);
                let inner = outline(t, c);
                items.push(json!({"t": "section", "id": id, "type": ty, "kind": article::section_kind(ty), "title": title, "items": inner}));
            }
            other => t.note("outline", &format!("<{other}>")),
        }
    }
    Value::Array(items)
}

fn points(t: &mut Tracker, e: &Element) -> Value {
    t.el(e);
    Value::Array(
        e.elements()
            .map(|p| {
                t.el(p);
                json!([t.attr_i64(p, "x"), t.attr_i64(p, "y")])
            })
            .collect(),
    )
}

fn date_point(t: &mut Tracker, e: &Element) -> Value {
    t.el(e);
    let mut m = Map::new();
    for k in ["year", "month", "day", "approx", "bc", "year_billion"] {
        if let Some(v) = t.attr(e, k)
            && !v.is_empty()
        {
            m.insert(
                k.into(),
                v.parse::<i64>().map(Value::from).unwrap_or_else(|_| v.into()),
            );
        }
    }
    Value::Object(m)
}

/// Every attribute of an element, as a JSON object with numeric values where they parse.
fn attrs_json(t: &mut Tracker, e: &Element) -> Value {
    t.el_all(e);
    let mut m = Map::new();
    for (k, v) in &e.attrs {
        m.insert(
            k.clone(),
            if !v.is_empty() && v.bytes().all(|b| b.is_ascii_digit()) && v.len() < 16 {
                v.parse::<i64>().map(Value::from).unwrap_or_else(|_| v.clone().into())
            } else {
                v.clone().into()
            },
        );
    }
    Value::Object(m)
}

pub fn convert(t: &mut Tracker, root: &Element) -> DataItem {
    t.el_all(root); // every root attribute is kept in `attrs` (and promoted to columns where useful)
    let mut d = DataItem {
        ref_id: root.attr("refid").and_then(|v| v.parse().ok()).unwrap_or(0),
        group: root.attr("group").unwrap_or("").to_string(),
        attrs: root.attrs.clone(),
        ..Default::default()
    };
    // The preview paragraph's <inlinetitle/> is filled with the title, which may follow it.
    let mut scratch = Tracker::default();
    let mut preview_title: Vec<Inline> = root.child("title").map(|e| inl(&mut scratch, e)).unwrap_or_default();
    if let Some(a) = root.child("alttitle") {
        model::push_text(&mut preview_title, " or ");
        preview_title.extend(inl(&mut scratch, a));
    }
    for c in root.elements() {
        match c.name.as_str() {
            "title" => d.title = inl(t, c),
            "fulltitle" => d.full_title = inl(t, c),
            "alttitle" => d.alt_title = inl(t, c),
            "credit" => d.credit = inl(t, c),
            "caption" => {
                // Home-screen buttons misuse <caption> for their colours.
                let props: Vec<&Element> = c
                    .elements()
                    .filter(|x| matches!(x.name.as_str(), "MouseOverColor" | "AtRestColor" | "DecorationOffset"))
                    .collect();
                if !props.is_empty() {
                    t.el(c);
                    let mut m = Map::new();
                    for p in props {
                        t.el(p);
                        m.insert(p.name.clone(), model::squash(&p.text()).into());
                    }
                    d.meta.insert("button_style".into(), Value::Object(m));
                } else {
                    d.caption = inl(t, c);
                }
            }
            "preview" => {
                t.el(c);
                let mut a = article::convert(t, c, &article::ItemInfo {
                    title: Some(&preview_title),
                    placements: &[],
                    inline_media: true,
                });
                d.preview = std::mem::take(&mut a.blocks);
            }
            "files" => {
                t.el(c);
                for f in c.elements() {
                    t.el(f);
                    let raw = f.text().trim().to_string();
                    d.files.push(FileRef {
                        role: f.name.clone(),
                        name: baggage_key(&raw),
                        raw,
                        width: t.attr_u32(f, "x"),
                        height: t.attr_u32(f, "y"),
                        src_id: t.attr_u32(f, "srcid"),
                        disc: t.attr_i64(f, "disc"),
                    });
                }
            }
            "forward" | "reverse" => {
                t.el(c);
                for a in c.elements() {
                    t.el(a);
                    if a.name != "assoc" {
                        t.note("data", &format!("{}/<{}>", c.name, a.name));
                    }
                    d.assocs.push(Assoc {
                        forward: c.name == "forward",
                        target: t.attr_u32(a, "refid").unwrap_or(0),
                        group: t.attr(a, "group").unwrap_or("").to_string(),
                        sec: t.attr_u32(a, "sec"),
                        sequence: t.attr_i64(a, "sequence"),
                        appears: t.attr_u32(a, "appears"),
                        par: t.attr_u32(a, "par"),
                        reviewed: t.attr(a, "reviewed") == Some("1"),
                    });
                }
            }
            "outline" => d.outline = Some(outline(t, c)),
            "children" => {
                t.el(c);
                for k in c.elements() {
                    t.el(k);
                    let seq = t.attr_i64(k, "Sequence");
                    let id = if k.name == "assoc" {
                        t.attr_u32(k, "refid")
                    } else {
                        k.text().trim().parse().ok()
                    };
                    if k.name == "assoc" {
                        for a in ["group", "sec", "sequence"] {
                            t.attr(k, a);
                        }
                    }
                    match id {
                        Some(id) => d.children.push((k.name.clone(), id, seq)),
                        None => t.note("data", &format!("children/<{}> without id", k.name)),
                    }
                }
            }
            "source" => {
                t.el(c);
                t.attr(c, "align");
                let mut m = Map::new();
                for s in c.elements() {
                    let k = match s.name.as_str() {
                        "work" => "work",
                        "title" => "title",
                        "date" => "date",
                        "ref" => "reference",
                        "author" => "author",
                        _ => {
                            let mut v = Vec::new();
                            let mut cx = Ctx::new(t);
                            inline::element(&mut cx, s, &mut v);
                            add_field(&mut m, "text", model::inlines_json(&v));
                            continue;
                        }
                    };
                    let v = inl_json(t, s);
                    add_field(&mut m, k, v);
                }
                let loose = c.children.iter().any(|n| matches!(n, super::xml::Node::Text(x) if !x.trim().is_empty()));
                if loose {
                    t.note("data", "source with loose text");
                    m.insert("text".into(), inl_json(t, c));
                }
                d.meta.insert("source".into(), Value::Object(m));
            }
            "name" => {
                if c.elements().any(|x| x.name == "first" || x.name == "last") {
                    let v = name_parts(t, c);
                    d.meta.insert("name".into(), v);
                } else {
                    let v = inl_json(t, c);
                    d.meta.insert("name".into(), v);
                }
            }
            "sort_name" => {
                let v = name_parts(t, c);
                d.meta.insert("sort_name".into(), v);
            }
            "timespan" => {
                t.el(c);
                let mut m = Map::new();
                for p in c.elements() {
                    m.insert(p.name.clone(), date_point(t, p));
                }
                d.meta.insert("timespan".into(), Value::Object(m));
            }
            "table" => {
                let v = tables::table(t, c);
                d.table_html = Some(tables::render_html(&v));
                d.meta.insert("table".into(), v);
            }
            "quickfacts" => {
                let v = tables::quickfacts(t, c);
                d.table_html = Some(tables::render_html(&v));
                d.meta.insert("table".into(), v);
            }
            "faf" => {
                let v = tables::faf(t, c);
                d.table_html = Some(tables::render_html(&v));
                d.meta.insert("table".into(), v);
            }
            "quadrant" | "shed" => {
                let v = points(t, c);
                d.meta.insert(c.name.clone(), v);
            }
            "default" => {
                // Panorama defaults (virtual tours): width, pan/light angle, 3D camera script.
                let v = attrs_json(t, c);
                d.meta.insert("panorama".into(), v);
            }
            "providers" => {
                t.el(c);
                let v: Vec<Value> = c.elements().map(|p| attrs_json(t, p)).collect();
                d.meta.insert("providers".into(), Value::Array(v));
            }
            other => match leaf_key(other) {
                Some(k) => {
                    let v = inl_json(t, c);
                    add_field(&mut d.meta, k, v);
                }
                None => {
                    t.note("unmapped data element", other);
                    let v = inl_json(t, c);
                    add_field(&mut d.meta, &format!("x_{other}"), v);
                }
            },
        }
    }
    d
}

/// The one string-keyed DATA record (`list/expert.xml` in DataESK): the contributor list.
pub fn experts(t: &mut Tracker, root: &Element) -> Vec<(u32, Value)> {
    t.el(root);
    t.attr(root, "Classname");
    let mut out = Vec::new();
    for e in root.elements() {
        t.el(e);
        let id = t.attr_u32(e, "refid").unwrap_or(0);
        let mut m = Map::new();
        m.insert("deluxe".into(), (t.attr(e, "deluxe") == Some("1")).into());
        m.insert("standard".into(), (t.attr(e, "standard") == Some("1")).into());
        for c in e.elements() {
            let v = inl_json(t, c);
            m.insert(c.name.clone(), v);
        }
        out.push((id, Value::Object(m)));
    }
    out
}

/// A title for records that have none: contributors (name parts) and quotation authors.
pub fn fallback_title(d: &DataItem) -> String {
    match d.meta.get("name") {
        Some(Value::Object(m)) => ["first", "middle", "last"]
            .iter()
            .filter_map(|k| m.get(*k).and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join(" "),
        Some(v @ Value::Array(_)) => {
            let inl: Vec<Inline> = v
                .as_array()
                .unwrap()
                .iter()
                .filter_map(tables::inline_from_json)
                .collect();
            model::squash(&model::plain(&inl))
        }
        _ => String::new(),
    }
}

/// JSON of the whole item (stored in `item.meta_json`).
pub fn meta_json(d: &DataItem) -> Value {
    let mut m = d.meta.clone();
    let mut attrs = Map::new();
    for (k, v) in &d.attrs {
        attrs.insert(k.clone(), v.clone().into());
    }
    m.insert("source_attrs".into(), Value::Object(attrs));
    for (k, v) in [
        ("title", &d.title),
        ("full_title", &d.full_title),
        ("alt_title", &d.alt_title),
        ("caption", &d.caption),
        ("credit", &d.credit),
    ] {
        if !v.is_empty() {
            m.insert(k.into(), model::inlines_json(v));
        }
    }
    if !d.preview.is_empty() {
        m.insert("preview".into(), article::blocks_json(&d.preview));
    }
    if !d.children.is_empty() {
        m.insert(
            "children".into(),
            d.children
                .iter()
                .map(|(k, id, s)| json!({"kind": k, "ref": id, "seq": s}))
                .collect(),
        );
    }
    Value::Object(m)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::xml;

    #[test]
    fn media_record() {
        let d = xml::parse(r#"<data refid="461550745" group="media" ticon="image" origin="EE" InVB="1" revision="105">
	<title>Grévy’s Zebra</title>
	<credit>Darrell Gulin</credit>
	<caption>The Grévy’s zebra (<it>Equus grevyi</it>) is large.</caption>
	<files>
		<thumb x="64" y="64">msencdata::baggage/t790545a.jtn</thumb>
		<image x="556" y="368">msencdata::baggage/T790545A.jpg</image>
	</files>
	<reverse>
		<assoc refid="761575094" group="article" sec="1" sequence="4"/>
	</reverse>
</data>"#).unwrap();
        let mut t = Tracker::default();
        let it = convert(&mut t, &d.root);
        assert_eq!(it.ref_id, 461550745);
        assert_eq!(model::plain(&it.title), "Grévy’s Zebra");
        assert_eq!(it.files[1].name, "t790545a.jpg");
        assert_eq!(it.files[1].width, Some(556));
        assert_eq!(it.assocs[0].target, 761575094);
        assert!(!it.assocs[0].forward);
        assert!(matches!(&it.caption[1], Inline::Style(model::Style::Em, _)));
    }
}
