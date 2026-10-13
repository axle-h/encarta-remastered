//! Article bodies (`CONT*.AKC` `<content>` records, and the sidebar XML in `MD*.EIT/baggage`)
//! → our block model, plain paragraph text, link list and HTML.

use super::inline::{self, Ctx};
use super::model::{self, Inline};
use super::render;
use super::track::Tracker;
use super::xml::{Element, Node};
use serde_json::{Map, Value, json};
use std::fmt::Write;

#[derive(Debug, Clone, PartialEq)]
pub enum Block {
    Para {
        id: Option<u32>,
        c: Vec<Inline>,
    },
    Section {
        id: u32,
        type_code: Option<u32>,
        level: u32,
        /// Number labels from outer to inner (`["III", "A"]`).
        number: Vec<String>,
        title: Vec<Inline>,
        blocks: Vec<Block>,
    },
    /// Display heading of an essay-style article (`headline`, types 32–37).
    Heading {
        id: Option<u32>,
        level: u8,
        centered: bool,
        c: Vec<Inline>,
    },
    /// Lead paragraph of an essay-style article (`intro`).
    Intro {
        id: Option<u32>,
        c: Vec<Inline>,
    },
    /// Author line (`author`).
    Byline {
        id: Option<u32>,
        c: Vec<Inline>,
    },
    Rule,
    /// One bibliography entry (sidebar dialect `bib`).
    Bib(Bib),
    /// A media item or sidebar placed in this section (from the item's `forward` associations).
    Media {
        ref_id: u32,
        group: String,
        sequence: i64,
    },
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Bib {
    pub author: Vec<Inline>,
    pub title: Vec<Inline>,
    pub series: Vec<Inline>,
    pub publisher: Vec<Inline>,
    pub edition: Vec<Inline>,
    pub volumes: Vec<Inline>,
    pub date: Vec<Inline>,
    pub annotation: Vec<Inline>,
}

/// Heading style of a section (`section@type`). It is a style, not the nesting depth: ContDlx has
/// type 6 at the top level and type 5 nested; the depth is our `level`.
pub fn section_kind(code: Option<u32>) -> &'static str {
    match code {
        Some(4) => "introduction",
        Some(5) => "major",
        Some(6) => "minor",
        Some(7) => "subminor",
        _ => "major",
    }
}

