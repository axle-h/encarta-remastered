//! A small, strict XML reader that builds an owned tree.
//!
//! The decoded records are well-formed XML (checked by `akc check`), so this reader only needs the
//! subset the disc uses: elements, attributes in either quote style, text, character references,
//! the five predefined entities, the XML declaration, comments, processing instructions and CDATA.
//! Unknown named entity references are kept verbatim in the text and counted, so the completeness
//! report can list them.

use anyhow::{Result, bail};

#[derive(Debug, Clone, PartialEq)]
pub enum Node {
    El(Element),
    Text(String),
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Element {
    pub name: String,
    pub attrs: Vec<(String, String)>,
    pub children: Vec<Node>,
}

impl Element {
    pub fn attr(&self, name: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }

    /// Child elements, skipping text.
    pub fn elements(&self) -> impl Iterator<Item = &Element> {
        self.children.iter().filter_map(|n| match n {
            Node::El(e) => Some(e),
            Node::Text(_) => None,
        })
    }

    pub fn child(&self, name: &str) -> Option<&Element> {
        self.elements().find(|e| e.name == name)
    }

    /// All text below this element, concatenated (markup dropped).
    pub fn text(&self) -> String {
        let mut s = String::new();
        self.collect_text(&mut s);
        s
    }

    fn collect_text(&self, out: &mut String) {
        for c in &self.children {
            match c {
                Node::Text(t) => out.push_str(t),
                Node::El(e) => e.collect_text(out),
            }
        }
    }

    /// Depth-first walk over this element and every descendant element.
    pub fn walk<'a>(&'a self, f: &mut impl FnMut(&'a Element, usize)) {
        self.walk_at(0, f)
    }

    fn walk_at<'a>(&'a self, depth: usize, f: &mut impl FnMut(&'a Element, usize)) {
        f(self, depth);
        for e in self.elements() {
            e.walk_at(depth + 1, f);
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Document {
    pub root: Element,
    pub had_bom: bool,
    /// The XML declaration's body (`version="1.0" encoding="UTF-8"`), if any.
    pub decl: Option<String>,
    pub comments: usize,
    pub processing_instructions: usize,
    pub cdata_sections: usize,
    pub doctype: Option<String>,
    /// Named entity references other than the five predefined ones (kept verbatim in text).
    pub unknown_entities: Vec<String>,
}

pub fn parse(input: &str) -> Result<Document> {
    let mut p = Parser {
        s: input.as_bytes(),
        src: input,
        pos: 0,
        doc: Document::default(),
    };
    if p.s.starts_with(b"\xEF\xBB\xBF") {
        p.pos = 3;
        p.doc.had_bom = true;
    }
    let mut root = None;
    loop {
        p.skip_ws_and_bom();
        if p.pos >= p.s.len() {
            break;
        }
        if p.starts(b"<?") {
            p.pi()?;
        } else if p.starts(b"<!--") {
            p.comment()?;
        } else if p.starts(b"<!DOCTYPE") {
            p.doctype()?;
        } else if p.starts(b"<") {
            if root.is_some() {
                bail!("second root element at byte {}", p.pos);
            }
            root = Some(p.element()?);
        } else {
            bail!("text outside the root element at byte {}", p.pos);
        }
    }
    p.doc.root = match root {
        Some(r) => r,
        None => bail!("no root element"),
    };
    Ok(p.doc)
}

struct Parser<'a> {
    s: &'a [u8],
    src: &'a str,
    pos: usize,
    doc: Document,
}

fn is_name_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.' | b':') || b >= 0x80
}

impl Parser<'_> {
    fn starts(&self, p: &[u8]) -> bool {
        self.s[self.pos..].starts_with(p)
    }

    fn skip_ws_and_bom(&mut self) {
        loop {
            while self.pos < self.s.len() && self.s[self.pos].is_ascii_whitespace() {
                self.pos += 1;
            }
            if self.starts(b"\xEF\xBB\xBF") {
                self.pos += 3;
                self.doc.had_bom = true;
            } else {
                break;
            }
        }
    }

