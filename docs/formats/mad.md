# MAD atlas database (`*.MAD`, read by MOBB200.DLL)

Status: **codec, index, rasters, geometry and the gazetteer tables are specified, implemented and validated.**
The clean-room Rust reader (`crates/encarta-formats/src/mad/`, CLI `mad`) was written from this document
and reproduces every count in [§0.2](#02-validation-summary) and every self-check value (§4.5, §5.8, §7.8,
§8.6); see [§11](#11-implementation). Some tables are still only partly understood (see §9). Points where
the implementation found this document wrong or ambiguous are marked **Correction (implementation)**.

The container layer (file header, TOC, the HD/CD split, the section list) is described in
[atlas-and-3d.md §1.1](atlas-and-3d.md#11-top-level-header-and-toc-solved-all-four-files-parse-offsetlen-chain-is-exact).
This document starts from "I have the bytes of one named section".

Contents: [0 Overview](#0-overview) · [1 Conventions](#1-conventions) · [2 INDX index](#2-indx-index) ·
[3 Huffman decode tables](#3-huffman-decode-tables) · [4 Grid sections (field codec)](#4-grid-sections-the-field-codec) ·
[5 RMAP rasters](#5-rmap-raster-pyramid-terrain-section) · [6 Projection](#6-the-octahedral-projection) ·
[7 Grid table schemas](#7-grid-table-schemas) · [8 Plain tables](#8-plain-tables) ·
[9 Open points and next steps](#9-open-points-and-next-steps) · [10 Provenance](#10-provenance) ·
[11 Implementation](#11-implementation)

---

## 0. Overview

### 0.1 Section kinds

| Kind | Sections | Spec |
|---|---|---|
| `INDX` index (key → byte range) | `*Lookup`, `*Lku`, `Find Ndx*`, `FindGECContextLookup*`, and one per raster inside `Terrain` | §2 |
| **Grid**: a field-codec model followed by compressed records; always paired with an INDX section that locates the records | `GeoObjGrid*`, `GeoGemGrid`, `Find Grid*`, `FindGECContextGrid*`, `EntityConv`, `Bitmap`, `LegendGrid`, `PDCAltitudeGrid`, `MillerTerrainGrid` | §4, §7 |
| **RMAP** raster pyramid | `Terrain` (12 rasters in WLD, 2 in SWLD) | §5, §6 |
| Plain (uncompressed) tables | everything else | §8 |

Grid ↔ index pairs (the index may live in the other half of the database, see atlas-and-3d.md §1):

| Grid section | Index section | Record key |
|---|---|---|
| `GeoGemGrid` (CD) | `GeoGemLookup` (HD) | spatial cell key (§7.8) |
| `GeoObjGrid<lcid>` (CD) | `GeoObjLookup<lcid>` (HD) | spatial cell key (§7.7) |
| `Find Grid<lcid>` (CD) | `Find Ndx<lcid>` (CD) | page number 0… |
| `FindGECContextGrid<lcid>` (CD) | `FindGECContextLookup<lcid>` (CD) | same page numbers as `Find Grid` |
| `EntityConv` (HD) | `EntityConvLookup` (HD) | page number 0… |
| `Bitmap` (HD) | `BitmapLookup` (HD) | symbol id (0x10000…) |
| `LegendGrid` (HD) | `LegendLookup` (HD) | LCID (0x0009, 0x0409) |
| `PDCAltitudeGrid` (HD) | `PDCAltitudeLookup` (HD) | MapStyle id (0…0x13, 0x15…0x21) |
| `MillerTerrainGrid` (CD) | `MillerTerrainLku` (CD) | tile number 0… |

`<lcid>` is the 4-hex-digit language suffix (`0409` en-US, `0009` neutral English). The `LangId` section
lists the LCIDs present as u16 values; MOBB opens one `GeoObjLookup/GeoObjGrid` pair per listed LCID.

### 0.2 Validation summary

All numbers come from the private prototype run over the four MAD files.

| Item | Result |
|---|---|
| INDX sections | every index parses; for every raster and grid, the last record ends exactly at the end of its data area |
| RMAP tiles | **445,176 / 445,176** tiles decode (WLD_CD 357,792 tiles of which 188,702 are fill tiles; SWLD_CD 87,384 of which 68,526 fill). Every compressed tile consumes all of its bits except 0–32 trailing padding bits. All compressed tile lengths are multiples of 4. |
| Raster geometry | all 12 rasters reassemble into seamless globes (HYPSO, SATELLITE, NIGHT, MOON, TECTONIC, climate, POPULATION). GeoGem border vectors drawn on top of HYPSO/SATELLITE line up to within a pixel (Europe, Australia/New Zealand, South America checked by eye). |
| Grid records | LegendGrid 2/2, Bitmap 149/149, EntityConv 1,772/1,772, FindGECContextGrid0409 1,356/1,356 (+0009 1/1), MillerTerrainGrid 1,263/1,263, Find Grid0409 1,356/1,356 (+0009 1/1), GeoObjGrid0409 11,808/11,808, GeoObjGrid0009 11,808/11,808: each record decodes with every field exactly filled and 0–31 unused bits at the end |
| GeoGemGrid | **all 217,323 distinct WLD records** (2,754,023 keys) and all 20,491 SWLD records decode with exact bit consumption; 54,448,380 WLD points, every coordinate in 0…1023; all 2,947,064 WLD object references (and 166,539 SWLD) resolve into GeoObjGrid0409; coastlines/borders draw a correct world map and line up with the rasters |
| PDCAltitudeGrid | 33/33 records (WLD and SWLD) |
| Gazetteer plain tables | Find Data (3 instances), Find Key (3), EntityConvKey (2) parse to exactly their length; G1G2 decodes Find strings to readable names; EntityCenter centroids land on the right countries |
| Gazetteer joins | see §7.10 (all 1,813,920 entity ids consistent across GeoObj/EntityConv; Find and EntityCenter ids are subsets) |

---

## 1. Conventions

- All fixed-layout integers are **little-endian**. `u8/u16/u32` unsigned, `i32` signed, `f32` IEEE single.
- **Bit streams** (grid records, raster tiles) are read as a sequence of **u32 little-endian words**, each
  consumed **most-significant bit first**. `read(n)` (0 ≤ n ≤ 32) returns the next `n` bits as an unsigned
  integer whose first bit is its most significant bit; `read(0)` = 0. Fields straddle word boundaries freely.
  `peek32()` returns the next 32 bits without consuming them (bits beyond the end of the data read as 0).
- **Trailing partial word (grid records only).** A grid record of `n` bytes with `r = n mod 4 ≠ 0` has its
  last `r` bytes placed in the **high-order** end of the final word: with final bytes `b0 … b(r−1)`, the last
  word is the LE u32 of the 4 bytes `[0]*(4−r) ++ [b0 … b(r−1)]`. Example: LegendGrid record 0x9 is 111
  bytes; its last 3 bytes `24 00 03` form the word `0x03002400`. (Raster tiles are always a multiple of 4
  bytes, so the question does not arise there.)
- **Correction (implementation): counting unused bits.** "Bits left unread" (§4.4, §0.2, §7.8) is measured
  against the record's data bits, `8 · n`, not against whole words: the zero bits that pad a partial final word
  come *after* the data bits in reading order. The GeoGem self-check record (65,237 bytes, §7.8) has 4 unused
  bits by this count, 28 by a whole-word count. With this definition every grid record on the disc ends with
  0–31 unused bits.
- A *phrase*, *table index*, *context* etc. are defined where they are used.

---

## 2. INDX index

### 2.1 Layout

```
+0      "INDX"
+4      u32  npages
+8      u32  last_key[npages]       ascending; last_key[p] = largest key covered by page p
+8+4n   npages × 2048-byte pages
```
Page `p` covers keys `first(p) … last_key[p]`, where `first(0) = 0` and `first(p) = last_key[p−1] + 1`.
(Section sizes check: `8 + 4n + 2048n`.)

### 2.2 Page encoding

A page starts with `u32 base` (the byte offset, inside the paired data area, of the first record in this
page), followed by a byte-oriented op stream. Decode with state `key = first(p)`, `off = base`, `len = 0`,
and repeat until `key > last_key[p]` (bytes after that point are padding):

| First byte `c` | Bytes | Meaning |
|---|---|---|
| `0x00–0x7F` | 2 | **entry**: `off += len; len = (c << 8) \| b1` (15-bit length, big-endian); key `key` → `(off, len)`; `key += 1` |
| `0xFC–0xFF` | 3 | entry with 18-bit length: `len = ((c & 3) << 16) \| b1 << 8 \| b2` (after `off += len`) |
| `0xF0–0xF7` | 5 | entry with 31-bit length: `len = (b1 & 0x7F) << 24 \| b2 << 16 \| b3 << 8 \| b4` (after `off += len`) |
| `0x80–0x9F` | 1 | **skip** `c & 0x1F` keys (they have no record) |
| `0xA0–0xAF` | 2 | skip `(c & 0x0F) << 8 \| b1` keys |
| `0xB0–0xBF` | 4 | skip `(c & 0x0F) << 24 \| b1 << 16 \| b2 << 8 \| b3` keys |
| `0xC0–0xDF` | 1 | **repeat**: the next `c & 0x1F` keys map to the same `(off, len)` as the previous entry |
| `0xE0–0xE7` | 2 | repeat `(c & 7) << 8 \| b1` keys |
| `0xE8–0xEF`, `0xF8–0xFB` | 3 | repeat `((c & 7) << 8 \| b1) << 8 \| b2` keys |

Notes:
- Test the classes in this order: `c < 0x80`; `c & 0xFC == 0xFC`; `c & 0xF8 == 0xF0`; then by `c & 0xE0`
  (0x80 skip, 0xA0 skip, 0xC0 repeat, 0xE0 repeat; within 0xA0 and 0xE0 classes bit 0x10 resp. 0x08 selects
  the longer form). `0xF8–0xFB` therefore fall into the 3-byte repeat form.
- `off` is relative to the start of the paired **data area**: for grids, the start of the Grid section
  (the record offsets are larger than the model at its start, §4.1); for rasters, the raster's tile data
  area (§5.3).
- A key whose length is 0 is treated as absent. Repeat ops are heavily used in `GeoGemLookup`
  (2,754,023 keys share 217,323 distinct records).
- Lookup: binary-search `last_key` for the first page with `last_key[p] ≥ key`, then walk that page.
  Simplest implementation: decode each page once into a sorted key→(off,len) table.

### 2.3 Worked example (`LegendLookup`, WLD_HD)

`49 4E 44 58 | 01 00 00 00 | 09 04 00 00 |` page: `8D 00 00 00 | 89 | 00 6F | A3 FF | 00 6F | …padding`
→ one page covering keys 0…0x409; base 0x8D; `89` skips keys 0–8; `00 6F` gives key 9 → (0x8D, 0x6F);
`A3 FF` skips 0x3FF keys (10…0x408); `00 6F` gives key 0x409 → (0x8D+0x6F = 0xFC, 0x6F). LegendGrid is
363 = 0xFC + 0x6F bytes long.

RMAP example (TECTONIC): one page, `last_key = 0x300000FF`, base 0, then `80` (skip 0), four 2-byte entries
(keys 0–3), `BF FF FF FC` (skip 0x0FFFFFFC keys → key 0x10000000), …

---

## 3. Huffman decode tables

Both codecs (grid fields §4, raster tiles §5.5) store their Huffman codes **already in decoding form**: a
list of `(limit, base, len)` triplets and a symbol array. No code-length construction is needed.

### 3.1 Table blob

A table is identified by an **anchor** byte position `A` inside a larger buffer:

```
A − 12·k … A − 1   further triplets (k ≥ 0), lowest address first
A + 0   u32 limit     \
A + 4   i32 base       } the anchor triplet
A + 8   u32 len       /
A + 12  u32 nsym      low 16 bits = number of symbols (grid tables: high 16 bits 0;
                      raster tables: high 16 bits are nonzero, meaning unknown, not needed)
A + 16  symbols[nsym] u16 (grid tables, raster long-run table) or u8 (raster value and run tables)
```
Triplets are stored contiguously, so the triplet "below" the one at address `q` is at `q − 12`. The
triplet at the **lowest** address always has `limit = 0xFFFFFFFF` (sentinel; validated on all 221 grid
tables). Walking down from the anchor, `limit` increases and `len` increases.

For **grid** models (§4.1) each table is a stand-alone blob: `u32 a` (byte offset from the blob start to
its anchor, so `A = blob + a`), then the triplets starting at `blob + 4` (so the sentinel triplet is first),
then `nsym`, the u16 symbols, and 2 trailing bytes of no meaning. Validated: `(a − 4) mod 12 = 0`, sentinel
at `blob + 4`, `blob_size = a + 16 + 2·nsym + 2` for all 221 tables.

### 3.2 Decoding one symbol

```
w = peek32()
q = A
while limit(q) < w:  q -= 12          # unsigned compare
idx = (w >> (32 − len(q))) + base(q)  # (len is never 0 in the shipped data)
consume len(q) bits
return symbols[idx]
```
This is ordinary canonical-Huffman decoding: `limit` is the largest left-justified 32-bit window that
still belongs to codes of that length, and `base` maps a `len`-bit code to its symbol index.
(MOBB peeks only the bits of the current word first and fetches the next word only when needed; that is
equivalent to a full 32-bit peek because every `limit` has all-ones below its code bits.)

Example, raster TECTONIC model 1, run table, context 0 (anchor 2320 in the model block): triplets from the
anchor down: `(0x7FFFFFFF, 0, 1)`, `(0xBFFFFFFF, −1, 2)`, `(0xDFFFFFFF, −4, 3)`, `(0xEFFFFFFF, −25, 5)` …
`(0xFFFFFFFF, −524158, 19)`; symbols start `1, 2, 3, 4, 5, …`. So the 1-bit code `0` is run code 1, `10` is
run code 2, `110` is run code 3 (idx = 6 − 4 = 2) and so on.

---

## 4. Grid sections: the field codec

A Grid section = **model** (starting at section offset 0) + records. Records are located through the
paired INDX section (§2); record offsets are relative to the section start (the first page's `base` equals
the model size, e.g. 0x8D = 141 for LegendGrid).

### 4.1 Model serialization

```
u16 ntables
ntables × { u32 size; u8 blob[size] }          Huffman table blobs (§3.1), referenced by index
u16 nfields
nfields × field descriptor:
    u8  nctx            number of contexts (≥ 1)
    u8  filter          post-filter mode: 0 none, 1 repeat-sentinel, 2 delta (−32000 bias)
    u16 sentinel        value used by filter 1
    u8  modulus         the decoder returns (context mod modulus), see §4.2 step 5
    u8  table[4·nctx]   for context c: table[4c+0] = LZ distance, +1 = LZ length,
                        +2 = run count, +3 = main symbol table (indices into the blob list)
    u16 nphrases
    nphrases × { u16 count; u16 value[count] }   phrase dictionary of this field
```
The fields are numbered 0…nfields−1; each table schema (§7) says which field numbers it reads in which
order. Model sizes: LegendGrid 141, Bitmap 220, EntityConv 17,322, GeoObjGrid0409 25,968, Find Grid0409
51,489, GeoGemGrid 37,655 bytes.

Shipped data: every field has `filter = 0`. Only GeoGemGrid field 6 has more than one context (4 contexts,
modulus 2). Phrase dictionaries exist in GeoObjGrid field 17 (236 phrases), Find Grid fields 5/7/8/9/12,
FindGECContextGrid0409 field 1 and GeoGemGrid fields 2, 3 and 6.

### 4.2 Decoding one field

`decode_field(f, n, kind)` fills exactly `n` values (`kind` = word or byte) from the record's bit stream:

1. **Context.** If `nctx(f) ≥ 2`, `ctx = read(ceil(log2(nctx)))`, else `ctx = 0`. Select the four
   tables `DIST, LEN, RUN, SYM` = `table[4·ctx + 0…3]`.
2. Define `get(T)` = decode one symbol from table `T` (§3.2), then apply the **escapes**: if the symbol is
   `0xFFFF` return `read(11)`; if `0xFFFE` return `read(16)`; otherwise return the symbol. Escapes apply to
   all four tables.
3. Repeat until `n` values have been produced (let `out` be the output so far, `i = len(out)`):

| `s = get(SYM)` | Action |
|---|---|
| `< 0xFEFB` | literal: append `s` (byte fields: append `s & 0xFF`) |
| `0xFEFB … 0xFFF9` | phrase `s − 0xFEFB` of this field: append all its values (byte fields: low byte of each) |
| `0xFFFA` | run: `k = get(RUN)`; append `out[i−2]` `k` times |
| `0xFFFB` | run: `k = get(RUN)`; append `out[i−1]` `k` times |
| `0xFFFC` | mirrored copy: `d = get(DIST)`, `k = get(LEN)`; for `j = 0…k−1`: append `out[i−1−d−j]` |
| `0xFFFD` | forward copy: `d = get(DIST)`, `k = get(LEN)`; for `j = 0…k−1`: append `out[i−2−d+j]` (may overlap the bytes being written, copy one value at a time) |

   Note that a 16-bit escape on `SYM` can itself produce a value ≥ 0xFEFB, which is then treated as the
   command of that value. If the output would exceed `n`, or ends at a count other than `n`, the record is
   corrupt ("Compressed word/byte-section corrupt?").
4. **Post-filter** (never used in the shipped files, specified for completeness), for `i = 1…n−1` in order:
   - filter 2 (word fields only): `out[i] = (out[i] + out[i−1] − 32000) mod 65536`;
   - filter 1: if `out[i] == sentinel` (byte fields: low byte of sentinel) then `out[i] = out[i−1]`.
5. **Return value**: `ctx mod modulus` (0 when `nctx < 2`). GeoGemGrid uses it (§7.8).

Byte and word fields share this procedure; the only differences are the truncation to the low byte and
that filter 2 applies to words only.

### 4.3 Array primitives used by the schemas

| Name | Meaning |
|---|---|
| `W(f, n)` | `decode_field(f, n, word)` → `n` u16 |
| `B(f, n)` | `decode_field(f, n, byte)` → `n` u8 |
| `D(f, n)` | four byte fields: `B(f,n)`, `B(f+1,n)`, `B(f+2,n)`, `B(f+3,n)` are bytes 3 (MSB), 2, 1, 0 of `n` u32 values; uses field numbers `f … f+3` |
| `W1(f)` | `W(f, 1)[0]` |

When `n = 0` nothing is read (no context bits either): every schema guards zero counts this way.

### 4.4 Decoding a record

Read the record bytes (offset/length from the index), set up the bit reader (§1, including the partial
final word rule), run the schema's field sequence (§7). After the last field, between 0 and 31 bits remain
unread in every record on the disc; a reader can use "remaining bits < 32" as an integrity check.

### 4.5 Worked example (LegendGrid record key 0x9, WLD_HD)

Model: `ntables = 2`; table 0 is the 26-byte blob `04 00 00 00 | FF FF FF FF 00 00 00 00 01 00 00 00 |
02 00 00 00 | FF FF FE FF | 01 00` — one triplet `(0xFFFFFFFF, 0, 1)` and symbols `[0xFFFF, 0xFFFE]`,
i.e. "1 bit: 0 → 11-bit escape, 1 → 16-bit escape" (table 1 is identical apart from its padding bytes).
`nfields = 7`, each with `nctx 1, filter 0, sentinel 0, modulus 1`, no phrases; fields 0–2 use table 0,
fields 3–6 table 1.

Record at 141, 111 bytes, first words `0x00300000 0x10000010`. Field 0: bit `0` → escape 11 → `00000000011`
= 3 (three legend entries). Field 1 (9 values): `0`+`00000000000` = 0, `0`+`00000000001` = 1, `0`+`0…0` = 0, …
The full record decodes to the entries `(0,1,0) "1,000,000+ people"`, `(1,1,2) "Populated Places"`,
`(2,1,3) "National Capital"` (§7.1) using 886 of its 888 bits.

---

## 5. RMAP raster pyramid (`Terrain` section)

### 5.1 RMAP header (section offset 0, 0x54 bytes)

| Off | Type | Value / meaning |
|---|---|---|
| 0x00 | char[4] | `RMAP` |
| 0x04 | u32 | `0x00020001` (MOBB requires this value) |
| 0x08 | u32 | 0x54, header length |
| 0x0C | u32 | 4 (unknown) |
| 0x10 | f32 | 0.00175 (unknown; ≈ 0.1° in radians) |
| 0x14 | u32, u32 | TESS chunk offset (0x54) and length (0x14854) |
| 0x1C | u32, u32 | RAST chunk offset (0x148A8) and length (0x9C8 WLD) |
| 0x24 | … | zero |

Offsets are relative to the start of the `Terrain` section.

### 5.2 TESS chunk (globe mesh, regenerable)

`"TESS"`, `u32 0x34` (offset of the vertex table), `u32 0x4020` (its length), `u32 0x4054` (triangle table
offset), `u32 0x10000` (its length), then four `(u32 offset, u32 length = 0x200)` pairs of unknown small
tables. All offsets relative to the TESS start.
- Vertex table: 1026 × `{f32 x, y, z, 1.0}` unit vectors: the two poles `(0,0,±1)` first, then the vertices
  of the projection grid of §6 at subdivision 16 (spacing 90°/16 along octant edges). **All 1026 vertices
  are reproduced by the §6 parametrisation** (to 4 decimals), which is the main evidence for its interior rule.
- Triangle table: 2048 × 32 bytes = `{u32 code, u32 v0, v1, v2, f32 cx, cy, cz, r}` (8 × 4⁴ triangles;
  `c` is the unit centre, `r` ≈ 0.07 probably an angular bound). The code layout is not decoded. A converter
  does not need TESS.

### 5.3 RAST chunk and raster entries

`"RAST"`, `u32 n`, then `n` entries of 0xD0 bytes. Offsets inside an entry are relative to the **RAST
chunk start**.

**Correction (implementation):** the RAST length in the RMAP header (0x9C8 in WLD) covers only the `RAST`
magic, the count and the entries. The LOOK blocks, indexes, tile data and model blocks that the entries
point to lie **after** it, still addressed from the RAST start (WLD HYPSO's LOOK block is at RAST + 2504,
i.e. immediately after the chunk). A reader must resolve entry offsets against the rest of the `Terrain`
section from the RAST start, not against the RAST chunk alone.

| Off | Type | Meaning |
|---|---|---|
| 0x00 | char[32] | raster name (`HYPSO`, `SATELLITE`, …), matched case-insensitively |
| 0x20 | u32 | `L`, the finest level (levels 0…L exist; HYPSO 8, SATELLITE 5, NIGHT 4, MOON/TECTONIC/POPULATION 3, RAIN*/TEMP* 2, LOCATOR 0) |
| 0x24 | u32 | largest compressed tile length (buffer size; e.g. TECTONIC 0x1C38 = 7224) |
| 0x28 | u32, u32 | LOOK block offset, length |
| 0x30 | u32 | number of `(scale, level)` pairs at 0x50 (HYPSO 1, others 0) |
| 0x34 | f32 | display scale value (HYPSO 150, SATELLITE 2000, NIGHT 4000, TECTONIC 8000, MOON 14000, LOCATOR 60000); semantics not confirmed |
| 0x38 | u32, u32 | INDX offset, length (§2) |
| 0x40 | u32, u32 | tile data area offset, length (INDX offsets are relative to its start) |
| 0x48 | u32, u32 | model block offset, length (§5.5) |
| 0x50 | 16 × {f32 scale, u32 level} | scale→level overrides (HYPSO: `(150.0, 8)`); MOBB uses them to cap the level for a given scale |

### 5.4 LOOK, RCT, CMT (colour styles)

`LOOK`: `"LOOK"`, `u32 nstyles`, then `nstyles × {char name[32]; u32 rct_off, rct_len; u32 cmt_off, cmt_len}`.
**`rct_off`/`cmt_off` are relative to the LOOK block start.** **Correction (implementation):** the LOOK length
in the RAST entry (HYPSO: 344 = 8 + 7 × 48) covers only this directory; the RCT and CMT tables follow it.

- `RCT`: `"RCT\0"`, `u32 ncolors`, `256 × {u8 b, g, r, x}`: the style's list of distinct colours (legend
  palette; HYPSO 107, TECTONIC 118, POLITICAL 3 …).
- `CMT`: `"CMT\0"`, `256 × {u8 b, g, r, x; u8 shade; u8 kind; u16 0}`: the **colour of each raster value**.
  This is the table to render with: `rgb(pixel) = (CMT[v].r, CMT[v].g, CMT[v].b)`. Colour bytes are in
  **B, G, R** order (confirmed visually: HYPSO oceans blue, land green/brown, ice white).
  - `shade` (low 4 bits used): a hill-shading level (HYPSO uses 2…12 with 7 = flat; raster values come in
    blocks of 11, one per shade step, e.g. 17, 28, 39 … are flat land classes).
  - `kind`: MOBB maps 0→1, 1→2, 2→4, 3→5, other→0 into a per-value attribute; observed HYPSO uses 0 (land)
    and 2 (water/ice), TECTONIC 4 everywhere. Meaning not confirmed (probably water/land masking).
    **Correction (implementation): the labels are the other way round.** In every HYPSO-based style, values
    1–66 have `kind 0` and are **water** (ocean value 17 at 0° N 140° W and 55° S 132° E); values 67 and up have
    `kind 2` and are **land, ice included** (Sahara 116, Amazon 94, Greenland 171). At level 4, 70.5 % of the
    image's pixels are kind 0, close to Earth's ocean fraction. `kind 4` is on values that HYPSO never uses and on
    every TECTONIC value; SATELLITE has 4 water values (174–177, kind 0) and one kind-4 value (178, 5 pixels).

MOBB checks the magics `RCT\0` (0x544352) and `CMT\0` (0x544D43) before accepting a style.

### 5.5 Model block

```
+0x00 u32 magic       0x1234FB00 (MOBB accepts any value whose (magic | 7) == 0x1234FB07)
+0x04 u32 length
+0x08 u32 dir         offset of the model directory (0x1C)
+0x0C u32 area        offset of the Huffman table area
+0x10 u32 width       127
+0x14 u32 height      127
+0x18 u32 npix        16129 = 127 × 127
dir:   u16 nmodels    (2; must be < 3), u16 model_off[nmodels]         offsets relative to dir
model: u16 nlists     (3; at most 16),  u16 list_off[nlists]           offsets relative to dir
list:  u16 nctx,      u16 entry[nctx]                                  anchor = area + 4·entry  (§3.1)
```
Each model has three context lists: list 0 = **run** tables (256 contexts), list 1 = **value** tables
(256 contexts), list 2 = **long-run** table (1 context). Run and value tables have u8 symbols; the
long-run table has u16 symbols.

### 5.6 Tile codec

Tile record = `(off, len)` from the raster's INDX, inside the tile data area.

- `len == 1`: **fill tile**, all 16129 pixels equal that byte (257,228 of 445,176 tiles).
- Otherwise read a bit stream (§1). `mode = read(8)` (= the 4th byte of the record, the MSB of the first word):
  - `mode 0`: raw: the 16129 pixels are the bytes starting at record byte 4 (never occurs on the disc);
  - `mode 1`: not supported by MOBB (never occurs);
  - `mode 2`: RLE with model 0, then **transpose** (§ below);
  - `mode 3`: RLE with model 1, no transpose.

**RLE (both modes).** State: `pv = 0` (previous value), `pr = 0` (previous run code), `col = 127`
(pixels left in the current row of the output scan). Repeat until 16129 pixels have been written:

```
v = decode(value_table[pv])            # u8 symbol
r = decode(run_table[pr])              # u8 symbol
if r == 1:
    emit v once; col -= 1; if col == 0: col = 127
else:
    k = r; if r == 255: k = 255 + decode(long_run_table[0])     # u16 symbol
    fill = v
    while k > 0:
        c = min(k, col); emit fill c times; k -= c; col -= c
        if col == 0: col = 127; fill = scan[pos − 127]          # pos = next write position
pv = v; pr = r
```
In words: a run that reaches the end of a 127-pixel row continues on the next row with the value that
starts the row just completed (the pixel "above" the continuation point). `r = 0` is a zero-length run.
Emitting more than 16129 pixels is an error.

**Mode 2 transpose.** The RLE output `S` (row-major 127 × 127) is transposed: `O[i·127 + j] = S[j·127 + i]`.
For mode 3, `O = S`. **`O` is the tile buffer** used in §5.7. (Handling the two modes this way is what makes
neighbouring mode-2 and mode-3 tiles, and parents and children, agree; validated across all rasters.)

### 5.7 Tile addressing

INDX key = `level << 28 | m`, where `m` interleaves the bits of two coordinates `X, Y ∈ [0, 2^(level+1))`:
for bit `b` from `level` down to 0, append `X_b` then `Y_b` (so `X` supplies the more significant bit of
each pair). Level `ℓ` has `4^(ℓ+1)` tiles (4, 16, 64 …; HYPSO level 8 has 262,144). Every key of every level
up to `L` is present.

The tiles of level `ℓ` form one square image of side `S = 127 · 2^(ℓ+1)` pixels, `h = S/2`:
tile `(X, Y)` covers **rows** `X·127 … X·127+126` and **columns** `Y·127 … Y·127+126`. Within a tile, with
`O[a·127 + b]` the tile buffer:

- tiles in the top-left (`X < 2^ℓ`, `Y < 2^ℓ`) and bottom-right (`X ≥ 2^ℓ`, `Y ≥ 2^ℓ`) quadrants are placed
  as is: `image[X·127 + a][Y·127 + b] = O[a·127 + b]`;
- tiles in the other two quadrants are rotated 90° counter-clockwise:
  `image[X·127 + 126 − b][Y·127 + a] = O[a·127 + b]`.

The resulting image is the octahedral projection of §6: north pole at the centre `(h, h)`, south pole at
the four corners, equator = the diamond through the edge midpoints, seen from outside the globe (not
mirrored). Pixels are cells: pixel `(row, col)` covers `[col, col+1) × [row, row+1)`, so its centre is at
`dx = col + 0.5 − h`, `dy = row + 0.5 − h` relative to the image centre (x right, y down).

### 5.8 Self-check values

SHA-256 of the 16129-byte tile buffer `O` (§5.6), WLD_CD:

| Raster | Key | Record (off, len) | Mode | `O[0..8]` | SHA-256 |
|---|---|---|---|---|---|
| TECTONIC | 0x00000000 | (0, 4380) | 2 | 13 18 13 13 13 18 7 17 | `f18f48d345416d040f1110d529de5878b4e93f491a4239b237f1a6fb109b3fb5` |
| TECTONIC | 0x10000005 | (43492, 4048) | 2 | 24 25 25 25 24 10 13 18 | `868a31aa11d279d8f4cd8c13e8fd38f92562ebb24de39b3b43eb6eee1c6c8e3b` |
| TECTONIC | 0x300000FF | (1346284, 240) | 2 | 13 × 8 | `fe1b52c12cc9931501a40320a4792c93d38bf85330d0ebb561fe6537c3bd620d` |
| HYPSO | 0x00000000 | (0, 3540) | 2 | 171 171 171 171 171 171 136 169 | `30408780e8077c7937faa31d63ea307420843f2a45825552429623c2bbe51bb7` |
| HYPSO | 0x20000021 | (256628, 6120) | 2 | 94 94 94 94 94 94 94 105 | `8173b0e35c70d10b1baf4e147f2ae160d174e2372064683879a7ece02b3a5d95` |
| HYPSO | 0x80000000 | (111233644, 1) | fill 0xAB | 171 × 8 | `8eff7ce9c14a9d40b650cf195e74024d3d29418f98cad853a9b933b03fd5cb34` |
| SATELLITE | 0x50000000 | (4343070, 544) | 2 | 169 × 8 | `5a628b09152e73e2ec37dc8b1e4da896951c12257677cb5849a8d2c53d493208` |
| MOON | 0x00000003 | (25900, 8560) | 3 | 153 153 153 117 112 47 100 94 | `52184dcfe8eab648df6d12666f361a069bbcc820087d55389437e4609653fcd0` |

TECTONIC key 0: the first record bytes are `69 B2 97 02` (mode byte 0x02). Its most frequent values are
30 (2040 px), 41 (1553), 24 (1449).

---

## 6. The octahedral projection

Shared by the raster image of §5.7. (The vector tables use a simpler per-octant latitude/longitude grid,
§7.8.)

### 6.1 Faces

Let the image centre be the origin, `dx` to the right, `dy` downwards, half-size `h`. The four **axis
vertices** (edge midpoints of the image) lie on the equator:

| Vertex | Position | Longitude |
|---|---|---|
| right | `(+h, 0)` | 90° E |
| top | `(0, −h)` | 180° |
| left | `(−h, 0)` | 90° W |
| bottom | `(0, +h)` | 0° (Greenwich) |

For a point `(dx, dy)`, its quadrant picks `A` = the right vertex if `dx ≥ 0` else the left one, and
`B` = the top vertex if `dy < 0` else the bottom one. Then
- **northern** triangle if `|dx| + |dy| ≤ h`: pole `P` = north pole,
  `r = (|dx| + |dy|) / h`, `s = |dy| / (|dx| + |dy|)` (s = 0 when at the pole);
- **southern** triangle otherwise: pole `P` = south pole (the image corner), with `ex = h − |dx|`,
  `ey = h − |dy|`: `r = (ex + ey) / h`, `s = ex / (ex + ey)`.

### 6.2 Point on the sphere

With `slerp(p, q, t)` the constant-speed great-circle interpolation between unit vectors,

```
a = slerp(P, A, r)        # on the meridian of A, angular distance 90°·r from the pole
b = slerp(P, B, r)        # on the meridian of B
v = slerp(a, b, s)
lat = asin(v.z),  lon = atan2(v.y, v.x)      with A, B = (cos lon, sin lon, 0)
```
So distance from the pole is linear along the two face edges, and each "row" `r = const` is the great-circle
arc between the two edge points (it bulges towards the pole). This rule reproduces all 1026 TESS vertices
(§5.2); the raster mosaics reprojected with it line up with the GeoGem vectors of §7.8.

### 6.3 Inverse (lat/lon → image)

Needed for reprojection to Web Mercator. Pick the hemisphere from the sign of `lat` and the quadrant from the
90° longitude wedge `[lonA, lonB]` that contains `lon`. Find `r ∈ [0, 1]` by bisection: for a trial `r`,
`a, b` as above; `v` lies on the pole side of the great circle through `a, b` exactly when `r` is too large.
Then `s = angle(a, v) / angle(a, b)`, and
- northern: `|dx| = (1 − s)·r·h`, `|dy| = s·r·h`;
- southern: `|dx| = h − s·r·h`, `|dy| = h − (1 − s)·r·h`;
with the signs of `dx`, `dy` given by the quadrant. 40 bisection steps give round-trip errors below 10⁻⁸ px.

---

## 7. Grid table schemas

Notation: fields are decoded strictly in the order written; `W/B/D/W1` as in §4.3. "Validated" counts are
in §0.2.

### 7.1 LegendGrid (key = LCID)

```
n      = W1(0)
items  = W(1, 3n)        n triplets (a, b, c)
total  = D(2, 1)[0]      fields 2–5
text   = W(6, total)     UTF-16 code units: n strings, each terminated by 0
```
WLD_HD, key 0x409 and 0x9 (identical): `(0,1,0) "1,000,000+ people"`, `(1,1,2) "Populated Places"`,
`(2,1,3) "National Capital"`; `total = 66`. Meaning of the triplet not confirmed (probably legend row,
symbol kind, bitmap/style index).

### 7.2 Bitmap (key = symbol id, 0x10000 … 0x10094, 149 records)

```
(w, h) = W(0, 2)
pixels = B(1, w·h)        row-major palette indices
```
Key 0x10000: 6 × 6, pixels start `1 2 2 2 2 2 1 3 3 3 3 2 …`. Pixels index the colours of `BitmapPalette`
(§8.7).

### 7.3 MillerTerrainGrid (key = tile number 0 … 0x4EE, 1263 records)

```
(w, h, t) = W(0, 3)
pixels    = B(1, w·h)
```
Key 0: `w = 256, h = 255, t = 0`; key 2: `256 × 129, t = 2`. Pixels are raster values like HYPSO
(first values 17, 28, 30 …). Observed structure: the keys form consecutive *sets* (t restarts at 0), and the
sets repeat the same level pattern — 4 tiles (two 256 × 255 over two 256 × 129, i.e. a 512 × 384 map), 12
tiles (t ≤ 13; 256- and 128-wide, 255- and 130-tall), 35 tiles (t ≤ 52), 117 tiles (t ≤ 208), all
256 × 255 except edge tiles — one set per flat-map raster listed in `MillerStyles` (HYPSO, SATELLITE, NIGHT,
TECTONIC, the climate rasters, POPULATION, MOON). `t` is a slot number in the level's tile grid; missing
slots have no tile. The grid widths and the projection parameters (probably in the `MillerStyles` header,
§8.7) are not yet worked out. All 1263 records decode.

### 7.4 EntityConv (entity id → GeoObj entry; key = page 0 … 0x6EB, 1772 records)

```
n = W1(0)
a = D(1, n)        fields 1–4: entity ids, ascending (page 0: 1, 2, 3, 4, …; page 1 starts at 1331)
p = W(5, n); q = W(6, n)        GeoObj record key = p·4096 + q
d = W(7, n)                     entry index inside that GeoObj record
```
**Meaning (validated):** entity `a[i]` is GeoObjGrid record `p·4096 + q`, entry `d` — true for all
1,813,920 entities (every GeoObj id appears exactly once, in exactly that place). The page for an entity id
is found with `EntityConvKey` (§8.4: the first entity id of each page). Page 0: `n = 1024`,
`a = 1 2 3 …`, keys `2 2 0 …`, entries `108 97 122 …` (entity 1 = GeoObj key 2 entry 108 = "Aruba (Neth.)").

### 7.5 FindGECContextGrid (key = page, same pages as Find Grid)

```
n = W1(0);  t = W(1, n);  c = W(2, n)
```
A `(type, context)` pair list (§7.6) for the entries of the same Find Grid page; `n` equals the value stored
for that page in `Find Key` (§8.4, validated on all 1356 pages). 0409 page 0: `n = 1867`, pairs
`(1,118) (0,7) (0,38) (0,96) (1,7) …` = the types/contexts of Find Grid page 0 entries 1, 2, 3, 4, 5 … It is
redundant with Find Grid for a converter (which entries are included is not worked out).

### 7.6 Find Grid (place-name search index; key = page 0 … 0x54B, ≤ 2048 entries per page)

```
n    = W1(0)
id   = D(1, n)       fields 1–4: entity id
type = W(5, n)       index into the feature-type descriptions of Find Data (§8.5)
ctx  = W(6, n)       index into the context strings of Find Data (§8.5)
fl   = B(7, n)       flags
pre  = B(8, n)       0…5
nlet = B(9, n)       letter count of the key
hi = W1(10); lo = W1(11);  L = hi·32768 + lo
s    = B(12, L)      n NUL-terminated byte strings in the G1G2 encoding (§8.3)
```
Entries are sorted search keys; an entity has one or more (1,205,729 entities have one, the others two or
more: alternative names, word rotations, codes). For every one of the 1,798,103 entity ids, all its entries carry
the **same `type` and `ctx`** (validated), so a gazetteer row is `(id, type, ctx)` plus names.

Examples (0409; ␈ stands for byte 0x08): entity 3 has `type 30` (= "country"), `ctx 236` (= "Asia"), keys
`Afghānestān [Afghanistan]`, `␈Da␈Shāhī Dowlat [Afghanistan]` …; entity 35473 (Volta) has type
"river", context "Ghana"; entity 10034993 has type "county", context "Arizona, United States"
(GeoObj name "Navajo"); London (entity 7398111) has type "Sovereign Capital Million+ City", context
"United Kingdom".

**Keys.** Each stored string is a key fragment:
- `fl & 0x80` clear: the string is a complete key and becomes the current *base*;
- `fl & 0x80` set: the key continues the current base; an empty string means "same key as the base"
  (used for homonyms: page 1287 entries 1416–1419 are four different entities named "Volta").
  Other observed bits: `0x40` key is a word rotation (contains byte 0x08 between the rotated words, e.g.
  `Antigua␈Casita`), `0x20` key adds words to the base, `0x10` key ends with a bracketed canonical name
  (`" [Antigua and Barbuda]"`).
- For `fl & 0x80` with a non-empty fragment, the key is the base's **first word** followed by the
  fragment, where the first word is the base up to (not including) its first byte 0x20 (space), 0x2D
  (hyphen), 0x08 or 0x09 after position 0: base `` `abīl ad Dodah`` + fragment `" Ajlub"` → `` `abīl Ajlub``;
  base `"‘Abb al Jinnāt"` + `" al Khaznah"` → `"‘Abb al Khaznah"`; base `"Antigua"` + `" and Barbuda"` →
  `"Antigua and Barbuda"`. The base itself is not changed by continuation entries.
- `pre` = number of leading characters shared with the previous key, capped at 5 (`Volsvitz → Volt` 3,
  `Volt → Volta` 4; for continuation entries it equals the length of the base's first word, capped).
- `nlet` = number of letters of the key without the bracketed part: alphanumerics plus the transliteration
  marks ‘ ’ ‛ ' \` ” “; spaces, dots and hyphens are not counted (`"Antigua and Barbuda"` 17,
  `"Navajo I.R."` 8, `"‘Abb al Khaznah"` 13). Check: with these two rules, 211,761 of 230,851 continuation
  entries on every 7th page rebuild to a key whose letter count equals `nlet`, and 163,626 of 166,461 full
  keys match. The misses are almost all rotated keys (bytes 0x08/0x09 inside, `fl & 0x40`) whose counting
  details are not settled.
- The display name of an entity is better taken from GeoObj (UTF-16, §7.7); Find Grid contributes
  `type`, `ctx` and the alternative search keys.

### 7.7 GeoObjGrid<lcid> (feature names; key = spatial cell, §7.8)

```
n     = W1(0)
id    = D(1, n)        fields 1–4: entity id
cls   = B(5, n)
c     = D(6, n)        fields 6–9
m     = number of i with c[i] ≠ 0
e     = D(10, m)       fields 10–13  \
f     = W(14, m)                       } scattered, in order, to the entries with c ≠ 0
g     = W(15, m)                     /
len   = B(16, n)       name lengths in UTF-16 code units
total = Σ len
text  = W(17, total)   (only if total > 0) names back to back, not terminated
```
Names may contain U+0004 separating alternative names (`"Danube\u0004Donau"`). Road objects of the shield
classes are named by their route number (`"95"`, `"A4"`, `"E25"`); the shield itself comes from the label style
(mad-styling.md §8.1). Record key 0 (WLD, 0409):
`n = 389, m = 378`; entries `(35473, 0, 995, "Volta")`, `(23347, 0, 3910, "Niger")`, `(37422, 0, 3195,
"Congo")`, `(41819, 0, 3675, "Lualaba")`, `(8598, 0, 3375, "Danube\u0004Donau")`; `e/f/g` start
`(1224, 81, 329), (312, 173, 971), (441, 780, 206)`.

What is known about the fields:
- `id` is the entity id shared with Find Grid, EntityConv and EntityCenter (§7.10). GeoObj holds 1,813,920
  distinct ids, each exactly once.
- `cls` is 0 in every entry of the shipped files.
- `c` looks like a display-scale threshold in thousands, the same unit as `PDCAttribute.scale / 1000`:
  national capitals have 200 (London, Paris), small places 0, Lake Victoria 1055 (not validated further).
- `f, g` lie in 0…1023, but they are **not** the feature's position in the record's cell (tested against the
  GeoGem points of 9 cities: Frankfurt, Mannheim, Dallas … do not match); `e` is unknown (4990 for several
  German cities, 16563 Dallas). Positions come from GeoGem (§7.8) or EntityCenter (§8.6).
- GeoObj record `k`, entry `j` is "object (k, j)", the target of GeoGem object references (§7.8).

### 7.8 GeoGemGrid (geometry; key = spatial cell)

**Cell keys.** The plain table `GeoFaceScale` (§8.2) defines 10 *bands* (scale ranges). Band `i` has a grid
of `n_i × n_i` cells on each of the 8 octants of the globe; its keys are the consecutive range
`first_i … first_i + 8·n_i² − 1` (first band starts at 0). For a key: `rem = key − first_i`,
`octant = rem / n²`, `row = (rem mod n²) / n`, `col = rem mod n`. WLD bands: n = 1, 1, 1, 2, 4, 8, 16, 64,
256, 1024; keys 0…8,948,407 (the shipped keys go up to 8,901,582).

**Cell → latitude/longitude.** With `δ = 90° / n` and cell coordinates `x, y ∈ 0…1023`:
`φ = row·δ + y·δ/1023`, `λ = col·δ + x·δ/1023` (degrees within the octant), and octant bits
`sx = −1 if octant & 1`, `sy = −1 if octant & 2`, `sz = −1 if octant & 4` (else +1):
```
latitude  = sz · φ
longitude = atan2(sy · sin λ, sx · cos λ)
```
(MOBB builds the 3D point `(sx cos φ cos λ, sy cos φ sin λ, sz sin φ)`; it snaps points with |z| > 0.999995
exactly onto the pole.) Example: band 3 key 0x18 = octant 0, row 0, col 0, n = 2: first point `(x, y) =
(0, 184)` → (8.0938° N, 0.0000°), a point of the river Volta (object (0, 0), §7.7).

**Record.**
```
n    = W1(0)                 number of objects
m    = W1(1)
w    = W(2, m)               object reference stream (below) → n refs (cellkey, index)
parts= W(3, n)               number of parts per object;  T = Σ parts
cls  = W(4, T)               PDCAttribute row index of each part (§8.1)
c2   = number of parts whose class has geometry kind ≠ 0
np2  = W(5, c2)              point counts of those parts (only if c2 > 0); parts of kind 0 have 1 point
Ptot = Σ point counts
crd  = W(6, 2·Ptot)          coordinate symbols; layout = return value of this field (§4.2 step 5)
```
Parts are listed object by object (object 0's parts first), points part by part.

**Shared records (implementation note).** When `GeoGemLookup` maps several keys to one record (repeat
ops, §2.2), the coordinates are **relative to each key's own cell**: the record is placed once per key.
The 37,123 shared WLD records are tiny (6–51 bytes) and almost all are whole-cell squares, e.g. one
"Region - label on goto" part with points `(1023,0) (1023,1023) (0,1023) (0,0)` used by 586,986 keys.
§0.2's point count (54,448,380) counts each distinct record once.

*Object reference stream* (`w`): state `ref = 0`, `idx = 0`; read words until `n` references are produced:

| Word | Effect |
|---|---|
| `0x100, a` | `ref += 1 + a`; `idx = 0` |
| `0x101, a, b` | `ref += 1 + (a << 8) + b`; `idx = 0` |
| `0x102, a, b, c` | `ref += 1 + (a << 16) + (b << 8) + c`; `idx = 0` |
| `0x103, a, b, c, d` | `ref += 1 + (a << 24) + (b << 16) + (c << 8) + d`; `idx = 0` |
| `0x104, a` | `idx += 1 + a` |
| `0x105, a, b` | `idx += 1 + (a << 8) + b` |
| `0x107, a, b` | produce `(a << 8) + b + 1` references |
| any other `v` | produce `v + 1` references |

"Produce" emits `(ref, idx)` and then increments `idx`. `ref` is a **GeoObjGrid record key** and `idx` an
entry index in that record (validated: all 2,947,064 references in WLD resolve). In practice
`ref` is the key of the same or a coarser (ancestor) cell.

*Coordinates.* `layout = 0`: `crd` alternates `x0 y0 x1 y1 …`; `layout = 1`: all `x` then all `y`. Each of
the two streams is then decoded independently with a **predictive delta** whose state `(prev, cur)` starts at
`(0, 0)` and runs across all points of the record:

```
step(v):
    if v ≥ 64:  new = v − 64                         # absolute
    else:
        vel = cur − prev
        if vel > 4:  new = cur + v − 28              # i.e. predicted velocity 4
        else:        new = cur + max(vel, −4) + v − 32
    prev = cur; cur = new; return new
```
Decoded values lie in 0…1023 in every validated record.

*Grouping.* MOBB then sorts objects and joins consecutive parts with the same geometry kind (1, 2, 3) and
the same `PDCAttribute.b12` value into strokes (its internal draw lists); this is presentation logic, not
needed to extract the geometry.

**Self-check (WLD_CD, key 0x18, record (1382675, 65237)):** `n = 627, m = 370, T = 6466, Ptot = 167106,
layout = 1`, 4 bits unused; refs start `(0,0) (0,1) (0,2) …`, parts `1 1 1 …`, classes `41 41 41 …`
(row 41 = "River"), point counts `10 45 18 4 35 23 …`; `X = 0 7 9 10 6 6 …`, `Y = 184 176 170 165 147 143 …`;
SHA-256 over all `(X, Y)` as u16 LE pairs = `8e4dd103d39773599588253d96a64bf0f9bb626a906136fbd84947d57a002cf3`.
Object (0, 0) is GeoObj key 0 entry 0, "Volta".

### 7.9 PDCAltitudeGrid (display rules; key = MapStyle id)

Keys are the `MapStyle` ids (§8.7: 0…19, 21…33; 33 records). A record lists display rules, one per
(feature class, scale range):
```
n    = W1(0)
cls  = W(1, n)          PDCAttribute row
b2   = B(2, n)          two nibbles: low, high (MOBB turns a high nibble of 0 into 1)
b3   = B(3, n)
b4   = B(4, n)          two nibbles
w5 … w13  = W(5, n) … W(13, n)
b14, b15, b16 = B(14, n), B(15, n), B(16, n)
w17  = W(17, n)
b18, b19, b20 = B(18, n), B(19, n), B(20, n)
w21 … w24 = W(21, n) … W(24, n)
smax = D(25, n)         fields 25–28: maximum scale denominator of the rule (MOBB divides by 1000)
smin = D(29, n)         fields 29–32: minimum scale denominator
```
MOBB remaps `w6 … w13`, `w17`, `w21 … w24` through the pen table (they are `GeoPenDef` pen numbers; 0xFFFF =
none) and, depending on the class's geometry kind, takes widths from them. Style 0 ("Atlas Default"):
`n = 1254`; the first rules are class 0 ("PPL - non-capital million+ city") for the scale ranges
50,000,000–4,000,000, 3,999,999–150,000 and 149,999–1,000, with `w5 = 114, 106, 105`, `w6 = 79, 36, 106`,
`w7 = 107`. Field meanings beyond the scale range and the class are open.

### 7.10 How the gazetteer tables join

All of these use one **entity id** space (ids 1 … 161,775,207):

| Table | Entities | Gives |
|---|---|---|
| GeoObjGrid0409 | 1,813,920 (each once) | display name(s) in UTF-16, cell, scale value (`c`), label fields (`e, f, g`, open) |
| EntityConv (+ EntityConvKey) | the same 1,813,920 | entity → (GeoObj record key, entry) |
| Find Grid0409 | 1,798,103 (subset) | feature type, administrative context, search keys |
| EntityCenter | 2,015 (subset) | centroid (countries and major divisions) |
| GeoGemGrid | references `(GeoObj key, entry)` | the geometry (points, lines, areas) and its PDCAttribute classes |

So a converter can produce one gazetteer row per entity: `id → GeoObj name → Find type and context →
geometry from the GeoGem parts that reference (key, entry)`, with EntityCenter as the centroid where present.
Validated joins: EntityConv ↔ GeoObj 1,813,920 / 1,813,920; Find ids ⊂ GeoObj ids; EntityCenter ids ⊂
both; 2,947,064 / 2,947,064 GeoGem references resolve; per-entity `(type, ctx)` consistent for all
1,798,103 Find entities.

---

## 8. Plain tables

### 8.1 PDCAttribute (feature classes; 606 records × 62 bytes)

| Off | Type | Meaning |
|---|---|---|
| 0 | u16 | class id (0, 1, … with gaps; ids up to 10117). **GeoGem refers to classes by row position, not id.** |
| 2 | u8 | length of the name |
| 3 | u8 | geometry kind: 0 point (143 rows), 1 radius point (6), 2 line (127), 3 area (330) |
| 4 | u16 | style/pen group (e.g. 1 coastline, 43 water, 55 roads, 134 time zones). ENCARTA.EXE classifies map hits by it (mad-styling.md §16, V11 result) |
| 6 | u8 | routable-road flag (21 rows set: motorways, arterials, RoutableNP …; mad-styling.md §15 item 8) |
| 7 | u32 | scale value; MOBB divides by 1000 (200000 cities, 400000 lines/areas, 30000 landmarks) |
| 11 | u8 | 0 in all rows |
| 12 | u8 | grouping value used when joining parts (0, 1, 2, 5, 10, 100, 120 …) |
| 13 | u8 | priority/label value (1…255) |
| 14 | char[48] | name, NUL-terminated, cp1252 (`"PPL - non-capital million+ city"`, `"Coastline"`, `"River"`, `"Sovereign - boundary"`) |

Row 0: `00 00 | 1F | 00 | 0B 00 | 00 | 40 0D 03 00 | 00 | 00 | 17 | "PPL - non-capital million+ city"`.

### 8.2 GeoFaceScale (band table; 10 records × 10 bytes)

`u32 max_scale, u32 min_scale, u16 log2(n)`. MOBB divides both scales by 1000. WLD (= SWLD):

| Band | max | min | n | first key |
|---|---|---|---|---|
| 0 | 50,000,000 | 40,000,000 | 1 | 0 |
| 1 | 39,999,000 | 20,000,000 | 1 | 8 |
| 2 | 19,999,000 | 14,000,000 | 1 | 16 |
| 3 | 13,999,000 | 8,000,000 | 2 | 24 |
| 4 | 7,999,000 | 4,000,000 | 4 | 56 |
| 5 | 3,999,000 | 2,000,000 | 8 | 184 |
| 6 | 1,999,000 | 800,000 | 16 | 696 |
| 7 | 799,000 | 150,000 | 64 | 2,744 |
| 8 | 149,000 | 30,000 | 256 | 35,512 |
| 9 | 29,000 | 1,000 | 1024 | 559,800 |

The scales read as map-scale denominators (1:50,000,000 … 1:1,000).

### 8.3 G1G2 to Unicode (8-bit place-name encoding)

Find Grid keys and Find Data strings are 8-bit strings in a shifting code page called **G1G2**. The table is
`n × {u8 set, u8 byte, u16 unicode}` (412 records in WLD). It fills three 256-entry maps: `set = 1` →
map 1, `set = 2` → map 2, anything else → map 0. Before loading, map 0 is the identity and maps 1 and 2 are
all U+0020. In the shipped table map 0 is Windows-1252 (0x80 → €, 0x91 → ‘, 0x92 → ’, 0x9A → š …), map 1
holds precomposed Latin letters with diacritics (0xAA → ā, 0xCC → ī, 0xF1 → ū, 0x7F → Ā, 0xB9 → č …) and
map 2 more letters and symbols (0x21 → Ĕ, 0x71 → °, 0x40 → Δ …).

Decoding a string, with `cs = 0` at the start:
- byte 0 ends the string;
- bytes 1…5 are **shift codes** and produce nothing: `cs = byte − 2` (2 → map 0, 3 → map 1, 4 → map 2,
  5 → Shift-JIS, 1 → "raw": the following bytes are taken as code points as they are);
- bytes 8 and 9 reset `cs` to 0 and are then output like ordinary bytes (0x08 is the word-rotation marker
  in Find keys);
- otherwise output `map[cs][byte]` (cs 0–2), the byte itself (raw), or in Shift-JIS mode the code-page-932
  character of a lead byte 0x81–0x9F / 0xE0–0xFC plus the next byte (single bytes otherwise).

Examples: `91 41 62 62 03 AA 02 73` → "‘Abbās"; `41 66 67 68 03 AA 02 6E 65 73 74 03 AA 02 6E` →
"Afghānestān". The shift state starts at 0 for every string (when a Find key is built from a base and a fragment, decode the joined bytes as one string).

### 8.4 Key tables (`Find Key<lcid>`, `EntityConvKey`)

```
u16 n; u8 has_text; u8 has_value
n × { if has_text: u8 len, u16 text[len];  if has_value: u32 value }
```
- `EntityConvKey`: `n = 1772` (one per EntityConv page), no text, `value` = first entity id of the page
  (1, 1331, 2463, …). Find the EntityConv page of an entity by binary search.
- `Find Key0409`: `n = 1356` (one per Find Grid page); `text` is the first key of the page as u16 collation
  weights (page 0: `40 1410 1310 1450 1230 …`; the weight table is presumably `Unicode Attributes`),
  `value` = the number of FindGECContext entries of that page (1867, 1921, …; validated for all pages).

All four instances parse to exactly their section length.

### 8.5 Find Data<lcid> (feature types and context strings)

Two consecutive parts; both parse to exactly the section length in WLD 0409/0009 and SWLD 0409.

**Part 1 (feature-type taxonomy).** Header `u32 S, u32 R, u16 nA, u16 nB, u16 nC, u16 nD`, then
1. `S` bytes string pool (NUL-terminated G1G2 strings);
2. `R × 10 bytes`: `{u16 id, u16 k, u16 c, u16 u, u16 d}` type records (`c` indexes the `nC` list and `d` the `nD` list of item 6);
3. `nA × {u16 record, u32 pool offset}` (a name list);
4. `nB × u32` pool offsets: **the feature-type descriptions, indexed by Find Grid `type`**
   (30 "country", 9 "river", 12 "county", …);
5. `nB × u16`, `nB × u8` (MOBB rescales these bytes by 16/3), `nB × u16`, `nB × u8`, `nB × u8`: per-type
   attributes, meaning open;
6. `nC × u16` and `nD × u16` record lists.

WLD 0409: `S = 10589, R = 1185, nA = nB = nD = 1254, nC = 771`; the pool starts "universe", "Human-Defined
Entities", "Physically-Defined Entities", "Line or Corridor", "hemisphere", "country", "Populated Place" …

**Part 2 (administrative context).** Header `u32 S2, u32 N, u32 K, u32 M, u16 X, u16 C`, then
1. `S2` bytes string pool;
2. `N × 16 bytes` context nodes `{u32 id, u16 nkids, u16 nlinks, u32 kids_start, u32 links_start}`;
3. `K × u32` (child node indices), `M × u16` (link values);
4. `X × 9 bytes` `{u32 node, u32 pool offset, u8 flag}`: names of the nodes, sorted by name (node 0 =
   "Aruba", "ABW", "AW"; flag 0 marks the preferred name);
5. `C × u32` pool offsets: **the context strings, indexed by Find Grid `ctx`** ("Asia", "Ghana",
   "Arizona, United States", "United Kingdom", "" …).

WLD 0409: `S2 = 323409, N = 8662, K = 8677, M = 10326, X = 19395, C = 9544`.

### 8.6 EntityCenter (entity centroids)

```
u32 npages; u32 first_id[npages]
npages × 4092-byte pages, each up to 341 records of 12 bytes:  {u32 entity, u32 cell_key, u16 x, u16 y}
a record with entity 0 ends a page early (only the last page is short)
```
`cell_key, x, y` is a position in the GeoGem cell system (§7.8). WLD: 6 pages (341 × 5 + 325 = 2030 records, then 4 spare bytes), 2015 distinct
entities (countries, territories, first-level divisions …). Checks: entity 1 → cell 2803155 (band 9),
`(744, 204)` → 12.498° N, 69.937° W (Aruba); entity 2 → 17.146° N, 61.821° W (Antigua and Barbuda);
entity 3 → 33.833° N, 66.025° E (Afghanistan).

### 8.7 Other plain tables (structure from the MOBB loader, not yet decoded in detail)

| Table | Known so far |
|---|---|
| `LangId` | u16 LCIDs (WLD `09 04 09 00` = 0x0409, 0x0009) |
| `BitmapPalette` (300 B) | 60 × 5 bytes `{u8 index; u8 r, g, b; u8 transparent}`: colour of each `Bitmap` palette index. Index 0 is pure green `(0, 255, 0)` with `transparent = 1` (the key colour; all others 0). Rendering the 149 bitmaps with R, G, B in this order gives plausible map symbols (red/yellow city dots, park and mine icons, fill patterns); order confirmed only visually |
| `BitmapColor` (3000 B, optional) | 600 × 5 bytes `{u16 key, 3 colour bytes}` |
| `BitmapDimDir` (298 B) | `u8 width[149]` then `u8 height[149]`, in bitmap key order — identical to the sizes stored in the `Bitmap` records (validated for all 149; 36 bitmaps are not square). Lets a reader size symbols without decoding them |
| `MapStyle` | (full layout: mad-styling.md §3, names are char[32]; the char[26] read below cut "Atlas Physical Features-No HS") 33 × 115 bytes: `+0x00 char[26]` style name ("Atlas Default", "Atlas Political", "Atlas Satellite Night", "VE Road" …), `+0x20 char[10]` RAST raster name, `+0x40 char[12]` LOOK style name (e.g. "Atlas Political" = HYPSO + POLITICAL, "Atlas Outline" = HYPSO + GENERAL), `+0x60 u16` style id (= PDCAltitudeGrid key), then 3 bytes that look like a background colour ("Atlas Default" `EB EA E1`, "Atlas Satellite Night" `10 21 42`) and further parameters (open) |
| `GeoPenDef` | 1231 × 38 bytes: `u16 head, u8, u8 is_head, u32, 4 × u8, u16, u8, 3 colour bytes, 5 × u8 flags, u8, u8 (minus 0x80 = signed), 5 × u8, 8 unused bytes`. Records with `is_head = 1` start a pen; the others are additional layers chained to pen `head` (casing, centre line …) |
| `GeoLabel` | 422 × 27 bytes, byte layout from the parser: `u8, colour[3], u8, u8, u8, u8, u8, u8, u8, u8 (two nibbles), u8, colour[3], u8, colour[3], colour[3], 4 × u8` — label styles (text colour, halo/outline colours, sizes, font index into `Label Font`?); meanings open |
| `MillerStyles` (344 B) | header `u16, u16, u16, u16 nlevels = 5`, then 5 × `{u16, u16}` pairs `(2, 0x8000) (1, 0x8000) (3, 0x8000) (2, 0) (7, 0xFB00) (4, 0) (13, 0xF700) (8, 0) (26, 0xED00) (18, 0x0B00)` (probably the per-level tile grid sizes and fractional edges of the flat map), then **Correction (atlas converter):** the 5 level sizes are `u32`s starting at byte 7, after a `u8` level count (the pairs above are those `u32`s misread as `u16` pairs from byte 8). The header is `u16 version (0x0100), u16 tile width 256, u16 tile height 255, u8 nlevels` (mad-styling.md §12.1). **Correction (implementation, round 2):** after the level table comes a `u8` raster count (11; the `0x0B` of the last "pair"), then the per-raster entries; the parser checks the count and that the entries end exactly at the section end. **Correction (implementation):** there is no separate list of LOOK names. Every entry is `raster\0 (look\0)* \0 u32 scale, u8 level count`: a raster name, its LOOK names, an empty string, then the numbers. The first entry is HYPSO with six looks (GENERAL, GENERALFLAT, STATISTICAL, POLITICAL, HYPSOFLAT, HYPSO), scale 250, 3 levels. The others each have one look: SATELLITE/satellite 2000 3, NIGHT/night 4000 3, TECTONIC 8000 3, RAINJAN, RAINJULY, RAINAVG, TEMPJAN, TEMPJULY (14000 2), POPULATION 8000 3, MOON 8000 3. That makes 11 entries, which parse to exactly the section length |
| `Label Font` | `{u8 font id, u8 charset, u8 len, UTF-16 name}` records ("Times New Roman", "MS Mincho", "Arial", "MS PGothic" …); the id and charset were read as one u16 before (mad-styling.md §7.1) |
| `ShieldId` | 27 × 3 bytes `{u8 n, u16 value}` |
| `GeoTrans`, `Unicode Attributes`, `MinRect`, `Font Conv` | specified in mad-styling.md §6, §11.2, §13.2, §11.1; parsed by `tables::geo_trans`, `unicode_attributes`, `min_rects`, `font_conv` |

---

## 9. Open points and next steps

In priority order:

1. **Find rotated keys** (§7.6): the counting/joining details of keys containing 0x08/0x09 (≈8 % of
   continuation entries do not yet satisfy the `nlet` check). Display names do not depend on this (they
   come from GeoObj); search keys do.
2. **GeoObj fields** `c, e, f, g` (scale threshold? label offsets/priority?) — needed for label placement.
3. **Styling**: `MapStyle`, `GeoPenDef` (probably indexed by `PDCAttribute` +4), `GeoLabel`, `Label Font`,
   `Font Conv`, `ShieldId`, `BitmapPalette`/`BitmapColor`/`BitmapDimDir` → MapLibre style and sprite sheet.
   Parsers are listed in §10.
4. **Find Data** details: the type records and per-type attribute arrays (part 1), the context-node tree
   (part 2) — the strings that matter are already reachable.
5. **PDCAltitudeGrid** field meanings (§7.9) together with `GeoPenDef`; `MinRect`; `GeoTrans`.
6. **MillerTerrainGrid** tiling and `MillerStyles` (flat maps).
7. LegendGrid triplet meaning, the CMT `kind` byte, RAST `scale` semantics, TESS triangle codes and the four
   small TESS tables, `Unicode Attributes` (collation weights used by `Find Key`).
8. VM check of one raster tile and one GeoGem cell against the original display (orientation is already
   confirmed by self-consistency and by the vector/raster overlay, so this is a formality).

---

## 10. Provenance

Ghidra addresses in MOBB200.DLL (MD5 `cd1cbfb782c6394bb0043fad29840613`, identical in `EE/ENCARTA`,
`EE/KIDS`, `AREF/COMPNTS`); nothing above depends on them.

| Topic | Functions |
|---|---|
| INDX load / page search / page walk | FUN_63532e17, FUN_63532ee2, FUN_63532eb7, FUN_63532aae, FUN_63532c79 |
| Grid model deserialisation | FUN_635494cc (context tables FUN_6354964b) |
| Grid bit reader, Huffman, raw bits | FUN_63524980, FUN_6352487d, FUN_635248f0, FUN_63524950 |
| Field decoders (word / byte / dword planes) | FUN_635249f0, FUN_63524e50, FUN_635252e0 (wrappers FUN_63524e10, FUN_635252a0) |
| Grid record fetch | FUN_63505060 (record buffer = length + 8 slack bytes) |
| Table name list (62 entries) | data at 0x63606330; name formatter FUN_63547ec0 |
| Database open (which parser reads which table) | FUN_63518ec5, calling FUN_635575c1 (GeoFaceScale), FUN_63555f3a / FUN_6356ca26 (PDCAttribute), FUN_63549327 (BitmapPalette), FUN_6354937c (BitmapColor), FUN_63555a66 (MapStyle), FUN_63556934 (GeoPenDef), FUN_63555675 (GeoLabel), FUN_6355e946 (GeoTrans), FUN_63518433 (G1G2 to Unicode), FUN_63583434 (Unicode Attributes), FUN_6355780e (ShieldId), FUN_6353d098 (Label Font), FUN_63518367 (Font Conv), FUN_6355f45e (MinRect), FUN_63552c70 (EntityConvKey) |
| Schema readers | LegendGrid FUN_6356cc23, Bitmap FUN_6351c413, MillerTerrainGrid FUN_635750b7, EntityConv FUN_6351c785, FindGECContextGrid FUN_6351c88a, Find Grid FUN_6351ca78, GeoObjGrid FUN_635226dc, GeoGemGrid FUN_6351e045 (+ FUN_6351d394 refs, FUN_6351d4bc, FUN_6351dcb3, FUN_6351dcf4, coordinates FUN_6351cdc0 / FUN_6351d760, cell frame FUN_63525430, FUN_63525490, band lookup FUN_635574d7), PDCAltitudeGrid FUN_6356c2ba |
| RMAP open, raster select, LOOK/CMT | FUN_63571d71, FUN_63571be5, FUN_6357185a, CMT use FUN_63575591 |
| Raster model and tile decode | FUN_63507f40, FUN_63507e80, FUN_635080f0, FUN_63508350 (mode 2), FUN_635081e0 (RLE), FUN_63507e20 (transpose), FUN_63507d50 (Huffman); decoder table at 0x63606270 |
| Tile key and triangle geometry | FUN_635710c6 (key interleave), FUN_6357101d, FUN_63570aa7, FUN_63570e69, FUN_63570d1d (8 face triangles), FUN_6356ac30 (tile split along its anti-diagonal into two triangle textures) |
| Styles | MapStyle FUN_63555a66, GeoPenDef FUN_63556934 / FUN_6357800b / FUN_635568cf (pen chains), GeoLabel FUN_63555675 / FUN_63522439 |
| Gazetteer | G1G2 load FUN_63518433, G1G2 tokenizer FUN_6356b910 / FUN_6356b9c0 / FUN_6356bad0 (Shift-JIS via FUN_63518268), key tables FUN_63552c70, Find Data FUN_6354cd40 (part 1) and FUN_6354d080 (part 2), Find open FUN_6354d6d0, EntityCenter FUN_6350b0cd / FUN_6350b20e |

---

## 11. Implementation

`crates/encarta-formats/src/mad/` was written from this document: `toc` (container, HD+CD join by
section name), `index` (§2; each page is decoded into runs of consecutive keys sharing one record),
`bits` + `huffman` + `grid` (§1, §3, §4), `rmap` (§5: tiles, Morton keys, quadrant rotation, level mosaic,
LOOK → RGBA, octahedral → lat/lon window resampling), `octahedral` (§6), `cells` (§7.8 cell keys,
§8.2 bands), `schemas` (§7), `tables` (§8), `g1g2` (§8.3), `gazetteer` (§7.10 joins), `check` (whole-database
validation). Choices that differ from the text but are equivalent:

- **Inverse projection (§6.3) in closed form.** For a point `v` in the face with pole `P` and equator vertices
  `A`, `B`, the arc at angular distance θ from the pole passes through `v` exactly when
  `tan θ = (v·A + v·B) / (v·P)`, so `r = (2/π) · atan2(v·A + v·B, v·P)` with no bisection. The 1026 TESS
  vertices round-trip through it to the subdivision-16 grid (checked by `mad check`), and unit tests round-trip
  6,561 points to below 10⁻⁶ px.
- **Huffman decoding** walks the stored steps upwards from a start step looked up by the top 8 bits of the
  window. This gives the same result as walking down from the anchor.

`mad check --all` (WLD + SWLD) decodes all 445,176 tiles and every grid record, and checks every count in §0.2
and every self-check value. It takes about 1.5 s wall time (≈ 10 s CPU) on 24 cores. Integration tests are in
`crates/encarta-formats/tests/mad.rs`.

Observations made while implementing (not yet folded into the sections above):
- SWLD: 87,380 HYPSO tiles (levels 0…7) + 4 LOCATOR; GeoGem 35,512 keys, 5,404,800 points, 271,720 parts;
  31,662 GeoObj/EntityConv entities; Find Grid0409 31,556 entities (111,542 entries); EntityCenter 1,870
  entities; LegendGrid holds 1 record. SWLD has no `MillerTerrain*` sections.
- WLD: Find Grid0409 holds 2,775,244 entries; GeoGem 5,121,126 parts.
- In the GeoGem bands 3–8, Europe has (almost) no `Coastline` (PDCAttribute rows 50/101) parts; the land/sea
  edge comes from `Ocean` areas (row 80) and the hypsometric/bathymetric areas of band 7. Sovereign
  boundaries (row 57) drawn from band 6 line up with HYPSO level 6 to the pixel.
