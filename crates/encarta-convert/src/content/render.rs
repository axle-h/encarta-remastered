//! Our HTML renderer: semantic HTML fragments with our own `enc-` class names.
//!
//! Links point at server routes we define (`/item/<refId>`, `#p<para>` for paragraphs); the web
//! client may rewrite them. Nothing here is derived from the original XSL templates.

use super::model::{Inline, LinkKind, Style};
use std::fmt::Write;

pub fn esc(s: &str, out: &mut String) {
    for ch in s.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            c => out.push(c),
        }
    }
}

pub fn escaped(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    esc(s, &mut o);
    o
}

/// Where a link to an item goes.
pub fn item_href(ref_id: u32, para: Option<u32>) -> String {
    match para {
        Some(p) => format!("/item/{ref_id}#p{p}"),
        None => format!("/item/{ref_id}"),
    }
}

pub fn inlines(v: &[Inline], out: &mut String) {
    for n in v {
        inline(n, out);
    }
}

pub fn inlines_html(v: &[Inline]) -> String {
    let mut s = String::new();
    inlines(v, &mut s);
    s
}

fn wrap(tag: &str, class: Option<&str>, c: &[Inline], out: &mut String) {
    out.push('<');
    out.push_str(tag);
    if let Some(cl) = class {
        let _ = write!(out, " class=\"{cl}\"");
    }
    out.push('>');
    inlines(c, out);
    let _ = write!(out, "</{tag}>");
}

pub fn inline(n: &Inline, out: &mut String) {
    match n {
        Inline::Text(t) => esc(t, out),
        Inline::Style(s, c) => match s {
            Style::Em => wrap("em", None, c, out),
            Style::Strong => wrap("strong", None, c, out),
            Style::Sub => wrap("sub", None, c, out),
            Style::Sup => wrap("sup", None, c, out),
            Style::Underline => wrap("u", None, c, out),
            Style::SmallCaps => wrap("span", Some("enc-sc"), c, out),
        },
        Inline::Break => out.push_str("<br>"),
        Inline::Link(l) => {
            let pre = l.kind.prefix();
            if !pre.is_empty() {
                if pre == "(" {
                    out.push('(');
                } else {
                    out.push_str("<span class=\"enc-see\">");
                    esc(pre, out);
                    out.push_str("</span>");
                }
            }
            let kind = l.kind.name();
            match (l.ref_id, &l.url) {
                (None, Some(u)) | (Some(_), Some(u)) if l.kind == LinkKind::Web || l.ref_id.is_none() => {
                    if u.starts_with("http://") || u.starts_with("https://") {
                        out.push_str("<a class=\"enc-link enc-web\" rel=\"noopener\" href=\"");
                        esc(u, out);
                        out.push_str("\">");
                    } else {
                        // msencarta:// application URLs (stat tables, map views): the client maps them.
                        out.push_str("<a class=\"enc-link enc-app\" data-app-url=\"");
                        esc(u, out);
                        out.push_str("\">");
                    }
                }
                (Some(r), _) => {
                    let _ = write!(
                        out,
                        "<a class=\"enc-link enc-{kind}\" href=\"{}\" data-ref=\"{r}\"",
                        item_href(r, l.para)
                    );
                    if let Some(p) = l.para {
                        let _ = write!(out, " data-para=\"{p}\"");
                    }
                    out.push('>');
                }
                (None, _) => out.push_str("<a class=\"enc-link enc-dead\">"),
            }
            inlines(&l.c, out);
            out.push_str("</a>");
            out.push_str(l.kind.suffix());
        }
        Inline::TitleRef(c) => wrap("span", Some("enc-title-ref"), c, out),
        Inline::Image { id, placement } => {
            let _ = write!(
                out,
                "<img class=\"enc-inline-img enc-place-{placement}\" data-file=\"{0}.gif\" src=\"/media/file/{0}.gif\" alt=\"\">",
                escaped(&id.to_ascii_lowercase())
            );
        }
        Inline::Frac(a, b) => {
            out.push_str("<span class=\"enc-frac\"><sup>");
            esc(a, out);
            out.push_str("</sup>\u{2044}<sub>");
            esc(b, out);
            out.push_str("</sub></span>");
        }
        Inline::Stack(a, b) => {
            out.push_str("<span class=\"enc-stack\"><span class=\"enc-stack-top\">");
            esc(a, out);
            out.push_str("</span><span class=\"enc-stack-bottom\">");
            esc(b, out);
            out.push_str("</span></span>");
        }
        Inline::Radical(s) => {
            out.push_str("<span class=\"enc-radical\">\u{221A}<span class=\"enc-radicand\">");
            esc(s, out);
            out.push_str("</span></span>");
        }
        Inline::Overline(s) => {
            out.push_str("<span class=\"enc-overline\">");
            esc(s, out);
            out.push_str("</span>");
        }
        Inline::Vector(s) => {
            out.push_str("<span class=\"enc-vector\">");
            esc(s, out);
            out.push_str("</span>");
        }
        Inline::Place { gid, c, .. } => {
            match gid {
                Some(g) => {
                    let _ = write!(out, "<a class=\"enc-place\" href=\"/atlas/place/{g}\" data-place=\"{g}\">");
                }
                None => out.push_str("<a class=\"enc-place\">"),
            }
            inlines(c, out);
            out.push_str("</a>");
        }
        Inline::Quote { align, c } => {
            let _ = write!(out, "<blockquote class=\"enc-quote enc-align-{align}\">");
            inlines(c, out);
            out.push_str("</blockquote>");
        }
        Inline::List { style, items } => {
            let tag = if *style == "numbered" { "ol" } else { "ul" };
            let _ = write!(out, "<{tag} class=\"enc-list enc-list-{style}\">");
            for it in items {
                out.push_str("<li>");
                inlines(it, out);
                out.push_str("</li>");
            }
            let _ = write!(out, "</{tag}>");
        }
        Inline::Formula(c) => wrap("div", Some("enc-formula"), c, out),
        Inline::Unknown { name, c } => {
            if name == "note" {
                wrap("span", Some("enc-note"), c, out)
            } else {
                let _ = write!(out, "<span class=\"enc-unknown\" data-source=\"{}\">", escaped(name));
                inlines(c, out);
                out.push_str("</span>");
            }
        }
    }
}

