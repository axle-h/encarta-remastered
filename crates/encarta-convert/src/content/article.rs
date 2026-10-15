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
        col: Column,
    },
    /// Several media items shown in one box in the margin column (one at a time, with a strip of
    /// thumbnails to switch).
    Cluster(Vec<MediaRef>),
    /// Encarta Kids media tiles (two-column grid beside the text).
    MediaGrid(Vec<MediaRef>),
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
            | Inline::TitleRef { title: c, .. }
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

/// Which page layout the reader uses for the item (`article.layout`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layout {
    /// Encyclopedia article: text column with media in the text column (wide pictures, sidebar
    /// teasers) and in the left margin column, placed beside the paragraphs of their section.
    Encyclopedia,
    /// Encarta Kids article: text on the right, a grid of media tiles on the left; only inline
    /// tables (`tableinline`, the "Facts About …" table) sit in the text, at the end of their section.
    Kids,
    /// Sidebar view: header frame (banner, title) and body frame; all media in one cluster at the
    /// top left of the body.
    Sidebar,
}

impl Layout {
    pub fn name(self) -> &'static str {
        match self {
            Layout::Encyclopedia => "encyclopedia",
            Layout::Kids => "kids",
            Layout::Sidebar => "sidebar",
        }
    }
}

/// Where a placed item is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Column {
    /// Spans the text column (wide pictures, video, sidebar teasers, Kids inline tables).
    Text,
    /// Left margin column (small pictures, audio, interactives, tables, timeline items, clusters).
    Margin,
    /// Kids media tile grid.
    Grid,
}

impl Column {
    pub fn name(self) -> &'static str {
        match self {
            Column::Text => "text",
            Column::Margin => "margin",
            Column::Grid => "grid",
        }
    }
}

/// One item the article places (from its DATA forward associations).
#[derive(Debug, Clone, PartialEq)]
pub struct Placement {
    pub sec: u32,
    pub ref_id: u32,
    pub group: String,
    pub sequence: i64,
    pub col: Column,
}

/// A member of a cluster or of the Kids media grid.
#[derive(Debug, Clone, PartialEq)]
pub struct MediaRef {
    pub ref_id: u32,
    pub group: String,
    pub sequence: i64,
}

/// What the article converter needs to know about the item.
pub struct ItemInfo<'a> {
    pub title: Option<&'a [Inline]>,
    /// The item's alternative title (empty when it has none).
    pub alt_title: &'a [Inline],
    pub placements: &'a [Placement],
    pub layout: Layout,
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
            alt_title: info.alt_title,
            squash: false,
        },
        art: &mut art,
        ord: 0,
        seen_ids: Default::default(),
        loose_bib: false,
    };
    let mut blocks = b.blocks(text, None, 0, &[]);
    let mut placed: Vec<Placement> = info.placements.iter().filter(|p| is_inline_group(&p.group)).cloned().collect();
    placed.sort_by_key(|p| (p.sequence, p.ref_id));
    let has = |g: &str| info.placements.iter().any(|p| p.group == g);
    let reserved = Reserved {
        archives: has("archive"),
        further_reading: has("furtherread"),
    };
    match info.layout {
        Layout::Encyclopedia => place_media(&mut blocks, &placed, reserved),
        Layout::Sidebar => {
            let mut items: Vec<Block> = Vec::new();
            if placed.len() == 1 {
                let mut m = media_block(&placed[0]);
                if let Block::Media { col, .. } = &mut m {
                    *col = Column::Margin;
                }
                items.push(m);
            } else if !placed.is_empty() {
                items.push(Block::Cluster(placed.iter().map(media_ref).collect()));
            }
            blocks.splice(0..0, items);
        }
        Layout::Kids => {
            let (inline, grid): (Vec<Placement>, Vec<Placement>) =
                placed.into_iter().partition(|p| p.group == "tableinline");
            for p in inline {
                let mut b = media_block(&p);
                if let Block::Media { col, .. } = &mut b {
                    *col = Column::Text;
                }
                if let Some(b) = append_to_section(&mut blocks, p.sec, b) {
                    let at = blocks.iter().rposition(|b| !matches!(b, Block::Section { .. })).map_or(0, |i| i + 1);
                    blocks.insert(at, b);
                }
            }
            if !grid.is_empty() {
                blocks.push(Block::MediaGrid(grid.iter().map(media_ref).collect()));
            }
        }
    }
    art.blocks = blocks;
    art
}

fn media_ref(p: &Placement) -> MediaRef {
    MediaRef {
        ref_id: p.ref_id,
        group: p.group.clone(),
        sequence: p.sequence,
    }
}

fn media_block(p: &Placement) -> Block {
    Block::Media {
        ref_id: p.ref_id,
        group: p.group.clone(),
        sequence: p.sequence,
        col: p.col,
    }
}

