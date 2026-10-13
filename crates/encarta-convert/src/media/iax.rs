//! `.iax` interactive descriptors: enough parsing to list the files they reference
//! (docs/media.md §2.7). An `.iax` is UTF-8 XML whose root `<iai>` carries the interactive's id,
//! title, size and player control; file names appear as element text (`<image>x.swf</image>`,
//! `<audiofile>`, `<audio>`, `<imagefile>`, `<altimage>`, `<pipgraphic>`, `<bgimage>` …) and as a
//! few attribute values (`pipgraphic=`, `audio=`, `animout=`). Rather than hard-code that list,
//! every text or attribute value that looks like a media file name is reported.

use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reference {
    /// `element` for text, `element@attribute` for an attribute value.
    pub element: String,
    pub name: String,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Iax {
    /// Attributes of the root element (`id`, `friendlyname`, `width`, `height`, `basecontroltype` …).
    pub root: BTreeMap<String, String>,
    pub references: Vec<Reference>,
}

const MEDIA_EXTS: &[&str] = &[
    "swf", "jpg", "jpeg", "gif", "png", "bmp", "mp3", "wav", "wma", "wmv", "mid", "smi", "xml", "iax", "flv", "txt",
];

fn looks_like_file(v: &str) -> bool {
    let v = v.trim();
    if v.is_empty() || v.len() > 200 || v.contains(['<', '>', '"', '|', '*', '?', '\n']) {
        return false;
    }
    match v.rsplit_once('.') {
        Some((stem, ext)) => !stem.is_empty() && MEDIA_EXTS.contains(&ext.to_ascii_lowercase().as_str()),
        None => false,
    }
}

fn unescape(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

/// Splits `name="v" name2='w'` into pairs.
fn attributes(s: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut rest = s;
    while let Some(eq) = rest.find('=') {
        let name = rest[..eq].trim().rsplit(char::is_whitespace).next().unwrap_or("").to_string();
        let after = rest[eq + 1..].trim_start();
        let Some(q) = after.chars().next().filter(|c| *c == '"' || *c == '\'') else {
            rest = &rest[eq + 1..];
            continue;
        };
        let body = &after[1..];
        let end = body.find(q).unwrap_or(body.len());
        out.push((name, unescape(&body[..end])));
        rest = &body[(end + 1).min(body.len())..];
    }
    out
}

pub fn parse(text: &str) -> Iax {
    let mut iax = Iax::default();
    let mut stack: Vec<String> = Vec::new();
    let mut rest = text;
    let mut seen_root = false;
    while let Some(lt) = rest.find('<') {
        let between = &rest[..lt];
        if let Some(el) = stack.last() {
            let t = unescape(between.trim());
            if looks_like_file(&t) {
                iax.references.push(Reference { element: el.clone(), name: t.trim().to_string() });
            }
        }
        rest = &rest[lt..];
        if rest.starts_with("<!--") {
            rest = rest.find("-->").map_or("", |e| &rest[e + 3..]);
            continue;
        }
        if rest.starts_with("<![CDATA[") {
            rest = rest.find("]]>").map_or("", |e| &rest[e + 3..]);
            continue;
        }
        let end = rest.find('>').unwrap_or(rest.len());
        let tag = &rest[1..end];
        rest = rest.get(end + 1..).unwrap_or("");
        if tag.starts_with('?') || tag.starts_with('!') {
            continue;
        }
        if let Some(closing) = tag.strip_prefix('/') {
            let name = closing.trim().to_ascii_lowercase();
            if let Some(pos) = stack.iter().rposition(|s| *s == name) {
                stack.truncate(pos);
            }
            continue;
        }
        let self_closing = tag.ends_with('/');
        let tag = tag.trim_end_matches('/');
        let name: String = tag
            .chars()
            .take_while(|c| !c.is_whitespace())
            .collect::<String>()
            .to_ascii_lowercase();
        for (k, v) in attributes(&tag[name.len().min(tag.len())..]) {
            if !seen_root {
                iax.root.insert(k.to_ascii_lowercase(), v.clone());
            }
            if looks_like_file(&v) {
                iax.references.push(Reference { element: format!("{name}@{}", k.to_ascii_lowercase()), name: v.trim().to_string() });
            }
        }
        seen_root = true;
        if !self_closing {
            stack.push(name);
        }
    }
    iax
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_references() {
        let x = "\u{feff}<?xml version=\"1.0\"?><iai friendlyname=\"Plate &amp; tectonics\" id=\"0005d840\" \
            width=\"574\" height=\"401\"><slides><slide><caption><body>&lt;html&gt;See map.jpg here&lt;/html&gt;</body>\
            <image/></caption><image>0005d840.swf</image><audiofile>Intro Talk.MP3</audiofile>\
            <pipfct pipgraphic=\"usedpip.swf\" x=\"1.5\"/><imagerefid/><altimage> alt_1.jpg </altimage>\
            <!-- <image>ignored.swf</image> --></slide></slides></iai>";
        let iax = parse(x);
        assert_eq!(iax.root.get("id").map(String::as_str), Some("0005d840"));
        assert_eq!(iax.root.get("friendlyname").map(String::as_str), Some("Plate & tectonics"));
        let refs: Vec<(String, String)> =
            iax.references.iter().map(|r| (r.element.clone(), r.name.clone())).collect();
        assert_eq!(
            refs,
            vec![
                ("image".into(), "0005d840.swf".into()),
                ("audiofile".into(), "Intro Talk.MP3".into()),
                ("pipfct@pipgraphic".into(), "usedpip.swf".into()),
                ("altimage".into(), "alt_1.jpg".into()),
            ]
        );
    }

    #[test]
    fn file_name_heuristic() {
        assert!(looks_like_file("a.swf"));
        assert!(looks_like_file("Wall 9.JPG"));
        assert!(!looks_like_file("1.5"));
        assert!(!looks_like_file(".swf"));
        assert!(!looks_like_file("<html>x.jpg"));
    }
}