/// A paragraph: `<p>` unless it holds block content, then a `<div>` (HTML forbids blocks in `<p>`).
pub fn paragraph(id: Option<u32>, class: &str, c: &[Inline], out: &mut String) {
    let tag = if c.iter().any(Inline::is_block) {
        "div"
    } else {
        "p"
    };
    out.push('<');
    out.push_str(tag);
    if let Some(i) = id {
        // paragraph ids are link targets; intro and byline ids are separate numberings
        match class {
            "enc-intro" => {
                let _ = write!(out, " id=\"i{i}\"");
            }
            "enc-byline" => {
                let _ = write!(out, " id=\"b{i}\"");
            }
            _ => {
                let _ = write!(out, " id=\"p{i}\" data-para=\"{i}\"");
            }
        }
    }
    let _ = write!(out, " class=\"{class}\">");
    inlines(c, out);
    let _ = write!(out, "</{tag}>");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::model::Link;

    #[test]
    fn renders_links_and_escapes() {
        let v = vec![
            Inline::Text("a < b ".into()),
            Inline::Link(Link {
                kind: LinkKind::SeeAlso,
                ref_id: Some(5),
                para: Some(2),
                url: None,
                c: vec![Inline::Style(Style::Em, vec![Inline::Text("X".into())])],
            }),
            Inline::Frac("1".into(), "2".into()),
        ];
        let h = inlines_html(&v);
        assert_eq!(
            h,
            "a &lt; b <span class=\"enc-see\">See also </span><a class=\"enc-link enc-see-also\" href=\"/item/5#p2\" data-ref=\"5\" data-para=\"2\"><em>X</em></a><span class=\"enc-frac\"><sup>1</sup>\u{2044}<sub>2</sub></span>"
        );
        let mut p = String::new();
        paragraph(
            Some(3),
            "enc-p",
            &[Inline::Quote {
                align: "left",
                c: vec![Inline::Text("v".into())],
            }],
            &mut p,
        );
        assert!(p.starts_with("<div id=\"p3\""));
    }
}
