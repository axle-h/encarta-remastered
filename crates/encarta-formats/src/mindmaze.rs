//! MindMaze question bank: `MINDMAZE.DB` (questions) and `MINDMAZE.IDX` (category groups).
//! Spec: `docs/formats/mindmaze.md`.

use anyhow::{Context, Result, ensure};
use serde::Serialize;

/// Number of groups in the IDX file: 9 categories × 4 levels.
pub const GROUP_COUNT: usize = 36;
pub const LEVELS: usize = 4;

/// Category labels inferred from the questions (not stored on disc), indexed by `group / 4`.
pub const CATEGORY_LABELS: [&str; 9] = [
    "Physical science and technology",
    "Life science",
    "Geography",
    "History",
    "Social science",
    "Religion, mythology and philosophy",
    "Art, language and literature",
    "Performing arts",
    "Sports, games and hobbies",
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Answer {
    /// Raw text: SGML entities (`&eacute;`) and `<it>`/`<scp>` markup are kept verbatim.
    pub text: String,
    /// Content id of the article about this answer (761…).
    pub article_id: u32,
    /// Answer-location word; identical for all four answers (probably a paragraph number).
    pub location: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Question {
    /// 1-based record number in the DB.
    pub id: u32,
    /// Byte offset of the record in the DB.
    pub offset: u32,
    pub clue: String,
    /// `answers[0]` is the correct answer.
    pub answers: [Answer; 4],
    /// Group from the IDX (category × 4 + level − 1), when an index was supplied.
    pub group: Option<u8>,
}

impl Question {
    pub fn category(&self) -> Option<usize> {
        self.group.map(|g| g as usize / LEVELS)
    }
    /// Difficulty level 1 (easy) … 4 (hard).
    pub fn level(&self) -> Option<usize> {
        self.group.map(|g| g as usize % LEVELS + 1)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct IndexEntry {
    pub id: u32,
    pub offset: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Index {
    pub group_sizes: Vec<u32>,
    /// Entries per group, in file order.
    pub groups: Vec<Vec<IndexEntry>>,
}

/// High half of IBM code page 437 (0x80–0xFF).
const CP437_HIGH: [char; 128] = [
    'Ç', 'ü', 'é', 'â', 'ä', 'à', 'å', 'ç', 'ê', 'ë', 'è', 'ï', 'î', 'ì', 'Ä', 'Å', //
    'É', 'æ', 'Æ', 'ô', 'ö', 'ò', 'û', 'ù', 'ÿ', 'Ö', 'Ü', '¢', '£', '¥', '₧', 'ƒ', //
    'á', 'í', 'ó', 'ú', 'ñ', 'Ñ', 'ª', 'º', '¿', '⌐', '¬', '½', '¼', '¡', '«', '»', //
    '░', '▒', '▓', '│', '┤', '╡', '╢', '╖', '╕', '╣', '║', '╗', '╝', '╜', '╛', '┐', //
    '└', '┴', '┬', '├', '─', '┼', '╞', '╟', '╚', '╔', '╩', '╦', '╠', '═', '╬', '╧', //
    '╨', '╤', '╥', '╙', '╘', '╒', '╓', '╫', '╪', '┘', '┌', '█', '▄', '▌', '▐', '▀', //
    'α', 'ß', 'Γ', 'π', 'Σ', 'σ', 'µ', 'τ', 'Φ', 'Θ', 'Ω', 'δ', '∞', 'φ', 'ε', '∩', //
    '≡', '±', '≥', '≤', '⌠', '⌡', '÷', '≈', '°', '∙', '·', '√', 'ⁿ', '²', '■', '\u{a0}',
];

/// Decodes code page 437 text (ASCII plus the table above).
pub fn decode_cp437(bytes: &[u8]) -> String {
    bytes.iter().map(|&b| if b < 0x80 { b as char } else { CP437_HIGH[(b - 0x80) as usize] }).collect()
}

struct Reader<'a> {
    b: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        let s = self.b.get(self.pos..self.pos + n).with_context(|| format!("truncated at {:#x}", self.pos))?;
        self.pos += n;
        Ok(s)
    }
    fn u8(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }
    fn u16(&mut self) -> Result<u16> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into().unwrap()))
    }
    fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
}

/// Parses every record of `MINDMAZE.DB`. `group` is left empty.
pub fn parse_db(db: &[u8]) -> Result<Vec<Question>> {
    let mut r = Reader { b: db, pos: 0 };
    let mut out = Vec::new();
    while r.pos < db.len() {
        let offset = r.pos as u32;
        let id = out.len() as u32 + 1;
        let reserved = r.u32()?;
        ensure!(reserved == 0, "question {id}: reserved word {reserved}");
        let clue_len = r.u32()? as usize;
        let clue = decode_cp437(r.take(clue_len)?);
        let mut answers = Vec::with_capacity(4);
        for _ in 0..4 {
            let n = r.u8()? as usize;
            let text = decode_cp437(r.take(n)?);
            let article_id = r.u32()?;
            let location = r.u16()?;
            answers.push(Answer { text, article_id, location });
        }
        let answers: [Answer; 4] = answers.try_into().unwrap();
        out.push(Question { id, offset, clue, answers, group: None });
    }
    Ok(out)
}