/// Appends a block after the paragraphs of section `sec` (before its subsections). Returns the
/// block back when the section does not exist.
fn append_to_section(blocks: &mut [Block], sec: u32, b: Block) -> Option<Block> {
    let mut b = Some(b);
    for x in blocks.iter_mut() {
        if let Block::Section { id, blocks: inner, .. } = x {
            if *id == sec {
                let at = inner.iter().rposition(|b| !matches!(b, Block::Section { .. })).map_or(0, |i| i + 1);
                inner.insert(at, b.take().unwrap());
                return None;
            }
            b = append_to_section(inner, sec, b.take().unwrap());
            b.as_ref()?;
        }
    }
    b
}

/// Groups that the article shows inline beside its text (checked against the reference VM on 33
/// articles); the other associations (quotations, yearbook articles, web links, further reading,
/// contributors, maps, live links) are article resources shown elsewhere.
pub fn is_inline_group(g: &str) -> bool {
    matches!(
        g,
        "media" | "audio" | "audionested" | "video" | "flash" | "table" | "tableinline" | "sidebar"
            | "composite" | "virttour" | "vtour" | "surround" | "factsandfigures" | "timeevent" | "timetopic"
    )
}

/// Groups that may share one cluster box (the reader shows one at a time with a thumbnail strip).
pub fn is_clusterable(g: &str) -> bool {
    matches!(g, "media" | "audio" | "audionested" | "flash" | "table" | "composite")
}

/// Groups laid out in the timeline stream: one per paragraph, and what does not fit moves on to
/// the next section. Virtual tours are laid out like timeline items (VM).
fn is_timeline_stream(g: &str) -> bool {
    matches!(g, "timeevent" | "timetopic" | "virttour")
}

/// Splits `n` items over `m` slots front-loaded: each slot takes ceil(remaining / remaining slots).
pub fn distribute(n: usize, m: usize) -> Vec<usize> {
    let mut out = vec![0; m];
    let mut left = n;
    for (i, slot) in out.iter_mut().enumerate() {
        let c = left.div_ceil(m - i);
        *slot = c;
        left -= c;
    }
    out
}

/// Sidebar slots: `n` sidebars a whole number of paragraphs apart from `start`, over a section of
/// `m` slots; consecutive when there are more sidebars than paragraphs.
fn spread(n: usize, start: usize, m: usize) -> Vec<usize> {
    let span = if start == 0 { m.max(1) } else { m.saturating_sub(1).max(1) };
    if n <= span {
        let step = span / n.max(1);
        (0..n).map(|k| start + k * step).collect()
    } else {
        (0..n).map(|k| start + k).collect()
    }
}

/// A placement unit: one item, or several clusterable items shown in one box.
#[derive(Debug)]
enum Unit {
    One(Placement),
    Cluster(Vec<Placement>),
}

impl Unit {
    fn of(mut v: Vec<Placement>) -> Unit {
        if v.len() == 1 { Unit::One(v.remove(0)) } else { Unit::Cluster(v) }
    }
}

/// Article resources the reader lays out as invisible items in the text before the primary items,
/// each taking one paragraph slot (`docs/content-schema.md` §2 "Reserved paragraphs").
#[derive(Debug, Default, Clone, Copy)]
pub struct Reserved {
    /// The article has `archive` associations (the Archives module).
    pub archives: bool,
    /// The article has `furtherread` associations (the Further Reading module).
    pub further_reading: bool,
}

/// One section after its sidebars and timeline items are laid out; the primary items and Facts and
/// Figures are laid out by [`finish_section`] over the slots still free.
struct Streams {
    m: usize,
    /// (slot, order within the slot, unit): primary, sidebar, timeline, Facts and Figures.
    out: Vec<(usize, u8, Unit)>,
    used: std::collections::HashSet<usize>,
    primary: Vec<Placement>,
    facts: Vec<Placement>,
}

impl Streams {
    /// The first (or last) paragraph slot nothing has taken. Slot 0 belongs to the primary items
    /// when the section has any.
    fn free_slot(&self, from_end: bool) -> Option<usize> {
        let ok = |s: &usize| !self.used.contains(s) && !(*s == 0 && !self.primary.is_empty());
        if from_end { (0..self.m).rev().find(ok) } else { (0..self.m).find(ok) }
    }
}

/// The reader's layout of one section's items over its `m` paragraphs (inferred from the VM captures;
/// rules in `docs/content-schema.md` §2). `carried` are timeline-stream items the previous section
/// had no paragraph for. Returns `(slot, unit)`, where slot `k < m` means "before paragraph k" and
/// `m` after the last paragraph, and the items to carry into the next section.
#[cfg(test)]
fn layout_section(items: Vec<Placement>, m: usize, carried: Vec<Placement>) -> (Vec<(usize, Unit)>, Vec<Placement>) {
    let (s, carry) = layout_streams(items, m, carried);
    (finish_section(s), carry)
}

