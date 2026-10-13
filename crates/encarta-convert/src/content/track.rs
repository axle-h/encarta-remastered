//! Records which source elements and attributes the converter consumed, so the completeness
//! report can check the survey inventory against what was actually mapped.

use super::xml::Element;
use serde::Serialize;
use std::collections::BTreeMap;
use std::collections::HashMap;

#[derive(Default, Clone, Serialize)]
pub struct Usage {
    pub count: u64,
    pub attrs: BTreeMap<String, u64>,
}

#[derive(Default, Clone)]
pub struct Tracker {
    /// element name → usage, per source label (set by the caller via `merge_into`).
    pub elements: HashMap<String, Usage>,
    /// Problems and notable facts, `category → key → count` (unknown entities, glyphs …).
    pub notes: BTreeMap<String, BTreeMap<String, u64>>,
    /// First record id seen for each note key (for looking it up).
    pub note_examples: HashMap<(String, String), String>,
    /// Record id currently being converted (for note examples).
    pub current: String,
}

impl Tracker {
    /// Marks an element as consumed.
    pub fn el(&mut self, e: &Element) {
        if let Some(u) = self.elements.get_mut(&e.name) {
            u.count += 1;
        } else {
            self.elements.insert(
                e.name.clone(),
                Usage {
                    count: 1,
                    ..Default::default()
                },
            );
        }
    }

    /// Marks an element as consumed together with all of its attributes.
    pub fn el_all(&mut self, e: &Element) {
        self.el(e);
        for (k, _) in &e.attrs {
            self.mark_attr(&e.name, k);
        }
    }

    fn mark_attr(&mut self, el: &str, attr: &str) {
        let u = match self.elements.get_mut(el) {
            Some(u) => u,
            None => self.elements.entry(el.to_string()).or_default(),
        };
        if let Some(c) = u.attrs.get_mut(attr) {
            *c += 1;
        } else {
            u.attrs.insert(attr.to_string(), 1);
        }
    }

    /// Reads an attribute and marks it as consumed.
    pub fn attr<'a>(&mut self, e: &'a Element, name: &str) -> Option<&'a str> {
        let v = e.attr(name);
        if v.is_some() {
            self.mark_attr(&e.name, name);
        }
        v
    }

    pub fn attr_u32(&mut self, e: &Element, name: &str) -> Option<u32> {
        self.attr(e, name).and_then(|v| v.trim().parse().ok())
    }

    pub fn attr_i64(&mut self, e: &Element, name: &str) -> Option<i64> {
        self.attr(e, name).and_then(|v| v.trim().parse().ok())
    }

    pub fn note(&mut self, category: &str, key: &str) {
        let m = self.notes.entry(category.to_string()).or_default();
        if let Some(c) = m.get_mut(key) {
            *c += 1;
        } else {
            m.insert(key.to_string(), 1);
            self.note_examples
                .insert((category.to_string(), key.to_string()), self.current.clone());
        }
    }

    pub fn merge(&mut self, o: Tracker) {
        for (k, u) in o.elements {
            let t = self.elements.entry(k).or_default();
            t.count += u.count;
            for (a, n) in u.attrs {
                *t.attrs.entry(a).or_default() += n;
            }
        }
        for (cat, m) in o.notes {
            let t = self.notes.entry(cat).or_default();
            for (k, n) in m {
                *t.entry(k).or_default() += n;
            }
        }
        for (k, v) in o.note_examples {
            self.note_examples.entry(k).or_insert(v);
        }
    }
}
