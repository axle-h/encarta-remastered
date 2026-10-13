//! Turns a record's tokens into XML text (spec §8).

use super::dict::{Kind, Words};
use super::tokens::SHARED;
use anyhow::{Result, bail, ensure};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Neutral,
    AfterWord,
    InStartTag,
}

/// Emitter settings taken from the shared dictionary.
#[derive(Debug, Clone, Copy)]
pub struct EmitFlags {
    pub capitalize: bool,
    pub self_closing: bool,
}

/// Open element: the start-tag token and how many directly nested same-name elements it stands for.
struct Open {
    tag: u32,
    count: u32,
}

/// Renders `tokens` and checks that the output is exactly `ulen` bytes.
pub fn emit(
    tokens: &[u32],
    block: &Words,
    shared: &Words,
    flags: EmitFlags,
    ulen: usize,
) -> Result<Vec<u8>> {
    let out = render(tokens, block, shared, flags, ulen)?;
    ensure!(
        out.len() == ulen,
        "decoded {} bytes, index says {ulen}",
        out.len()
    );
    Ok(out)
}

/// Renders `tokens`, failing as soon as the output exceeds `limit` bytes.
fn render(
    tokens: &[u32],
    block: &Words,
    shared: &Words,
    flags: EmitFlags,
    limit: usize,
) -> Result<Vec<u8>> {
    let mut out: Vec<u8> = Vec::with_capacity(limit.min(1 << 24));
    let mut mode = Mode::Neutral;
    let mut cap_pending = flags.capitalize;
    let mut stack: Vec<Open> = Vec::new();
    let word = |t: u32| {
        if t & SHARED != 0 {
            shared.get(t & !SHARED)
        } else {
            block.get(t)
        }
    };

    for (i, &t) in tokens.iter().enumerate() {
        let (kind, text) = word(t);
        let close_start = |out: &mut Vec<u8>, mode: Mode| {
            if mode == Mode::InStartTag {
                out.push(b'>');
            }
        };
        match kind {
            Kind::Word => {
                match mode {
                    Mode::InStartTag => out.push(b'>'),
                    Mode::AfterWord => out.push(b' '),
                    Mode::Neutral => {}
                }
                let at = out.len();
                out.extend_from_slice(text);
                if cap_pending {
                    cap_pending = false;
                    let b = &mut out[at];
                    if b.is_ascii_alphabetic() {
                        *b ^= 0x20;
                    }
                }
                mode = Mode::AfterWord;
            }
            Kind::Raw | Kind::SentenceEnd => {
                close_start(&mut out, mode);
                out.extend_from_slice(text);
                mode = Mode::Neutral;
                if kind == Kind::SentenceEnd {
                    cap_pending = true;
                }
            }
            Kind::StartTag => {
                close_start(&mut out, mode);
                out.push(b'<');
                out.extend_from_slice(text);
                mode = Mode::InStartTag;
                match stack.last_mut() {
                    Some(top) if top.tag == t => top.count += 1,
                    _ => stack.push(Open { tag: t, count: 1 }),
                }
            }
            Kind::AttrName | Kind::BareValue | Kind::SingleQuoted | Kind::DoubleQuoted => {
                ensure!(
                    mode == Mode::InStartTag,
                    "token {i}: {kind:?} outside a start tag"
                );
                let (lead, quote): (&[u8], &[u8]) = match kind {
                    Kind::AttrName => (b" ", b""),
                    Kind::BareValue => (b"=", b""),
                    Kind::SingleQuoted => (b"='", b"'"),
                    _ => (b"=\"", b"\""),
                };
                out.extend_from_slice(lead);
                out.extend_from_slice(text);
                out.extend_from_slice(quote);
            }
            Kind::EndTagNamed => {
                close_start(&mut out, mode);
                out.extend_from_slice(b"</");
                out.extend_from_slice(text);
                out.push(b'>');
                mode = Mode::Neutral;
            }
            Kind::EndTag => {
                let n = u16::from_be_bytes([text[0], text[1]]) as usize;
                if stack.is_empty() || n >= stack.len() {
                    bail!(
                        "token {i}: end tag {n} levels below top of a {}-entry stack",
                        stack.len()
                    );
                }
                if mode == Mode::InStartTag && !flags.self_closing && n == 0 {
                    out.extend_from_slice(b"/>");
                    pop_one(&mut stack);
                } else {
                    close_start(&mut out, mode);
                    let target = stack.len() - 1 - n;
                    stack.truncate(target + 1);
                    let (_, name) = word(stack[target].tag);
                    out.extend_from_slice(b"</");
                    out.extend_from_slice(name);
                    out.push(b'>');
                    pop_one(&mut stack);
                }
                mode = Mode::Neutral;
            }
            Kind::EmptyClose => {
                ensure!(
                    flags.self_closing && !stack.is_empty() && mode == Mode::InStartTag,
                    "token {i}: misplaced empty-element close"
                );
                out.extend_from_slice(b"/>");
                pop_one(&mut stack);
                mode = Mode::Neutral;
            }
        }
        if out.len() > limit {
            bail!("token {i}: output exceeds indexed length {limit}");
        }
    }
    Ok(out)
}

