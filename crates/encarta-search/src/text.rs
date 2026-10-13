//! Text helpers shared by the builder and the query side: the analyzers, key normalisation and
//! HTML-to-text.

use tantivy::tokenizer::{
    AsciiFoldingFilter, Language, LowerCaser, RemoveLongFilter, SimpleTokenizer, Stemmer, TextAnalyzer,
    TokenStream,
};

/// Name of the stemming analyzer used by every full-text field.
pub const STEM: &str = "enc_stem";

/// Lower-case, ASCII-folded words, no stemming: the analyzer behind [`normalize`].
pub fn plain_analyzer() -> TextAnalyzer {
    TextAnalyzer::builder(SimpleTokenizer::default())
        .filter(RemoveLongFilter::limit(48))
        .filter(LowerCaser)
        .filter(AsciiFoldingFilter)
        .build()
}

/// [`plain_analyzer`] plus the English Snowball stemmer.
pub fn stem_analyzer() -> TextAnalyzer {
    TextAnalyzer::builder(SimpleTokenizer::default())
        .filter(RemoveLongFilter::limit(48))
        .filter(LowerCaser)
        .filter(AsciiFoldingFilter)
        .filter(Stemmer::new(Language::English))
        .build()
}

/// Calls `f` for every plain (unstemmed, folded, lower-case) word of `text`.
pub fn for_each_word(analyzer: &mut TextAnalyzer, text: &str, mut f: impl FnMut(&str)) {
    let mut stream = analyzer.token_stream(text);
    while stream.advance() {
        f(&stream.token().text);
    }
}

/// The lookup key of a title or query: its plain words joined by single spaces
/// (`"Grévy’s Zebra"` → `"grevy s zebra"`).
pub fn normalize_with(analyzer: &mut TextAnalyzer, text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for_each_word(analyzer, text, |w| {
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(w);
    });
    out
}

/// [`normalize_with`] using a fresh plain analyzer.
pub fn normalize(text: &str) -> String {
    normalize_with(&mut plain_analyzer(), text)
}

/// Plain text of an HTML fragment: tags dropped (block tags become spaces), the common entities
/// decoded, white space collapsed.
pub fn html_to_text(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    while let Some(i) = rest.find(['<', '&']) {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        if rest.starts_with('<') {
            match rest.find('>') {
                Some(j) => {
                    let tag = &rest[1..j];
                    let name = tag.trim_start_matches('/').split([' ', '/']).next().unwrap_or("");
                    if !matches!(
                        name.to_ascii_lowercase().as_str(),
                        "em" | "i" | "b" | "strong" | "sub" | "sup" | "span" | "a" | "u" | "small"
                    ) {
                        out.push(' ');
                    }
                    rest = &rest[j + 1..];
                }
                None => {
                    out.push_str(rest);
                    rest = "";
                }
            }
        } else {
            let end = rest[1..].find(';').map(|j| j + 1).filter(|&j| j <= 10);
            let decoded = end.and_then(|j| decode_entity(&rest[1..j]));
            match (end, decoded) {
                (Some(j), Some(c)) => {
                    out.push(c);
                    rest = &rest[j + 1..];
                }
                _ => {
                    out.push('&');
                    rest = &rest[1..];
                }
            }
        }
    }
    out.push_str(rest);
    collapse_ws(&out)
}

fn decode_entity(name: &str) -> Option<char> {
    Some(match name {
        "amp" => '&',
        "lt" => '<',
        "gt" => '>',
        "quot" => '"',
        "apos" => '\'',
        "nbsp" => ' ',
        _ => {
            let num = name.strip_prefix('#')?;
            let code = match num.strip_prefix(['x', 'X']) {
                Some(hex) => u32::from_str_radix(hex, 16).ok()?,
                None => num.parse().ok()?,
            };
            char::from_u32(code)?
        }
    })
}

/// Collapses runs of white space (including no-break spaces) into one ASCII space and trims.
pub fn collapse_ws(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for word in s.split(|c: char| c.is_whitespace()).filter(|w| !w.is_empty()) {
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(word);
    }
    out
}

/// At most `max` characters of `s`, cut at a word boundary, with an ellipsis when cut.
pub fn truncate_words(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let cut: String = s.chars().take(max).collect();
    let cut = match cut.rfind(' ') {
        Some(i) if i > max / 2 => &cut[..i],
        _ => &cut[..],
    };
    format!("{}…", cut.trim_end_matches([',', ';', ':', ' ']))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_titles() {
        assert_eq!(normalize("Grévy’s Zebra"), "grevy s zebra");
        assert_eq!(normalize("  ‘Akko "), "akko");
        assert_eq!(normalize("Expo '67: Geodesic Dome"), "expo 67 geodesic dome");
    }

    #[test]
    fn strips_html() {
        assert_eq!(
            html_to_text("<p id=\"p1\">The <em>Equus</em> &amp; co<br>x&#233;&nbsp;y</p>"),
            "The Equus & co xé y"
        );
        assert_eq!(html_to_text("a &unknown; b < c"), "a &unknown; b < c");
    }

    #[test]
    fn truncates() {
        assert_eq!(truncate_words("one two three four", 9), "one two…");
        assert_eq!(truncate_words("short", 9), "short");
    }
}