/// `(section id, parent section id, level, number label, plain title, type code)`.
pub type SectionRow = (u32, Option<u32>, u32, String, String, Option<u32>);
/// `(para id, ordinal in article, kind, target ref, target para, url, plain text)`.
pub type LinkRow = (Option<u32>, u32, &'static str, Option<u32>, Option<u32>, Option<String>, String);

#[derive(Debug, Default)]
pub struct Article {
    pub blocks: Vec<Block>,
    /// `(para id, section id, ordinal, plain text)` for every paragraph-like block with an id.
    pub paragraphs: Vec<(u32, Option<u32>, u32, String, &'static str)>,
    /// `(section id, parent section id, level, number label, plain title, type code)`.
    pub sections: Vec<SectionRow>,
    /// `(para id, ordinal in article, kind, target ref, target para, url, plain text)`.
    pub links: Vec<LinkRow>,
    pub duplicate_para_ids: u32,
}

struct Builder<'a, 'b> {
    cx: Ctx<'a>,
    art: &'b mut Article,
    ord: u32,
    seen_ids: std::collections::HashSet<u32>,
    /// The previous block was built from loose bibliography fields.
    loose_bib: bool,
}

fn merge_bib(dst: &mut Bib, src: Bib) {
    for (d, s) in [
        (&mut dst.author, src.author),
        (&mut dst.title, src.title),
        (&mut dst.series, src.series),
        (&mut dst.publisher, src.publisher),
        (&mut dst.edition, src.edition),
        (&mut dst.volumes, src.volumes),
        (&mut dst.date, src.date),
        (&mut dst.annotation, src.annotation),
    ] {
        if !s.is_empty() {
            if !d.is_empty() {
                model::push_text(d, "; ");
            }
            d.extend(s);
        }
    }
}

impl Builder<'_, '_> {
    fn para_block(&mut self, e: &Element) -> (Option<u32>, Vec<Inline>) {
        self.cx.t.el(e);
        let id = self.cx.t.attr_u32(e, "id");
        let mut c = inline::children(&mut self.cx, e);
        model::trim_ends(&mut c);
        (id, c)
    }

    /// `kind` separates the id spaces: `p` (pkey, the target of paragraph links), `heading`,
    /// `intro` and `byline` number their elements independently.
    fn record_para(&mut self, kind: &'static str, id: Option<u32>, section: Option<u32>, c: &[Inline]) {
        if let Some(i) = id {
            if kind == "p" && !self.seen_ids.insert(i) {
                self.art.duplicate_para_ids += 1;
            }
            self.ord += 1;
            self.art
                .paragraphs
                .push((i, section, self.ord, model::squash(&model::plain(c)), kind));
        }
        collect_links(c, id, &mut self.art.links);
    }

    /// Children of `<text>` or `<section>` → blocks.
    fn blocks(&mut self, parent: &Element, section: Option<u32>, level: u32, numbers: &[String]) -> Vec<Block> {
        let mut out = Vec::new();
        let mut stray: Vec<Node> = Vec::new();
        for n in &parent.children {
            let e = match n {
                Node::Text(t) => {
                    if !t.trim().is_empty() {
                        stray.push(n.clone());
                    }
                    continue;
                }
                Node::El(e) => e,
            };
            self.loose_bib = false;
            match e.name.as_str() {
                "pkey" | "example" => {
                    self.flush_stray(&mut stray, section, &mut out);
                    let (id, c) = self.para_block(e);
                    self.record_para("p", id, section, &c);
                    out.push(Block::Para { id, c });
                }
                "section" => {
                    self.flush_stray(&mut stray, section, &mut out);
                    out.push(self.section(e, section, level + 1, numbers));
                }
                "sectiontitle" | "sec" | "seca" | "secb" | "secc" => {} // read by `section`
                "headline" => {
                    let (id, c) = self.para_block(e);
                    let code = self.cx.t.attr_u32(e, "type").unwrap_or(0);
                    let (lv, centered) = match code {
                        32 => (1, false),
                        33 => (2, false),
                        34 => (3, false),
                        35 => (1, true),
                        36 => (2, true),
                        37 => (3, true),
                        _ => {
                            self.cx.t.note("headline", &format!("type {code}"));
                            (2, false)
                        }
                    };
                    self.record_para("heading", id, section, &c);
                    out.push(Block::Heading {
                        id,
                        level: lv,
                        centered,
                        c,
                    });
                }
                "intro" => {
                    let (id, c) = self.para_block(e);
                    self.record_para("intro", id, section, &c);
                    out.push(Block::Intro { id, c });
                }
                "author" => {
                    let (id, c) = self.para_block(e);
                    self.record_para("byline", id, section, &c);
                    out.push(Block::Byline { id, c });
                }
                "rule" => {
                    self.cx.t.el(e);
                    out.push(Block::Rule);
                }
                "bib" => {
                    self.flush_stray(&mut stray, section, &mut out);
                    out.push(Block::Bib(bib(&mut self.cx, e)));
                }
                "au" | "ti" | "pub" | "ann" | "ser" | "pubn" | "fdt" | "edt" | "voln" => {
                    // Bibliography fields outside a <bib> (a few sidebar files): one entry per run.
                    self.flush_stray(&mut stray, section, &mut out);
                    self.cx.t.note("bib", "fields outside <bib> grouped into an entry");
                    let starts_new = matches!(out.last(), Some(Block::Bib(b)) if !b.author.is_empty() && e.name == "au");
                    let one = bib(&mut self.cx, &Element {
                        name: "bib".into(),
                        attrs: vec![],
                        children: vec![Node::El(e.clone())],
                    });
                    match out.last_mut() {
                        Some(Block::Bib(b)) if self.loose_bib && !starts_new => merge_bib(b, one),
                        _ => out.push(Block::Bib(one)),
                    }
                    self.loose_bib = true;
                    continue;
                }
                _ => {
                    // Inline markup directly under a section (a few records): gather into a paragraph.
                    stray.push(n.clone());
                }
            }
        }
        self.flush_stray(&mut stray, section, &mut out);
        out
    }

    fn flush_stray(&mut self, stray: &mut Vec<Node>, section: Option<u32>, out: &mut Vec<Block>) {
        if stray.is_empty() {
            return;
        }
        let mut c = inline::nodes(&mut self.cx, stray);
        model::trim_ends(&mut c);
        stray.clear();
        if c.is_empty() {
            return;
        }
        self.cx.t.note("article", "loose inline content wrapped in a paragraph");
        // bibliography fragments outside <bib> (sidebar dialect) arrive here as unknown nodes
        collect_links(&c, None, &mut self.art.links);
        let _ = section;
        out.push(Block::Para { id: None, c });
    }

    fn section(&mut self, e: &Element, parent: Option<u32>, level: u32, numbers: &[String]) -> Block {
        self.cx.t.el(e);
        let id = self.cx.t.attr_u32(e, "id").unwrap_or(0);
        let type_code = self.cx.t.attr_u32(e, "type");
        let mut number = numbers.to_vec();
        let mut title = Vec::new();
        for c in e.elements() {
            match c.name.as_str() {
                "sec" | "seca" | "secb" | "secc" => {
                    self.cx.t.el(c);
                    let s = c.text().trim().to_string();
                    let depth = match c.name.as_str() {
                        "sec" => 0,
                        "seca" => 1,
                        "secb" => 2,
                        _ => 3,
                    };
                    number.truncate(depth.min(number.len()));
                    while number.len() < depth {
                        number.push(String::new());
                    }
                    number.push(s);
                }
                "sectiontitle" => {
                    title = inline::title(self.cx.t, c);
                }
                _ => {}
            }
        }
        // The innermost label is the one this section adds; keep the outer path from the parent.
        self.art.sections.push((
            id,
            parent,
            level,
            number.last().cloned().unwrap_or_default(),
            model::plain(&title),
            type_code,
        ));
        if !(level == 1 && type_code == Some(4)) && type_code == Some(4) {
            self.cx.t.note("section", "introduction type below top level");
        }
        self.cx
            .t
            .note("section type/level", &format!("type {type_code:?} level {level}"));
        let blocks = self.blocks(e, Some(id), level, &number);
        Block::Section {
            id,
            type_code,
            level,
            number,
            title,
            blocks,
        }
    }
}

fn collect_links(
    c: &[Inline],
    para: Option<u32>,
    out: &mut Vec<LinkRow>,
) {
    for n in c {
        match n {
            Inline::Link(l) => {
                let ord = out.len() as u32;
                out.push((
                    para,
                    ord,
                    l.kind.name(),
                    l.ref_id,
                    l.para,
                    l.url.clone(),
                    model::squash(&model::plain(&l.c)),
                ));
            }
            Inline::Style(_, c)
            | Inline::TitleRef(c)
            | Inline::Place { c, .. }
            | Inline::Formula(c)
            | Inline::Unknown { c, .. }
            | Inline::Quote { c, .. } => collect_links(c, para, out),
            Inline::List { items, .. } => items.iter().for_each(|i| collect_links(i, para, out)),
            _ => {}
        }
    }
}

fn bib(cx: &mut Ctx, e: &Element) -> Bib {
    cx.t.el(e);
    let mut b = Bib::default();
    fn field(cx: &mut Ctx, e: &Element) -> Vec<Inline> {
        cx.t.el(e);
        let mut v = inline::children(cx, e);
        model::squash_inlines(&mut v);
        v
    }
    fn add(dst: &mut Vec<Inline>, mut v: Vec<Inline>) {
        if !dst.is_empty() && !v.is_empty() {
            model::push_text(dst, "; ");
        }
        dst.append(&mut v);
    }
    for n in &e.children {
        let c = match n {
            Node::El(c) => c,
            Node::Text(t) => {
                if !t.trim().is_empty() {
                    cx.t.note("bib", "loose text");
                    add(&mut b.annotation, vec![Inline::Text(t.trim().to_string())]);
                }
                continue;
            }
        };
        match c.name.as_str() {
            "au" => add(&mut b.author, field(cx, c)),
            "ti" => add(&mut b.title, field(cx, c)),
            "ser" => add(&mut b.series, field(cx, c)),
            "ann" => add(&mut b.annotation, field(cx, c)),
            "pubn" => add(&mut b.publisher, field(cx, c)),
            "fdt" => add(&mut b.date, field(cx, c)),
            "edt" => add(&mut b.edition, field(cx, c)),
            "voln" => add(&mut b.volumes, field(cx, c)),
            "pub" => {
                cx.t.el(c);
                // A publication statement; nested <pub> and stray <au>/<ti> occur in a few files.
                let inner = bib(cx, &Element {
                    name: "pub".into(),
                    attrs: vec![],
                    children: c.children.clone(),
                });
                add(&mut b.publisher, inner.publisher);
                add(&mut b.date, inner.date);
                add(&mut b.edition, inner.edition);
                add(&mut b.volumes, inner.volumes);
                add(&mut b.author, inner.author);
                add(&mut b.title, inner.title);
                add(&mut b.series, inner.series);
                add(&mut b.annotation, inner.annotation);
            }
            _ => {
                let mut v = Vec::new();
                inline::element(cx, c, &mut v);
                add(&mut b.annotation, v);
            }
        }
    }
    b
}

/// What the article converter needs to know about the item.
pub struct ItemInfo<'a> {
    pub title: Option<&'a [Inline]>,
    /// `(section id, media ref, group, sequence)` from the item's forward associations.
    pub placements: &'a [(u32, u32, String, i64)],
    /// Interleave media with the text (encyclopedia); Kids articles list them after the text.
    pub inline_media: bool,
}

/// Converts a `<content>` record (or a sidebar `<text>` root).
pub fn convert(t: &mut Tracker, root: &Element, info: &ItemInfo) -> Article {
    let mut art = Article::default();
    t.el(root);
    let text_el;
    let text = if root.name == "content" {
        t.attr(root, "refid");
        t.attr(root, "revision");
        match root.child("text") {
            Some(x) => x,
            None => return art,
        }
    } else {
        text_el = root;
        text_el
    };
    t.el(text);
    t.attr(text, "refid");
    t.attr(text, "xml:space");
    let mut b = Builder {
        cx: Ctx {
            t,
            title: info.title,
            squash: false,
        },
        art: &mut art,
        ord: 0,
        seen_ids: Default::default(),
        loose_bib: false,
    };
    let mut blocks = b.blocks(text, None, 0, &[]);
    if info.inline_media {
        place_media(&mut blocks, info.placements);
    } else {
        let mut v: Vec<_> = info.placements.iter().filter(|p| is_inline_group(&p.2)).collect();
        v.sort_by_key(|p| (p.3, p.1));
        blocks.extend(v.into_iter().map(|(_, r, g, q)| Block::Media {
            ref_id: *r,
            group: g.clone(),
            sequence: *q,
        }));
    }
    art.blocks = blocks;
    art
}

/// Groups that the article shows inline beside its text (checked against the reference VM on 20
/// articles); the other associations (quotations, yearbook articles, web links, further reading,
/// contributors, maps, live links) are article resources shown elsewhere.
pub fn is_inline_group(g: &str) -> bool {
    matches!(
        g,
        "media" | "audio" | "audionested" | "video" | "flash" | "table" | "tableinline" | "sidebar"
            | "composite" | "virttour" | "vtour" | "surround" | "factsandfigures" | "timeevent" | "timetopic"
    )
}

/// How many of `n` items go before each of `m` paragraphs. Observed in the reference VM: with
/// fewer items than paragraphs they are spread evenly (one every m/n paragraphs); with more,
/// each paragraph takes ceil(remaining items / remaining paragraphs).
pub fn distribute(n: usize, m: usize) -> Vec<usize> {
    let mut out = vec![0; m];
    if m == 0 {
        return out;
    }
    if n <= m {
        for k in 0..n {
            out[k * m / n] += 1;
        }
    } else {
        let mut left = n;
        for (i, slot) in out.iter_mut().enumerate() {
            let c = left.div_ceil(m - i);
            *slot = c;
            left -= c;
        }
    }
    out
}

/// Inserts media/sidebar placements into their section, interleaved before its paragraphs in
/// sequence order. Placements for a section that does not exist (articles without sections) go
/// to the top-level paragraphs.
fn place_media(blocks: &mut Vec<Block>, placements: &[(u32, u32, String, i64)]) {
    let mut by_sec: std::collections::BTreeMap<u32, Vec<Block>> = Default::default();
    let mut sorted: Vec<_> = placements.iter().filter(|p| is_inline_group(&p.2)).cloned().collect();
    // Sidebars follow the media of their section (as in the reference app).
    sorted.sort_by_key(|p| (p.0, p.2 == "sidebar", p.3, p.1));
    for (sec, r, g, seq) in sorted {
        by_sec.entry(sec).or_default().push(Block::Media {
            ref_id: r,
            group: g,
            sequence: seq,
        });
    }
    let mut left = Vec::new();
    for (sec, items) in by_sec {
        if let Some(items) = insert_into_section(blocks, sec, items) {
            left.extend(items);
        }
    }
    if !left.is_empty() {
        interleave(blocks, left);
    }
}

fn interleave(blocks: &mut Vec<Block>, items: Vec<Block>) {
    let paras: Vec<usize> = blocks
        .iter()
        .enumerate()
        .filter(|(_, b)| matches!(b, Block::Para { .. } | Block::Intro { .. }))
        .map(|(i, _)| i)
        .collect();
    if paras.is_empty() {
        let at = blocks.iter().take_while(|b| matches!(b, Block::Media { .. })).count();
        for (k, m) in items.into_iter().enumerate() {
            blocks.insert(at + k, m);
        }
        return;
    }
    let counts = distribute(items.len(), paras.len());
    let mut it = items.into_iter();
    let mut out = Vec::with_capacity(blocks.len() + counts.iter().sum::<usize>());
    let mut pi = 0;
    for (i, b) in std::mem::take(blocks).into_iter().enumerate() {
        if pi < paras.len() && paras[pi] == i {
            for _ in 0..counts[pi] {
                out.extend(it.next());
            }
            pi += 1;
        }
        out.push(b);
    }
    out.extend(it);
    *blocks = out;
}

/// Returns the items back when the section is not found.
fn insert_into_section(blocks: &mut [Block], sec: u32, items: Vec<Block>) -> Option<Vec<Block>> {
    let mut items = Some(items);
    for b in blocks.iter_mut() {
        if let Block::Section { id, blocks: inner, .. } = b {
            if *id == sec {
                interleave(inner, items.take().unwrap());
                return None;
            }
            items = insert_into_section(inner, sec, items.take().unwrap());
            items.as_ref()?;
        }
    }
    items
}

fn bib_json(b: &Bib) -> Value {
    let mut m = Map::new();
    m.insert("t".into(), "bib".into());
    for (k, v) in [
        ("author", &b.author),
        ("title", &b.title),
        ("series", &b.series),
        ("publisher", &b.publisher),
        ("edition", &b.edition),
        ("volumes", &b.volumes),
        ("date", &b.date),
        ("annotation", &b.annotation),
    ] {
        if !v.is_empty() {
            m.insert(k.into(), model::inlines_json(v));
        }
    }
    Value::Object(m)
}

pub fn block_json(b: &Block) -> Value {
    let with_id = |t: &str, id: &Option<u32>, c: &[Inline]| {
        let mut m = Map::new();
        m.insert("t".into(), t.into());
        if let Some(i) = id {
            m.insert("id".into(), (*i).into());
        }
        m.insert("c".into(), model::inlines_json(c));
        Value::Object(m)
    };
    match b {
        Block::Para { id, c } => with_id("p", id, c),
        Block::Intro { id, c } => with_id("intro", id, c),
        Block::Byline { id, c } => with_id("byline", id, c),
        Block::Heading {
            id,
            level,
            centered,
            c,
        } => {
            let mut v = with_id("heading", id, c);
            v["level"] = (*level).into();
            if *centered {
                v["align"] = "center".into();
            }
            v
        }
        Block::Rule => json!({"t": "rule"}),
        Block::Bib(b) => bib_json(b),
        Block::Media {
            ref_id,
            group,
            sequence,
        } => json!({"t": "media", "ref": ref_id, "group": group, "seq": sequence}),
        Block::Section {
            id,
            type_code,
            level,
            number,
            title,
            blocks,
        } => json!({
            "t": "section",
            "id": id,
            "kind": section_kind(*type_code),
            "type": type_code,
            "level": level,
            "number": number,
            "title": model::inlines_json(title),
            "blocks": blocks.iter().map(block_json).collect::<Vec<_>>(),
        }),
    }
}

pub fn blocks_json(v: &[Block]) -> Value {
    Value::Array(v.iter().map(block_json).collect())
}

/// What the renderer may know about a placed media item.
pub struct MediaInfo {
    pub title_html: String,
    pub caption_html: String,
    pub thumb: Option<String>,
    pub group: String,
}

pub fn render_blocks(v: &[Block], media: &dyn Fn(u32) -> Option<MediaInfo>, out: &mut String) {
    for b in v {
        render_block(b, media, out);
    }
}

fn render_bib(b: &Bib, out: &mut String) {
    out.push_str("<div class=\"enc-bib\">");
    for (class, v, sep) in [
        ("enc-bib-author", &b.author, " "),
        ("enc-bib-title", &b.title, " "),
        ("enc-bib-series", &b.series, " "),
        ("enc-bib-edition", &b.edition, " "),
        ("enc-bib-volumes", &b.volumes, " "),
        ("enc-bib-publisher", &b.publisher, ", "),
        ("enc-bib-date", &b.date, ". "),
    ] {
        if !v.is_empty() {
            let _ = write!(out, "<span class=\"{class}\">");
            render::inlines(v, out);
            out.push_str("</span>");
            out.push_str(sep);
        }
    }
    if !b.annotation.is_empty() {
        out.push_str("<div class=\"enc-bib-annotation\">");
        render::inlines(&b.annotation, out);
        out.push_str("</div>");
    }
    out.push_str("</div>");
}

pub fn render_block(b: &Block, media: &dyn Fn(u32) -> Option<MediaInfo>, out: &mut String) {
    match b {
        Block::Para { id, c } => render::paragraph(*id, "enc-p", c, out),
        Block::Intro { id, c } => render::paragraph(*id, "enc-intro", c, out),
        Block::Byline { id, c } => render::paragraph(*id, "enc-byline", c, out),
        Block::Heading {
            id,
            level,
            centered,
            c,
        } => {
            let tag = format!("h{}", level + 2);
            let _ = write!(out, "<{tag} class=\"enc-heading{}\"", if *centered { " enc-align-center" } else { "" });
            if let Some(i) = id {
                let _ = write!(out, " id=\"h{i}\"");
            }
            out.push('>');
            render::inlines(c, out);
            let _ = write!(out, "</{tag}>");
        }
        Block::Rule => out.push_str("<hr class=\"enc-rule\">"),
        Block::Bib(b) => render_bib(b, out),
        Block::Media {
            ref_id,
            group,
            ..
        } => {
            let info = media(*ref_id);
            let g = info.as_ref().map_or(group.as_str(), |i| i.group.as_str());
            let _ = write!(
                out,
                "<figure class=\"enc-media enc-media-{}\" data-ref=\"{ref_id}\" data-group=\"{}\">",
                render::escaped(g),
                render::escaped(group)
            );
            if let Some(i) = info {
                let _ = write!(out, "<a href=\"{}\">", render::item_href(*ref_id, None));
                if let Some(th) = &i.thumb {
                    let _ = write!(
                        out,
                        "<img class=\"enc-thumb\" src=\"/media/file/{0}\" data-file=\"{0}\" alt=\"\">",
                        render::escaped(th)
                    );
                }
                out.push_str("</a><figcaption>");
                if !i.title_html.is_empty() {
                    let _ = write!(out, "<span class=\"enc-media-title\">{}</span>", i.title_html);
                }
                if !i.caption_html.is_empty() {
                    let _ = write!(out, "<span class=\"enc-media-caption\">{}</span>", i.caption_html);
                }
                out.push_str("</figcaption>");
            }
            out.push_str("</figure>");
        }
        Block::Section {
            id,
            type_code,
            level,
            number,
            title,
            blocks,
        } => {
            let _ = write!(
                out,
                "<section class=\"enc-section enc-{} enc-level-{level}\" id=\"s{id}\" data-section=\"{id}\">",
                section_kind(*type_code)
            );
            let h = (level + 1).min(6);
            let _ = write!(out, "<h{h} class=\"enc-section-title\">");
            if let Some(n) = number.last().filter(|n| !n.is_empty()) {
                let _ = write!(out, "<span class=\"enc-secnum\">{}</span> ", render::escaped(n));
            }
            render::inlines(title, out);
            let _ = write!(out, "</h{h}>");
            render_blocks(blocks, media, out);
            out.push_str("</section>");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::xml;

    const SAMPLE: &str = r#"<content refid="9" revision="100"><text refid="9" xml:space="preserve"><section id="1" type="4"><sectiontitle>INTRODUCTION</sectiontitle><pkey id="1"><inlinetitle></inlinetitle>, a <xref type="8" RefID="7">horse</xref>.</pkey><sec>I</sec></section>
<section type="5" id="2"><sectiontitle>RANGE</sectiontitle>
<pkey id="6">Text <i>x</i>.</pkey><section type="6" id="3"><sectiontitle>Sub</sectiontitle><pkey id="7">Deep.</pkey><sec>II</sec><seca>A</seca></section>
<sec>II</sec></section></text></content>"#;

    #[test]
    fn distributes_like_the_reference_app() {
        assert_eq!(distribute(5, 3), vec![2, 2, 1]);
        assert_eq!(distribute(6, 4), vec![2, 2, 1, 1]);
        assert_eq!(distribute(4, 8), vec![1, 0, 1, 0, 1, 0, 1, 0]);
        assert_eq!(distribute(1, 3), vec![1, 0, 0]);
    }

    #[test]
    fn converts_sections_paragraphs_links() {
        let d = xml::parse(SAMPLE).unwrap();
        let mut t = Tracker::default();
        let title = vec![Inline::Text("Zebra".into())];
        let placements = vec![(2, 461, "media".to_string(), 1)];
        let a = convert(
            &mut t,
            &d.root,
            &ItemInfo {
                title: Some(&title),
                placements: &placements,
                inline_media: true,
            },
        );
        assert_eq!(a.blocks.len(), 2);
        assert_eq!(a.paragraphs.len(), 3);
        assert_eq!(a.paragraphs[0].3, "Zebra, a horse.");
        assert_eq!(a.links.len(), 1);
        assert_eq!(a.links[0].3, Some(7));
        assert_eq!(a.sections.len(), 3);
        assert_eq!(a.sections[2].3, "A");
        match &a.blocks[1] {
            Block::Section { blocks, number, .. } => {
                assert_eq!(number, &vec!["II".to_string()]);
                assert!(matches!(blocks[0], Block::Media { ref_id: 461, .. }));
                match &blocks[2] {
                    Block::Section { number, level, .. } => {
                        assert_eq!(number, &vec!["II".to_string(), "A".to_string()]);
                        assert_eq!(*level, 2);
                    }
                    o => panic!("{o:?}"),
                }
            }
            o => panic!("{o:?}"),
        }
        let mut h = String::new();
        render_blocks(&a.blocks, &|_| None, &mut h);
        assert!(h.contains("<p id=\"p1\" data-para=\"1\" class=\"enc-p\"><span class=\"enc-title-ref\">Zebra</span>, a <a class=\"enc-link enc-qv\" href=\"/item/7\" data-ref=\"7\">horse</a>.</p>"));
        assert!(h.contains("<span class=\"enc-secnum\">A</span> Sub"));
        let j = blocks_json(&a.blocks);
        assert_eq!(j[0]["kind"], "introduction");
    }
}
