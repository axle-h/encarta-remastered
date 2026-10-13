//! The tantivy schema.
//!
//! One index holds two document types:
//! - **item** documents (one per searchable item: article, media item, dictionary entry,
//!   timeline event, map, quotation …) carry the ranking fields: `title` (boosted), `aliases`
//!   (PFX sort/display titles), `headings` (section titles) and `body` (the whole text);
//! - **para** documents (one per article paragraph) carry `para_text` plus the paragraph id, so
//!   that a hit can deep-link to `#p<id>`.
//!
//! The two use separate text fields so their BM25 length statistics do not mix.

use crate::text::{STEM, plain_analyzer, stem_analyzer};
use tantivy::Index;
use tantivy::schema::{
    FAST, Field, INDEXED, IndexRecordOption, STORED, STRING, Schema, TextFieldIndexing, TextOptions,
};

pub const DOC_ITEM: u64 = 0;
pub const DOC_PARA: u64 = 1;

/// Broad result categories (the `kind` facet). The code is the `cat_code` fast field value.
pub const CATEGORIES: [&str; 9] =
    ["article", "media", "dictionary", "timeline", "map", "quotation", "sidebar", "web", "other"];

pub fn category_code(name: &str) -> Option<u64> {
    CATEGORIES.iter().position(|c| *c == name).map(|i| i as u64)
}

/// Product editions (the `edition` facet); bit `1 << i` in `edition_mask`.
pub const EDITIONS: [&str; 2] = ["deluxe", "kids"];

#[derive(Clone, Copy, Debug)]
pub struct Fields {
    pub doc_type: Field,
    pub ref_id: Field,
    pub category: Field,
    pub cat_code: Field,
    pub item_kind: Field,
    pub edition: Field,
    pub edition_mask: Field,
    pub dictionary: Field,
    pub title: Field,
    pub title_html: Field,
    pub title_key: Field,
    pub aliases: Field,
    pub headings: Field,
    pub body: Field,
    pub summary: Field,
    pub prior: Field,
    pub para_kind: Field,
    pub para_id: Field,
    pub section_id: Field,
    pub section_title: Field,
    pub para_text: Field,
}

fn text(stored: bool) -> TextOptions {
    let o = TextOptions::default().set_indexing_options(
        TextFieldIndexing::default()
            .set_tokenizer(STEM)
            .set_index_option(IndexRecordOption::WithFreqsAndPositions),
    );
    if stored { o.set_stored() } else { o }
}

pub fn build_schema() -> (Schema, Fields) {
    let mut b = Schema::builder();
    let fields = Fields {
        doc_type: b.add_u64_field("doc_type", INDEXED | FAST),
        ref_id: b.add_u64_field("ref_id", INDEXED | STORED | FAST),
        category: b.add_text_field("category", STRING | STORED),
        cat_code: b.add_u64_field("cat_code", INDEXED | FAST),
        item_kind: b.add_text_field("item_kind", STRING | STORED),
        edition: b.add_text_field("edition", STRING | STORED),
        edition_mask: b.add_u64_field("edition_mask", FAST),
        dictionary: b.add_text_field("dictionary", STRING | STORED),
        title: b.add_text_field("title", text(true)),
        title_html: b.add_text_field("title_html", STORED),
        title_key: b.add_text_field("title_key", STRING),
        aliases: b.add_text_field("aliases", text(false)),
        headings: b.add_text_field("headings", text(false)),
        body: b.add_text_field("body", text(false)),
        summary: b.add_text_field("summary", STORED),
        prior: b.add_f64_field("prior", FAST),
        para_kind: b.add_text_field("para_kind", STRING | STORED),
        para_id: b.add_u64_field("para_id", STORED),
        section_id: b.add_u64_field("section_id", STORED),
        section_title: b.add_text_field("section_title", STORED),
        para_text: b.add_text_field("para_text", text(true)),
    };
    (b.build(), fields)
}

/// Looks the fields up in an opened index (schema field names are the contract).
pub fn fields_of(schema: &Schema) -> tantivy::Result<Fields> {
    let f = |n: &str| schema.get_field(n);
    Ok(Fields {
        doc_type: f("doc_type")?,
        ref_id: f("ref_id")?,
        category: f("category")?,
        cat_code: f("cat_code")?,
        item_kind: f("item_kind")?,
        edition: f("edition")?,
        edition_mask: f("edition_mask")?,
        dictionary: f("dictionary")?,
        title: f("title")?,
        title_html: f("title_html")?,
        title_key: f("title_key")?,
        aliases: f("aliases")?,
        headings: f("headings")?,
        body: f("body")?,
        summary: f("summary")?,
        prior: f("prior")?,
        para_kind: f("para_kind")?,
        para_id: f("para_id")?,
        section_id: f("section_id")?,
        section_title: f("section_title")?,
        para_text: f("para_text")?,
    })
}

pub fn register_analyzers(index: &Index) {
    index.tokenizers().register(STEM, stem_analyzer());
    index.tokenizers().register("enc_plain", plain_analyzer());
}