    fn skip_ws(&mut self) {
        while self.pos < self.s.len() && self.s[self.pos].is_ascii_whitespace() {
            self.pos += 1;
        }
    }

    fn find(&self, pat: &[u8]) -> Result<usize> {
        self.s[self.pos..]
            .windows(pat.len())
            .position(|w| w == pat)
            .map(|i| self.pos + i)
            .ok_or_else(|| anyhow::anyhow!("unterminated construct at byte {}", self.pos))
    }

    fn pi(&mut self) -> Result<()> {
        let end = self.find(b"?>")?;
        let body = &self.src[self.pos + 2..end];
        if let Some(rest) = body.strip_prefix("xml") {
            if rest.starts_with(|c: char| c.is_ascii_whitespace()) {
                self.doc.decl = Some(rest.trim().to_string());
            } else {
                self.doc.processing_instructions += 1;
            }
        } else {
            self.doc.processing_instructions += 1;
        }
        self.pos = end + 2;
        Ok(())
    }

    fn comment(&mut self) -> Result<()> {
        let end = self.find(b"-->")?;
        self.doc.comments += 1;
        self.pos = end + 3;
        Ok(())
    }

    fn doctype(&mut self) -> Result<()> {
        // No internal subsets on this disc; a bracket would need nesting support.
        let end = self.find(b">")?;
        self.doc.doctype = Some(self.src[self.pos + 9..end].trim().to_string());
        self.pos = end + 1;
        Ok(())
    }

    fn name(&mut self) -> Result<String> {
        let start = self.pos;
        while self.pos < self.s.len() && is_name_byte(self.s[self.pos]) {
            self.pos += 1;
        }
        if start == self.pos {
            bail!("expected a name at byte {}", start);
        }
        Ok(self.src[start..self.pos].to_string())
    }

    fn element(&mut self) -> Result<Element> {
        // Iterative over the open-element stack: some records nest deeply.
        let (first, empty) = self.open_tag()?;
        if empty {
            return Ok(first);
        }
        let mut stack: Vec<Element> = vec![first];
        let mut text = String::new();
        loop {
            if self.pos >= self.s.len() {
                bail!("unexpected end of input inside <{}>", stack.last().unwrap().name);
            }
            let b = self.s[self.pos];
            if b == b'<' {
                if self.starts(b"</") {
                    flush(&mut text, stack.last_mut().unwrap());
                    self.pos += 2;
                    let name = self.name()?;
                    self.skip_ws();
                    if !self.starts(b">") {
                        bail!("bad end tag at byte {}", self.pos);
                    }
                    self.pos += 1;
                    let el = stack.pop().unwrap();
                    if el.name != name {
                        bail!("</{name}> closes <{}>", el.name);
                    }
                    match stack.last_mut() {
                        Some(parent) => parent.children.push(Node::El(el)),
                        None => return Ok(el),
                    }
                } else if self.starts(b"<!--") {
                    self.comment()?;
                } else if self.starts(b"<![CDATA[") {
                    let end = self.find(b"]]>")?;
                    text.push_str(&self.src[self.pos + 9..end]);
                    self.doc.cdata_sections += 1;
                    self.pos = end + 3;
                } else if self.starts(b"<?") {
                    self.pi()?;
                } else {
                    flush(&mut text, stack.last_mut().unwrap());
                    let (el, empty) = self.open_tag()?;
                    if empty {
                        stack.last_mut().unwrap().children.push(Node::El(el));
                    } else {
                        stack.push(el);
                    }
                }
            } else if b == b'&' {
                self.reference(&mut text)?;
            } else {
                let start = self.pos;
                while self.pos < self.s.len() && !matches!(self.s[self.pos], b'<' | b'&') {
                    self.pos += 1;
                }
                text.push_str(&self.src[start..self.pos]);
            }
        }
    }

