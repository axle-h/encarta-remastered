//! Source inline markup → our [`Inline`] nodes. Covers every inline element of the AKC article,
//! data, tree and sidebar dialects (the dictionaries have their own module).

use super::entities::{self, SpecialGlyph};
use super::model::{self, Inline, Link, LinkKind, Style};
use super::track::Tracker;
use super::xml::{Element, Node};

pub struct Ctx<'a> {
    pub t: &'a mut Tracker,
    /// The item's title, used to fill `<inlinetitle/>`.
    pub title: Option<&'a [Inline]>,
    /// Collapse whitespace in text (pretty-printed sources: titles, trees, tables).
    pub squash: bool,
}

impl<'a> Ctx<'a> {
    pub fn new(t: &'a mut Tracker) -> Self {
        Ctx {
            t,
            title: None,
            squash: false,
        }
    }
}

/// Line breaks in running text are layout only; `<br>` is the real break.
fn clean_text(s: &str) -> std::borrow::Cow<'_, str> {
    if s.contains(['\r', '\n', '\t']) {
        let mut o = String::with_capacity(s.len());
        let mut prev_cr = false;
        for ch in s.chars() {
            match ch {
                '\r' => {
                    o.push(' ');
                    prev_cr = true;
                    continue;
                }
                '\n' if prev_cr => {}
                '\n' | '\t' => o.push(' '),
                c => o.push(c),
            }
            prev_cr = false;
        }
        o.into()
    } else {
        s.into()
    }
}

fn text(cx: &mut Ctx, s: &str, out: &mut Vec<Inline>) {
    let mut s = clean_text(s);
    if s.chars().any(|c| ('\u{E000}'..='\u{F8FF}').contains(&c)) {
        let mut o = String::with_capacity(s.len());
        for c in s.chars() {
            match entities::private_use(c) {
                Some(r) => o.push_str(r),
                None => {
                    if ('\u{E000}'..='\u{F8FF}').contains(&c) {
                        cx.t.note("unmapped glyph", &format!("private-use U+{:04X}", c as u32));
                    }
                    o.push(c);
                }
            }
        }
        s = o.into();
    }
    let r = entities::resolve_literal(&s, |n| cx.t.note("unknown literal entity", n));
    if r.len() != s.len() || r != s {
        cx.t.note("literal entity", "resolved");
    }
    model::push_text(out, &r);
}

/// Converts the children of `el` to inline nodes.
pub fn children(cx: &mut Ctx, el: &Element) -> Vec<Inline> {
    let mut out = Vec::new();
    for c in &el.children {
        match c {
            Node::Text(s) => text(cx, s, &mut out),
            Node::El(e) => element(cx, e, &mut out),
        }
    }
    if cx.squash {
        model::squash_inlines(&mut out);
    }
    out
}

/// Converts a list of nodes (not necessarily one element's children).
pub fn nodes(cx: &mut Ctx, ns: &[Node]) -> Vec<Inline> {
    let mut out = Vec::new();
    for c in ns {
        match c {
            Node::Text(s) => text(cx, s, &mut out),
            Node::El(e) => element(cx, e, &mut out),
        }
    }
    if cx.squash {
        model::squash_inlines(&mut out);
    }
    out
}

fn styled(cx: &mut Ctx, e: &Element, s: Style, out: &mut Vec<Inline>) {
    cx.t.el(e);
    let c = children(cx, e);
    out.push(Inline::Style(s, c));
}

pub fn quote_align(code: Option<u32>) -> &'static str {
    match code {
        Some(27) => "center",
        Some(29) => "right",
        Some(38) => "justify",
        _ => "left", // 30, and no type
    }
}

