//! Search for the converted Encarta content: a tantivy full-text index (titles, aliases, section
//! headings, paragraphs with deep links), autocomplete over Encarta's word wheel, and spelling
//! suggestions from the disc's lexicons.
//!
//! Build with [`content_db::build_from_content_db`] (the `encarta-convert search` command) or
//! with [`build::IndexBuilder`] from typed documents; query with [`query::SearchIndex`].

mod bin_io;
pub mod build;
pub mod content_db;
pub mod query;
mod ro_dir;
pub mod schema;
pub mod spell;
pub mod suggest;
pub mod text;

pub use build::{BuildOptions, BuildStats, IndexBuilder, ItemDoc, ParaDoc};
pub use query::{Hit, SearchIndex, SearchRequest, SearchResponse};
pub use spell::SpellResult;
pub use suggest::Suggestion;
