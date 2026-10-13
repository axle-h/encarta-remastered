//! Character tables: `<entity tag="…"/>` names, and the three private fonts that `<fs type>` runs
//! are typed in. Our own tables, written from the ISO 8879 / HTML entity sets and Unicode, the
//! Adobe Symbol encoding (public), and the glyphs of the fonts shipped in `AREF/FONTS` (rendered
//! and identified by eye; see `docs/content-schema.md` §Characters).

/// Unicode text for an entity tag, or `None` when the tag is unknown.
pub fn entity(tag: &str) -> Option<&'static str> {
    Some(match tag {
        // markup-significant and spacing
        "amp" => "&",
        "lt" => "<",
        "gt" => ">",
        "quot" => "\"",
        "apos" => "'",
        "nbsp" => "\u{A0}",
        "emsp" => "\u{2003}",
        "thinsp" => "\u{2009}",
        // punctuation
        "hellip" => "…",
        "mdash" => "—",
        "ndash" => "–",
        "lsquo" => "‘",
        "rsquo" => "’",
        "ldquo" => "“",
        "rdquo" => "”",
        "deg" => "°",
        "acute" => "´",
        "lcub" => "{",
        "rcub" => "}",
        "frac1.2" => "½",
        // ISO Latin 1
        "aacute" => "á",
        "Aacute" => "Á",
        "agrave" => "à",
        "Agrave" => "À",
        "acirc" => "â",
        "atilde" => "ã",
        "auml" => "ä",
        "aring" => "å",
        "Aring" => "Å",
        "aelig" => "æ",
        "ccedil" => "ç",
        "Ccedil" => "Ç",
        "eacute" => "é",
        "Eacute" => "É",
        "egrave" => "è",
        "ecirc" => "ê",
        "euml" => "ë",
        "iacute" => "í",
        "Iacute" => "Í",
        "igrave" => "ì",
        "icirc" => "î",
        "Icirc" => "Î",
        "iuml" => "ï",
        "ntilde" => "ñ",
        "oacute" => "ó",
        "Oacute" => "Ó",
        "ograve" => "ò",
        "ocirc" => "ô",
        "otilde" => "õ",
        "ouml" => "ö",
        "Ouml" => "Ö",
        "oslash" => "ø",
        "Oslash" => "Ø",
        "uacute" => "ú",
        "ugrave" => "ù",
        "ucirc" => "û",
        "uuml" => "ü",
        "Uuml" => "Ü",
        "yacute" => "ý",
        "yuml" => "ÿ",
        // ISO Latin 2 and extended Latin
        "amacr" | "amacro" => "ā",
        "Amacr" => "Ā",
        "abreve" => "ă",
        "acaron" => "ǎ",
        "adot" => "ȧ",
        "cacute" => "ć",
        "ccaron" => "č",
        "Ccaron" => "Č",
        "Dstrok" => "Đ",
        "ecaron" => "ě",
        "Ecaron" => "Ě",
        "edot" => "ė",
        "emacr" => "ē",
        "eogon" => "ę",
        "gbreve" => "ğ",
        "imacr" => "ī",
        "inodot" => "ı",
        "Idot" => "İ",
        "lstrok" | "lslash" => "ł",
        "Lstrok" => "Ł",
        "nacute" => "ń",
        "ncaron" => "ň",
        "obreve" => "ŏ",
        "odblac" => "ő",
        "omacr" => "ō",
        "Omacr" => "Ō",
        "rcaron" => "ř",
        "sacute" => "ś",
        "Sacute" => "Ś",
        "scaron" => "š",
        "Scaron" => "Š",
        "scedil" => "ş",
        "Scedil" => "Ş",
        "tcedil" => "ţ",
        "Tcedil" => "Ţ",
        "ubreve" => "ŭ",
        "umacr" => "ū",
        "uring" => "ů",
        "zacute" => "ź",
        "zcaron" => "ž",
        "Zdot" => "Ż",
        // transliteration letters (Arabic, Semitic, Sanskrit romanisation)
        "dcedil" => "ḑ",
        "hcedil" => "ḩ",
        "Hcedil" => "Ḩ",
        "hlowdot" => "ḥ",
        "Hlowdot" => "Ḥ",
        "nlowdot" => "ṇ",
        "tlowdot" => "ṭ",
        "Tlowdot" => "Ṭ",
        "hlowmacr" => "ẖ",
        "Hlowmacr" => "H\u{331}",
        "zlowmacr" => "ẕ",
        "Zlowmacron" => "Ẕ",
        // letters with combining marks that have no precomposed form
        "acedil" => "a\u{327}",
        "ecedil" => "ȩ",
        "numl" => "n\u{308}",
        "cbreve" => "c\u{306}",
        "rbreve" => "r\u{306}",
        "zbreve" => "z\u{306}",
        // further names found in the search exports, MindMaze and timeline strings
        "Acirc" => "Â",
        "Auml" => "Ä",
        "Cacute" => "Ć",
        "Dmacr" => "D\u{304}",
        "Egrave" => "È",
        "Lslash" => "Ł",
        "Ntilde" => "Ñ",
        "Obreve" => "Ŏ",
        "Rcaron" => "Ř",
        "Sgrave" => "S\u{300}",
        "Uacute" => "Ú",
        "Zcaron" => "Ž",
        "aogon" => "ą",
        "dcaron" => "ď",
        "ebreve" => "ĕ",
        "oelig" => "œ",
        "ocaron" => "ǒ",
        "scedilla" => "ş",
        "szlig" => "ß",
        "tcaron" => "ť",
        "ucaron" => "ǔ",
        "ucedil" => "u\u{327}",
        "udblac" => "ű",
        "umacron" => "ū",
        "uumlaut" => "ü",
        "zdot" => "ż",
        "idot" => "i",
        "middot" => "·",
        "beta" => "β",
        "trade" => "™",
        "excl" => "!",
        "grave" => "`",
        "frac12" => "½",
        "tab" => " ",
        "ldaquo" => "«",
        "rdaquo" => "»",
        "elowdotcirc" => "ệ",
        "olowdotcirc" => "ộ",
        // misspelt names, mapped to what the text needs
        "acuate" => "´",
        "ecute" => "é",
        "eumul" => "ë",
        "dquo" => "\"",
        "rquo" | "rsquote" | "Lsquo" => "’",
        "uml" => "ü",    // "D&uml;rer" (Dürer)
        "sbreve" => "š", // "Milo&sbreve;" (Miloš)
        // chemical bonds and a time signature in MindMaze clues
        "z.1horbar" => "—",
        "z.2horbar" => "=",
        "z.3horbar" => "≡",
        "z.notation3.4" => "3/4",
        // Vietnamese
        "acircacute" => "ấ",
        "ecircacute" => "ế",
        "ocircgrave" => "ồ",
        _ => return None,
    })
}