pub fn element(cx: &mut Ctx, e: &Element, out: &mut Vec<Inline>) {
    match e.name.as_str() {
        // upper-case forms occur in search-export titles copied from the dictionaries
        "i" | "it" | "It" | "ital" | "I" | "ITAL" => styled(cx, e, Style::Em, out),
        "b" | "BOLD" => styled(cx, e, Style::Strong, out),
        "smallcaps" | "scp" | "sc" | "smallcap" | "SC" => styled(cx, e, Style::SmallCaps, out),
        "sub" | "inf" | "SUB" => styled(cx, e, Style::Sub, out),
        "sup" | "SUP" => styled(cx, e, Style::Sup, out),
        "u" => styled(cx, e, Style::Underline, out),
        "span" => {
            let st = cx.t.attr(e, "style").unwrap_or("");
            if st.contains("small-caps") {
                styled(cx, e, Style::SmallCaps, out)
            } else {
                cx.t.note("inline", &format!("span style {st}"));
                let c = children(cx, e);
                out.extend(c);
            }
        }
        "br" | "break" | "p" => {
            // `p` occurs only empty, inside fact-table values, where it separates two lines.
            cx.t.el(e);
            out.push(Inline::Break);
        }
        "xref" => {
            cx.t.el(e);
            let code = cx.t.attr_u32(e, "type");
            let kind = code.and_then(LinkKind::from_code).unwrap_or_else(|| {
                cx.t.note("xref", &format!("unknown type {code:?}"));
                LinkKind::Qv
            });
            let ref_id = cx.t.attr_u32(e, "RefID").filter(|&r| r != 0);
            let para = cx.t.attr_u32(e, "paraID");
            let url = cx.t.attr(e, "URL").map(str::to_string);
            let c = children(cx, e);
            out.push(Inline::Link(Link {
                kind,
                ref_id,
                para,
                url,
                c,
            }));
        }
        "xr" => {
            // Sidebar and fact-table dialect: <xr type syntax><x id para/>text</xr>
            cx.t.el(e);
            let ty = cx.t.attr(e, "type").unwrap_or("").to_ascii_lowercase();
            cx.t.attr(e, "syntax"); // `sep`: a separate see-also line; layout is ours
            let mut target = None;
            let mut para = None;
            let mut rest = Vec::new();
            for n in &e.children {
                match n {
                    Node::El(x) if x.name == "x" => {
                        cx.t.el(x);
                        target = cx.t.attr(x, "id").map(str::to_string);
                        para = cx.t.attr_u32(x, "para").filter(|&p| p != 0);
                    }
                    other => rest.push(other.clone()),
                }
            }
            let c = nodes(cx, &rest);
            let (kind, ref_id, url) = match (ty.as_str(), target) {
                ("url", Some(u)) => (LinkKind::Web, None, Some(u)),
                (_, Some(t)) => match t.parse::<u32>() {
                    Ok(r) => (
                        if ty == "sea" {
                            LinkKind::SidebarSee
                        } else {
                            LinkKind::Qv
                        },
                        Some(r),
                        None,
                    ),
                    Err(_) => (LinkKind::Web, None, Some(t)),
                },
                (_, None) => {
                    cx.t.note("xr", "no target");
                    (LinkKind::Qv, None, None)
                }
            };
            out.push(Inline::Link(Link {
                kind,
                ref_id,
                para,
                url,
                c,
            }));
        }
        "inlinetitle" => {
            cx.t.el(e);
            if !e.children.is_empty() {
                cx.t.note("inlinetitle", "has content");
            }
            match cx.title {
                Some(t) => {
                    cx.t.note("inlinetitle", "filled from item title");
                    out.push(Inline::TitleRef(t.to_vec()));
                }
                None => {
                    cx.t.note("inlinetitle", "no title available");
                    out.push(Inline::TitleRef(Vec::new()));
                }
            }
        }
        "inlinebmp" => {
            cx.t.el(e);
            let id = cx.t.attr(e, "id").unwrap_or("").to_string();
            let placement = match cx.t.attr_u32(e, "type") {
                Some(28) => "inline",
                Some(29) => "right",
                Some(30) => "left",
                _ => "center", // 27
            };
            out.push(Inline::Image { id, placement });
        }
        "fs" => {
            cx.t.el(e);
            let font = cx.t.attr_u32(e, "type").unwrap_or(0);
            let text = e.text();
            font_run(cx, font, &text, out);
        }
        "entity" if !e.children.is_empty() => {
            // Sidebar dialect: <entity tag='smallcaps'>bc</entity> wraps text instead of naming a character.
            let tag = cx.t.attr(e, "tag").unwrap_or("").to_string();
            if tag != "smallcaps" {
                cx.t.note("unknown entity", &format!("{tag} (with content)"));
            }
            styled(cx, e, Style::SmallCaps, out)
        }
        "entity" => {
            cx.t.el(e);
            let tag = cx.t.attr(e, "tag").unwrap_or("");
            match entities::entity_owned(tag) {
                Some(s) => model::push_text(out, &s),
                None => match entities::marker(tag) {
                    Some(m) => cx.t.note("entity marker", m),
                    None => cx.t.note("unknown entity", tag),
                },
            }
        }
        "quote" => {
            cx.t.el(e);
            let align = quote_align(cx.t.attr_u32(e, "type"));
            let mut c = children(cx, e);
            model::trim_ends(&mut c);
            out.push(Inline::Quote { align, c });
        }
        "list" => {
            cx.t.el(e);
            let style = match cx.t.attr(e, "type") {
                Some("19") | Some("bullet") => "bullet",
                Some("18") => "square",
                Some("20") => "numbered",
                _ => "plain", // type 1
            };
            let mut items = Vec::new();
            for c in e.elements() {
                match c.name.as_str() {
                    "listitem" | "li" => {
                        cx.t.el(c);
                        let mut it = children(cx, c);
                        model::trim_ends(&mut it);
                        items.push(it);
                    }
                    "br" => cx.t.el(c), // stray line break between items: our list layout breaks anyway
                    _ => {
                        cx.t.note("list", &format!("unexpected <{}>", c.name));
                        let mut v = Vec::new();
                        element(cx, c, &mut v);
                        items.push(v);
                    }
                }
            }
            out.push(Inline::List { style, items });
        }
        "fl" => {
            cx.t.el(e);
            let mut c = children(cx, e);
            model::trim_ends(&mut c);
            out.push(Inline::Formula(c));
        }
        "notation" => {
            cx.t.el(e);
            let ty = cx.t.attr(e, "type").unwrap_or("");
            let mut num = String::new();
            let mut den = String::new();
            for it in e.elements() {
                cx.t.el(it);
                match cx.t.attr(it, "pos") {
                    Some("numerator") => num = it.text(),
                    Some("denominator") => den = it.text(),
                    other => cx.t.note("notation", &format!("item pos {other:?}")),
                }
            }
            if ty != "fraction" {
                cx.t.note("notation", &format!("type {ty}"));
            }
            out.push(Inline::Frac(num, den));
        }
        "place" => {
            cx.t.el(e);
            let gid = cx.t.attr_u32(e, "gid");
            let rid = cx.t.attr_u32(e, "rid");
            let c = children(cx, e);
            out.push(Inline::Place { gid, rid, c });
        }
        "au" | "ti" | "pub" | "pubn" | "fdt" | "edt" | "voln" | "ser" | "ann" => {
            // A bibliography field nested inside another field (two sidebar files): flattened.
            cx.t.el(e);
            cx.t.note("bib", "nested field flattened");
            let c = children(cx, e);
            for n in c {
                match n {
                    Inline::Text(s) => model::push_text(out, &s),
                    n => out.push(n),
                }
            }
        }
        "tbd" => {
            // Empty "to be done" placeholder in some media titles: carries nothing.
            cx.t.el(e);
            cx.t.note("dropped", "tbd placeholder");
        }
        "ENC" => {
            // Editorial note attached to a quotation's context ("An Escort is a … Ford").
            cx.t.el(e);
            let c = children(cx, e);
            out.push(Inline::Unknown {
                name: "note".into(),
                c,
            });
        }
        _ => {
            cx.t.note("unmapped inline", &e.name);
            let c = children(cx, e);
            out.push(Inline::Unknown {
                name: e.name.clone(),
                c,
            });
        }
    }
}