/// First half of a section's layout: sidebars and the timeline stream (with the items carried in).
fn layout_streams(items: Vec<Placement>, m: usize, carried: Vec<Placement>) -> (Streams, Vec<Placement>) {
    let mut sidebars = Vec::new();
    let mut timeline = Vec::new();
    let mut facts = Vec::new();
    let mut primary = Vec::new();
    for p in items {
        if p.group == "sidebar" {
            sidebars.push(p);
        } else if is_timeline_stream(&p.group) {
            timeline.push(p);
        } else if p.group == "factsandfigures" {
            facts.push(p);
        } else {
            primary.push(p);
        }
    }
    let first = usize::from(!primary.is_empty());
    // (slot, order within the slot, unit): primary, sidebar, timeline, Facts and Figures.
    let mut out: Vec<(usize, u8, Unit)> = Vec::new();
    let sb_slots = spread(sidebars.len(), first, m);
    let mut used: std::collections::HashSet<usize> = sb_slots.iter().copied().collect();
    for (pos, p) in sb_slots.iter().zip(sidebars) {
        out.push((*pos, 1, Unit::One(p)));
    }
    let mut carry = Vec::new();
    let mut place_timeline = |pos: usize, p: Placement, out: &mut Vec<(usize, u8, Unit)>| {
        if pos < m {
            out.push((pos, 2, Unit::One(p)));
            true
        } else {
            carry.push(p);
            false
        }
    };
    // Carried items join the section's own timeline items; alone, they take the paragraphs from
    // the first one on without keeping them from the other items.
    if timeline.is_empty() {
        for (k, p) in carried.into_iter().enumerate() {
            place_timeline(first + k, p, &mut out);
        }
    } else {
        timeline.splice(0..0, carried);
    }
    if !timeline.is_empty() {
        // One per paragraph, avoiding the sidebars' paragraphs when there are enough others.
        let n = timeline.len();
        let avail: Vec<usize> = (first..m).filter(|s| !used.contains(s)).collect();
        let slots: Vec<usize> = if n <= avail.len() {
            let step = avail.len() / n;
            (0..n).map(|k| avail[k * step]).collect()
        } else {
            (0..n).map(|k| first + k).collect()
        };
        for (pos, p) in slots.into_iter().zip(timeline) {
            if place_timeline(pos, p, &mut out) {
                used.insert(pos);
            }
        }
    }
    (
        Streams {
            m,
            out,
            used,
            primary,
            facts,
        },
        carry,
    )
}

/// Second half of a section's layout: Facts and Figures closes it; the primary items share the
/// paragraph slots still free.
fn finish_section(s: Streams) -> Vec<(usize, Unit)> {
    let Streams {
        m,
        mut out,
        used,
        primary,
        facts,
    } = s;
    for p in facts {
        out.push((m, 3, Unit::One(p)));
    }
    let mut free: Vec<usize> = (0..m.max(1)).filter(|i| !used.contains(i)).collect();
    if free.is_empty() {
        free.push(0);
    }
    if primary.len() > free.len() {
        // More items than paragraphs: the items that cannot share a box take their own paragraphs,
        // evenly spaced, and the others fill the rest front-loaded, several to a cluster.
        let (single, clusterable): (Vec<Placement>, Vec<Placement>) =
            primary.into_iter().partition(|p| !is_clusterable(&p.group));
        let rest: Vec<usize> = if single.len() >= free.len() {
            let mut it = single.into_iter();
            for (slot, c) in free.iter().zip(distribute(it.len(), free.len())) {
                for p in it.by_ref().take(c) {
                    out.push((*slot, 0, Unit::One(p)));
                }
            }
            vec![*free.last().unwrap()]
        } else {
            let mine: Vec<usize> = (0..single.len()).map(|k| free[k * free.len() / single.len()]).collect();
            for (slot, p) in mine.iter().zip(single) {
                out.push((*slot, 0, Unit::One(p)));
            }
            free.into_iter().filter(|s| !mine.contains(s)).collect()
        };
        let mut it = clusterable.into_iter();
        for (slot, c) in rest.iter().zip(distribute(it.len(), rest.len())) {
            let group: Vec<Placement> = it.by_ref().take(c).collect();
            if !group.is_empty() {
                out.push((*slot, 0, Unit::of(group)));
            }
        }
    } else if !primary.is_empty() {
        let step = free.len() / primary.len();
        for (k, p) in primary.into_iter().enumerate() {
            out.push((free[k * step], 0, Unit::One(p)));
        }
    }
    out.sort_by_key(|x| (x.0, x.1));
    out.into_iter().map(|(s, _, u)| (s, u)).collect()
}

fn unit_block(u: Unit) -> Block {
    match u {
        Unit::One(p) => media_block(&p),
        Unit::Cluster(v) => Block::Cluster(v.iter().map(media_ref).collect()),
    }
}

