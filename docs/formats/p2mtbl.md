# Place-to-media table: `P2MTBL.DAT`

Status: **fully decoded structurally** (every byte accounted for). The meaning of the link-kind codes is inferred from the
linked items' titles; four kinds are only partly identified. Source: observation of the file and cross-references to
`catalog2/{content,data}.ecn`, the PFX titles, `MSWORLD.SDB` and `MSWORLD.ITR $WW$CTCATALOG`.
Reader: `encarta_formats::p2m`. Dump: `smallfmt p2m <P2MTBL.DAT>`.

Location: `EE/ENCARTA/P2MTBL.DAT` (227,124 B). `EE/KIDS/P2MTBL.DAT` is byte-identical. Read by the atlas component
(`AREF/COMPNTS/ATCONT.DLL`, `CP2MObject`). All integers are little-endian u32.

**What it is:** for each atlas place (a country, city, region, physical feature …) the list of Encarta items attached to
it: its article(s), the flag and anthem, "Facts and Figures", music clips and "Media Maps" photos. Despite the old
"place-to-map" reading of the name, the keys are places and the values are content ids ("place to media").

## Layout

```
header   u32 entryCount = 8  (number of directory entries below, the end sentinel included)
         8 × { u32 firstPlaceId, u32 blockOffset }
                            entry 0 = { 1, 0x44 }; entries 1–6 give the first id and offset of blocks 2–7;
                            entry 7 = { last id + 1, 0 } is the end sentinel (its offset is 0)
blocks   7 × block, contiguous from 0x44 to end of file
block    u32 blockLength     (bytes, including these 8 header bytes)
         u32 placeCount
         placeCount × place
place    u32 placeId
         u32 linkCount
         linkCount × { u32 kind, u32 contentId }
```

- The block directory is a one-level B-tree index. The header words are explained by the original reader (`ATCONT.DLL`,
  `CP2MObject`, 2026-10-15): it reads the first u32, `entryCount * 8 + 4` header bytes in all, then looks a place up by scanning
  the entries for the first one whose key is greater than the id and taking the block before it. So `8` is the entry count
  and the old "b = 1" is the first key (the lowest place id, 1) of entry 0. Seen as pairs (offset, limit) as in the earlier
  reading, the limit of a block is the **first place id of the next block** (exclusive upper bound); for the last block it is
  the last id + 1. Inside a block the reader walks the places one by one (a place with no links would still take 16 bytes). Blocks are ≤ 0x8000 bytes (32 KB pages that are not padded).
- Block offsets: 0x44, 0x802C, 0x10024, 0x18024, 0x2001C, 0x28014, 0x3000C. Each block ends exactly where the next
  begins, and the last ends at the file size.
- Place ids ascend strictly across the whole file. Totals: **12,253 places, 16,122 links**, no duplicate links.
  Links within a place are not sorted by kind.
- The number of blocks is the entry count minus one (the sentinel); also derivable from the first block offset.

## Place ids

The atlas's own entity ids (one id space shared with the rest of the atlas):
- All **193 `MSWORLD.SDB` country ids** occur (3 = Afghanistan, 2 = Antigua and Barbuda, 12 = Australia); these are also
  the `dfg/<id>.dfg` flag names and the `L` field of the catalog records (`msworld.md`).
- The entity column of `D*PLACES.TXT` (`msworld.md`) uses the same ids (cities such as 3359 = Beijing).
- Range 1 … 161,482,423; 10,590 places have a single link.

## Link kinds

| kind | count | content ids | meaning (inferred) |
|---|---|---|---|
| 8 | 9,436 | 761/762 (8,087), 631 (1,260), 174, 461, 701 | **article** about the place (title = place name) |
| 1 | 190 | 461 | "*Country*: Flag and Anthem" media item (country places) |
| 14 | 189 | 461 (usually kind-1 id + 1: 142 of 189) | **flag** image (`group="media"`, "Afghanistan Flag") — label `flag` |
| 15 | 193 | 461 (usually kind-1 id + 2: 145 of 189) | **national anthem** audio (`group="audio"`, "National Anthem of Algeria") — label `anthem` |
| 2 | 193 | 631 | "*Country* Facts and Figures" |
| 12 | 329 | 631 | music clip ("Burundi: The Mbira", "France: The Quadrille") |
| 5 | 98 | 631 | Media Maps photo, general landmark ("Aral Sea", "Fabled White Cliffs of Dover") |
| 37 = 5 + 1·32 | 505 | 631 | Media Maps photo, economy ("Fishing in Bahrain", "Making Rum") |
| 69 = 5 + 2·32 | 774 | 631 | Media Maps photo, landscape and climate ("Mountainous Armenia") |
| 101 = 5 + 3·32 | 716 | 631 | Media Maps photo, people and culture ("Children of American Samoa") |
| 133 = 5 + 4·32 | 1,237 | 631 | Media Maps photo, architecture and settlements ("Sod House", "Hilltop Castle in Cyprus") |
| 165 = 5 + 5·32 | 325 | 631 | Media Maps photo, animals ("Red Knot", "Lesser Indian Mongoose") |
| 13 | 171 | 631, none titled in PFX | **Map Trek stops**: all are `group="maptrektour"` DATA records ("Extreme Heat", "Ice Sheets: July Temperature") — label `maptrek-stop` |
| 16 | 1,766 | 631; 1,135 of them have bodies in `content.ecn` | **"about this map" notes**: `group="aboutmap"` records (1,135; landmark notes such as "Temple of Heaven", "Buckingham Palace"; the rest are not in DataStd/DataDlx) — label `about-map` |

The photo kinds look like `5 | (theme << 5)` with theme 0–5. The theme labels above are ours, read off the titles. The
2009 atlas shows none of these photos or sounds (VM 2026-10-15: no media style, Map Customizer option or pushpin layer,
and a place's click-menu submenu is only Map / Read Article), so there are no on-screen labels to match. The photo ids are the keys of `MSWORLD.ITR $WW$CTCATALOG` (3,819 of its keys
appear here), which supplies their credit lines and file names.

## Validation (whole file)
- Every block parses to exactly its length; `keyLimit` matches; ids ascend.
- Kind 8: 7,445 of 9,436 ids are in `content.ecn` (the rest are 631/174 items that live in `data.ecn` or not at all).
- Kinds 13–16 were identified from the DATA records (`docs/vm-findings.md`). The photo themes (5 + 32·n) have no
  on-screen labels in 2009 (not shown by the atlas). The header words `8` and `1` are the directory entry count and the first key (above).

## Modern target
SQLite `place_link(place_id, kind, content_id)` plus a kind-label table.
