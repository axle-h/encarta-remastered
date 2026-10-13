//! `P2MTBL.DAT`: atlas place id → linked content ids. Spec: `docs/formats/p2mtbl.md`.

use anyhow::{Context, Result, ensure};
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Link {
    pub kind: u32,
    pub content_id: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Place {
    pub place_id: u32,
    pub links: Vec<Link>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Block {
    pub offset: u32,
    /// First place id of the next block (exclusive upper bound of this block).
    pub key_limit: u32,
    pub length: u32,
    pub place_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct P2mTable {
    /// Header words 0 and 1 (observed 8 and 1; meaning unknown).
    pub header: [u32; 2],
    pub blocks: Vec<Block>,
    pub places: Vec<Place>,
}

impl P2mTable {
    /// Binary search for a place's links.
    pub fn links(&self, place_id: u32) -> Option<&[Link]> {
        self.places
            .binary_search_by_key(&place_id, |p| p.place_id)
            .ok()
            .map(|i| self.places[i].links.as_slice())
    }
}

/// Label for a link kind, as inferred from the linked items (see the spec).
pub fn kind_label(kind: u32) -> &'static str {
    match kind {
        8 => "article",
        1 => "flag-and-anthem",
        14 => "flag-media",
        15 => "flag-companion-media",
        2 => "facts-and-figures",
        12 => "music",
        5 => "photo:general",
        37 => "photo:economy",
        69 => "photo:landscape",
        101 => "photo:people",
        133 => "photo:architecture",
        165 => "photo:animals",
        13 => "unknown-13",
        16 => "unknown-16",
        _ => "unknown",
    }
}

fn u32_at(b: &[u8], off: usize) -> Result<u32> {
    let s = b.get(off..off + 4).with_context(|| format!("u32 at {off:#x} out of range"))?;
    Ok(u32::from_le_bytes(s.try_into().unwrap()))
}

pub fn parse(b: &[u8]) -> Result<P2mTable> {
    let header = [u32_at(b, 0)?, u32_at(b, 4)?];
    let first = u32_at(b, 8)? as usize;
    ensure!(first >= 12 && (first - 12).is_multiple_of(8), "first block offset {first:#x} does not fit a directory");
    let count = (first - 12) / 8;
    ensure!(u32_at(b, 8 + count * 8)? == 0, "block directory not terminated by 0");

    let mut blocks = Vec::with_capacity(count);
    let mut places: Vec<Place> = Vec::new();
    let mut expected_offset = first;
    for i in 0..count {
        let offset = u32_at(b, 8 + i * 8)?;
        let key_limit = u32_at(b, 12 + i * 8)?;
        ensure!(offset as usize == expected_offset, "block {i} at {offset:#x}, expected {expected_offset:#x}");
        let length = u32_at(b, offset as usize)?;
        let place_count = u32_at(b, offset as usize + 4)?;
        let end = offset as usize + length as usize;
        ensure!(end <= b.len(), "block {i} runs past end of file");
        let mut pos = offset as usize + 8;
        let block_start = places.len();
        for _ in 0..place_count {
            let place_id = u32_at(b, pos)?;
            let n = u32_at(b, pos + 4)? as usize;
            pos += 8;
            ensure!(pos + n * 8 <= end, "place {place_id} runs past its block");
            let links = (0..n)
                .map(|j| Ok(Link { kind: u32_at(b, pos + j * 8)?, content_id: u32_at(b, pos + j * 8 + 4)? }))
                .collect::<Result<Vec<_>>>()?;
            pos += n * 8;
            if let Some(last) = places.last() {
                ensure!(place_id > last.place_id, "place ids not ascending at {place_id}");
            }
            ensure!(place_id < key_limit, "place {place_id} not below block key limit {key_limit}");
            places.push(Place { place_id, links });
        }
        ensure!(pos == end, "block {i} has {} unparsed bytes", end as isize - pos as isize);
        if let (Some(first_here), Some(prev)) = (places.get(block_start), blocks.last()) {
            let prev: &Block = prev;
            ensure!(prev.key_limit == first_here.place_id, "block {i} does not start at the previous key limit");
        }
        blocks.push(Block { offset, key_limit, length, place_count });
        expected_offset = end;
    }
    ensure!(expected_offset == b.len(), "{} bytes after the last block", b.len() - expected_offset);
    Ok(P2mTable { header, blocks, places })
}

#[cfg(test)]
mod tests {
    use super::*;

    type TestPlace = (u32, Vec<(u32, u32)>);

    fn build(blocks: &[Vec<TestPlace>], limits: &[u32]) -> Vec<u8> {
        let first = 12 + 8 * blocks.len();
        let bodies: Vec<Vec<u8>> = blocks
            .iter()
            .map(|places| {
                let mut v = Vec::new();
                for (id, links) in places {
                    v.extend(id.to_le_bytes());
                    v.extend((links.len() as u32).to_le_bytes());
                    for (k, c) in links {
                        v.extend(k.to_le_bytes());
                        v.extend(c.to_le_bytes());
                    }
                }
                let mut b = ((v.len() + 8) as u32).to_le_bytes().to_vec();
                b.extend((places.len() as u32).to_le_bytes());
                b.extend(v);
                b
            })
            .collect();
        let mut out = 8u32.to_le_bytes().to_vec();
        out.extend(1u32.to_le_bytes());
        let mut off = first;
        for (body, lim) in bodies.iter().zip(limits) {
            out.extend((off as u32).to_le_bytes());
            out.extend(lim.to_le_bytes());
            off += body.len();
        }
        out.extend(0u32.to_le_bytes());
        for body in bodies {
            out.extend(body);
        }
        out
    }

    #[test]
    fn parses_blocks() {
        let data = build(
            &[vec![(1, vec![(8, 761_000_001)]), (2, vec![(1, 461_000_001), (2, 631_000_001)])], vec![(9, vec![(165, 631_000_002)])]],
            &[9, 10],
        );
        let t = parse(&data).unwrap();
        assert_eq!(t.header, [8, 1]);
        assert_eq!(t.blocks.len(), 2);
        assert_eq!(t.places.len(), 3);
        assert_eq!(t.links(2).unwrap()[1], Link { kind: 2, content_id: 631_000_001 });
        assert_eq!(kind_label(t.links(9).unwrap()[0].kind), "photo:animals");
        assert!(t.links(5).is_none());
    }

    #[test]
    fn rejects_wrong_limits() {
        let data = build(&[vec![(1, vec![])], vec![(9, vec![])]], &[5, 10]);
        assert!(parse(&data).is_err());
    }
}