/// A section in reading order: its id, plain title and number of paragraphs of its own.
struct SectionInfo {
    id: u32,
    title: String,
    m: usize,
}

fn own_paragraphs(blocks: &[Block]) -> Vec<usize> {
    blocks
        .iter()
        .enumerate()
        .filter(|(_, b)| matches!(b, Block::Para { .. } | Block::Intro { .. }))
        .map(|(i, _)| i)
        .collect()
}

fn collect_sections(blocks: &[Block], out: &mut Vec<SectionInfo>) {
    for b in blocks {
        if let Block::Section { id, title, blocks: inner, .. } = b {
            out.push(SectionInfo {
                id: *id,
                title: model::plain(title).trim().to_string(),
                m: own_paragraphs(inner).len(),
            });
            collect_sections(inner, out);
        }
    }
}

/// Inserts media/sidebar placements into their sections. The sections are walked in reading order
/// three times: sidebars and timeline items first (timeline items a section has no paragraph for
/// move on to the next one), then the reserved paragraphs of the article's modules, then the
/// primary items over what is left. Placements for a section that does not exist (articles without
/// sections) go to the top-level paragraphs.
fn place_media(blocks: &mut Vec<Block>, placements: &[Placement], reserved: Reserved) {
    let mut by_sec: std::collections::BTreeMap<u32, Vec<Placement>> = Default::default();
    for p in placements {
        by_sec.entry(p.sec).or_default().push(p.clone());
    }
    let mut secs = Vec::new();
    collect_sections(blocks, &mut secs);
    let mut carry = Vec::new();
    let mut layouts: Vec<Streams> = Vec::with_capacity(secs.len());
    for s in &secs {
        let items = by_sec.remove(&s.id).unwrap_or_default();
        let (l, c) = layout_streams(items, s.m, std::mem::take(&mut carry));
        layouts.push(l);
        carry = c;
    }
    reserve_module_slots(&secs, &mut layouts, reserved);
    let mut slots = layouts.into_iter().map(finish_section);
    apply_sections(blocks, &mut slots);
    if !carry.is_empty() {
        // Nothing follows the last section: its carried items close it.
        let units = carry.into_iter().map(|p| unit_block(Unit::One(p))).collect();
        append_to_last_section(blocks, units);
    }
    let mut left: Vec<Placement> = by_sec.into_values().flatten().collect();
    if !left.is_empty() {
        left.sort_by_key(|p| (p.sequence, p.ref_id));
        let m = own_paragraphs(blocks).len();
        let (l, carry) = layout_streams(left, m, Vec::new());
        insert_units(blocks, finish_section(l));
        let units = carry.into_iter().map(|p| unit_block(Unit::One(p))).collect();
        append_to_last_section(blocks, units);
    }
}

/// The Archives and Further Reading modules each hold one paragraph slot that shows nothing
/// (VM: an empty item `DIV`), placed after the sidebars and timeline items and before the primary
/// items: Archives at the first free slot of the section titled "History", or, without one, at the
/// first free slot of the article; Further Reading at the last free slot of the last section. With
/// no free slot they go after the section's last paragraph, which changes nothing.
fn reserve_module_slots(secs: &[SectionInfo], layouts: &mut [Streams], reserved: Reserved) {
    if reserved.archives {
        match secs.iter().position(|s| s.title.eq_ignore_ascii_case("history")) {
            Some(i) => {
                if let Some(k) = layouts[i].free_slot(false) {
                    layouts[i].used.insert(k);
                }
            }
            None => {
                if let Some((l, k)) = layouts.iter_mut().find_map(|l| l.free_slot(false).map(|k| (l, k))) {
                    l.used.insert(k);
                }
            }
        }
    }
    if reserved.further_reading
        && let Some(l) = layouts.last_mut()
        && let Some(k) = l.free_slot(true)
    {
        l.used.insert(k);
    }
}

/// Puts each section's laid-out units into its blocks, in the order of [`collect_sections`].
fn apply_sections(blocks: &mut [Block], slots: &mut impl Iterator<Item = Vec<(usize, Unit)>>) {
    for b in blocks.iter_mut() {
        if let Block::Section { blocks: inner, .. } = b {
            if let Some(s) = slots.next() {
                insert_units(inner, s);
            }
            apply_sections(inner, slots);
        }
    }
}

fn append_to_last_section(blocks: &mut Vec<Block>, units: Vec<Block>) {
    if let Some(Block::Section { blocks: inner, .. }) =
        blocks.iter_mut().rev().find(|b| matches!(b, Block::Section { .. }))
    {
        append_to_last_section(inner, units);
    } else {
        blocks.extend(units);
    }
}

