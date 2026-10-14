//! Our normalised content model: a small set of inline node types and block types.
//! JSON encoding (documented in `docs/content-schema.md`): text is a JSON string, every other
//! node is an object with a `t` (type) field.

use serde_json::{Map, Value, json};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Style {
    Em,
    Strong,
    SmallCaps,
    Sub,
    Sup,
    Underline,
}

impl Style {
    pub fn name(self) -> &'static str {
        match self {
            Style::Em => "em",
            Style::Strong => "strong",
            Style::SmallCaps => "smallcaps",
            Style::Sub => "sub",
            Style::Sup => "sup",
            Style::Underline => "u",
        }
    }
}

/// How a cross-reference is introduced (source `xref@type`, see the schema doc).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkKind {
    /// Plain quick-verify link inside running text (type 8).
    Qv,
    /// Web link (type 9, `URL`), or `xr type="url"` in fact tables.
    Web,
    /// "See also X" opening a cross-reference list (10).
    SeeAlso,
    /// Further entry of a "See also" list (11).
    SeeAlsoNext,
    /// "(see also X)" in parentheses (12).
    SeeAlsoParen,
    /// "see also X" embedded in a sentence (13).
    SeeAlsoEmbed,
    /// "See X" opening a list (14).
    See,
    /// Further entry of a "See" list (15).
    SeeNext,
    /// "(X)" parenthetical reference (16).
    Paren,
    /// "see X" embedded in a sentence (17).
    SeeEmbed,
    /// Sidebar dialect `xr type="sea"` (see-also list item) and `qv`.
    SidebarSee,
}

impl LinkKind {
    pub fn from_code(code: u32) -> Option<LinkKind> {
        Some(match code {
            8 => LinkKind::Qv,
            9 => LinkKind::Web,
            10 => LinkKind::SeeAlso,
            11 => LinkKind::SeeAlsoNext,
            12 => LinkKind::SeeAlsoParen,
            13 => LinkKind::SeeAlsoEmbed,
            14 => LinkKind::See,
            15 => LinkKind::SeeNext,
            16 => LinkKind::Paren,
            17 => LinkKind::SeeEmbed,
            _ => return None,
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            LinkKind::Qv => "qv",
            LinkKind::Web => "web",
            LinkKind::SeeAlso => "see-also",
            LinkKind::SeeAlsoNext => "see-also-next",
            LinkKind::SeeAlsoParen => "see-also-paren",
            LinkKind::SeeAlsoEmbed => "see-also-embed",
            LinkKind::See => "see",
            LinkKind::SeeNext => "see-next",
            LinkKind::Paren => "paren",
            LinkKind::SeeEmbed => "see-embed",
            LinkKind::SidebarSee => "sidebar-see",
        }
    }

    /// Text our renderer puts before the link (the original UI adds the same words).
    pub fn prefix(self) -> &'static str {
        match self {
            LinkKind::SeeAlso | LinkKind::SeeAlsoEmbed => "See also ",
            LinkKind::See => "See ",
            LinkKind::SeeEmbed => "see ",
            LinkKind::SeeAlsoParen | LinkKind::Paren => "(",
            _ => "",
        }
    }