fn glyph(g: SpecialGlyph, out: &mut Vec<Inline>) {
    match g {
        SpecialGlyph::Text(s) => model::push_text(out, s),
        SpecialGlyph::Frac(a, b) => out.push(Inline::Frac(a.into(), b.into())),
        SpecialGlyph::Stack(a, b) => out.push(Inline::Stack(a.into(), b.into())),
        SpecialGlyph::Radical(s) => out.push(Inline::Radical(s.into())),
        SpecialGlyph::Overline(s) => out.push(Inline::Overline(s.into())),
        SpecialGlyph::Vector(s) => out.push(Inline::Vector(s.into())),
    }
}

/// A run of text typed in one of the private fonts (`<fs type>`), converted to Unicode.
pub fn font_run(cx: &mut Ctx, font: u32, text: &str, out: &mut Vec<Inline>) {
    for ch in text.chars() {
        let c = ch as u32;
        match font {
            1 => match entities::symbol_font(c) {
                Some(u) => model::push_text(out, u.encode_utf8(&mut [0; 4])),
                None => {
                    cx.t.note("unmapped glyph", &format!("symbol U+{c:04X}"));
                    model::push_text(out, ch.encode_utf8(&mut [0; 4]));
                }
            },
            2 => match entities::specialty_font(c) {
                Some(g) => glyph(g, out),
                None => {
                    cx.t.note("unmapped glyph", &format!("specialty U+{c:04X}"));
                    model::push_text(out, ch.encode_utf8(&mut [0; 4]));
                }
            },
            6 | 7 => match entities::reference_font(font, c) {
                Some(g) => glyph(g, out),
                None => {
                    cx.t.note("unmapped glyph", &format!("reference{font} U+{c:04X}"));
                    model::push_text(out, ch.encode_utf8(&mut [0; 4]));
                }
            },
            // 3 = MS PGothic, 8 = MS Reference Serif: already Unicode; the font is only a hint.
            3 | 8 => match entities::private_use(ch) {
                Some(r) => model::push_text(out, r),
                None => {
                    if (0xE000..=0xF8FF).contains(&c) {
                        cx.t.note("unmapped glyph", &format!("font{font} private-use U+{c:04X}"));
                    }
                    model::push_text(out, ch.encode_utf8(&mut [0; 4]));
                }
            },
            _ => {
                cx.t.note("unmapped glyph", &format!("font{font} U+{c:04X}"));
                model::push_text(out, ch.encode_utf8(&mut [0; 4]));
            }
        }
    }
}