/// Inserts laid-out units before their paragraphs; slot `m` goes after the last paragraph, before
/// the subsections.
fn insert_units(blocks: &mut Vec<Block>, slots: Vec<(usize, Unit)>) {
    if slots.is_empty() {
        return;
    }
    let paras = own_paragraphs(blocks);
    let m = paras.len();
    let mut at: std::collections::BTreeMap<usize, Vec<Block>> = Default::default();
    for (slot, u) in slots {
        at.entry(slot.min(m)).or_default().push(unit_block(u));
    }
    let mut out = Vec::with_capacity(blocks.len() + at.len());
    let mut pi = 0;
    for (i, b) in std::mem::take(blocks).into_iter().enumerate() {
        if pi < m && paras[pi] == i {
            out.extend(at.remove(&pi).unwrap_or_default());
            pi += 1;
        }
        out.push(b);
    }
    let tail: Vec<Block> = at.into_values().flatten().collect();
    let k = out.iter().rposition(|b| !matches!(b, Block::Section { .. })).map_or(0, |i| i + 1);
    out.splice(k..k, tail);
    *blocks = out;
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
            col,
        } => json!({"t": "media", "ref": ref_id, "group": group, "seq": sequence, "col": col.name()}),
        Block::Cluster(items) => json!({"t": "cluster", "col": "margin", "items": items.iter().map(media_ref_json).collect::<Vec<_>>()}),
        Block::MediaGrid(items) => json!({"t": "media-grid", "col": "grid", "items": items.iter().map(media_ref_json).collect::<Vec<_>>()}),
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

fn media_ref_json(m: &MediaRef) -> Value {
    json!({"ref": m.ref_id, "group": m.group, "seq": m.sequence})
}

pub fn blocks_json(v: &[Block]) -> Value {
    Value::Array(v.iter().map(block_json).collect())
}

