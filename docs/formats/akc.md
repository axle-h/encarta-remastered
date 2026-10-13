# .AKC article/data packages (CONT*.AKC, DATA*.AKC, DATAF*.AKC, REL*.AKC)

Status: **format understood at the architecture level; decoder not yet written.**
Reader lives in `AREF/COMPNTS/MSENCDAT.DLL` (class `CStorageAKC`, exposed through an
asynchronous pluggable URL protocol `CDataProtocol`). Ghidra output: `analysis/decompiled/ghidra/MSENCDAT/`.

## Not encrypted
No crypto imports; the high entropy comes from a word-level Huffman compressor.
The function initialising the bit-reader over each record just sets pointers — there's no
descramble pass.

## File layout (little-endian)
| off  | field |
|------|-------|
| 0x00 | magic `" CKA"` (0x414B4320) |
| 0x04 | version = 1 |
| 0x08 | package GUID (matches `DELUXE.INI [Packages]` and `catalog2/guid.lst`) |
| 0x18 | flags (1 = empty stub package, e.g. CONTEDK.AKC) |
| 0x1C | offset of shared dictionary blob (= 0x30 + label length) |
| 0x20 | length of shared dictionary blob |
| 0x24 | offset of record index |
| 0x28 | length of record index |
| 0x2C | length of UTF-16 XML label at 0x30 (`<label guid=... ><tool>BuildContent ...`) |

Records sit between the dictionary blob and the index.

## Record index (bit-packed)
- u32 record count, then a sparse table every 32 records `(u32 first_key, u32 bit_offset)`,
  then bit-packed per-record fields (field widths are read from the stream with 5-bit
  lengths). Binary search on the sparse table, then a linear scan inside the 32-record group.
- Each record yields: key (content id), compressed offset, compressed length, uncompressed length.

## Codec (magic "DicT", 0x44696354, in the decoder context)
- **Shared dictionary** (blob at 0x1C) — decoded once at open: Huffman tables plus a word list.
- Per record: optional per-record dictionary (negative token indices), then canonical-Huffman
  coded token stream (10-bit fast lookup tables, ~320-symbol alphabet), with a deflate-style
  length/extra-bits table (21 base/extra-bit pairs), which suggests LZ-style repeats over tokens.
- Tokens are **XML-aware**. Each dictionary word carries a 4-bit kind:
  0 word with automatic leading space (and sentence-capitalisation flag), 1 raw word,
  2 raw word + reset capitalisation, 3 `<name` (pushes onto a tag stack), 4 attribute name,
  5 `=value`, 6 `='value'`, 7 `="value"`, 8 `</` + pop, 9 `</name>`, 10 `/>`.
  The emitter inserts the closing `>` lazily.
- Output is XML text (the sidebars in MD*.EIT show the target dialect: `<text refid>`,
  `<pkey id>`, `<xr type="qv"><x id=".." para=".."/>`, `<entity tag='smallcaps'>`).

## Plan
1. Write a spec in our own words (this file grows) from analysis of the decoder.
2. Implement `akc.rs` in Rust from the spec (not by translating decompiler output).
3. Validate: every record decodes to well-formed XML; total decoded size matches the
   uncompressed lengths in the index; cross-check ids against `catalog2/*.ecn`.
4. Oracle: the original app doesn't run under Wine with reasonable effort. Use the Windows XP
   reference VM (see `docs/PLAN.md`, step 0) to view or export a sample of articles as private,
   gitignored test fixtures.

Effort: **L** (roughly 1–2 weeks focused work). This is the critical path — the article text exists nowhere else.
