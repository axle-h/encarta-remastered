# Megalith lexicons: `*.LEX`

Status: **fully decoded**. For all 12 files on the disc the decoded word count equals the header's word count, every
byte of the node stream belongs to exactly one node, and every symbol and shared-node table entry is used.
Source: observation only (hex dumps, trial parsing, and comparison with the headwords in `ENG_SPA.EIT`).
Reader: `encarta_formats::lex`. Exploration script: `tools/explore-py/lex.py`.

All integers are little-endian unless stated otherwise.

## Files

| file | LCID(s) | words | used by (from the `<LexFile>` of each dictionary's `$_EDictSignature2007_$`) |
|---|---|---|---|
| `EE/ENCARTA/EESPLA01.LEX` | 0x409 | 331,198 | speller for search ("did you mean") |
| `EE/KIDS/EESPLA01.LEX` | 0x409 | 104,370 | Kids speller (a different, smaller list) |
| `EE/ENCARTA/EDICT/DICTA.LEX` | 0x409 | 159,530 | English dictionary (`DICTA.EIT`) |
| `EDICT/THESA.LEX` | 0x409 | 38,940 | thesaurus (`THESA.EIT`) |
| `EDICT/BILAD.LEX` / `BILAE` / `BILAF` / `BILAI` | 0x409 (BILAF: 0x409, 0x809, 0x1009, 0xC09) | 19,548 / 15,236 / 14,627 / 15,437 | English→German/Spanish/French/Italian (`ENG_GER` … `ENG_ITA.EIT`) |
| `EDICT/BILDA.LEX` / `BILEA` / `BILFA` / `BILIA` | 0x407 / 0xC0A / 0x40C / 0x410 | 30,507 / 21,516 / 16,339 / 15,997 | German/Spanish/French/Italian→English |

Each lexicon is a word list, case-sensitive, and stored as a byte-coded **DAWG** (a trie whose identical subtrees are
shared). It holds no per-word data, only membership. The bilingual lists are the single-word tokens of the
headwords (e.g. BILAE holds `Ecuadorean`, `king-size`, `B&B`). The speller lists include proper nouns and digits.

## File layout

```
0x00  u32   magic 0x779FF320            (bytes 20 F3 9F 77)
0x04  u16   lexicon stamp (0x20E, 0x208, 0x2C8, 0x2CF, 0x2D8, 0x388; varies by file). Opaque: `MSSPELL3.DLL` copies the
            u32 at 0x04 (this value plus the version) back to the caller of `SpellerOpenLex` and uses it for nothing else
0x06  u16   0x0100                      (probably a format version)
0x08  u32   64
0x0C  8×u16 LCID list, ended by a 0 u16, the rest filled with 0xCCCC
0x1C  u32   n                           (number of u16 units in the next field)
0x20  n×u16 copyright string, UTF-16, NUL-terminated: "copyright2000 Megalith Corporation" (8 files),
            "Copyright 2000 Megalith Corporation" (BILEA, BILIA) or "copyright 2005 Microsoft
            Corporation" (BILAF, BILFA); the last u16 of the n is not part of the
            string: it is 0x002D ('-') in most files and 0 in BILEA and BILIA (meaning unknown;
            perhaps the word-joining character)
        u32   0                         (length of an older, table-based block; 0 here, so the body follows at once)
body: (offsets below are relative to the body start = 0x20 + 2n + 4)
+0x00 u32   1
+0x04 u32   0
+0x08 u16   longest word, in characters (checked: equals the decoded maximum)
+0x0A u16   most arcs in one node (checked: equals the decoded maximum)
+0x0C u16   2   length in u16 units of the symbol-code descriptor at +0x30 (read by MSSPELL3.DLL)
+0x0E u16   0   length of an empty region that follows it (also read)
+0x10 u16   2   length of the shared-node-code descriptor at +0x34
+0x12 u16   2 or 1   length of the skip-code descriptor at +0x38 (1: four bytes either way after 4-byte alignment)
+0x14 u32   word count                  (checked: equals the decoded count)
+0x18 u32   nSkip   entries in the skip table
+0x1C u32   nSym    entries in the symbol table
+0x20 u32   0
+0x24 u32   nNode   entries in the shared-node table
+0x28 u32   stream offset, relative to the body = 0x3C + 4 × (nSym + nNode + nSkip)
+0x2C u32   stream length; the stream runs to the end of the file
+0x30 4 B   code descriptor for symbol codes
+0x34 4 B   code descriptor for shared-node codes
+0x38 4 B   code descriptor for skip codes
+0x3C       symbol table:      nSym  × { u16 char (UTF-16 unit), u16 flags }
            shared-node table: nNode × u32 stream offset of a node
            skip table:        nSkip × u32 byte distance
            node stream
```

Note the table order in the file (symbols, nodes, skips) differs from the order of the counts in the header.

### Code descriptors and table codes

A **code** is a 1- or 2-byte index into one of the three tables. Its descriptor is `u16 direct, u8 0, u8 escapes`, with
`direct + escapes = 256` (a table with at most 256 entries has `direct = 256, escapes = 0`). Reading a code:

```
b = next byte
if b < direct: index = b
else:          index = direct + (b - direct) * 256 + next byte
```

`direct + escapes × 256` is always at least the table size. The tables are ordered by frequency, so frequent entries get
1-byte codes. Example: EESPLA01 symbol codes are `(253, 3)`, so `FD 60` is symbol 349.

## Node stream

The root node is at stream offset 0. A **node** is a run of arcs, and its last arc carries flag 0x002. Nodes sit back to
back over the whole stream, so a linear scan from offset 0 also finds every node. An **arc** is:

```
symbol code                        → (char, flags) from the symbol table
[node code]       if flags & 0x020 → continuation node (see below)
then, if flags & 0x010 (arc has a child), exactly one of:
  flags & 0x040 : nothing; the child node starts right after the end of this node
  flags & 0x080 : node code    → child = shared-node table[index]
  flags & 0x100 : 3 bytes, big-endian → child = that stream offset
  none of these : skip code    → child = cursor + skip table[index]   (see below)
```

Flag bits (only these 25 combinations occur: 0x1, 0x3, 0x10–0x13, 0x23, 0x32, 0x33, 0x52, 0x53, 0x72, 0x73, 0x90–0x93,
0xB2, 0xB3, 0x110–0x113, 0x132, 0x133):

| bit | meaning |
|---|---|
| 0x001 | a word ends at this arc (the prefix plus this char is in the list) |
| 0x002 | last arc of the node |
| 0x010 | the arc has a child node |
| 0x020 | **continuation**: after this node's arcs, the arcs of the node named by the node code are also siblings at this level (shared tail of a sibling list). Seen only together with 0x002. The node code comes straight after the symbol code, before any child field |
| 0x040 | child is placed right after this node (seen only on a last arc) |
| 0x080 | child is a shared node, given by a node code |
| 0x100 | child is given by an absolute 24-bit big-endian stream offset |

**Skip-coded children (cursor).** Children that are not shared are laid out after their parent node in arc order. The
child of the 0x040 arc comes first, right after the node. Each skip value is the distance from the previous skip-coded
child of the same node. For the first skip-coded arc of a node, it is the distance from the byte just after that arc's
skip code. So, per node:

```
cursor = none
for each arc with a skip code:
    if cursor is none: cursor = position just after this skip code
    cursor += skip[index]; child = cursor
```

Worked example (BILAE root, offsets in the stream): `1B BC` = symbol 27 `('a', 0x10)` with skip code 188 → skip 305.
The cursor starts at 2, so the child of `a` is at 307. The node there has arcs l, x, c, d, e … (`al…`, `ax…`, `ac…`). The
next arc is `FF 22 EE` = symbol 544 `('B', 0x10)` with skip code 238 → 2374, so the child of `B` is at 307 + 2374 = 2681.
The root's last arc `FE 34` = symbol 306 `('A', 0x52)` has its child right after the root, at 129.
In the A node, `FE FA FF E8` = `('G', 0x90)` with node code 487 → stream offset 2834. The node there is the single arc
`FF 43` = `('M', 0x03)`, which ends the word "AGM". `ATM` uses the same shared node.

**Arc order** within a node is the encoder's own and is not alphabetical (the BILAE root runs a, B … Z, z, b … y, A).
A sorted word list has to be sorted after decoding.

### Decoding the word list

Depth-first from the root with an empty prefix. For each node, collect its arcs and append the arcs of any
continuation nodes, recursively. For each arc, `word = prefix + char`. Emit the word if 0x001 is set, and descend into
the child if 0x010 is set. Because subtrees are shared, words are produced by walking paths, not by visiting nodes once.

### Validation (all 12 files)

- decoded word count = header word count, and all words are distinct;
- longest word = header +0x08; most arcs in a node = header +0x0A;
- the linear node scan covers the stream exactly (no gaps or overlaps), and every child or continuation offset lands on a
  node start;
- every symbol and shared-node table entry is referenced (DICTA uses 649 of its 655 skip entries);
- BILAE: 15,235 of 15,236 words are tokens of the `ENG_SPA.EIT` headwords (the exception, `B&B`, is written there as
  `B<entity…>B`).

### Unknowns (no effect on the word list)

**Static + live (`MSSPELL3.DLL` inside `ENCARTA.EXE`, 2026-10-15).** The magic `0x779FF320` occurs in no other binary on
the disc. The app loads the speller lazily: the first search that runs calls `SpellerInit`, then `SpellerOpenLex` (path of
`EESplA01.lex` in the install folder, language 0xFFFF = any, record type 4 = this format), which maps the whole file and
succeeds, filling the out record with the open lexicon, the u32 at 0x04 (`0x0100020E`) and the LCID 0x409. Afterwards
`SpellerCheck` is called once per word. Of the *.lex files only EESPLA01 was opened (the dictionary files were not touched
while searching).

The earlier reading that the loader rejects our files was wrong. After the copyright string (`0x20 + 2n`) the open routine
reads a u32 length. Zero is not a failure: it means there is no older table-based block, and the loader parses the
body directly at that point (the decompiler had split this tail into a separate function, which hid it). The routine
that parses the body (`FUN_3f005561`) requires the first u16 to be below 2 (we have 1) and then reads the u16s at body
+0x0C, +0x0E, +0x10 and +0x12 as the lengths of the four small regions that start at +0x30 (each aligned to 4 bytes),
and the u32s at +0x14 (stream offset), +0x1C, +0x20 and +0x24 as the counts of the symbol, 0-sized and shared-node
tables that follow. That is exactly our layout: the three "code descriptors" at +0x30/+0x34/+0x38 are the regions whose
sizes sit at +0x0C..+0x12. Data breakpoints on the mapped file confirmed: +0x0C and +0x10 (as dwords) are read only inside
this parse, during open; the u32 at 0x04 is read once, to hand back to the caller. Nothing read them again during
spelling checks, and nothing read the copyright's trailing u16 (the caller passed no name buffer, so the copyright is not
even copied).

Still opaque: the header stamp at 0x04 (returned to the caller, not used by the speller), the trailing u16 of the
copyright field (0x2D/0; never read in the search path), and the body u32 at +0x04 and +0x20 (zero). The lexicon carries no
affix or frequency data that we could find. Suggestion behaviour (edit distance and ranking in MSSPELL3.DLL) is not in
the file. Our rebuild replaces it (SymSpell or an `fst` over these word lists).