/// Replaces SGML-style references left as literal text (`&eacute;` after XML unescaping, or in
/// the non-XML MindMaze and timeline strings). Calls `unknown` for names not in the table, which
/// stay verbatim.
pub fn resolve_literal(s: &str, mut unknown: impl FnMut(&str)) -> std::borrow::Cow<'_, str> {
    if !s.contains('&') {
        return s.into();
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        let after = &rest[i + 1..];
        let end = after
            .char_indices()
            .take(33)
            .find(|(_, c)| !(c.is_ascii_alphanumeric() || *c == '#' || *c == '.'))
            .map(|(j, c)| (j, c == ';'));
        match end {
            Some((j, true)) if j > 0 => {
                let name = &after[..j];
                match entity_owned(name) {
                    Some(t) => out.push_str(&t),
                    None => {
                        // "&c." (et cetera) followed by a semicolon is not a reference
                        if !name.ends_with('.') {
                            unknown(name);
                        }
                        out.push('&');
                        out.push_str(name);
                        out.push(';');
                    }
                }
                rest = &after[j + 1..];
            }
            _ => {
                out.push('&');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out.into()
}

/// Tags that are not characters but editorial markers. They produce no text.
pub fn marker(tag: &str) -> Option<&'static str> {
    match tag {
        // Forces an entry to the top of an alphabetical list (index and related-article trees).
        "sortattop" => Some("sort-top"),
        // Only in two "new article placeholder" sidebars.
        "startcomma" => Some("start-comma"),
        _ => None,
    }
}

/// Entity text including numeric `#233` tags.
pub fn entity_owned(tag: &str) -> Option<String> {
    if let Some(num) = tag.strip_prefix('#') {
        let n = if let Some(h) = num.strip_prefix(['x', 'X']) {
            u32::from_str_radix(h, 16).ok()?
        } else {
            num.parse().ok()?
        };
        return char::from_u32(n).map(String::from);
    }
    entity(tag).map(str::to_string)
}

/// Private-use code points of the MS Reference text fonts (`AREF/FONTS/REF*.TTF`, `GRGAREF.TTF`):
/// letters with diacritics Unicode lacks as one code point. Identified by rendering the glyphs
/// and reading the words they occur in; `docs/content-schema.md` lists the uncertain ones.
pub fn private_use(c: char) -> Option<&'static str> {
    Some(match c {
        '\u{F6FE}' => "ą",
        '\u{F700}' => "å",  // "Lågen", "Stångebro" (glyph: a with ring/dot above)
        '\u{F717}' => "C\u{306}",
        '\u{F718}' => "c\u{306}",
        '\u{F71B}' => "ć",  // "Milošević"; also in "flèche" (approximate)
        '\u{F727}' => "ᗡ",  // reversed capital D ("Ds back to back")
        '\u{F72F}' => "ę",
        '\u{F73D}' => "Ϝ",  // early letter form in the history of G (approximate)
        '\u{F73E}' => "Ⅎ",  // turned F (approximate)
        '\u{F745}' => "ḫ",
        '\u{F746}' => "Ḫ",
        '\u{F760}' => "M\u{304}",
        '\u{F762}' => "∉",  // "a ∉ S (that is, a is not an element of S)"
        '\u{F76B}' => "n\u{308}",
        '\u{F794}' => "s\u{306}",
        '\u{F795}' => "S\u{306}",
        '\u{F79C}' => "S\u{331}",
        '\u{F79E}' => "S\u{304}",
        '\u{F7AE}' => "ų",
        '\u{F7C2}' => "x\u{304}",
        '\u{F7CA}' => "z\u{306}",
        '\u{F7CC}' => "z\u{327}", // BGN romanisation "Muḩāfaz̧at"
        '\u{F7CD}' => "Z\u{327}",
        '\u{F7D7}' => "œ\u{306}", // Khmer romanisation "Stœ̆ng"
        '\u{F7D8}' => "r\u{306}",
        '\u{F7DA}' => "p\u{304}",
        _ => return None,
    })
}