    fn reference(&mut self, out: &mut String) -> Result<()> {
        let end = self.find(b";")?;
        let body = &self.src[self.pos + 1..end];
        if end - self.pos > 40 {
            bail!("bad entity reference at byte {}", self.pos);
        }
        let ch = if let Some(hex) = body.strip_prefix("#x").or_else(|| body.strip_prefix("#X")) {
            u32::from_str_radix(hex, 16).ok().and_then(char::from_u32)
        } else if let Some(dec) = body.strip_prefix('#') {
            dec.parse::<u32>().ok().and_then(char::from_u32)
        } else {
            match body {
                "amp" => Some('&'),
                "lt" => Some('<'),
                "gt" => Some('>'),
                "quot" => Some('"'),
                "apos" => Some('\''),
                _ => None,
            }
        };
        match ch {
            Some(c) => out.push(c),
            None => {
                if body.starts_with('#') {
                    bail!("bad character reference &{body};");
                }
                self.doc.unknown_entities.push(body.to_string());
                out.push('&');
                out.push_str(body);
                out.push(';');
            }
        }
        self.pos = end + 1;
        Ok(())
    }

    fn attr_value(&mut self) -> Result<String> {
        let q = self.s[self.pos];
        if q != b'"' && q != b'\'' {
            bail!("unquoted attribute value at byte {}", self.pos);
        }
        self.pos += 1;
        let mut v = String::new();
        loop {
            if self.pos >= self.s.len() {
                bail!("unterminated attribute value");
            }
            let b = self.s[self.pos];
            if b == q {
                self.pos += 1;
                return Ok(v);
            } else if b == b'&' {
                self.reference(&mut v)?;
            } else {
                let start = self.pos;
                while self.pos < self.s.len() && self.s[self.pos] != q && self.s[self.pos] != b'&' {
                    self.pos += 1;
                }
                v.push_str(&self.src[start..self.pos]);
            }
        }
    }
}

fn flush(text: &mut String, el: &mut Element) {
    if !text.is_empty() {
        if let Some(Node::Text(prev)) = el.children.last_mut() {
            prev.push_str(text);
        } else {
            el.children.push(Node::Text(std::mem::take(text)));
        }
        text.clear();
    }
}

impl Parser<'_> {
    /// Reads `<name attr=…>` or `<name …/>`; the flag is true for a self-closing tag.
    fn open_tag(&mut self) -> Result<(Element, bool)> {
        self.pos += 1; // '<'
        let name = self.name()?;
        let mut el = Element {
            name,
            ..Default::default()
        };
        loop {
            self.skip_ws();
            if self.starts(b"/>") {
                self.pos += 2;
                return Ok((el, true));
            }
            if self.starts(b">") {
                self.pos += 1;
                return Ok((el, false));
            }
            let k = self.name()?;
            self.skip_ws();
            if !self.starts(b"=") {
                bail!("attribute {k} without a value at byte {}", self.pos);
            }
            self.pos += 1;
            self.skip_ws();
            let v = self.attr_value()?;
            el.attrs.push((k, v));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_nested_with_entities() {
        let d = parse("\u{feff}<?xml version=\"1.0\"?>\r\n<a x='1' y=\"&amp;b\">t&lt;<b/>u&#233;<c>v</c>&foo;</a>\r\n")
            .unwrap();
        assert!(d.had_bom);
        assert_eq!(d.decl.as_deref(), Some("version=\"1.0\""));
        assert_eq!(d.root.attr("y"), Some("&b"));
        assert_eq!(d.root.text(), "t<uév&foo;");
        assert_eq!(d.unknown_entities, vec!["foo"]);
        assert_eq!(d.root.elements().count(), 2);
    }

    #[test]
    fn rejects_mismatched_tags() {
        assert!(parse("<a><b></a></b>").is_err());
        assert!(parse("<a/><b/>").is_err());
    }
}
