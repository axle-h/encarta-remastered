# MindMaze question bank: `MINDMAZE.DB` / `MINDMAZE.IDX`

Status: **fully decoded** (structure and every record). One field is interpreted by hypothesis only (the
answer "location" word, see below). Source: observation of the two files only, plus cross-checks of the
ids against `catalog2/content.ecn` and the PFX titles. Reader: `encarta_formats::mindmaze`.
Dump: `smallfmt mindmaze <MINDMAZE.DB> <MINDMAZE.IDX>`.

Location: `EE/ENCARTA/MINDMAZE.DB` (1,468,275 B) and `MINDMAZE.IDX` (64,304 B). `MINDMAZE.EIT` holds only
the game artwork (`baggage/`), not questions. All integers are little-endian.

## MINDMAZE.DB: question records

The file is a plain concatenation of variable-length records with no header and no padding.
Parsing from offset 0 to the end yields exactly **8,020 records** and lands exactly on the file size.

| field | type | notes |
|---|---|---|
| reserved | u32 | always 0 |
| clue length | u32 | byte length of the clue |
| clue | bytes | the question text (a short clue sentence) |
| 4 × answer | | the **first** answer is the correct one; the other three are distractors |
| ↳ text length | u8 | |
| ↳ text | bytes | answer text ("Bombsight") |
| ↳ article id | u32 | content id of the article to read about this answer (always a 761… id) |
| ↳ location | u16 | see below |

**Question id = record index + 1** (1 … 8020). This is what `MINDMAZE.IDX` uses.

### Text encoding
- Bytes are 7-bit ASCII plus a few high bytes that are **code page 437** (DOS), not Windows-1252:
  `0xA4` = ñ ("Vicu\xA4a", "El Ni\xA4o", "Bu\xA4uel"), `0x91` = æ ("Chronographi\x91"), `0x9C` = £ ("the symbol \x9C. Pound").
  Only those three byte values occur (22 times in all). The reader maps the whole upper half through CP437.
- Text also carries **SGML-style character entities** (`&eacute;`, `&rsquo;`, `&amacr;`, `&lstrok;`, `&omacr;`, … about 40 names)
  and light inline markup: `<it>…</it>` (italic, e.g. titles and taxa) and `<scp>…</scp>` (small caps, e.g. `<scp>bc</scp>`).
  The entity names follow the ISO 8879 entity sets (`&amacr;` ā, `&lstrok;` ł), the same convention as the article XML.
  The reader keeps entities and markup verbatim; resolution belongs to the shared text-rendering layer.

### The `location` word (interpretation unconfirmed)
- It is the same for all four answers of a record in all 8,020 records. 7,255 records have 1; the others range 2 … 258.
- Questions whose correct answer points into a long survey article have large values: three questions about China
  (Xia and Shang dynasties, Hua Guofeng, the 1979 Vietnam incursion) all point to article 761573055 with 163, 163, 255, 258,
  rising with the position of that topic in the article.
- Working hypothesis: **the paragraph or section number in the answer article where the clue is answered**
  (1 = the start of the article), used by the game's "read the article" hint. Confirm it once AKC article bodies decode
  (compare the paragraph numbering with the search index's `para_range`), or in the reference VM.

### Cross-references (validated over the whole file)
- 32,080 article ids, all 761…; 32,013 exist in `content.ecn` and in the PFX titles. **44 distinct ids (67 references) are
  missing** from the 2009 catalog (articles dropped since the question bank was written). A converter should keep the
  answer text and mark the link as dead.
- 115 records point all four answers at one article (questions about parts of a single topic, e.g. the four film gauges).

## MINDMAZE.IDX: category index

64,304 B = 16,076 u32 = 36 + 2 × 8,020.

| offset | content |
|---|---|
| 0 | **36 × u32 group sizes** (306, 299, 229, 194, 363, …); they sum to 8,020 |
| 0x90 | 8,020 × `(u32 question id, u32 DB byte offset)`, group after group in the order of the sizes |

- Every offset is the start of record `id − 1` in the DB; every question appears in exactly one group; within a group the
  ids ascend.
- **The 36 groups are 9 categories × 4 difficulty levels** (group = category × 4 + level − 1). The names are not stored in
  either file. From the content they are, in order:

| groups | category (inferred from the questions) |
|---|---|
| 0–3 | Physical science and technology |
| 4–7 | Life science (biology, health, animals and plants) |
| 8–11 | Geography |
| 12–15 | History |
| 16–19 | Social science (psychology, law, economics) |
| 20–23 | Religion, mythology and philosophy |
| 24–27 | Art, language and literature |
| 28–31 | Performing arts (music, dance, film, broadcasting) |
| 32–35 | Sports, games and hobbies (including dog breeds) |

  Level 1 is easy and level 4 hard: group 8 asks for Beijing, London, Mount Everest; group 11 for Leipzig and Kraków.
  The category labels are our inference; the UI strings live in the program resources (check the VM for the exact wording).

## Modern target
JSON or SQLite rows: `{id, category, level, clue, answers: [{text, article_id, alive}], location}`, with entities resolved
to Unicode at conversion time.