/// Adobe Symbol encoding (font switch type 1), code → Unicode.
pub fn symbol_font(c: u32) -> Option<char> {
    let greek_upper = "ΑΒΧΔΕΦΓΗΙϑΚΛΜΝΟΠΘΡΣΤΥςΩΞΨΖ";
    let greek_lower = "αβχδεφγηιϕκλμνοπθρστυϖωξψζ";
    Some(match c {
        0x41..=0x5A => greek_upper.chars().nth((c - 0x41) as usize)?,
        0x61..=0x7A => greek_lower.chars().nth((c - 0x61) as usize)?,
        0x22 => '∀',
        0x24 => '∃',
        0x27 => '∋',
        0x2A => '∗',
        0x2D => '−',
        0x40 => '≅',
        0x5C => '∴',
        0x5E => '⊥',
        0x60 => '‾',
        0x7E => '∼',
        0xA1 => 'ϒ',
        0xA2 => '′',
        0xA3 => '≤',
        0xA4 => '⁄',
        0xA5 => '∞',
        0xA6 => 'ƒ',
        0xA7 => '♣',
        0xA8 => '♦',
        0xA9 => '♥',
        0xAA => '♠',
        0xAB => '↔',
        0xAC => '←',
        0xAD => '↑',
        0xAE => '→',
        0xAF => '↓',
        0xB0 => '°',
        0xB1 => '±',
        0xB2 => '″',
        0xB3 => '≥',
        0xB4 => '×',
        0xB5 => '∝',
        0xB6 => '∂',
        0xB7 => '•',
        0xB8 => '÷',
        0xB9 => '≠',
        0xBA => '≡',
        0xBB => '≈',
        0xBC => '…',
        0xBF => '↵',
        0xC0 => 'ℵ',
        0xC1 => 'ℑ',
        0xC2 => 'ℜ',
        0xC3 => '℘',
        0xC4 => '⊗',
        0xC5 => '⊕',
        0xC6 => '∅',
        0xC7 => '∩',
        0xC8 => '∪',
        0xC9 => '⊃',
        0xCA => '⊇',
        0xCB => '⊄',
        0xCC => '⊂',
        0xCD => '⊆',
        0xCE => '∈',
        0xCF => '∉',
        0xD0 => '∠',
        0xD1 => '∇',
        0xD2 | 0xE2 => '®',
        0xD3 | 0xE3 => '©',
        0xD4 | 0xE4 => '™',
        0xD5 => '∏',
        0xD6 => '√',
        0xD7 => '⋅',
        0xD8 => '¬',
        0xD9 => '∧',
        0xDA => '∨',
        0xDB => '⇔',
        0xDC => '⇐',
        0xDD => '⇑',
        0xDE => '⇒',
        0xDF => '⇓',
        0xE0 => '◊',
        0xE1 => '〈',
        0xE5 => '∑',
        0xF1 => '〉',
        0xF2 => '∫',
        0x20..=0x7F => char::from_u32(c)?, // digits and ASCII punctuation are unchanged
        _ => return None,
    })
}