/// Inline content of a title-like element, whitespace collapsed.
pub fn title(t: &mut Tracker, e: &Element) -> Vec<Inline> {
    t.el(e);
    let mut cx = Ctx {
        t,
        title: None,
        squash: true,
    };
    children(&mut cx, e)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::xml;

    fn conv(s: &str) -> Vec<Inline> {
        let d = xml::parse(s).unwrap();
        let mut t = Tracker::default();
        let title = vec![Inline::Text("Zebra".into())];
        let mut cx = Ctx {
            t: &mut t,
            title: Some(&title),
            squash: false,
        };
        children(&mut cx, &d.root)
    }

    #[test]
    fn styles_links_and_title() {
        let v = conv(
            r#"<pkey id="1"><inlinetitle/>, a <xref type="8" RefID="12" paraID="3"><i>horse</i></xref> &amp; <fs type="1">p</fs><fs type="2">y</fs><entity tag="eacute"/></pkey>"#,
        );
        assert_eq!(v[0], Inline::TitleRef(vec![Inline::Text("Zebra".into())]));
        match &v[2] {
            Inline::Link(l) => {
                assert_eq!(l.kind, LinkKind::Qv);
                assert_eq!(l.ref_id, Some(12));
                assert_eq!(l.para, Some(3));
            }
            o => panic!("{o:?}"),
        }
        assert_eq!(v[3], Inline::Text(" & π".into()));
        assert_eq!(v[4], Inline::Frac("1".into(), "2".into()));
        assert_eq!(v[5], Inline::Text("é".into()));
        assert_eq!(model::plain(&v), "Zebra, a horse & π1/2é");
    }

    #[test]
    fn sidebar_links_and_lists() {
        let v = conv(
            r#"<pkey><xr type="qv"><x id="761561691" para="0"></x>Greek art</xr><list type="bullet"><li>a</li><li>b</li></list><xr type="url"><x id="msencarta://view=MapView"/>map</xr></pkey>"#,
        );
        match &v[0] {
            Inline::Link(l) => {
                assert_eq!(l.ref_id, Some(761561691));
                assert_eq!(l.para, None);
            }
            o => panic!("{o:?}"),
        }
        assert!(matches!(&v[1], Inline::List { style: "bullet", items } if items.len() == 2));
        assert!(matches!(&v[2], Inline::Link(l) if l.kind == LinkKind::Web));
    }

    #[test]
    fn unknown_entity_is_reported() {
        let d = xml::parse(r#"<t>a<entity tag="zzz"/>b<entity tag="sortattop"/></t>"#).unwrap();
        let mut t = Tracker::default();
        let mut cx = Ctx::new(&mut t);
        let v = children(&mut cx, &d.root);
        assert_eq!(v, vec![Inline::Text("ab".into())]);
        assert_eq!(t.notes["unknown entity"]["zzz"], 1);
        assert_eq!(t.notes["entity marker"]["sort-top"], 1);
    }
}
