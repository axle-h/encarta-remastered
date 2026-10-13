# .AKC article/data packages (CONT*.AKC, DATA*.AKC, DATAF*.AKC, REL*.AKC)

Status: **implemented** (`encarta_formats::akc`, `akc` CLI; see [§15](#15-implementation)). The Rust decoder,
written from this document alone, decodes every record of every AKC file on the disc (634,248 records,
2,135,837,381 bytes of XML; 634,249 / 2,136,740,166 counting both identical DataESK copies) to well-formed
XML of exactly the indexed length, and reproduces every §11 value. The spec was first validated by a private prototype.

The reader lives in `AREF/COMPNTS/MSENCDAT.DLL` (class `CStorageAKC`, served through the pluggable
URL protocol `CDataProtocol`). Provenance pointers (Ghidra addresses) are listed at the end; nothing
below depends on them.

Contents: [1 Conventions](#1-conventions) · [2 File header](#2-file-header) ·
[3 Record index](#3-record-index) · [4 Codec overview](#4-codec-overview) ·
[5 Huffman codes](#5-huffman-codes) · [6 Dictionaries](#6-dictionaries) ·
[7 Token stream](#7-token-stream) · [8 XML emitter](#8-xml-emitter) ·
[9 Decoding a record](#9-decoding-a-record-end-to-end) · [10 Worked examples](#10-worked-examples) ·
[11 Self-check values](#11-self-check-values) · [12 What the XML looks like](#12-what-the-decoded-xml-looks-like) ·
[13 Edge cases and errors](#13-edge-cases-and-errors) · [14 Provenance and open points](#14-provenance-and-open-points) ·
[15 Implementation](#15-implementation)

---

## 1. Conventions

- Integers in the file header and in the fixed part of the index are **u32 little-endian**.
- **Bit streams** (index bit area, shared dictionary, compressed blocks) are read as a sequence of
  **16-bit little-endian words**, each consumed **most-significant bit first**. Equivalently: swap each
  byte pair, then read the result as one big-endian MSB-first bit string. A multi-bit field is read
  MSB first, so `read(n)` returns the next `n` bits as an unsigned integer whose first bit read is
  its most significant bit. `read(0)` = 0 and consumes nothing. Fields freely straddle word boundaries.
  Every bit stream has an even byte length (an odd length is an error, see §13).
- A **bit offset** into a stream counts from bit 0 = MSB of the first 16-bit word. Seeking to bit `b`
  means word `b >> 4`, skipping `b & 15` bits of it.
- **`num()`**, the variable-length unsigned integer used in dictionary headers:
  `w = read(5)`; if `w == 0` the value is 0, otherwise it is `(2^w − 1) + read(w)`.
  (So 1 → `00001 0`, 2 → `00001 1`, 3..6 → `00010 xx`, …)
- Text produced by the codec is **bytes**; on this disc they are UTF-8 (see §12).

## 2. File header

| off  | size | field |
|------|------|-------|
| 0x00 | u32 | magic `0x414B4320` (bytes `20 43 4B 41`, " CKA") |
| 0x04 | u32 | version = 1 (the reader rejects anything else) |
| 0x08 | 16  | package GUID (matches `DELUXE.INI [Packages]` and `catalog2/guid.lst`) |
| 0x18 | u32 | flags: bit 0 set = **empty stub package** (no dictionary, no index; all offsets/lengths below are 0) |
| 0x1C | u32 | absolute file offset of the **shared dictionary** bit stream |
| 0x20 | u32 | its length in bytes |
| 0x24 | u32 | absolute file offset of the **record index** |
| 0x28 | u32 | its length in bytes |
| 0x2C | u32 | byte length of the label at 0x30 |
| 0x30 | …   | label: UTF-16LE XML with BOM, e.g. `<label guid='{…}'><tool><name>BuildContent</name>…` (build metadata only; not needed for decoding). The label's `guid` attribute names the header GUID, but not always in the same case (DataFDlx's minimal label `<label guid='{4fa815c4-…}'/>` is lower-case) |

The shared dictionary immediately follows the label (`0x1C == 0x30 + label length` in every file).
The compressed record blocks lie between the dictionary and the index; the index is the last thing in the file.
There is **no checksum** anywhere (header, index or blocks).

Confirmed on all 22 `.AKC` files under `extracted/iso/EE/{ENCARTA,KIDS}`: magic and version always match;
the 8 stubs (`CONTEDK/CONTESK/DATAEDK` and Kids `CONTECK/CONTEDK/CONTESK/DATAECK/DATAEDK`) have flags = 1
and zero offsets; every other file has flags = 0.

## 3. Record index

Records are identified by a **key**, which is the **local record number** used by the catalog
(`catalog2/*.ecn` packed location, see `search-catalog-misc.md` §3). On this disc keys are dense:
record `i` has key `i`, for `0 ≤ i < N`. (The format allows gaps, see the key-delta rule below,
but none occur.) Each record resolves to: the file offset and byte length of the **compressed block**
holding it, its **uncompressed length**, the **index of its first token** within the block's token
stream (`start`) and its **token count** (`ntok`).

Records come in two shapes:
- **direct**: the record owns a whole compressed block (`start = 0`). Used for large records.
- **block member**: many consecutive records share one compressed block of roughly 64 KB; the record
  is the token range `[start, start + ntok)` of that block's token stream. The members of a block tile
  its token stream exactly, in increasing `start` order (confirmed for every block on the disc).

### 3.1 Fixed part (u32 LE, relative to the index start)

| off | field |
|-----|-------|
| 0 | `N` = number of records |
| 4 | **sparse table**: `G + 1` pairs `(u32 first_key, u32 bit_offset)`, where `G = ceil((N − 1) / 32)` (= `(N + 30) >> 5` in integer arithmetic) |
| 4 + 8(G+1) | `M` = number of **block entries** |
| 8 + 8(G+1) | `M` × u32 bit offsets of the block entries |
| 4·(4 + 2G + M) | start of the **index bit area** (header, block entries, record groups) |

All bit offsets (sparse table, block-entry table) are measured from bit 0 of the **index start**
(the first bit of the word holding `N`), not from the start of the bit area.

**Grouping.** Sparse entry `i` points at the first record of group `i`. Records `0 … N−2` are cut
into groups of 32 (entries `0 … G−1`, the last of them possibly shorter: it holds `((N − 1) mod 32)`
records, where 0 means 32), and the very last record `N − 1` is always alone in sparse entry `G`.
For ContStd, N = 44,055, G = 1,377: groups 0–1375 hold 32 records, group 1376 holds 22, entry 1377 holds record 44,054.
For N = 1 (DataESK) G = 0 and the single sparse entry holds the single record.

### 3.2 Bit-area header

Read at bit offset `32·(4 + 2G + M)`:

1. `kw = read(5)`: width of key deltas.
2. Twelve **field descriptors**, each `base = read(32)`, `w = read(5)`, `range = read(w)`;
   the descriptor's value width is `w` and its maximum is `base + range`.
   The first six (`D0…D5`) describe direct records, the next six (`S0…S5`) block entries:

| # | D (direct records) | S (block entries) |
|---|---|---|
| 0 | `max` = number of direct records (0 = none); `w` is read like any other but never used | `max` = `M`; `w` = width of a block-entry number |
| 1 | uncompressed length | uncompressed-length base |
| 2 | compressed block length | compressed block length |
| 3 | block file offset (absolute) | block file offset (absolute) |
| 4 | token count | token-count base |
| 5 | unused (all zero) | unused (all zero) |

Reading a field "with descriptor X" means `X.base + read(X.w)`.

The block entries (pointed to by the block-entry table) come right after the header, followed by the
record groups. A decoder should nevertheless seek by the stored offsets rather than assume this.

### 3.3 One record

For every record of a group except the first: **key delta** — if `kw == 0` the delta is 1; otherwise
`flag = read(1)`, and the delta is `read(kw)` if `flag == 1`, else 1. The first record of a group has
key `first_key` from the sparse table. (On this disc `kw = 1` and the flag is always 0, except DataESK where `kw = 0`.)

Then the record body:

1. **Shape.** If `D0.max == 0` the record is a block member. Otherwise, if `S0.max == 0` it is direct.
   Otherwise `read(1)`: 1 = direct, 0 = block member.
2. **Direct**: `offset` (D3), `clen` (D2), `ulen` (D1), `ntok` (D4), in that order; `start = 0`.
3. **Block member**: `e = read(S0.w)`; `e` must be `< S0.max`. Then, in a *separate* reader positioned
   at block entry `e`'s bit offset (the main reader does not move), read the block entry:
   `offset` (S3), `clen` (S2), `ulen_base` (S1), `ulen_w = read(5)`, `ntok_base` (S4),
   `ntok_w = read(5)`, `start_w = read(5)`.
   Back in the main reader: `ulen = ulen_base + read(ulen_w)`, `ntok = ntok_base + read(ntok_w)`,
   `start = read(start_w)`.

Every token emits at least one byte (§8), so `ntok ≤ ulen` for every record; a decoder may reject an
index entry that violates it (it bounds the work a corrupt index can cause). Confirmed for all 634,248 records.

**Enumerating** all records: for each sparse entry, seek to its bit offset, and read its records as
above. **Looking up** key `k`: binary-search the sparse table for the last entry with `first_key ≤ k`
(fail if `k` exceeds the last entry's key and the last entry is not `k`); walk that group until the
running key is `≥ k`; it is found only if equal.

### 3.4 Observed index values

| file | N | direct | block entries M | blocks | kw |
|---|---|---|---|---|---|
| ContDlx | 69,949 | 26 | 955 | 981 | 1 |
| ContStd | 44,055 | 42 | 522 | 564 | 1 |
| DataDlx | 175,793 | 1 | 505 | 506 | 1 |
| DataStd | 121,055 | 1 | 447 | 448 | 1 |
| DataFDlx | 28,112 | 106 | 740 | 846 | 1 |
| DataFStd | 152,433 | 425 | 2,831 | 3,256 | 1 |
| RelDlx | 970 | 2 | 14 | 16 | 1 |
| RelStd | 34,606 | 485 | 981 | 1,466 | 1 |
| DataESK (Encarta and Kids copies are identical) | 1 | 1 | 0 | 1 | 0 |
| ContKDC | 637 | 0 | 12 | 12 | 1 |
| DataKDC | 5,874 | 0 | 24 | 24 | 1 |
| DataFKDC | 100 | 0 | 1 | 1 | 1 |
| RelKDC | 663 | 0 | 3 | 3 | 1 |

(`blocks` = distinct compressed blocks = direct + M; confirmed.)

## 4. Codec overview

Each package has one **shared dictionary** (decoded once when the package is opened) and each
compressed block starts with its own **block dictionary**, followed by the block's **token stream**.

- A dictionary is a list of **words**. A word is a byte string plus a 4-bit **kind** that tells the
  XML emitter how to render it (plain word, punctuation, `<tag`, attribute name, attribute value, end tag …).
- A token is a reference to one word of either the shared or the block dictionary.
  The token stream is Huffman coded over a 320-symbol alphabet: 32 symbols select a block-dictionary
  word class, 32 select a shared-dictionary word class, and 256 are LZ77-style **matches** that repeat
  earlier *tokens* (not bytes).
- The **emitter** turns the token sequence into XML text: it inserts spaces between words, writes the
  `>` of a start tag lazily, tracks open elements so that an end-tag token needs no name, and
  capitalises sentence starts.

### 4.1 Word classes (fixed table)

Word numbers within a dictionary are grouped into 32 **classes**. A token names a class and then
`extra(c)` bits select a word inside it. The table is fixed (it is not stored in the file):

| class `c` | 0 … 11 | 12 | 13 | 14 | … | 31 |
|---|---|---|---|---|---|---|
| `extra(c)` | 0 | 1 | 2 | 3 | … | 20 |
| `first(c)` | `c` | 12 | 14 | 18 | … | 1,048,586 |

i.e. `extra(c) = 0` for `c < 12`, `extra(c) = c − 11` for `c ≥ 12`, `first(0) = 0`,
`first(c+1) = first(c) + 2^extra(c)`. Full `first` list:
`0,1,2,3,4,5,6,7,8,9,10,11,12,14,18,26,42,74,138,266,522,1034,2058,4106,8202,16394,32778,65546,131082,262154,524298,1048586`
(and `first(32) = 2,097,162`). The original stores this as 21 pairs `(count, extra)`:
`(12,0),(1,1),(1,2),…,(1,20)`. Confirmed: with this table every dictionary and every token on the disc decodes.

The encoder puts frequent words in the small classes (shared word 0 of ContStd is `, `, 1 is `and`, 2 is `the`).

## 5. Huffman codes

### 5.1 Canonical code construction

Every Huffman code in the format is described only by a list of code lengths (0 = symbol unused,
otherwise 1 … 31). Codes are **canonical**, the same convention as Deflate (RFC 1951 §3.2.2):
sort the used symbols by (length, symbol number); the first gets the all-zero code of its length; each
next symbol gets the previous code + 1, shifted left by the length difference. Bits are read MSB first
from the stream, so a code is matched bit by bit from its most significant bit.

Validity rules applied by the original:
- At most 2,048 symbols; lengths ≤ 31.
- If two or more symbols are used, the code must be **complete** (Kraft sum exactly 1);
  over- or under-subscribed codes are errors.
- If exactly **one** symbol is used, its length must be 1, and decoding it consumes **zero bits**.
- If **no** symbol is used, the code is accepted but must never be decoded from.

The original decodes with lookup tables of 10 bits (token alphabet), 8 bits (character alphabets),
5 bits (prefix alphabet) and 6 bits (pre-tree), with a bit-by-bit tree walk for longer codes. That is an
implementation detail: any correct canonical decoder works. Long codes really occur (lengths up to 30 in
ContStd's shared token code; unused-but-allowed symbols are typically given a long length rather than 0).

### 5.2 Code-length serialization (`read_lengths(n)`)

`n` code lengths are read in one of three modes:

**Mode "pre-tree"** — first bit `1`:
1. `a = read(6)`. If `a == 0`, all `n` lengths are 0 (done). `a > 32` is an error.
2. `b = read(4)`; `b ≥ 12` is an error.
3. Pre-tree code lengths: `a` values `read(3)` for pre-tree symbols `0 … a−1`, then `b` values `read(3)`
   for symbols `32 … 32+b−1`. All other of the 44 pre-tree symbols have length 0. Build the canonical pre-tree.
4. Decode pre-tree symbols until `n` lengths have been produced:
   - symbol `s < 32`: the next length is `s`;
   - symbol `s ≥ 32`: with `k = s − 32`, repeat the **previous length** `2^k + read(k)` times
     (error if no length has been produced yet, or if this overshoots `n`).

**Mode "sparse"** — first bits `0 1`:
1. Zero all lengths. `bw = bitlength(n − 1)` (number of bits needed to write `n − 1`; e.g. 64 → 6, 288 → 9, 320 → 9).
2. `count = read(bw) + 1` (error if `> n`). `L = read(5)`. `prev = none`.
3. Repeat until `count` symbols have been assigned: `s = read(bw)` (error if `s ≥ n`).
   If `s == prev`, increment `L` (error if it exceeds 31) and assign nothing — this does not count.
   Otherwise set `length[s] = L` (overwriting any earlier assignment to `s`; `L = 0` is accepted and leaves `s`
   unused) and count it. Then `prev = s`. The resulting lengths are then subject to the §5.1 validity rules.
   (So symbols are listed in increasing length; repeating the last symbol bumps the length.)

**Mode "flat"** — first bits `0 0`:
`w = read(3)` (error if `> 5`), then `n` values `read(w)`, each ≤ 31.

Observed on the disc (counting both identical DataESK copies): the pre-tree mode is used for 32,296 of the 32,300 serialized codes; the sparse mode
4 times (prefix codes in block dictionaries); the flat mode never.

## 6. Dictionaries

### 6.1 Header

Both the shared dictionary (its own bit stream at header offset 0x1C) and each block dictionary
(the start of every compressed block) begin with:

| field | shared dictionary | block dictionary |
|---|---|---|
| `mode = read(1)` | must be **1** | must be **0** |
| `C = read(1)` | capitalisation enable (emitter, §8) | — (not present) |
| `S = read(1)` | self-closing tags allowed (emitter, §8) | — (not present) |
| `A = num()` | sanity bound (see below) | number of word symbols (tokens `< 64`) in this block's token stream; **0 = this block has no dictionary**: the header ends here |
| `X = num()` | unused (always 0) | unused (always 0) |
| `wc = num()` | word count | word count |
| `total = num()` | total bytes of word text (kind bytes excluded) | same |
| `maxlen = num()` | longest word in bytes **including** its kind byte | same |
| token code | `read_lengths(320)` | `reuse = read(1)`; if 1 reuse the shared token code, else `read_lengths(320)` |
| *(block only)* | | if `wc == 0`: the header ends here (no word list follows) |
| class sizes | see §6.2 | see §6.2 |
| prefix code | `read_lengths(64)` | `reuse` bit, then as above (reuses the shared prefix code) |
| even-position character code | `read_lengths(288)` | `reuse` bit, then as above |
| odd-position character code | `read_lengths(288)` | `reuse` bit, then as above |
| word list | §6.3 | §6.3 |

On this disc `C = 1` and `S = 1` in every shared dictionary. Every block on the disc has `A ≠ 0` and
`wc ≠ 0`, and the reuse bit is never set for the token code (it is set 256 times for the
prefix/even/odd codes, in DataFDlx, DataFStd, RelStd and RelKDC). A block whose `A == 0` keeps whatever token
code the decoder currently holds in the original implementation; it never occurs, so a decoder should
treat it as "use the shared token code and an empty block dictionary".

When the shared dictionary has `wc = 0` (DataFKDC, DataESK) only the class sizes and the word list are absent:
its prefix, even and odd codes are still serialized (blocks, by contrast, stop after the token code when `wc = 0`).

The shared dictionary's bit stream is consumed exactly (to within the last 16-bit word's padding) by this
parse; e.g. ContStd's 95,422-byte dictionary ends at bit 763,376 of 763,376.

`A` is checked by the original only as an upper bound on per-class fill (§6.3); in block dictionaries it
equals the number of literal (non-match) token symbols in the block, confirmed on sampled blocks. It is not
needed for decoding.

### 6.2 Class sizes

Only if `wc > 0`: for classes `c = 0, 1, 2, …` read `size[c] = num()` = total text bytes of the words in
class `c`, stopping after the first class with `first(c+1) ≥ wc` (that is the class containing the last word).
The sizes must sum to `total`. Class `c` holds word numbers `first(c) … min(first(c+1), wc) − 1`.

These sizes let a decoder lay out all word text in one buffer ordered by word number before the words
themselves arrive (the original does exactly that and stores each word as `(offset << 4) | kind`, with the
length implied by the next word's offset). A decoder that stores words individually can just read and
check them.

### 6.3 Word list

`wc` words follow, front-coded. Each word is built in a byte buffer whose **byte 0 is the kind** and whose
remaining bytes are the text.

For word `i = 0 … wc−1`:
1. **Shared prefix** `p`: 0 for the first word; otherwise decode a symbol `q` from the prefix code;
   `p = q` if `q < 32`, else with `k = q − 32`, `p = 2^k + 31 + read(k)`.
   `p` must not exceed the length of the previous word's buffer. Start the buffer with the first `p`
   bytes of the previous word's buffer (the kind byte counts as one of them).
2. **Characters**: repeatedly decode a symbol `t` from the **even** character code if the current buffer
   length is even, from the **odd** code if it is odd. If `t < 256`, append byte `t`. If `t ≥ 256`, the word
   ends and `t − 256` is its **class** (0 … 31). An empty buffer at that point is an error.
3. **Validate** the kind byte `buffer[0]` and the length `L` of the buffer:
   - kinds 0, 1, 2, 3, 4, 8: `L ≥ 2` (text must be non-empty);
   - kinds 5, 6, 7: any length (an empty attribute value is legal);
   - kind 9: `L = 3` and the two text bytes, read big-endian, are ≤ 255;
   - kind 10: `L = 1`, and only if the shared dictionary has `S = 1`;
   - any other kind byte: error.
4. **Place** the word: it becomes word number `first(class) + fill[class]`, then `fill[class]` += 1.
   Errors: the class is already full (`fill[class] ≥ min(first(c+1), wc) − first(c)`), `fill[class] ≥ A`, or the class's
   text bytes would exceed `size[class]`, or the running text total would exceed `total`, or the buffer
   would exceed `maxlen` bytes.
5. After all words: the text total must equal `total` (and so every class is exactly full).

### 6.4 Word kinds

| kind | name | text is | rendered as (§8) |
|---|---|---|---|
| 0 | word | a word | separating space if needed, the word; may be capitalised |
| 1 | raw | punctuation, whitespace, entity references, any glue | verbatim |
| 2 | raw + sentence end | like 1 (e.g. `. `, `). `) | verbatim, then arms capitalisation |
| 3 | start tag | element name | `<name` (the `>` comes later) |
| 4 | attribute name | attribute name | ` name` |
| 5 | bare value | value | `=value` |
| 6 | single-quoted value | value | `='value'` |
| 7 | double-quoted value | value | `="value"` |
| 8 | explicit end tag | element name | `</name>` |
| 9 | end tag | 2 bytes: big-endian `n` (0 … 255) | `</`*name of the element-stack entry n entries below the top*`>` (§8; an entry can stand for several nested same-name elements) |
| 10 | empty-element close | (none) | `/>` |

Kinds 5, 6 and 8 and kind 9 with `n > 0` never occur in token streams on this disc; implement them anyway.

## 7. Token stream

The token stream starts at the bit right after the block dictionary and runs to the end of the block
(the last 16-bit word is zero-padded; at most 15 padding bits, always zero on this disc).

Decode symbols `s` with the block's token code (§6.1) and append tokens to a flat token list until it holds
`start + ntok` tokens (for a whole block: the sum of its members' `ntok`):

- **`s < 32`: block-dictionary word.** `c = s`, `v = first(c) + read(extra(c))`; error if `v ≥` block `wc`.
  Append token "block word `v`".
- **`32 ≤ s < 64`: shared-dictionary word.** `c = s − 32`, `v = first(c) + read(extra(c))`;
  error if `v ≥` shared `wc`. Append token "shared word `v`".
- **`64 ≤ s < 320`: match.** `j = (s − 64) >> 4` (length code 0 … 15), `d = (s − 64) & 15` (distance bits).
  Length:

  | `j` | base length (before +2) | extra bits read |
  |---|---|---|
  | 0 … 7 | `j` | 0 |
  | 8 … 11 | `2j − 8 + read(1)` → 8 … 15 | 1 |
  | 12, 13, 14 | `k = j − 10` (2, 3, 4): `2^k + 12 + read(k)` → 16–19, 20–27, 28–43 | `k` |
  | 15 | `k = read(4)`, then `2^k + 43 + read(k)` → 44 … 65,578 | 4 + `k` |

  The match length is that value **+ 2** (so 2 … 65,580 tokens). Then the distance is
  `2^d + read(d)` (1 … 65,535). Read order: length extra bits first, then distance extra bits.
  Copy `length` tokens one at a time from `distance` tokens back (overlapping copies repeat, as in LZ77).
  If a match would carry past the number of tokens needed, the original stops at that count; tokens beyond
  it are never emitted, so truncating or copying the whole match gives the same output (it never happens here).
  Error if the distance reaches before the first token of the block.

Tokens of both dictionaries are mixed freely in one stream; a match copies tokens, whatever they refer to.
The original keeps the last 65,536 tokens in a ring buffer and decodes in chunks; a match never needs more
history than that, and with a flat list this does not matter. (Its chunking also makes a match that crosses
certain internal boundaries an error: from token 65,536 on, no match may cross a multiple of 4,096 tokens.
The encoder respects this — in the 161 streams longer than 65,536 tokens there are 1,316 matches crossing a
4,096 multiple below token 65,536 and none at or above it — so a flat decoder needs no such rule.)

Observed: direct records' token streams hold up to 504,146 tokens (RelStd); 64 KB shared blocks hold up to
about 50,000 tokens.

## 8. XML emitter

The emitter consumes the record's tokens `[start, start + ntok)` and appends bytes to the output.
State, reset at the start of **every record** (also for block members, whose first token is mid-block):

- `mode` ∈ { **neutral**, **after-word**, **in-start-tag** }; initially neutral.
- `cap_pending`: initially **true if the shared dictionary's `C = 1`**, else false.
- the **element stack**: entries `(word, count)`; initially empty. An entry records an element name (the
  identity of the start-tag *dictionary word*: which dictionary and which word number) and how many directly
  nested open elements of that same word it stands for.

`close_start_tag()` = if `mode == in-start-tag`, write `>`.

Per token, with the word's kind and text:

| kind | action |
|---|---|
| 0 | if in-start-tag write `>`; else if after-word write one space (0x20). Write text. If `cap_pending`: clear it, and **swap the ASCII case** of the first byte of the word's text (not the space or `>` written before it) (`a–z` → `A–Z`, `A–Z` → `a–z`, any other byte unchanged — `cap_pending` is cleared regardless). `mode = after-word`. |
| 1 | `close_start_tag()`; write text; `mode = neutral`. |
| 2 | as kind 1, then `cap_pending = true`. |
| 3 | `close_start_tag()`; write `<` + text; `mode = in-start-tag`. Push: if the top entry is the same start-tag word, increment its count, else push `(word, 1)`. |
| 4 | error unless in-start-tag. Write space + text. |
| 5 | error unless in-start-tag. Write `=` + text. |
| 6 | error unless in-start-tag. Write `='` + text + `'`. |
| 7 | error unless in-start-tag. Write `="` + text + `"`. |
| 8 | `close_start_tag()`; write `</` + text + `>`; `mode = neutral`. The stack is not touched. |
| 9 | `n` = the word's 2-byte value. Let `target` = (index of top entry) − `n`; error if negative or stack empty. **If** in-start-tag and `S == 0` and `n == 0`: write `/>`, decrement the top entry's count (dropping it at 0), `mode = neutral` (kind 10's action without its `S == 1` requirement). **Otherwise**: `close_start_tag()`; write `</` + name of entry `target` + `>`; discard every entry above `target`; decrement `target`'s count and drop it if it reaches 0; `mode = neutral`. |
| 10 | error unless `S == 1`, the stack is non-empty and in-start-tag. Write `/>`; decrement the top entry's count, dropping it at 0; `mode = neutral`. |

Consequences worth knowing:
- With `S = 1` (all files here) an element with no content that is closed by kind 9 comes out as
  `<x ...></x>` (e.g. `<inlinetitle></inlinetitle>` in CONT records), while kind 10 gives `<x .../>`.
- Kind-0 words get a leading space only after another kind-0 word. After raw glue, tags or values there is
  no automatic space; the glue words carry their own spaces (`, `, ` (`, `" `, `\r\n  `).
- Capitalisation is a *case swap*, so the encoder stores words in the case that swaps to the intended
  result: `Xml` after `<?` renders `xml`; `petrified` after `"?>\r\n` renders `Petrified`. A non-letter
  first byte (BOM, digit, UTF-8 lead byte) consumes `cap_pending` without change.
- The emitter does **no** escaping or entity processing: `&amp;`, `&lt;`, `&gt;` appear literally inside
  word texts, and named characters are emitted as markup (`<entity tag="hellip"/>`).
- If the record ends while in-start-tag, no `>` is written (never happens; all records end after an end tag
  or trailing whitespace).
- The output must reach exactly `ulen` bytes, and writing past `ulen` is an error.

## 9. Decoding a record end to end

```
open(file):
    header; if flags & 1: package is empty (no records)
    shared = parse_dictionary(bits(file[dict_off .. +dict_len]), shared_mode)
    records = enumerate_index(file[idx_off .. +idx_len])

decode(record):
    r = bits(file[record.offset .. +record.clen])          # clen is even
    block = parse_dictionary(r, block_mode, shared)
    tokens = decode_tokens(r, block.token_code or shared.token_code, record.start + record.ntok)
    out = emit(tokens[record.start .. record.start + record.ntok], shared.C, shared.S)
    assert len(out) == record.ulen
```

All members of a block see the same block dictionary and the same token prefix, so a bulk converter
should decode each block once (to the largest `start + ntok` of its members) and slice tokens per record;
the original re-decodes the block from its start for every record. Decoding the whole disc this way takes
about three minutes in an unoptimised Python prototype; the Rust decoder (§15) does it, with SHA-256 and an
XML parse of every record, in under a second on 24 threads (about 13 CPU-seconds).

## 10. Worked examples

### 10.1 Index: ContStd records 0 and 1

ContStd: index at 0x207A1B2, `N = 44,055`, `G = 1,377`, `M = 522` (u32 at index byte 11,028).
Bit area header at bit `32·(4 + 2·1377 + 522) = 104,960`. Header values:

| descriptor | base | w | max |
|---|---|---|---|
| `kw` | | 1 | |
| D0 count | 1 | 6 | 42 |
| D1 ulen | 132,321 | 19 | 545,732 |
| D2 clen | 30,828 | 17 | 132,646 |
| D3 offset | 31,791,298 | 22 | 33,921,932 |
| D4 ntok | 22,530 | 17 | 93,510 |
| D5 | 0 | 0 | 0 |
| S0 entry | 1 | 10 | 522 |
| S1 ulen base | 54 | 17 | 130,698 |
| S2 clen | 29,660 | 16 | 66,890 |
| S3 offset | 97,272 | 25 | 31,729,648 |
| S4 ntok base | 7 | 15 | 23,042 |
| S5 | 0 | 0 | 0 |

The header ends at bit 105,573, which is block entry 0's offset. Sparse entry 0 is `(0, 151509)`,
entry 1 is `(32, 152794)`.

Record 0, from bit 151,509 (both D0.max and S0.max are non-zero, so a shape bit is read):

| bit | read | bits | value | meaning |
|---|---|---|---|---|
| 151509 | 1 | `0` | 0 | block member |
| 151510 | 10 | `0011100111` | 231 | block entry 231 → bit offset 125,901 |
| 125901 | 25 | `0110111000101111111010000` | 14,442,448 | offset = 97,272 + 14,442,448 = **14,539,720** |
| 125926 | 16 | `1000100100100000` | 35,104 | clen = 29,660 + 35,104 = **64,764** |
| 125942 | 17 | `00001010000001110` | 5,134 | ulen_base = 54 + 5,134 = 5,188 |
| 125959 | 5 | `00111` | 7 | ulen_w |
| 125964 | 15 | `000001101000111` | 839 | ntok_base = 7 + 839 = 846 |
| 125979 | 5 | `01000` | 8 | ntok_w |
| 125984 | 5 | `10000` | 16 | start_w |
| 151520 | 7 | `0110110` | 54 | ulen = 5,188 + 54 = **5,242** |
| 151527 | 8 | `01101000` | 104 | ntok = 846 + 104 = **950** |
| 151535 | 16 | `1000110001111000` | 35,960 | start = **35,960** |

Record 1 continues at bit 151,551: key-delta flag `0` (key 1), shape `0`, entry `0000010100` = 20, …
→ offset 1,155,716, clen 57,210, ulen 814, ntok 162, start 16,331.

Zebra (ContStd key 22,865, catalog id 761575094): block entry 316, offset 20,011,212, clen 63,128,
ulen **16,503**, ntok 3,045, start 0.

### 10.2 Block dictionary header: DataFKDC record 37

DataFKDC (Kids, 100 records, one block at offset 340, clen 3,364). Record 37 has `start = 0`, `ntok = 45`,
`ulen = 280`. The block's first bytes are `c0 23 6e 08 22 5a a5 20`, i.e. words `0x23c0 0x086e 0x5a22 0x20a5`,
bits `0010001111000000 0000100001101110 0101101000100010 0010000010100101`:

| bit | read | value | field |
|---|---|---|---|
| 0 | 1 | 0 | block mode |
| 1 | 5 + 8 | 8, 240 | `A = 255 + 240 = 495` |
| 14 | 5 | 0 | `X = 0` |
| 19 | 5 + 8 | 8, 110 | `wc = 365` |
| 32 | 5 + 11 | 11, 546 | `total = 2047 + 546 = 2,593` |
| 48 | 5 + 4 | 4, 1 | `maxlen = 16` |
| 57 | 1 | 0 | token code not reused |
| 58 | 1 | 1 | pre-tree mode |
| 59 | 6, 4 | 11, 6 | `a = 11`, `b = 6` |

Pre-tree lengths for symbols 0…10: `3 0 0 6 5 5 4 4 3 3 3`; for symbols 32…37: `4 4 5 3 0 6`.
Canonical pre-tree codes: 0=`000`, 8=`001`, 9=`010`, 10=`011`, 35=`100`, 6=`1010`, 7=`1011`,
32=`1100`, 33=`1101`, 4=`11100`, 5=`11101`, 34=`11110`, 3=`111110`, 37=`111111`.
The first pre-tree symbols are 6, 32 (repeat ×1), 7, 8, 35 (`k = 3`, extra `001` → repeat ×9), 7, 6, 5, …
giving token-code lengths `6 6 7 8 8 8 8 8 8 8 8 8 8 7 6 5 5 4 3 4 0 …` (symbols 20–65 unused,
66–75 = `9 5 6 3 6 7 7 7 7 7`). Resulting canonical codes include: symbol 16 (block class 16) = `01111`,
14 = `100110`, 13 = `1011001`, 4 = `11011011`, 5 = `11011100`, 6 = `11011101`, 12 = `11100011`,
67 (match, `j = 0`, `d = 3`) = `10000`.

Class sizes (20 classes, since `first(20) = 522 ≥ 365`): `3 2 3 2 5 5 2 7 3 2 5 3 15 26 51 97 191 409 991 771` (sum 2,593).
The block's first words: 0 `–` (kind 0, UTF-8 en dash), 1 `of`, 2 `and`, 3 ` (` (kind 1), 4 `refid` (kind 4),
5 `group` (kind 4), 6 = end tag n = 0 (kind 9, text `00 00`), 7 `picture`. The token stream begins at bit 13,357.

### 10.3 Tokens and emitted text: DataFKDC record 37

| # | symbol (code) | extra | word (kind, text) | output |
|---|---|---|---|---|
| 0 | 16 (`01111`) | 5 bits = 20 | block 62: (0, `EF BB BF`) | `EF BB BF` (cap_pending cleared, unchanged) |
| 1 | 16 | 23 | block 65: (2, `<?`) | `<?` (arms capitalisation) |
| 2 | 16 | 7 | block 49: (0, `Xml`) | `xml` (case swapped) |
| 3 | 16 | 19 | block 61: (0, `version`) | ` version` |
| 4 | 13 (`1011001`) | 2 bits = 3 | block 17: (1, `="`) | `="` |
| 5 | 16 | 0 | block 42: (0, `1.0`) | `1.0` |
| 6 | 16 | 21 | block 63: (1, `" `) | `" ` |
| 7 | 16 | 9 | block 51: (0, `encoding`) | `encoding` |
| 8 | 13 | 3 | block 17: (1, `="`) | `="` |
| 9 | 16 | 6 | block 48: (0, `UTF-8`) | `UTF-8` |
| 10 | 16 | 22 | block 64: (2, `"?>\r\n`) | `"?>\r\n` (arms) |
| 11 | 16 | 24 | block 66: (3, `data`) | `<data` |
| 12 | 4 (`11011011`) | — | block 4: (4, `refid`) | ` refid` |
| 13 | 16 | 27 | block 69: (7, `210117893`) | `="210117893"` |
| 14 | 5 | — | block 5: (4, `group`) | ` group` |
| 15 | 16 | 30 | block 72: (7, `indx`) | `="indx"` |
| 16 | 14 (`100110`) | 3 bits = 4 | block 22: (1, `\r\n  `) | `>\r\n  ` |
| 17–20 | | | `<tree` ` ed` `="KDC"` `\r\n    ` | `<tree ed="KDC">\r\n    ` |
| 21–23 | | | (3 `entry`), (1 `\r\n      `), (3 `title`) | `<entry>\r\n      <title` |
| 24 | 16 | 17 | block 59: (0, `petrified`) | `>Petrified` (case swapped) |
| 25 | 14 | 1 | block 19: (0, `Forest`) | ` Forest` |
| 26 | 6 (`11011101`) | — | block 6: (9, n = 0) | `</title>` |

The first match symbol of this block is the 37th symbol (67: length 0 + 2 = 2, distance `2^3 + read(3)` = 10).
The complete record (280 bytes, SHA-256 `ed2d8093…e529a8`, see §11) is:

```
\xEF\xBB\xBF<?xml version="1.0" encoding="UTF-8"?>\r\n<data refid="210117893" group="indx">\r\n
  <tree ed="KDC">\r\n    <entry>\r\n      <title>Petrified Forest</title>\r\n
      <entry refid="701766968" group="media">\r\n        <title>picture</title>\r\n      </entry>\r\n
    </entry>\r\n  </tree>\r\n</data>
```
(line breaks added here after each `\r\n` for readability).

### 10.4 Shared dictionary: ContStd

Bits 0–2 = `1 1 1` (shared mode, `C = 1`, `S = 1`); `A`: `w = 23`, value 4,695,527 → 13,084,134; `X = 0`;
`wc`: `w = 15`, 2,743 → **35,510**; `total`: `w = 17`, 131,070 → **262,141**; `maxlen`: `w = 4`, 15 → 30.
Token code (pre-tree mode) starts at bit 87; its first lengths are
`10 10 10 10 10 10 10 11 11 12 12 12 11 10 10 9 9 9 9 8 8 8 8 8 8 9 12 30 30 30 30 30` (block classes 0–31),
`6 6 6 6 6 6 7 7 7 8 8 8 7 6 6 5 5 5 5 4 4 4 4 4 4 5 8 30 30 30 30 30` (shared classes 0–31), `17 11 9 7 …` (matches).
The class sizes are `2 3 3 2 2 2 2 1 2 4 3 3 4 13 19 61 130 313 689 1501 3133 6620 13976 29049 60771 124446 21387`.
Shared words 0–12: `, ` (1), `and`, `the`, `of`, `. ` (2), `in`, `to`, `a`, ` (` (1), `that`, `was`, `for`, `by`;
word 40 is (3, `sectiontitle`), word 41 is the kind-9 end tag `00 00`.

## 11. Self-check values

SHA-256 of the decoded bytes of single records (key = local record number):

| file | key | what | ulen | SHA-256 |
|---|---|---|---|---|
| ContStd | 22865 | Zebra (761575094) | 16,503 | `c91922f87c3114ce1412e70a3a50bec62212965ee7ac412305a12cb0941b182f` |
| ContStd | 11333 | 761561691 | 91,875 | `f4a57313898db964ae487a41a37a9f27331f22d8a336216a8983c2436a4b94e0` |
| ContStd | 0 | | 5,242 | `004d72a2ea74ff044569fdcad654c9af4dc306d5858b5995c82fc4062c049174` |
| ContDlx | 0 | | 16,190 | `12977be1c348e73fa4933b1df0c5612879a4b357f03ba4907ff0cae038824223` |
| DataStd | 81969 | Zebra metadata | 2,567 | `7b43f187e62a5d7275d73db710f956a19929d6203d0b0e414f7c71aa5a6cad37` |
| DataStd | 70437 | 761561691 metadata | 10,841 | `282238a956f34f8671a966f39d8dfc6da2a11d28c6e6fdb4ad22c76225c40c6e` |
| RelStd | 18691 | Zebra related articles | 26,655 | `60ad29310d30ea1ff0059a54c71e4660ce2d22fdd87f744543fc52fdce42e03f` |
| RelStd | 8667 | 761561691 related | 92,461 | `bbd282210fb6cc0f92bb73064eca03282a9d14e5e93209ea526310a9249e7743` |
| DataFStd | 0 | index-term entry | 2,381 | `e95d937e42ff278de4280894ea882883dfb98ba54b4aee31a60d0431d954f3bb` |
| DataESK | 0 | expert list (only record) | 902,785 | `c4a945de759a8dfaef241f8478553ba4cd32a478a089fdd649994befca99a44e` |
| DataFKDC | 37 | §10.3 | 280 | `ed2d8093977c7d2f07a19f762725e40553776ddc57cf894113ff883726e529a8` |
| ContKDC | 0 | | 4,926 | `f843ec5428d823c20e161d2cbf5e107673d772d393d0eae53236ccbfd344361e` |

Whole-file checks. **Aggregate** = SHA-256 over the concatenation of the 32-byte binary SHA-256 digests of
every record's decoded bytes, in key order. `bytes` = sum of all decoded lengths (= sum of index `ulen`).

| file | records | bytes | largest record | aggregate |
|---|---|---|---|---|
| ContDlx | 69,949 | 244,017,253 | 241,389 | `46f58a8660269878e62f9b22f52a044c2ced20c96357262de54b592935806dbc` |
| ContStd | 44,055 | 139,646,112 | 545,732 | `a0d91a6e88244d522e9e8500508e80c0df3ad80b1204d7b8b9c59648ac68929b` |
| DataDlx | 175,793 | 132,029,781 | 109,432 | `3e90d97ed830468f2912e62dfa2bc442d5a1d39579be0cb92465c697726874b6` |
| DataStd | 121,055 | 116,438,663 | 70,635 | `ccdae14250dcdc6269314f88cfc2ee9c73d6c6b76b06ffc6cc93c332fabc1a13` |
| DataFDlx | 28,112 | 207,266,022 | 997,508 | `d6097a546250f4172b818dfa656aef683f85adf4afc131395bcb6e32d136928c` |
| DataFStd | 152,433 | 841,228,341 | 2,668,109 | `9a70dcfb2cf4d239bb844eb2cda46bd96bd6cde1b8e67cefe7d74ae8e19ab51d` |
| RelDlx | 970 | 3,824,215 | 168,613 | `290e8aab2def2d9effc1062f932e1faa93f5ff189795676d27f95b0b58e8285b` |
| RelStd | 34,606 | 440,840,392 | 4,193,022 | `07a64ed293545751826eaf4f8a0c62aa818dc40f772c7c7f9a6d2610a3e5f3a3` |
| DataESK | 1 | 902,785 | 902,785 | `69b74216c3a297b38295174103c65d02740c7c2b5f93c635414757ded17d09cb` |
| ContKDC | 637 | 2,903,551 | 9,722 | `6c2fafc9218c3dbccc956bab55f5811ad9a7b26bca515881e8029662d42d8f2e` |
| DataKDC | 5,874 | 6,071,915 | 13,998 | `626a950aee1c2e52719157d0066b8bfe30b5823dceba330fa23fecc3c6d6930a` |
| DataFKDC | 100 | 38,180 | 980 | `e54b2a7396ecb753be822387454d997754f44b71a5272d972ad38e7ec49fa972` |
| RelKDC | 663 | 630,171 | 3,282 | `8850a5cb66da744d12c97b18c8ae3ad8ab5a3b032fb5b86906a695d293446437` |

Every record of every file listed is well-formed XML (checked with expat) and has exactly its index `ulen`.
(The Kids `DATAESK.AKC` is byte-identical to the Encarta one.)

## 12. What the decoded XML looks like

Text encoding is **UTF-8** throughout (e.g. `’` = `E2 80 99`, no-break space = `C2 A0`), with CRLF line
breaks inside records. Some records start with a UTF-8 BOM and an XML declaration; the codec reproduces
them as ordinary words.

| package | root element | prolog | main structure |
|---|---|---|---|
| CONT (Std, Dlx, KDC) | `<content refid revision>` | none | `<text refid xml:space="preserve">` → `<section id type>` → `<sectiontitle>`, `<pkey id>` paragraphs, `<sec>` (roman section number); sub-heads `<seca>`/`<secb>`/`<secc>`; inline `<xref type RefID [paraID]>`, `<i>`, `<b>`, `<smallcaps>`, `<sub>`, `<sup>`, `<list type>`/`<listitem>`, `<inlinetitle>` (empty: the title is inserted by the UI), `<inlinebmp type id>`, `<fs type>`, `<br>`, `<quote type>`. ContDlx adds Deluxe-only article types: `<headline type id>`, `<intro id>`, `<author id>`, `<rule>`, `<xref type URL>` (web links). Trailing CRLF after `</content>`. |
| DATA (Std, Dlx, KDC) | `<data refid group …>` | none | per-item metadata: `group` (`article`, `media`, …), `<title>`, `<preview>` (first paragraph), `<forward>`/`<reverse>` with `<assoc refid group sec sequence>`, `<outline>` (`<pkey>`, `<sec type id title>`), media `<credit>`, `<caption>`, `<files>` (`<ticon>`, `<picon>`, `<thumb>`, `<image>` with `x`,`y` and `msencdata::baggage/<file>` paths) |
| DATAF (Std, Dlx, KDC) | `<data refid group="indx">` | BOM + `<?xml version="1.0" encoding="UTF-8"?>` | index terms: `<tree ed="STE|STC|DLX|KDC">` → nested `<entry [refid group pn paragraphid]>` with `<title>` |
| REL (Std, Dlx, KDC) | `<relarts refid>` | BOM + declaration | `<tree ed>` → nested `<relart [nodeid] [refid group pkey]>` with `<title>`; `<entity tag>` for special characters |
| DataESK | `<list Classname="expert">` | none | `<expert refid deluxe standard>` → `<display_name>`, `<credentials>`, `<expert_type>`, `<expert_information>`, `<adjective>`, `<area>` |

`refid` on the root is the catalog content id of the record (Zebra's ContStd record has `refid="761575094"`),
which cross-validates the catalog mapping. Confirmed for every entry of `content.ecn`, `data.ecn` and `relart.ecn`:
all 626,973 Deluxe and 7,274 Kids entries land on a record whose root `refid` is the entry's id, and every
record of every non-stub package except DataESK (reached by the string key `list/expert.xml`) is catalogued exactly once. Element and attribute frequencies per package are in the
prototype's output; the converter should take the authoritative tag list from a full Rust decode.

## 13. Edge cases and errors

- **Stub packages** (`flags & 1`): offsets and lengths are 0. The original simply fails to open them.
  Treat as a package with zero records.
- **Odd lengths**: a compressed block, the shared dictionary or the index with an odd byte length is rejected.
- **Reading past the end of a stream**: never happens with valid data (streams end within the final 16-bit
  word). Treat it as corruption.
- **Direct vs member**: a direct record is simply a block with one member (`start = 0`, `ntok` = all tokens).
- **Blocks without a dictionary** (`A = 0`) and **dictionaries without words** (`wc = 0`): legal, never seen
  in blocks. The shared dictionaries of DataFKDC and DataESK have `wc = 0` (all their words are per block),
  but still carry full code tables.
- **Code-table reuse**: a block may reuse each of the shared token, prefix, even and odd codes by a 1 bit
  (§6.1). Reusing means using the shared dictionary's decoded code as-is.
- **Single-symbol codes** consume zero bits per symbol (§5.1).
- **Unicode/code pages**: the codec is byte-oriented and never converts text; words are UTF-8 fragments and
  a multi-byte character may in principle be split across words. Capitalisation only touches ASCII letters.
- **Key lookup failure**: a key that is not present (gap, beyond the last key) is "not found".
- **No checksums or encryption** anywhere.

## 14. Provenance and open points

Ghidra addresses in `MSENCDAT.DLL` (for re-checking only):
package open `FUN_45487560`; shared-dictionary load `FUN_4549eb60`; index attach `FUN_4549ec80`;
index lookup `FUN_4549c970` (descriptor reader `FUN_4549c8a0`/`FUN_4549c930`, bit seek `FUN_4549c6c0`);
record decode `FUN_4549ee70`; dictionary parse `FUN_4549da30`; code-length reader `FUN_4549d290`;
canonical table builder `FUN_4549d030`; `num()` `FUN_4549d870`; token decoder `FUN_4549e220`/`FUN_4549e560`;
emitter `FUN_4549e770`; word-class table at VA `0x454AE0B0` (21 × `(count, extra)` u32 pairs).

Open points (none block decoding):
- The meaning of the shared dictionary's `A` (13,084,134 in ContStd; not the sum of the blocks' `A`) and of
  `X` (always 0) is unknown; neither affects output.
- Kinds 5, 6, 8, kind 9 with `n > 0`, the sparse code-length mode's corner cases and the flat mode are
  specified from the reader's behaviour but are not exercised by this disc (except 4 sparse codes).
- `<inlinetitle>` is empty in the records and is filled by the app: the reference VM shows the article
  title there in bold (Zebra 761575094 starts with "**Zebra**," then the text). The converter fills it
  from the item title.

## 15. Implementation

Module `crates/encarta-formats/src/akc/` (`encarta_formats::akc`):

- `AkcFile::open(path)` memory-maps a package, parses the header and the whole index, and decodes the shared
  dictionary once. `records()` (key order, with block, `start`, `ntok`, `ulen`), `blocks()` (offset, length,
  members in token order), `record_info(key)`.
- `get(key)` / `record(key)` decode one record through a small cache of recently decoded blocks (members of one
  block share its dictionary and token prefix). `decode_block` + `render` give explicit control;
  `for_each_record` decodes everything in parallel over blocks (rayon), each block once; `iter()` is the
  sequential equivalent in block order. `xml_text(bytes)` strips the UTF-8 BOM and validates UTF-8.
- `Library::open(extracted, Edition::Deluxe | Edition::Kids)` joins the catalog (`catalog` module) to the AKC
  files: `content(id)`, `data(id)`, `related(id)`, `get(id, Table)`, `get_key("list/expert.xml")`.
- `check::check_file` decodes and verifies a whole package: exact lengths, member tiling of every token stream,
  zero padding (< 16 bits) after it, UTF-8, XML well-formedness (quick-xml plus root/attribute/reference checks),
  per-record SHA-256 and the §11 aggregate; `check::compare_with_spec` holds the §11 tables.
- Unit tests build a small package from scratch (all three code-length modes, reuse bits, an A = 0 block, an
  overlapping match, kind 9/10 rules) and flip every bit of it to check that corruption fails cleanly.
  `tests/akc.rs` asserts §3.4, §10 and every §11 value on the user's disc and the catalog `refid` cross-check
  (skipped without `extracted/`; run with `--release`).

CLI `akc` (`cargo run --release --bin akc -- …`; `--extracted DIR` defaults to `$ENCARTA_EXTRACTED`, then `extracted`):

```
akc info <file>                          header, shared dictionary, index descriptors
akc get <file> <local>                   one record's XML (raw bytes) to stdout
akc id <refId> [--kind content|data|relart] [--kids]   resolve via the catalog, print the XML
akc key <key> [--kids]                   string-keyed AKC record (list/expert.xml)
akc dump <file> <outdir>                 one <key>.xml per record
akc check [--no-xml] <file>... | --all   verify everything; --all also cross-checks both catalogs
```

`akc check --all` result: all 22 files (8 stubs), 634,249 records / 2,136,740,166 bytes, zero errors, every §11
record hash and all 13 aggregates match, 0 root-`refid` mismatches; 0.7 s wall.