/// Closes one element of the top entry, dropping the entry when it reaches zero.
fn pop_one(stack: &mut Vec<Open>) {
    if let Some(top) = stack.last_mut() {
        top.count -= 1;
        if top.count == 0 {
            stack.pop();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::dict::Word;
    use super::*;

    fn words(list: &[(Kind, &[u8])]) -> Words {
        let mut w = Words::default();
        for (k, t) in list {
            w.words.push(Word {
                start: w.text.len() as u32,
                len: t.len() as u32,
                kind: *k,
            });
            w.text.extend_from_slice(t);
        }
        w
    }

    fn run(tokens: &[u32], block: &Words, shared: &Words, s: bool) -> Result<String> {
        let flags = EmitFlags {
            capitalize: true,
            self_closing: s,
        };
        Ok(String::from_utf8(render(tokens, block, shared, flags, usize::MAX)?).unwrap())
    }

    #[test]
    fn spec_example_prolog_and_tags() {
        use Kind::*;
        let block = words(&[
            (Word, b"Xml"),         // 0
            (SentenceEnd, b"<?"),   // 1
            (Word, b"version"),     // 2
            (Raw, b"=\""),          // 3
            (Word, b"1.0"),         // 4
            (SentenceEnd, b"\"?>"), // 5
            (StartTag, b"data"),    // 6
            (AttrName, b"refid"),   // 7
            (DoubleQuoted, b"21"),  // 8
            (StartTag, b"title"),   // 9
            (Word, b"petrified"),   // 10
            (Word, b"Forest"),      // 11
            (EndTag, &[0, 0]),      // 12
            (Raw, b"\r\n"),         // 13
            (EndTagNamed, b"x"),    // 14
        ]);
        let shared = words(&[(EndTag, &[0, 1]), (EmptyClose, b"")]);
        let s = run(
            &[1, 0, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 12],
            &block,
            &shared,
            true,
        )
        .unwrap();
        assert_eq!(
            s,
            "<?xml version=\"1.0\"?><data refid=\"21\"><title>Petrified Forest</title>\r\n</data>"
        );

        // Nested same-name elements share a stack entry; kind 9 with n = 1 closes the outer one.
        let s = run(&[6, 6, 9, SHARED, 12], &block, &shared, true).unwrap();
        assert_eq!(s, "<data><data><title></data></data>");

        // Kind 10 (S = 1) vs kind 9 when S = 0.
        let s = run(&[6, SHARED | 1], &block, &shared, true).unwrap();
        assert_eq!(s, "<data/>");
        assert!(run(&[6, SHARED | 1], &block, &shared, false).is_err());
        let s = run(&[6, 12], &block, &shared, false).unwrap();
        assert_eq!(s, "<data/>");
        let s = run(&[6, 12], &block, &shared, true).unwrap();
        assert_eq!(s, "<data></data>");

        // Errors: attribute outside a start tag, end tag on an empty stack, overlong output.
        assert!(run(&[7], &block, &shared, true).is_err());
        assert!(run(&[12], &block, &shared, true).is_err());
        let flags = EmitFlags {
            capitalize: false,
            self_closing: true,
        };
        assert!(emit(&[10, 10], &block, &shared, flags, 10).is_err());
        assert!(emit(&[10], &block, &shared, flags, 9).is_ok());
        assert!(emit(&[10], &block, &shared, flags, 10).is_err());
    }
}
