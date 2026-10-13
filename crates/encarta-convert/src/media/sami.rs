//! SAMI (Synchronized Accessible Media Interchange) captions → WebVTT.
//!
//! Written from the public SAMI 1.0 description and the files on the disc (docs/media.md §2.5):
//! a `<STYLE>` block defines caption classes (`.ENUSCC {Name: …; lang: en-US-CC}`) and a source
//! style (`#Source`); the body is a list of `<SYNC Start=ms>` blocks, each holding `<P Class=…>`
//! paragraphs that replace what is on screen for that class. `&nbsp;` alone clears the screen.
//! The parser is tolerant: tags are matched case-insensitively, `</P>` and `<BODY>` are optional,
//! and table/body/font markup is ignored.

use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq)]
pub struct Paragraph {
    pub class: String,
    /// `ID=Source` marks the speaker/source line.
    pub source: bool,
    /// Text with `\n` for line breaks and `<i>`…`</i>` kept as markers.
    pub text: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Sync {
    pub start_ms: u64,
    pub duration_ms: Option<u64>,
    pub paragraphs: Vec<Paragraph>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Sami {
    /// Class name (upper-case) → (display name, language tag).
    pub classes: BTreeMap<String, (Option<String>, Option<String>)>,
    pub length_ms: Option<u64>,
    pub syncs: Vec<Sync>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Track {
    pub class: String,
    pub lang: Option<String>,
    pub label: Option<String>,
    pub cues: usize,
    pub vtt: String,
}

/// Decodes SAMI bytes: UTF-8 when valid (BOM optional), otherwise Windows-1252.
pub fn decode(bytes: &[u8]) -> String {
    let b = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
    match std::str::from_utf8(b) {
        Ok(s) => s.to_string(),
        Err(_) => encoding_rs::WINDOWS_1252.decode_without_bom_handling(b).0.into_owned(),
    }
}

fn attr(tag: &str, name: &str) -> Option<String> {
    // Attributes: name=value, name="value" or name='value'; case-insensitive names.
    let lower = tag.to_ascii_lowercase();
    let mut search = 0;
    while let Some(pos) = lower[search..].find(name) {
        let at = search + pos;
        search = at + name.len();
        let before_ok = at == 0 || !lower.as_bytes()[at - 1].is_ascii_alphanumeric();
        let rest = tag[at + name.len()..].trim_start();
        if !before_ok || !rest.starts_with('=') {
            continue;
        }
        let v = rest[1..].trim_start();
        let value = if let Some(q) = v.strip_prefix('"') {
            q.split('"').next().unwrap_or("")
        } else if let Some(q) = v.strip_prefix('\'') {
            q.split('\'').next().unwrap_or("")
        } else {
            v.split(|c: char| c.is_whitespace() || c == '>').next().unwrap_or("")
        };
        return Some(value.to_string());
    }
    None
}

fn entity(name: &str) -> Option<char> {
    if let Some(num) = name.strip_prefix('#') {
        let code = match num.strip_prefix(['x', 'X']) {
            Some(h) => u32::from_str_radix(h, 16).ok()?,
            None => num.parse().ok()?,
        };
        return char::from_u32(code);
    }
    Some(match name {
        "nbsp" => '\u{a0}',
        "amp" => '&',
        "lt" => '<',
        "gt" => '>',
        "quot" => '"',
        "apos" => '\'',
        "mdash" => '—',
        "ndash" => '–',
        "hellip" => '…',
        "lsquo" => '‘',
        "rsquo" => '’',
        "ldquo" => '“',
        "rdquo" => '”',
        "copy" => '©',
        "reg" => '®',
        "deg" => '°',
        "eacute" => 'é',
        "egrave" => 'è',
        "ecirc" => 'ê',
        "euml" => 'ë',
        "aacute" => 'á',
        "agrave" => 'à',
        "acirc" => 'â',
        "auml" => 'ä',
        "aring" => 'å',
        "atilde" => 'ã',
        "iacute" => 'í',
        "igrave" => 'ì',
        "icirc" => 'î',
        "iuml" => 'ï',
        "oacute" => 'ó',
        "ograve" => 'ò',
        "ocirc" => 'ô',
        "ouml" => 'ö',
        "otilde" => 'õ',
        "oslash" => 'ø',
        "uacute" => 'ú',
        "ugrave" => 'ù',
        "ucirc" => 'û',
        "uuml" => 'ü',
        "uring" => 'ů',
        "ntilde" => 'ñ',
        "ccedil" => 'ç',
        "szlig" => 'ß',
        "Eacute" => 'É',
        "Aacute" => 'Á',
        "Oacute" => 'Ó',
        "Uacute" => 'Ú',
        "Ntilde" => 'Ñ',
        "Ccedil" => 'Ç',
        _ => return None,
    })
}

fn decode_entities(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(pos) = rest.find('&') {
        out.push_str(&rest[..pos]);
        let tail = &rest[pos + 1..];
        match tail.find(';') {
            Some(end) if end <= 10 => match entity(&tail[..end]) {
                Some(c) => {
                    out.push(c);
                    rest = &tail[end + 1..];
                }
                None => {
                    out.push('&');
                    rest = tail;
                }
            },
            _ => {
                out.push('&');
                rest = tail;
            }
        }
    }
    out.push_str(rest);
    out
}

/// Parses the `.CLASS { … }` rules of the STYLE block.
fn parse_styles(css: &str, classes: &mut BTreeMap<String, (Option<String>, Option<String>)>) {
    let css = css.replace("<!--", " ").replace("-->", " ");
    for rule in css.split('}') {
        let Some((selector, body)) = rule.split_once('{') else { continue };
        let Some(name) = selector.trim().strip_prefix('.') else { continue };
        if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') {
            continue;
        }
        let mut label = None;
        let mut lang = None;
        for decl in body.split(';') {
            if let Some((k, v)) = decl.split_once(':') {
                match k.trim().to_ascii_lowercase().as_str() {
                    "name" => label = Some(v.trim().replace('_', " ")),
                    "lang" => lang = Some(v.trim().to_string()),
                    _ => {}
                }
            }
        }
        classes.insert(name.to_ascii_uppercase(), (label, lang));
    }
}

/// Normalises paragraph text: collapses whitespace, keeps `\n` from `<br>`.
fn tidy(text: &str) -> String {
    text.split('\n')
        .map(|line| line.split_whitespace().collect::<Vec<_>>().join(" "))
        .map(|l| l.replace('\u{a0}', " ").trim().to_string())
        .filter(|l| !l.is_empty() && l != "<i></i>")
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn parse(text: &str) -> Sami {
    let mut sami = Sami::default();
    let mut current: Option<Sync> = None;
    let mut para: Option<Paragraph> = None;
    let mut rest = text;

    fn close_para(para: &mut Option<Paragraph>, current: &mut Option<Sync>) {
        if let (Some(mut p), Some(sync)) = (para.take(), current.as_mut()) {
            p.text = tidy(&p.text);
            // A `<P>` without a class continues the previous paragraph's class (one file on the
            // disc has `<P Class=ENUSCC ID=Source>…</P><P>caption</P>`).
            if let (true, Some(prev)) = (p.class.is_empty(), sync.paragraphs.last()) {
                p.class = prev.class.clone();
            }
            sync.paragraphs.push(p);
        }
    }

    while !rest.is_empty() {
        let Some(lt) = rest.find('<') else {
            if let Some(p) = para.as_mut() {
                p.text.push_str(&decode_entities(rest));
            }
            break;
        };
        if let Some(p) = para.as_mut() {
            p.text.push_str(&decode_entities(&rest[..lt]));
        }
        rest = &rest[lt..];
        if rest.starts_with("<!--") {
            let end = rest.find("-->").map_or(rest.len(), |e| e + 3);
            rest = &rest[end..];
            continue;
        }
        let end = rest.find('>').map_or(rest.len(), |e| e + 1);
        let tag = &rest[1..end.saturating_sub(1).max(1)];
        rest = &rest[end..];
        let closing = tag.starts_with('/');
        let name: String = tag
            .trim_start_matches('/')
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric())
            .collect::<String>()
            .to_ascii_lowercase();
        match (name.as_str(), closing) {
            ("style", false) => {
                let lower = rest.to_ascii_lowercase();
                let stop = lower.find("</style").unwrap_or(rest.len());
                parse_styles(&rest[..stop], &mut sami.classes);
                rest = &rest[stop..];
            }
            ("samiparam", false) => {
                if let Some(len) = attr(tag, "length").and_then(|v| v.parse().ok()) {
                    sami.length_ms = Some(len);
                }
            }
            ("sync", false) => {
                close_para(&mut para, &mut current);
                if let Some(s) = current.take() {
                    sami.syncs.push(s);
                }
                let start = attr(tag, "start").and_then(|v| v.parse().ok());
                if let Some(start_ms) = start {
                    current = Some(Sync {
                        start_ms,
                        duration_ms: attr(tag, "duration").and_then(|v| v.parse().ok()),
                        paragraphs: Vec::new(),
                    });
                }
            }
            ("sync", true) | ("body", true) | ("sami", true) => {
                close_para(&mut para, &mut current);
                if let Some(s) = current.take() {
                    sami.syncs.push(s);
                }
            }
            ("p", false) => {
                close_para(&mut para, &mut current);
                if current.is_some() {
                    para = Some(Paragraph {
                        class: attr(tag, "class").unwrap_or_default().to_ascii_uppercase(),
                        source: attr(tag, "id").is_some_and(|v| v.eq_ignore_ascii_case("source")),
                        text: String::new(),
                    });
                }
            }
            ("p", true) => close_para(&mut para, &mut current),
            ("br", _) => {
                if let Some(p) = para.as_mut() {
                    p.text.push('\n');
                }
            }
            ("i" | "it" | "em", c) => {
                if let Some(p) = para.as_mut() {
                    p.text.push_str(if c { "</i>" } else { "<i>" });
                }
            }
            _ => {}
        }
    }
    close_para(&mut para, &mut current);
    if let Some(s) = current.take() {
        sami.syncs.push(s);
    }
    sami.syncs.sort_by_key(|s| s.start_ms);
    sami
}

fn timestamp(ms: u64) -> String {
    format!("{:02}:{:02}:{:02}.{:03}", ms / 3_600_000, ms / 60_000 % 60, ms / 1000 % 60, ms % 1000)
}

/// Escapes text for a WebVTT cue, keeping the `<i>`/`</i>` markers produced by the parser.
fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace("&lt;i&gt;", "<i>")
        .replace("&lt;/i&gt;", "</i>")
        .replace("-->", "--&gt;")
}

/// Default length of a final cue when neither `Duration` nor `SAMIParam Length` says otherwise.
const LAST_CUE_MS: u64 = 5000;

/// One WebVTT track per caption class that has text.
pub fn to_webvtt(sami: &Sami) -> Vec<Track> {
    let mut class_names: Vec<String> = sami
        .syncs
        .iter()
        .flat_map(|s| s.paragraphs.iter().map(|p| p.class.clone()))
        .collect();
    class_names.sort();
    class_names.dedup();
    // Paragraphs that still have no class belong to every class.
    if class_names.len() > 1 {
        class_names.retain(|c| !c.is_empty());
    }
    let mut tracks = Vec::new();
    for class in class_names {
        let syncs: Vec<&Sync> = sami
            .syncs
            .iter()
            .filter(|s| s.paragraphs.iter().any(|p| p.class == class || p.class.is_empty()))
            .collect();
        let mut body = String::new();
        let mut cues = 0;
        for (i, sync) in syncs.iter().enumerate() {
            let lines: Vec<String> = sync
                .paragraphs
                .iter()
                .filter(|p| (p.class == class || p.class.is_empty()) && !p.text.is_empty())
                .map(|p| {
                    if p.source {
                        format!("<c.source>{}</c>", escape(&p.text))
                    } else {
                        escape(&p.text)
                    }
                })
                .collect();
            if lines.is_empty() {
                continue;
            }
            let end = match (sync.duration_ms, syncs.get(i + 1)) {
                (Some(d), _) => sync.start_ms + d,
                (None, Some(next)) => next.start_ms,
                (None, None) => sami
                    .length_ms
                    .filter(|&l| l > sync.start_ms)
                    .unwrap_or(sync.start_ms + LAST_CUE_MS),
            };
            if end <= sync.start_ms {
                continue;
            }
            cues += 1;
            body.push_str(&format!("\n{}\n{} --> {}\n{}\n", cues, timestamp(sync.start_ms), timestamp(end), lines.join("\n")));
        }
        if cues == 0 {
            continue;
        }
        let (label, lang) = sami.classes.get(&class).cloned().unwrap_or((None, None));
        // `en-US-CC` → `en-US`: the `CC` suffix is SAMI's marker for closed captions.
        let lang = lang.map(|l| l.strip_suffix("-CC").map(str::to_string).unwrap_or(l));
        let mut vtt = String::from("WEBVTT\n");
        if let Some(l) = &lang {
            vtt.push_str(&format!("Language: {l}\n"));
        }
        vtt.push_str("\nSTYLE\n::cue(.source) {\n  color: silver;\n}\n");
        vtt.push_str(&body);
        tracks.push(Track { class, lang, label, cues, vtt });
    }
    tracks
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A synthetic SAMI file in the style found on the disc (no `<BODY>`, unclosed paragraphs,
    /// a source line, italics, entities, a clearing `&nbsp;` sync).
    const SAMPLE: &str = "<SAMI>\r\n<Head>\r\n\t<Title>Test Captions</Title>\r\n\
        \t<SAMIParam Length=9000 TimeStamp=\"ms\">\r\n\t<STYLE TYPE=\"text/css\">\r\n\t\t<!--\r\n\
        \t\tP {font-size: 10pt; font-family: sans-serif;}\r\n\
        \t\t.ENUSCC {margin-left: 0; Name: English_Captions; lang: en-US-CC; color: white;}\r\n\
        \t\t#Source {color: silver;}\r\n\t\t-->\r\n\t</Style>\r\n</Head>\r\n\
        <SYNC Start=0>\r\n\t<P Class=ENUSCC ID=Source>A Narrator</P>\r\n\
        \t<P Class=ENUSCC align=left>First   line &amp; more</P>\r\n</SYNC>\r\n\
        <SYNC Start=2500>\r\n\t<P Class=ENUSCC>( <it>music plays</it> )<br>second &mdash; row\r\n\
        <SYNC Start=4000>\r\n\t<P Class=ENUSCC>&nbsp;</P>\r\n\
        <SYNC Start=6000>\r\n\t<P Class=ENUSCC>caf&eacute; &lt;3 -> ok</P>\r\n</Body>\r\n</SAMI>\r\n";

    #[test]
    fn parses_structure() {
        let s = parse(SAMPLE);
        assert_eq!(s.length_ms, Some(9000));
        assert_eq!(
            s.classes.get("ENUSCC"),
            Some(&(Some("English Captions".into()), Some("en-US-CC".into())))
        );
        assert_eq!(s.syncs.len(), 4);
        assert_eq!(s.syncs[0].paragraphs.len(), 2);
        assert!(s.syncs[0].paragraphs[0].source);
        assert_eq!(s.syncs[0].paragraphs[1].text, "First line & more");
        assert_eq!(s.syncs[1].paragraphs[0].text, "( <i>music plays</i> )\nsecond — row");
        assert_eq!(s.syncs[2].paragraphs[0].text, "");
    }

    #[test]
    fn writes_webvtt() {
        let tracks = to_webvtt(&parse(SAMPLE));
        assert_eq!(tracks.len(), 1);
        let t = &tracks[0];
        assert_eq!(t.lang.as_deref(), Some("en-US"));
        assert_eq!(t.cues, 3);
        let expected = "WEBVTT\nLanguage: en-US\n\nSTYLE\n::cue(.source) {\n  color: silver;\n}\n\
            \n1\n00:00:00.000 --> 00:00:02.500\n<c.source>A Narrator</c>\nFirst line &amp; more\n\
            \n2\n00:00:02.500 --> 00:00:04.000\n( <i>music plays</i> )\nsecond — row\n\
            \n3\n00:00:06.000 --> 00:00:09.000\ncafé &lt;3 -&gt; ok\n";
        assert_eq!(t.vtt, expected);
    }

    #[test]
    fn duration_and_fallback_end() {
        let s = parse("<SAMI><BODY><SYNC Start=1000 Duration=500><P Class=X>a<SYNC Start=3000><P Class=X>b</BODY></SAMI>");
        let t = &to_webvtt(&s)[0];
        assert!(t.vtt.contains("00:00:01.000 --> 00:00:01.500\na"));
        assert!(t.vtt.contains("00:00:03.000 --> 00:00:08.000\nb"));
        assert_eq!(t.lang, None);
    }

    #[test]
    fn classless_paragraphs() {
        let s = parse("<SAMI><SYNC Start=0><P Class=A ID=Source>who<P>says this<SYNC Start=1000><P>and all<SYNC Start=2000><P Class=B>b</SAMI>");
        assert_eq!(s.syncs[0].paragraphs[1].class, "A");
        let t = to_webvtt(&s);
        assert_eq!(t.iter().map(|t| t.class.as_str()).collect::<Vec<_>>(), ["A", "B"]);
        assert!(t[0].vtt.contains("<c.source>who</c>\nsays this"));
        assert!(t[0].vtt.contains("and all"));
        assert!(t[1].vtt.contains("and all"));
    }

    #[test]
    fn decodes_cp1252() {
        assert_eq!(decode(b"caf\xe9 \x93q\x94"), "café “q”");
        assert_eq!(decode("café".as_bytes()), "café");
        assert_eq!(timestamp(3_723_004), "01:02:03.004");
    }
}