/// One glyph of the MS Reference Specialty font (font switch type 2).
#[derive(Debug, Clone, PartialEq)]
pub enum SpecialGlyph {
    Text(&'static str),
    /// Built-up fraction with a bar.
    Frac(&'static str, &'static str),
    /// Two numbers stacked without a bar (time signatures, nuclide mass/atomic numbers).
    Stack(&'static str, &'static str),
    /// Radical sign over the text.
    Radical(&'static str),
    /// Text with a vinculum (overline).
    Overline(&'static str),
    /// Vector name with an arrow above.
    Vector(&'static str),
}

pub fn specialty_font(c: u32) -> Option<SpecialGlyph> {
    use SpecialGlyph::*;
    Some(match c {
        0x23 => Text("⋮⋮"), // six shared electrons (triple bond, electron-dot)
        0x24 => Vector("AB"),
        0x41 => Vector("OA"),
        0x42 => Vector("OB"),
        // voice-leading arrow pairs (music: oblique, similar and contrary motion)
        0x26 => Stack("↗", "↘"),
        0x27 => Stack("→", "↘"),
        0x28 => Stack("↘", "↘"),
        0x29 => Stack("↗", "↗"),
        0x2A => Stack("→", "↗"),
        0x2B => Stack("↘", "↗"),
        0x2C => Stack("↘", "→"),
        0x2D => Stack("↗", "→"),
        0x32 => Text("\u{10907}"), // Phoenician heth
        0x35 => Text("\u{13254}"), // Egyptian hieroglyph (reed shelter, h)
        0x38 => Text("—"),          // single bond
        0x39 => Text("="),          // double bond
        0x3A => Text("≡"),          // triple bond
        0x70 => Text("4⟌12"),       // long division
        0x73 => Frac("1", "10"),
        0x74 => Frac("1", "100"),
        0x75 => Frac("1", "12"),
        0x77 => Frac("1", "16"),
        0x79 => Frac("1", "2"),
        0x7A => Frac("1", "200"),
        0x7C => Frac("1", "243"),
        0x7D => Frac("1", "27"),
        0x76 => Frac("1", "15750"),
        0x78 => Frac("1", "18"),
        0x7B => Frac("1", "200000"),
        0x7E => Frac("1", "28"),
        0x7F => Frac("1", "2π"),
        0x84 => Frac("1", "5000"),
        0x86 => Frac("1", "60"),
        0x8B => Frac("1", "a"),
        0x8C => Frac("1", "n"),
        0x8F => Frac("14", "07"),
        0xA0 => Frac("8", "9"),
        0xA5 => Frac("n", "k"),
        0xAD => Stack("2", "8"),
        0xB6 => Stack("3", "8"),
        0xB7 => Stack("35", "17"),
        0x80 => Frac("1", "3"),
        0x81 => Frac("1", "32"),
        0x82 => Frac("1", "4"),
        0x83 => Frac("1", "5"),
        0x85 => Frac("1", "6"),
        0x87 => Frac("1", "7"),
        0x88 => Frac("1", "8"),
        0x89 => Frac("1", "81"),
        0x8A => Frac("1", "9"),
        0x8D => Frac("10", "71"),
        0x8E => Frac("12", "4"),
        0x90 => Frac("16", "19"),
        0x91 => Frac("2", "10"),
        0x92 => Frac("2", "3"),
        0x93 => Frac("2", "7"),
        0x94 => Frac("3", "2"),
        0x95 => Frac("3", "4"),
        0x96 => Frac("3", "8"),
        0x97 => Frac("4", "3"),
        0x98 => Frac("4", "5"),
        0x99 => Frac("5", "6"),
        0x9A => Frac("5", "8"),
        0x9B => Frac("6", "8"),
        0x9C => Frac("7", "3"),
        0x9D => Frac("7", "8"),
        0x9E => Frac("8", "4"),
        0xA1 => Frac("a", "b"),
        0xA2 => Frac("c", "d"),
        0xA3 => Frac("c", "λ"),
        0xA4 => Frac("d", "c"),
        0xA6 => Stack("1", "0"),
        0xA7 => Stack("1", "1"),
        0xA8 => Stack("14", "7"),
        0xA9 => Stack("17", "8"),
        0xAA => Stack("2", "1"),
        0xAB => Stack("2", "2"),
        0xAC => Stack("2", "4"),
        0xAE => Stack("23", "11"),
        0xAF => Stack("235", "92"),
        0xB0 => Stack("238", "92"),
        0xB1 => Stack("238", "93"),
        0xB2 => Stack("24", "11"),
        0xB3 => Stack("24", "12"),
        0xB4 => Stack("3", "2"),
        0xB5 => Stack("3", "4"),
        0xB8 => Stack("4", "2"),
        0xB9 => Stack("4", "4"),
        0xBA => Stack("4", "8"),
        0xBB => Stack("5", "8"),
        0xBC => Stack("6", "4"),
        0xBD => Stack("6", "8"),
        0xBE => Stack("9", "8"),
        0xBF => Stack("A", "Z"),
        0xC0 => Stack("n", "k"),
        0xC1 => Radical("-1"),
        0xC2 => Radical("16"),
        0xC3 => Radical("2"),
        0xC4 => Radical("5"),
        0xC5 => Radical("s("),
        0xC6 => Overline("s-a"),
        0xC7 => Overline(")("),
        0xC8 => Overline("s-b"),
        0xC9 => Overline("s-c"),
        0xCA => Overline(")"),
        0xCB => Radical("xy"),
        _ => return None,
    })
}

/// MS Reference 1 (type 6) and MS Reference 2 (type 7) glyphs used on the disc.
pub fn reference_font(font: u32, c: u32) -> Option<SpecialGlyph> {
    use SpecialGlyph::*;
    Some(match (font, c) {
        (6, 0x23) => Text("♯"),
        (6, 0x24) => Text("♭"),
        (6, 0x30) => Text("⓪"),
        (6, 0x31) => Text("①"),
        (6, 0x32) => Text("②"),
        (6, 0x33) => Text("③"),
        (6, 0xB4) => Text("↗"),
        (7, 0x69) => Stack("2", "8"),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tables() {
        assert_eq!(entity("eacute"), Some("é"));
        assert_eq!(entity_owned("#233").as_deref(), Some("é"));
        assert_eq!(entity("nosuch"), None);
        let mut unk = Vec::new();
        assert_eq!(resolve_literal("Caf&eacute; & &zz; &#233;", |n| unk.push(n.to_string())), "Café & &zz; é");
        assert_eq!(unk, vec!["zz"]);
        assert_eq!(marker("sortattop"), Some("sort-top"));
        assert_eq!(symbol_font('p' as u32), Some('π'));
        assert_eq!(symbol_font('S' as u32), Some('Σ'));
        assert_eq!(symbol_font(0xC5), Some('⊕'));
        assert_eq!(specialty_font('y' as u32), Some(SpecialGlyph::Frac("1", "2")));
        assert_eq!(reference_font(6, '#' as u32), Some(SpecialGlyph::Text("♯")));
    }
}