/// Parses `MINDMAZE.IDX`.
pub fn parse_idx(idx: &[u8]) -> Result<Index> {
    ensure!(idx.len().is_multiple_of(4) && idx.len() >= GROUP_COUNT * 4, "IDX size {} is not 36 + 2n words", idx.len());
    let mut r = Reader { b: idx, pos: 0 };
    let group_sizes: Vec<u32> = (0..GROUP_COUNT).map(|_| r.u32()).collect::<Result<_>>()?;
    let total: u64 = group_sizes.iter().map(|&n| n as u64).sum();
    ensure!(
        (idx.len() - GROUP_COUNT * 4) as u64 == total * 8,
        "IDX group sizes sum to {total}, but {} pairs follow",
        (idx.len() - GROUP_COUNT * 4) / 8
    );
    let mut groups = Vec::with_capacity(GROUP_COUNT);
    for &n in &group_sizes {
        let g = (0..n).map(|_| Ok(IndexEntry { id: r.u32()?, offset: r.u32()? })).collect::<Result<Vec<_>>>()?;
        groups.push(g);
    }
    Ok(Index { group_sizes, groups })
}

/// Parses both files and assigns each question its group, checking that the index covers
/// every question exactly once with matching offsets.
pub fn parse(db: &[u8], idx: &[u8]) -> Result<Vec<Question>> {
    let mut questions = parse_db(db)?;
    let index = parse_idx(idx)?;
    for (g, entries) in index.groups.iter().enumerate() {
        for e in entries {
            let q = questions
                .get_mut((e.id as usize).wrapping_sub(1))
                .with_context(|| format!("IDX names unknown question {}", e.id))?;
            ensure!(q.offset == e.offset, "question {}: IDX offset {} != DB offset {}", e.id, e.offset, q.offset);
            ensure!(q.group.is_none(), "question {} is in two groups", e.id);
            q.group = Some(g as u8);
        }
    }
    if let Some(q) = questions.iter().find(|q| q.group.is_none()) {
        anyhow::bail!("question {} is in no group", q.id);
    }
    Ok(questions)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(clue: &[u8], answers: &[(&[u8], u32, u16)]) -> Vec<u8> {
        let mut v = 0u32.to_le_bytes().to_vec();
        v.extend((clue.len() as u32).to_le_bytes());
        v.extend(clue);
        for (t, id, loc) in answers {
            v.push(t.len() as u8);
            v.extend(*t);
            v.extend(id.to_le_bytes());
            v.extend(loc.to_le_bytes());
        }
        v
    }

    #[test]
    fn parses_db_and_index() {
        let a: &[(&[u8], u32, u16)] = &[(b"Right", 7, 3), (b"Wrong", 8, 3), (b"No", 9, 3), (b"Nope", 10, 3)];
        let r1 = record(b"First clue.", a);
        let r2 = record(b"El Ni\xa4o?", a);
        let mut db = r1.clone();
        db.extend(&r2);

        let mut idx = Vec::new();
        let mut sizes = [0u32; GROUP_COUNT];
        sizes[0] = 1;
        sizes[35] = 1;
        for s in sizes {
            idx.extend(s.to_le_bytes());
        }
        for (id, off) in [(2u32, r1.len() as u32), (1, 0)] {
            idx.extend(id.to_le_bytes());
            idx.extend(off.to_le_bytes());
        }

        let qs = parse(&db, &idx).unwrap();
        assert_eq!(qs.len(), 2);
        assert_eq!(qs[0].clue, "First clue.");
        assert_eq!(qs[0].answers[0].text, "Right");
        assert_eq!(qs[0].answers[3].article_id, 10);
        assert_eq!(qs[0].answers[1].location, 3);
        assert_eq!(qs[1].clue, "El Niño?");
        assert_eq!(qs[1].group, Some(0));
        assert_eq!(qs[0].group, Some(35));
        assert_eq!(qs[0].category(), Some(8));
        assert_eq!(qs[0].level(), Some(4));
    }

    #[test]
    fn rejects_bad_offsets_and_truncation() {
        let a: &[(&[u8], u32, u16)] = &[(b"A", 1, 1), (b"B", 2, 1), (b"C", 3, 1), (b"D", 4, 1)];
        let db = record(b"Q", a);
        assert!(parse_db(&db[..db.len() - 1]).is_err());
        let mut idx = Vec::new();
        for g in 0..GROUP_COUNT {
            idx.extend((if g == 0 { 1u32 } else { 0 }).to_le_bytes());
        }
        idx.extend(1u32.to_le_bytes());
        idx.extend(5u32.to_le_bytes());
        assert!(parse(&db, &idx).is_err());
    }

    #[test]
    fn cp437_high_bytes() {
        assert_eq!(decode_cp437(b"\x91 \x9c \xa4"), "æ £ ñ");
    }
}