/// What the renderer may know about a placed media item.
pub struct MediaInfo {
    pub title_html: String,
    pub caption_html: String,
    /// 64-pixel thumbnail (`files/thumb`, else `picon`).
    pub thumb: Option<String>,
    /// Margin-column picture (`files/picon`, about 192 px wide).
    pub picon: Option<String>,
    /// Full picture (`files/image`), shown when the item spans the text column.
    pub image: Option<String>,
    pub group: String,
    /// Sidebar teaser body (the sidebar's DATA preview).
    pub preview_html: String,
    /// Sidebar category label ("HISTORIC DOCUMENTS"), for sidebar teasers.
    pub category: Option<&'static str>,
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
        Block::Media { ref_id, group, col, .. } => render_media(*ref_id, group, *col, media, out),
        Block::Cluster(items) => {
            let refs: Vec<String> = items.iter().map(|m| m.ref_id.to_string()).collect();
            let _ = write!(out, "<div class=\"enc-media-cluster enc-col-margin\" data-refs=\"{}\">", refs.join(" "));
            for m in items {
                render_media(m.ref_id, &m.group, Column::Margin, media, out);
            }
            out.push_str("</div>");
        }
        Block::MediaGrid(items) => {
            out.push_str("<div class=\"enc-media-grid\">");
            for m in items {
                render_media(m.ref_id, &m.group, Column::Grid, media, out);
            }
            out.push_str("</div>");
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

fn render_media(ref_id: u32, group: &str, col: Column, media: &dyn Fn(u32) -> Option<MediaInfo>, out: &mut String) {
    let info = media(ref_id);
    let href = render::item_href(ref_id, None);
    if group == "sidebar" {
        // Sidebar teaser: a box in the text column with the category, title and preview.
        let _ = write!(
            out,
            "<aside class=\"enc-media enc-sidebar-teaser enc-col-{}\" data-ref=\"{ref_id}\" data-group=\"sidebar\"><a href=\"{href}\">",
            col.name()
        );
        out.push_str("<span class=\"enc-sb-hdr\">Sidebars</span>");
        if let Some(i) = &info {
            if let Some(c) = i.category {
                let _ = write!(out, "<span class=\"enc-sb-cat\">{}</span>", render::escaped(c));
            }
            let _ = write!(out, "<span class=\"enc-sb-title\">{}</span>", i.title_html);
            if !i.preview_html.is_empty() {
                let _ = write!(out, "<span class=\"enc-sb-body\">{}</span>", i.preview_html);
            }
        }
        out.push_str("<span class=\"enc-sb-open\">open sidebar</span></a></aside>");
        return;
    }
    let g = info.as_ref().map_or(group, |i| i.group.as_str());
    let _ = write!(
        out,
        "<figure class=\"enc-media enc-media-{} enc-col-{}\" data-ref=\"{ref_id}\" data-group=\"{}\">",
        render::escaped(g),
        col.name(),
        render::escaped(group)
    );
    if let Some(i) = info {
        let _ = write!(out, "<a href=\"{href}\">");
        let pic = match col {
            Column::Text => i.image.as_ref().or(i.picon.as_ref()).or(i.thumb.as_ref()),
            Column::Margin => i.picon.as_ref().or(i.thumb.as_ref()),
            Column::Grid => i.picon.as_ref().or(i.thumb.as_ref()),
        };
        if let Some(th) = pic {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::xml;

    const SAMPLE: &str = r#"<content refid="9" revision="100"><text refid="9" xml:space="preserve"><section id="1" type="4"><sectiontitle>INTRODUCTION</sectiontitle><pkey id="1"><inlinetitle></inlinetitle>, a <xref type="8" RefID="7">horse</xref>.</pkey><sec>I</sec></section>
<section type="5" id="2"><sectiontitle>RANGE</sectiontitle>
<pkey id="6">Text <i>x</i>.</pkey><section type="6" id="3"><sectiontitle>Sub</sectiontitle><pkey id="7">Deep.</pkey><sec>II</sec><seca>A</seca></section>
<sec>II</sec></section></text></content>"#;

    fn pl(sec: u32, r: u32, g: &str, seq: i64, col: Column) -> Placement {
        Placement {
            sec,
            ref_id: r,
            group: g.into(),
            sequence: seq,
            col,
        }
    }

    fn slots(v: &[(usize, Unit)]) -> Vec<(usize, Vec<u32>)> {
        v.iter()
            .map(|(s, u)| {
                let ids = match u {
                    Unit::One(p) => vec![p.ref_id],
                    Unit::Cluster(c) => c.iter().map(|p| p.ref_id).collect(),
                };
                (*s, ids)
            })
            .collect()
    }

    #[test]
    fn distributes_front_loaded() {
        assert_eq!(distribute(5, 3), vec![2, 2, 1]);
        assert_eq!(distribute(10, 7), vec![2, 2, 2, 1, 1, 1, 1]);
        assert_eq!(distribute(1, 3), vec![1, 0, 0]);
    }

    fn lay(v: Vec<Placement>, m: usize) -> Vec<(usize, Vec<u32>)> {
        let (s, carry) = layout_section(v, m, Vec::new());
        assert!(carry.is_empty(), "{carry:?}");
        slots(&s)
    }

    #[test]
    fn section_layout_rules() {
        use Column::*;
        // Few items: spaced floor(m / n) paragraphs apart from the first paragraph.
        let v = vec![pl(1, 1, "media", 1, Margin), pl(1, 2, "media", 2, Text), pl(1, 3, "media", 3, Margin)];
        assert_eq!(lay(v, 7), vec![(0, vec![1]), (2, vec![2]), (4, vec![3])]);
        // A sidebar takes the second paragraph; the other items spread over the remaining ones.
        let v = vec![pl(1, 1, "media", 1, Text), pl(1, 9, "sidebar", 1, Text), pl(1, 2, "media", 2, Text)];
        assert_eq!(lay(v, 7), vec![(0, vec![1]), (1, vec![9]), (4, vec![2])]);
        // Timeline items follow the sidebar; more items than free slots pack into clusters.
        let v = vec![
            pl(1, 1, "media", 1, Text),
            pl(1, 2, "media", 2, Margin),
            pl(1, 3, "audio", 3, Margin),
            pl(1, 7, "sidebar", 7, Text),
            pl(1, 8, "timeevent", 1, Margin),
        ];
        assert_eq!(lay(v, 3), vec![(0, vec![1, 2, 3]), (1, vec![7]), (2, vec![8])]);
        // Only sidebars: from the first paragraph.
        let v = vec![pl(1, 9, "sidebar", 1, Text)];
        assert_eq!(lay(v, 1), vec![(0, vec![9])]);
        // A video does not cluster; the pictures share one box beside it.
        let v = vec![pl(1, 1, "video", 1, Text), pl(1, 2, "media", 2, Margin), pl(1, 3, "media", 3, Margin)];
        assert_eq!(lay(v, 1), vec![(0, vec![1]), (0, vec![2, 3])]);
    }

    #[test]
    fn section_layout_streams() {
        use Column::*;
        // Timeline items skip the sidebars' paragraphs, a whole number of paragraphs apart
        // (United States History section 120).
        let v = vec![
            pl(1, 1, "media", 1, Margin),
            pl(1, 2, "media", 2, Margin),
            pl(1, 3, "media", 3, Margin),
            pl(1, 4, "sidebar", 4, Text),
            pl(1, 5, "sidebar", 5, Text),
            pl(1, 6, "timeevent", 6, Margin),
            pl(1, 7, "virttour", 7, Margin),
        ];
        assert_eq!(
            lay(v, 7),
            vec![(0, vec![1]), (1, vec![4]), (2, vec![6]), (3, vec![2]), (4, vec![5]), (5, vec![7]), (6, vec![3])]
        );
        // With more items than paragraphs, videos take their own paragraphs first (United Kingdom
        // section 215).
        let v = vec![
            pl(1, 1, "video", 1, Text),
            pl(1, 3, "media", 3, Margin),
            pl(1, 4, "media", 4, Margin),
            pl(1, 5, "media", 5, Margin),
            pl(1, 6, "video", 6, Text),
            pl(1, 7, "media", 7, Margin),
        ];
        assert_eq!(lay(v, 4), vec![(0, vec![1]), (1, vec![3, 4]), (2, vec![6]), (3, vec![5, 7])]);
        // Facts and Figures closes the section and leaves the paragraphs to the others (France).
        let v = vec![
            pl(1, 1, "audionested", 1, Margin),
            pl(1, 2, "factsandfigures", 2, Margin),
            pl(1, 3, "media", 3, Margin),
            pl(1, 4, "composite", 4, Margin),
        ];
        assert_eq!(lay(v, 10), vec![(0, vec![1]), (3, vec![3]), (6, vec![4]), (10, vec![2])]);
    }

    /// Lays out sections `(id, title, m, items)` like `place_media`, with the reserved module slots.
    fn lay_article(secs: Vec<(u32, &str, usize, Vec<Placement>)>, reserved: Reserved) -> Vec<Vec<(usize, Vec<u32>)>> {
        let infos: Vec<SectionInfo> = secs.iter().map(|(id, t, m, _)| SectionInfo { id: *id, title: t.to_string(), m: *m }).collect();
        let mut carry = Vec::new();
        let mut layouts = Vec::new();
        for (_, _, m, items) in secs {
            let (l, c) = layout_streams(items, m, std::mem::take(&mut carry));
            layouts.push(l);
            carry = c;
        }
        reserve_module_slots(&infos, &mut layouts, reserved);
        layouts.into_iter().map(|l| slots(&finish_section(l))).collect()
    }

    #[test]
    fn module_slots_are_reserved_before_the_primary_items() {
        use Column::*;
        let archives = Reserved { archives: true, further_reading: false };
        // Moon: no "History" section; the introduction's slots are all taken (slot 0 by the
        // pictures, 1 and 2 by events), so the Archives slot is section 2's first free one, after
        // its sidebar, and the four primary items skip it.
        let intro = vec![pl(1, 1, "media", 1, Margin), pl(1, 2, "timeevent", 2, Margin), pl(1, 3, "timeevent", 3, Margin)];
        let s2 = vec![
            pl(2, 10, "media", 1, Margin),
            pl(2, 11, "table", 2, Margin),
            pl(2, 12, "flash", 3, Margin),
            pl(2, 13, "media", 4, Margin),
            pl(2, 14, "sidebar", 5, Text),
        ];
        let got = lay_article(vec![(1, "", 3, intro), (2, "THE MOON SEEN FROM EARTH", 6, s2.clone())], archives);
        assert_eq!(got[0], vec![(0, vec![1]), (1, vec![2]), (2, vec![3])]);
        assert_eq!(got[1], vec![(0, vec![10]), (1, vec![14]), (3, vec![11]), (4, vec![12]), (5, vec![13])]);
        // Without archives the primary items are spaced over all free slots.
        let got = lay_article(vec![(2, "", 6, s2.clone())], Reserved::default());
        assert_eq!(got[0], vec![(0, vec![10]), (1, vec![14]), (2, vec![11]), (3, vec![12]), (4, vec![13])]);
        // A section titled exactly "History" takes it, even when earlier sections have room;
        // "History of Mathematics" does not count.
        let got = lay_article(
            vec![(1, "History of Mathematics", 4, vec![pl(1, 1, "media", 1, Margin), pl(1, 2, "media", 2, Margin)]), (9, "HISTORY", 3, vec![])],
            archives,
        );
        assert_eq!(got[0], vec![(0, vec![1]), (2, vec![2])]);
        let got = lay_article(vec![(1, "History of Mathematics", 4, vec![pl(1, 1, "media", 1, Margin), pl(1, 2, "media", 2, Margin)])], archives);
        assert_eq!(got[0], vec![(0, vec![1]), (2, vec![2])], "slot 1 reserved, step 1 over [2, 3]");
        // East Timor section 13 (the last): Further Reading takes the last free slot, before the
        // last paragraph; the four pictures then go one paragraph apart after the event.
        let s13 = vec![
            pl(13, 1, "media", 2, Margin),
            pl(13, 2, "timeevent", 1, Margin),
            pl(13, 3, "media", 3, Margin),
            pl(13, 4, "media", 4, Margin),
            pl(13, 5, "media", 5, Margin),
        ];
        let got = lay_article(vec![(12, "", 4, vec![]), (13, "Independence", 9, s13)], Reserved { archives: false, further_reading: true });
        assert_eq!(got[1], vec![(0, vec![1]), (1, vec![2]), (2, vec![3]), (3, vec![4]), (4, vec![5])]);
    }

    #[test]
    fn timeline_items_carry_into_the_next_section() {
        use Column::*;
        // Algebra section 2: one paragraph, taken by the picture; both events move on.
        let v = vec![pl(2, 1, "flash", 1, Margin), pl(2, 2, "timeevent", 2, Margin), pl(2, 3, "timeevent", 3, Margin)];
        let (s, carry) = layout_section(v, 1, Vec::new());
        assert_eq!(slots(&s), vec![(0, vec![1])]);
        assert_eq!(carry.iter().map(|p| p.ref_id).collect::<Vec<_>>(), vec![2, 3]);
        // They open the next section, one per paragraph, after its sidebar.
        let (s, carry) = layout_section(vec![pl(3, 4, "media", 1, Margin), pl(3, 5, "sidebar", 2, Text)], 6, carry);
        assert!(carry.is_empty());
        assert_eq!(slots(&s), vec![(0, vec![4]), (1, vec![5]), (1, vec![2]), (2, vec![3])]);
        // In the blocks: the next section in reading order is the subsection.
        let d = xml::parse(SAMPLE).unwrap();
        let mut t = Tracker::default();
        let placements = vec![pl(2, 10, "media", 1, Margin), pl(2, 11, "timeevent", 2, Margin)];
        let a = convert(
            &mut t,
            &d.root,
            &ItemInfo {
                title: None,
                alt_title: &[],
                placements: &placements,
                layout: Layout::Encyclopedia,
            },
        );
        let json = serde_json::to_string(&a.blocks.iter().map(block_json).collect::<Vec<_>>()).unwrap();
        let at = |r: &str| json.find(r).unwrap();
        assert!(at("\"ref\":11") > at("Text ") && at("\"ref\":11") < at("Deep."), "{json}");
    }

    #[test]
    fn converts_sections_paragraphs_links() {
        let d = xml::parse(SAMPLE).unwrap();
        let mut t = Tracker::default();
        let title = vec![Inline::Text("Zebra".into())];
        let alt = vec![Inline::Text("Equus".into())];
        let placements = vec![pl(2, 461, "media", 1, Column::Margin)];
        let a = convert(
            &mut t,
            &d.root,
            &ItemInfo {
                title: Some(&title),
                alt_title: &alt,
                placements: &placements,
                layout: Layout::Encyclopedia,
            },
        );
        assert_eq!(a.blocks.len(), 2);
        assert_eq!(a.paragraphs.len(), 3);
        assert_eq!(a.paragraphs[0].3, "Zebra or Equus, a horse.");
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
        assert!(h.contains("<p id=\"p1\" data-para=\"1\" class=\"enc-p\"><span class=\"enc-title-ref\">Zebra</span> or <span class=\"enc-title-ref enc-alt-title\">Equus</span>, a <a class=\"enc-link enc-qv\" href=\"/item/7\" data-ref=\"7\">horse</a>.</p>"));
        assert!(h.contains("<span class=\"enc-secnum\">A</span> Sub"));
        let j = blocks_json(&a.blocks);
        assert_eq!(j[0]["kind"], "introduction");
        let tr = &j[0]["blocks"][0]["c"][0];
        assert_eq!(tr["t"], "title-ref");
        assert_eq!(tr["title"], json!(["Zebra"]));
        assert_eq!(tr["alt"], json!(["Equus"]));
        assert_eq!(j[1]["blocks"][0]["col"], "margin");
    }

    #[test]
    fn kids_and_sidebar_layouts() {
        let d = xml::parse(SAMPLE).unwrap();
        let placements = vec![
            pl(1, 5, "tableinline", 3, Column::Margin),
            pl(2, 6, "media", 1, Column::Text),
            pl(2, 7, "audio", 2, Column::Margin),
        ];
        let conv = |layout| {
            let mut t = Tracker::default();
            convert(&mut t, &d.root, &ItemInfo { title: None, alt_title: &[], placements: &placements, layout })
        };
        let k = conv(Layout::Kids);
        match (&k.blocks[0], k.blocks.last().unwrap()) {
            (Block::Section { blocks, .. }, Block::MediaGrid(g)) => {
                assert!(matches!(blocks.last(), Some(Block::Media { ref_id: 5, col: Column::Text, .. })));
                assert_eq!(g.iter().map(|m| m.ref_id).collect::<Vec<_>>(), vec![6, 7]);
            }
            o => panic!("{o:?}"),
        }
        let s = conv(Layout::Sidebar);
        assert!(matches!(&s.blocks[0], Block::Cluster(c) if c.len() == 3 && c[0].ref_id == 6));
        let mut h = String::new();
        render_blocks(&s.blocks, &|_| None, &mut h);
        assert!(h.starts_with("<div class=\"enc-media-cluster enc-col-margin\" data-refs=\"6 7 5\">"));
    }
}
