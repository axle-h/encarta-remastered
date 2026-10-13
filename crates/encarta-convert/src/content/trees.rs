//! Nested title trees: related articles (`REL*.AKC` `<relarts>`) and index terms
//! (`DATAF*.AKC` `<data group="indx">`). Each record holds one tree per product edition
//! (`<tree ed="STD|STE|STC|DLX|KDC">`); identical trees are stored once with their edition list.

use super::inline;
use super::model::{self, Inline};
use super::track::Tracker;
use super::xml::{Element, Node};
use serde_json::{Map, Value};

pub struct Tree {
    pub editions: Vec<String>,
    pub json: String,
    pub nodes: u32,
}

pub struct Converted {
    pub ref_id: u32,
    /// Plain title of the first top-level node (index terms: the term itself).
    pub title: String,
    pub trees: Vec<Tree>,
}

fn starts_with_sort_marker(title: &Element) -> bool {
    title.children.iter().find_map(|n| match n {
        Node::Text(t) if t.trim().is_empty() => None,
        Node::El(e) => Some(e.name == "entity" && e.attr("tag") == Some("sortattop")),
        _ => Some(false),
    }) == Some(true)
}

fn node(t: &mut Tracker, e: &Element, count: &mut u32) -> Value {
    t.el(e);
    *count += 1;
    let mut m = Map::new();
    for (k, key) in [
        ("refid", "ref"),
        ("nodeid", "node"),
        ("pkey", "para"),
        ("paragraphid", "para"),
        ("pn", "pn"),
    ] {
        if let Some(v) = t.attr_u32(e, k) {
            m.insert(key.into(), v.into());
        }
    }
    if let Some(g) = t.attr(e, "group") {
        m.insert("group".into(), g.into());
    }
    if t.attr(e, "IsList") == Some("1") {
        m.insert("list".into(), true.into());
    }
    let mut children = Vec::new();
    for c in e.elements() {
        match c.name.as_str() {
            "title" => {
                if starts_with_sort_marker(c) {
                    m.insert("sort_top".into(), true.into());
                }
                let v: Vec<Inline> = inline::title(t, c);
                m.insert("title".into(), model::inlines_json(&v));
            }
            "relart" | "entry" => children.push(node(t, c, count)),
            other => t.note("tree", &format!("<{other}>")),
        }
    }
    if !children.is_empty() {
        m.insert("children".into(), Value::Array(children));
    }
    Value::Object(m)
}

pub fn convert(t: &mut Tracker, root: &Element) -> Converted {
    t.el(root);
    let ref_id = t.attr_u32(root, "refid").unwrap_or(0);
    t.attr(root, "group"); // always "indx" on DATAF roots
    let mut trees: Vec<Tree> = Vec::new();
    let mut title = String::new();
    for tr in root.elements() {
        t.el(tr);
        if tr.name != "tree" {
            t.note("tree", &format!("root/<{}>", tr.name));
            continue;
        }
        let ed = t.attr(tr, "ed").unwrap_or("").to_string();
        let mut count = 0;
        let nodes: Vec<Value> = tr.elements().map(|c| node(t, c, &mut count)).collect();
        if title.is_empty()
            && let Some(ti) = nodes.first().and_then(|f| f.get("title")).and_then(Value::as_array)
        {
            let inl: Vec<Inline> = ti.iter().filter_map(super::tables::inline_from_json).collect();
            title = model::squash(&model::plain(&inl));
        }
        let json = Value::Array(nodes).to_string();
        match trees.iter_mut().find(|x| x.json == json) {
            Some(x) => x.editions.push(ed),
            None => trees.push(Tree {
                editions: vec![ed],
                json,
                nodes: count,
            }),
        }
    }
    Converted {
        ref_id,
        title,
        trees,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::xml;

    #[test]
    fn dedupes_editions() {
        let x = r#"<relarts refid="5">
  <tree ed="STD"><relart nodeid="9"><title>
      <entity tag="sortattop"/>Mythology</title><relart refid="7" group="article" pkey="3"><title>Ass</title></relart></relart></tree>
  <tree ed="DLX"><relart nodeid="9"><title>
      <entity tag="sortattop"/>Mythology</title><relart refid="7" group="article" pkey="3"><title>Ass</title></relart></relart></tree>
  <tree ed="KDC"><relart refid="8" group="Kids"><title>Other</title></relart></tree>
</relarts>"#;
        let d = xml::parse(x).unwrap();
        let mut t = Tracker::default();
        let c = convert(&mut t, &d.root);
        assert_eq!(c.ref_id, 5);
        assert_eq!(c.trees.len(), 2);
        assert_eq!(c.trees[0].editions, vec!["STD", "DLX"]);
        assert_eq!(c.trees[0].nodes, 2);
        assert_eq!(c.title, "Mythology");
        let v: Value = serde_json::from_str(&c.trees[0].json).unwrap();
        assert_eq!(v[0]["sort_top"], true);
        assert_eq!(v[0]["title"][0], "Mythology");
        assert_eq!(v[0]["children"][0]["para"], 3);
    }
}