    pub fn suffix(self) -> &'static str {
        match self {
            LinkKind::SeeAlsoParen | LinkKind::Paren => ")",
            _ => "",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Link {
    pub kind: LinkKind,
    pub ref_id: Option<u32>,
    pub para: Option<u32>,
    pub url: Option<String>,
    pub c: Vec<Inline>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Inline {
    Text(String),
    Style(Style, Vec<Inline>),
    Break,
    Link(Link),
    /// The item's own title, inserted where the source has an empty `<inlinetitle/>`. The reader
    /// shows the title in bold and, when the item has an alternative title, " or " (plain) and the
    /// alternative title in bold: "**East Timor** or **Timor-Leste**, …".
    TitleRef { title: Vec<Inline>, alt: Vec<Inline> },
    /// Inline picture (`inlinebmp`): file stem in baggage and placement.
    Image { id: String, placement: &'static str },
    Frac(String, String),
    Stack(String, String),
    Radical(String),
    Overline(String),
    Vector(String),
    /// Atlas place name (`place gid rid`).
    Place { gid: Option<u32>, rid: Option<u32>, c: Vec<Inline> },
    /// Block quotation (verse) inside a paragraph.
    Quote { align: &'static str, c: Vec<Inline> },
    List { style: &'static str, items: Vec<Vec<Inline>> },
    /// Displayed formula (`fl`).
    Formula(Vec<Inline>),
    /// An element we have no mapping for: kept as a generic span (counted in the report).
    Unknown { name: String, c: Vec<Inline> },
}

impl Inline {
    pub fn is_block(&self) -> bool {
        matches!(
            self,
            Inline::Quote { .. } | Inline::List { .. } | Inline::Formula(_)
        )
    }
}

fn arr(v: &[Inline]) -> Value {
    Value::Array(v.iter().map(Inline::to_json).collect())
}

impl Inline {
    pub fn to_json(&self) -> Value {
        match self {
            Inline::Text(s) => Value::String(s.clone()),
            Inline::Style(s, c) => json!({"t": s.name(), "c": arr(c)}),
            Inline::Break => json!({"t": "br"}),
            Inline::Link(l) => {
                let mut m = Map::new();
                m.insert("t".into(), "link".into());
                m.insert("kind".into(), l.kind.name().into());
                if let Some(r) = l.ref_id {
                    m.insert("ref".into(), r.into());
                }
                if let Some(p) = l.para {
                    m.insert("para".into(), p.into());
                }
                if let Some(u) = &l.url {
                    m.insert("url".into(), u.clone().into());
                }
                m.insert("c".into(), arr(&l.c));
                Value::Object(m)
            }
            // `c` repeats `title` for readers written against schema version 1.
            Inline::TitleRef { title, alt } => json!({"t": "title-ref", "title": arr(title), "alt": arr(alt), "c": arr(title)}),
            Inline::Image { id, placement } => {
                json!({"t": "img", "id": id, "placement": placement})
            }
            Inline::Frac(a, b) => json!({"t": "frac", "num": a, "den": b}),
            Inline::Stack(a, b) => json!({"t": "stack", "top": a, "bottom": b}),
            Inline::Radical(s) => json!({"t": "radical", "c": s}),
            Inline::Overline(s) => json!({"t": "overline", "c": s}),
            Inline::Vector(s) => json!({"t": "vector", "c": s}),
            Inline::Place { gid, rid, c } => {
                let mut m = Map::new();
                m.insert("t".into(), "place".into());
                if let Some(g) = gid {
                    m.insert("gid".into(), (*g).into());
                }
                if let Some(r) = rid {
                    m.insert("rid".into(), (*r).into());
                }
                m.insert("c".into(), arr(c));
                Value::Object(m)
            }
            Inline::Quote { align, c } => json!({"t": "quote", "align": align, "c": arr(c)}),
            Inline::List { style, items } => json!({
                "t": "list", "style": style,
                "items": items.iter().map(|i| arr(i)).collect::<Vec<_>>()
            }),
            Inline::Formula(c) => json!({"t": "formula", "c": arr(c)}),
            Inline::Unknown { name, c } => json!({"t": "unknown", "name": name, "c": arr(c)}),
        }
    }
}

pub fn inlines_json(v: &[Inline]) -> Value {
    arr(v)
}

/// Plain text of inline content (search text, sort keys, plain titles).
pub fn plain(v: &[Inline]) -> String {
    let mut s = String::new();
    plain_into(v, &mut s);
    s
}

fn plain_into(v: &[Inline], s: &mut String) {
    for n in v {
        match n {
            Inline::Text(t) => s.push_str(t),
            Inline::Style(_, c) | Inline::Place { c, .. } | Inline::Formula(c) | Inline::Unknown { c, .. } => {
                plain_into(c, s)
            }
            Inline::TitleRef { title, alt } => {
                plain_into(title, s);
                if !alt.is_empty() {
                    s.push_str(" or ");
                    plain_into(alt, s);
                }
            }
            Inline::Quote { c, .. } => {
                s.push(' ');
                plain_into(c, s);
                s.push(' ');
            }
            Inline::Link(l) => {
                s.push_str(l.kind.prefix());
                plain_into(&l.c, s);
                s.push_str(l.kind.suffix());
            }
            Inline::Break => s.push(' '),
            Inline::Image { .. } => {}
            Inline::Frac(a, b) | Inline::Stack(a, b) => {
                s.push_str(a);
                s.push('/');
                s.push_str(b);
            }
            Inline::Radical(t) => {
                s.push('√');
                s.push_str(t);
            }
            Inline::Overline(t) | Inline::Vector(t) => s.push_str(t),
            Inline::List { items, .. } => {
                for i in items {
                    s.push(' ');
                    plain_into(i, s);
                }
                s.push(' ');
            }
        }
    }
}

/// Collapses runs of whitespace to one space and trims (for titles; not for body text).
pub fn squash(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut space = false;
    for ch in s.chars() {
        if ch.is_whitespace() && ch != '\u{A0}' {
            space = true;
        } else {
            if space && !out.is_empty() {
                out.push(' ');
            }
            space = false;
            out.push(ch);
        }
    }
    out
}

/// Trims leading/trailing whitespace of an inline run and collapses internal whitespace runs
/// (used for titles and table cells, where the source is pretty-printed).
pub fn squash_inlines(v: &mut Vec<Inline>) {
    fn walk(v: &mut [Inline]) {
        for n in v.iter_mut() {
            match n {
                Inline::Text(t) => {
                    let lead = t.starts_with(|c: char| c.is_whitespace() && c != '\u{A0}');
                    let trail = t.ends_with(|c: char| c.is_whitespace() && c != '\u{A0}');
                    let mut s = squash(t);
                    if lead && !s.is_empty() {
                        s.insert(0, ' ');
                    }
                    if trail && !s.is_empty() {
                        s.push(' ');
                    }
                    if s.is_empty() && !t.is_empty() {
                        s.push(' ');
                    }
                    *t = s;
                }
                Inline::Style(_, c)
                | Inline::Place { c, .. }
                | Inline::Formula(c)
                | Inline::Unknown { c, .. }
                | Inline::Quote { c, .. } => walk(c),
                Inline::TitleRef { title, alt } => {
                    walk(title);
                    walk(alt);
                }
                Inline::Link(l) => walk(&mut l.c),
                Inline::List { items, .. } => items.iter_mut().for_each(|i| walk(i)),
                _ => {}
            }
        }
    }
    walk(v);
    trim_ends(v);
}

/// Removes leading and trailing whitespace from the first and last text nodes.
pub fn trim_ends(v: &mut Vec<Inline>) {
    while let Some(Inline::Text(t)) = v.first_mut() {
        let s = t.trim_start_matches(|c: char| c.is_whitespace() && c != '\u{A0}');
        if s.is_empty() {
            v.remove(0);
        } else {
            *t = s.to_string();
            break;
        }
    }
    while let Some(Inline::Text(t)) = v.last_mut() {
        let s = t.trim_end_matches(|c: char| c.is_whitespace() && c != '\u{A0}');
        if s.is_empty() {
            v.pop();
        } else {
            *t = s.to_string();
            break;
        }
    }
}

/// Appends text, merging with a preceding text node.
pub fn push_text(v: &mut Vec<Inline>, s: &str) {
    if s.is_empty() {
        return;
    }
    if let Some(Inline::Text(t)) = v.last_mut() {
        t.push_str(s);
    } else {
        v.push(Inline::Text(s.to_string()));
    }
}
