# Search indexes, catalog and misc files

Status: **search (FINDDLX) and catalog formats fully understood and parsed**. ITIR indexes and misc files identified.
Parsers (independent Python, for exploration only): `tools/explore-py/`
(`idxe.py`, `idxw.py`, `idxew.py`, `pfx.py`, `cat.py`).

Sources used: observing the files, plus `ilspycmd` decompilation of the .NET assemblies
`EE/ENCARTA/{BTREE,SEARCH,FGSEARCH}.DLL`. ilspy output lives in `analysis/decompiled/ilspy/` (gitignored).
All integers are little-endian. "NETSTR" means a .NET `BinaryWriter` string: a 7-bit varint byte length, then UTF-8 bytes.

## Content-ID namespaces (the key to everything)

Every addressable item uses a decimal u32 `refId` = `prefix * 10^7 + n`, where `n` is usually 1,5xx,xxx.
Seen in the search index, the catalog, and the dictionary EIT file names:

| prefix | what (inferred from titles) |
|---|---|
| 76 (761…, 762…) | encyclopedia articles (762 = additional or regional articles) |
| 70 (701…) | multipurpose: sidebars, tables, timeline events, Kids articles, dictionary (bilingual) |
| 46 (461…) | media (images, audio, video, maps, charts) and yearbook-style "year : topic" items |
| 174, 176, 182, 861… | web links, interviews, essays, "on this day" and timeline items |
| 186 (1861…) | English dictionary (DICTA.EIT entries, quotations, proverbs) |
| 56 (561…) | bilingual dictionaries, thesaurus, quotations |
| 21 (210…) | Encarta subject index or "Find" terms (`Criticism, Literary : literary critics : Canadian`) |
| 63, 68, 48, 92, 94, 12x, … | further media and partner collections |

Article XML `<x id="761561691">` uses these same ids.

---

## 1. FINDDLX.CAB: the "Find" search index (SEARCH.DLL, FGSEARCH.DLL, BTREE.DLL)

`cabextract` produced `extracted/finddlx/`. Each file name has a SKU suffix (`_E_A_09_XL_R_D` for Deluxe, `_Kids_A_2009` for Kids).
The installer renames them, for example to `IDXEDLX.EFF`. In SEARCH.DLL, `OpenFindFiles(sku, dir, …, tla, …)` builds the names
`IdxW{tla}.{ext}`, `IdXE{tla}.{ext}`, `IdXEW{tla}.{ext}` and `IdXCM{tla}.ECT`, with ext `EFF` (flat file) or `EFG` (B-tree).
Update packs use `E{tla}{nnnnn}`, `EW…`, `DW…` and `W…`. FGSEARCH.DLL is the COM control that ENCARTA.EXE drives (`LastResultsXML` and similar).
None of the files is compressed or encrypted.

Every `.EFF` starts with a common header: NETSTR `"Copyright (c) Microsoft Corporation 2004"` (0x00–0x28), then the version bytes `major, minor, rev`.

### IdXE (IDXEDLX.EFF, 146 MB): index entries, the search-result records
- Version 1.6.0 (1.7 would add a trailing u32 "dual para").
- Records are **aligned to 16 bytes**, and `indexEntryId = file_offset / 16`.
- Record: `u8 type`, `i32 refId`, [`i32 dupeId` if type&0x40], [`u32 sectPara` if type&0x20], `u8 wordCount`,
  NETSTR `displayText` (UTF-8 with Encarta inline markup, e.g. `<it>…</it>` and `<entity tag="eacute"/>`), `u8 nParents`, `nParents × i32 parentRefId`.
- `type & 0x1F` = content type 0–27. Bit 0x80 = "list", 0x40 = has dupeId, 0x20 = has section or paragraph.
- `sectPara`: if the high u16 is 0, the low u16 is a section number. Otherwise it is (paraEnd<<16 | paraStart).
- Content types, observed: 0 article and its sections, 1 tables or lists, 2–11 dictionary or bilingual headwords,
  12 images, 13 maps, 14 video, 15 audio, 16 web links, 17 quotations, 19 "on this day", 21 timelines.
  SEARCH.DLL hard-codes the type→bucket and filter table (`ContentTypes` ctor).
- About 4.08 M records (Deluxe) and 62.5 k (Kids). A sequential walk also yields about 2% junk records (gaps between live records),
  so the reliable way to enumerate is through the postings.
- **It does contain titles**: every article title, its synonyms and alternate titles ("Paper Chase, The" / "The Paper Chase"),
  section titles (`Lepcis Magna : Roman Period`), and dictionary headwords. 53,040 distinct 761 and 762 ids.

### IdxW (IDXWDLX.EFF, 26 MB): the vocabulary B-tree (BTREE.DLL `BTreeFile`)
- Header: common header, version 1.8.0, NETSTR encoding (`"UTF8"`), then `u32 lastUpdate`, `u8 valid`, `u32 freeListHead`, `u32 iter`,
  `u16 maxKeysPerNode` (201), NETSTR key-factory class name (`Microsoft.Encarta.BTree.BTreeKey_String`), and `u32 rootNodeOffset` (near EOF).
- Node: `u16 nKeys`, then nKeys × (`u32 dataOffset`, `u8 deleted`, `u16 keyLen`, UTF-8 key), then `u16 nChildren`, nChildren × `i32 childOffset`.
  This is a classic B-tree (in-order walk: child[i], key[i], …, child[n]).
- `dataOffset` → WordEntry `{u32 wordId, u8 altId, u8 stopWordType}`. **wordId is a byte offset into IdXEW.**
- 1,120,732 normalised words (Deluxe) and 55,261 (Kids). The keys are lower-cased and accent-folded according to the ECT map.

### IdXEW (IDXEWDLX.EFF, 498 MB): postings
- Version 1.11.0, followed by 6 bool flags: `paraInT3, threeTiered, shortRank, paraInT2, skipOffset, refIdInT3` = `1,1,1,1,0,1`.
- At `wordId`: `u8 nAlt`. For each alt (word form, e.g. photosynthesize/-s/-d share one set):
  - `u8 altId`
  - `u16 n1` × T1 (title hit): `u24 indexEntryId, u8 wordOrder, u16 rank, u32 refId`
  - `u16 n2` × T2 (heading or alias hit): `u24 indexEntryId, u16 rank, u32 refId (bit31 = flag), u32 paraEnd<<16|paraStart`
  - `u16 n3` × T3 (body full-text hit): `u24 indexEntryId, u8 wordOrder, u32 refId (bit31 flag), u16 para`
- Example: "zebra" → T1 `Zebra` (761575094), T2 `Equus zebra` (same article, para 0x20), T3 `Perissodactyla` para 4.
  T3 lists are moderate (about 200 for "photosynthesis"), so they are curated or capped rather than exhaustive.

### IdXCM (IdxCMDLX.ECT, 2.3 KB): normalisation character map
`BinaryReader(Encoding.Unicode)`: NETSTR culture (`"en-US"` as UTF-16), `u8 answers`, `i32 n`=383, then n × (`char ch, char normal, char browse`), all UTF-16.
Folding table for accents and case. Replace with Unicode NFKD plus case-folding.

### PFX (PFXDLX.DAT, 22 MB): prefix-match "word wheel" (autocomplete), in SEARCH.DLL `Microsoft.Encarta.PrefixMatch`
- Two collections back to back: `i32 count, i32 LCID (0x409)`, then count × (`u32 refId, u16 classId, NETSTR sortTitle, NETSTR displayTitle`).
- Collection 0 = full titles (198,701), sorted by sortTitle. Collection 1 = partial or suffix keys (289,044), e.g. `burchelli` → `Equus burchelli`.
- classId: **1 = encyclopedia article** (41,235 distinct ids), 203–370 = media or sidebar kinds, 2020/2021 dictionary/quotations,
  2214 = Kids article, 16960 = index terms.
- **This is the cleanest title list.** It has article titles with display markup, sort keys, aliases and media titles.

### Verdict
- **Needed?** No. Everything here is derived from the content. Rebuild it with tantivy over the decoded article XML:
  titles boosted, a headings field, and the body (paragraph ids kept as stored fields).
- **Harvest once** (S): dump PFX and IdXE to JSON/SQLite as an alias, synonym and sort-title table.
  These editorial aliases ("Paper Chase, The", species names) and section anchors are hard to regenerate.
  The parsers already exist in the tools folder, so a Rust port is about 1 day.
- Re-implementing the original ranking (rank u16 values, tiers, dupe removal, buckets) for parity would be **L**. Not recommended.
- Effort: harvest **S**. New tantivy search with title/heading/body tiers and autocomplete (tantivy or an `fst` over PFX titles): **M**.

---

## 2. ITIR (`ITIRCL54.DLL`) `$WW…` / `$FX…` / `$OBJINST` inside EITs

| location | items |
|---|---|
| EDICT/DICTA, THESA, ENG_*/*_ENG .EIT | `$WWDictWW`, `$WWLookupWW`, (DICTA) `$OSParentalControl`, `$OSParentalControlWW`, `$OBJINST`, `$_EDictSignature[2007]_$` |
| MSWORLD.ITR | `$FX{Main,Titles,Img,Aud,AnimalImg}FTI`, `$WW{Main,Img,Aud,AnimalImg}KIndex`, `$WW$CTCATALOG`, `$OBJINST` |
| TIMELINE.EIT | `$FXTimelineFTI`, `$WWTimelineWW`, `$WW$CTCATALOG`, `$OBJINST` (no other files) |

- Each `$WW*`/`$FX*` is a directory holding `BTREE`, `DATA` and `PROPERTY`. `PROPERTY` starts `XX\x03\0` and carries the ITSS GUID `{0A9007BC-4076-11D3-8789-0000F8105754}`.
- `BTREE` itself begins with an `ITOLITLS` header plus `CAOL`, then `AOLL`/`AOLI` chunks (no `ITSF` block, so the `eit` tool refuses it).
  Leaf pages (from 0x2000) hold UTF-16 keys and record bytes.
- **Word wheel ($WW) record**, observed in `$WWDictWW`: `utf16z headword`, `utf16z sortSeq ("00059")`, small header bytes, `u32 refId` (e.g. 0x29D32750 = 701704016),
  then `u32 len` and a UTF-16 display string. **The refId equals the entry's file name** in the EIT (`701704016`, `1861583167` confirmed).
- `$FX*FTI/DATA` is bit-packed InfoTech full-text postings. `$OBJINST` is persisted ITIR object configuration (CLSID streams `l>=teB…`), not data.
- `$OSParentalControl*` = flagged-entry list and wheel for the dictionary's offensive-word filter.
- **`$WW$CTCATALOG` is the exception that carries data.** In MSWORLD.ITR each key is a title and the value is a record such as
  `i;L36640;B14632.jpg;P14632.JSM;U0;M631500205;CPaul A. Souders/Corbis`
  (type; L?; big image; picon; U?; **M = media refId**; **C = credit line**). 4,321 image records.
  TIMELINE.EIT's catalog is the event-title list.

**Dictionary keying (DICTA.EIT, about 105k entries).** File name = decimal refId: 81,810 × 186… (English),
17,933 × 561…, 5,289 × 701…, plus `_labelw.xml` and the `$…` items.
Content is UTF-8 XML `<DICTIONARY><HOMOGRAPH ID="US…GUID" Variety="US"><HEAD><HEADWORD><ENTRY>401(k)</ENTRY>…<DisplayHeadword>`.
The headword → id mapping can be regenerated from `<ENTRY>` and `<DisplayHeadword>`, plus the inflections (`<ONE-INFLECTION><ENTRY>`).
IdXE type 2–11 entries give the same mapping. The `$_EDictSignature2007_$` XML gives the dictionary name, GUID, `UniqTag` and `LexFile` (e.g. `DICTA` → `EDICT/DICTA.LEX`).

**Verdict:** not needed. Rebuild headword lookup, fuzzy lookup and full-text search with tantivy and an `fst` from the XML.
The exception is the MSWORLD `$WW$CTCATALOG` record strings: harvest them if the photo credits and file mapping are not found in DATA*.AKC (check that first).
A minimal leaf-page scanner (UTF-16 key + record) for that one index is **S–M**. A full ITIR B-tree/FTI decoder would be **L** and is not worth it.

---

## 3. Catalog: `CATALOG.DLX/catalog2` (MSENCDAT.DLL `HECatalog`)

**guid.lst**: UTF-16LE text with CRLF line breaks, 91 lines. **Package index = 0-based line number.** Blank lines are unused slots.
Each GUID maps to a file through `DELUXE.INI [Packages]` (e.g. line 4 `{0EA8A288…}` = ContDlx.akc, 69 = ContStd.akc, 47 = DataDlx.akc).

**Packed location** (u32, same in ECN and ECS): byte0 = package index, byte1 = local-id bits 16–23, u16 at +2 = local-id bits 0–15.
In u16 pairs that is `(pkgLo | localHi<<8, localLo)`. Local ids are dense (0…count−1) per package: ContDlx 69,949, DataDlx 175,793,
so the high byte really is used.

**ECN** (numeric key), used by content.ecn, data.ecn and relart.ecn:
```
"ECN\0" | N × u32 packedLocation | N × u32 refId (sorted ascending, parallel array) | u32 N
```
Lookup: binary-search refId, then take location[i].

| file | N | packages | meaning |
|---|---|---|---|
| content.ecn | 114,004 | ContDlx 69,949 / ContStd 44,055 | article and sidebar body XML (761 36k, 186 17.5k, 174 16k, 701 14k, 561 12k, 461 8k…) |
| data.ecn | 477,393 | DataDlx / DataStd / DataFStd / DataFDlx | per-item data or metadata records (210 index terms 180k, 701 134k, 761 38k, media…) |
| relart.ecn | 35,576 | RelStd 34,606 / RelDlx 970 | related-articles lists |

Example: 761575094 (Zebra) → content ContStd #22865, data DataStd #81969 (pkg byte 0x0A, localHi 1), relart RelStd #18691. 761561691 → ContStd #11333, DataStd #70437, RelStd #8667.
The local id is the record number inside that AKC (see `akc.md`; the AKC record index is keyed by content id, so the ids cross-validate).

**ECS** (string key), used by baggage.ecs and generic.ecs:
```
"ECS\0" | N × u32 packedLocation | N × fixed-width UTF-16 key, space-padded | u32 N
width = (filesize − 8 − 4N) / N   → baggage 34 B (17 chars), generic 144 B (72 chars)
```
Keys are sorted, and lookup is by binary search.
- baggage.ecs (65,892): media "baggage" file names, e.g. `0000ea10.gsm` → PiconStd.eit, `.gtn` → ThumbStd.eit, `.iax` → SwSTD02.eit
  (confirmed: `extracted/eit/CONTENT/PICONSTD.EIT/baggage/0000ea10.gsm`). Mapping: which media EIT holds which file.
- generic.ecs (6,408): path keys (`dswmedia/iaf/a/0000eebb/t304800a1_globe.swf`, `list/expert.xml`) → Sw*.eit / SwQuiz / DataESK.

**How a content id resolves**: `<x id="761561691">` → content.ecn → (ContStd.akc, local 11333) → AKC record → article XML.
Its metadata comes from data.ecn → Data*.AKC, and related links from relart.ecn → Rel*.AKC. Media files referenced by name use baggage.ecs → EIT package → `baggage/<name>`.
Kids uses the same scheme in `EE/KIDS/CATALOG.KDC`.

**Verdict:** needed at conversion time only. Not compressed. Effort **S** (½ day to port to Rust). Modern replacement: SQLite table `item(ref_id PK, kind, package, local_id)`
plus `file(name PK, package)`. Or skip it entirely once AKC records are exported keyed by refId.

---

## 4. Misc files

| file | what | structure / evidence | read by | need? | replacement | effort |
|---|---|---|---|---|---|---|
| EDICT/DICTA.LEX, THESA.LEX, BIL??.LEX | Lexicon for dictionary/thesaurus/bilingual **fuzzy or spelling lookup** ("did you mean"). Named by each EIT signature `<LexFile>` | magic `20 F3 9F 77`, version, LCID 0x409, filler `CC…`, UTF-16 `copyright2000 Megalith Corporation`, then packed or compressed trie/state tables (strings like `aReRiRoR…`) | EDICT.EXE / ENCARTAU.DLL with MSSPELL3.DLL | No | `fst` or SymSpell built from the headword lists | S (regenerate) / L (decode) |
| EESPLA01.LEX (748 KB, also in KIDS) | English **speller lexicon** (Megalith-format, same magic) used for search spelling suggestions | as above | ENCARTAU.DLL (`EESplA01.lex`) + MSSPELL3.DLL | No | Hunspell en_US or SymSpell over the PFX/IdxW vocabulary | S |
| TIMELINE.DAT (2.5 MB) | **Timeline** data (categories, regions, events) | header u32 table at 0x18 (`0x1C, 0x3A2C8, 0xDF35E, 0x3B248, 1, …, 6`). Category list: u32 bitflag + UTF-16Z name (Arts, Geologic Time, People and Society, Philosophy and Religion, Political History, Science and Technology). Region list the same way (Africa 0x01 … Southwest Asia 0x08). Then **16,465 fixed 70-byte event records**: `u16 1, u16 categoryBits, u16 regionBits, u32 seq, u32 refId (174…, 461…), u32 ?, u32 ?, u32 strOffset, u32 strLen, …, u32 time value (e.g. 0x4630C0)` and a UTF-16 string pool (about 3.5k titles). Date encoding not yet decoded. Not compressed | ENCARTAR/ENCARTA.EXE (`timeline.dat`, `CTimelineView`); TIMELINE.EIT holds only its search index | **Yes** (only source of the timeline layout and dates; event bodies are content ids) | JSON events `{refId, title, start, end, cats, regions}` | M |
| MINDMAZE.DB (1.4 MB) / .IDX (64 KB) | **MindMaze** quiz game question bank | DB records: `u32 0, u32 clueLen, clue (ASCII/CP1252)`, then 4 options × (`u8 len, text, u32 refId, u16 1`). The first option is the answer, and the refIds are the articles to read. IDX = 16,076 u32, of which about half are DB record offsets (probably offset/size or per-category tables). MINDMAZE.EIT = game art (`baggage/`) | ENCARTA.EXE (`CMindmazeView`), ENCARTAR.DLL | Optional (game) | JSON question list | S |
| P2MTBL.DAT (227 KB) | **Place-to-Map table**: links articles to atlas map views | header `u32 8, u32 1`, 8 descriptors (u16 off, u16 flags, u32), then lists of `(u32 refId 761…, u32 n)` followed by `(u32 mediaId 461…, u32 level)` pairs | AREF/COMPNTS/ATCONT.DLL (`CP2MObject`, `IP2MObject`) | Yes if the atlas is ported (article → map jump) | JSON or SQLite `article → [(map id, zoom)]` | S–M |
| PUSHPINS/*.GPD | Empty **user pushpin DB templates** (20–44 B, magic `PPIN..e\0` repeated at the end) | — | AREF/COMPNTS/PPDBMGR.DLL, PPMGR.DLL | No | User data in browser storage or the server DB | – |
| D1–D4.TXT, D1–D4PLACES.TXT | Geography Quiz difficulty settings and place pools (see `atlas-and-3d.md`) | text: `20 / 5 4 3 2 0 / 0 0 0 / .70 .20 .10`. Places rows `type group entity 0` → MAD entity ids | ENCARTA.EXE (`D%d.TXT`, `D%dPLACES.TXT`) | Optional | JSON | S (+MAD) |

---

## Summary of recommendations

| item | need at runtime? | plan | effort |
|---|---|---|---|
| FINDDLX index (IdXE/IdxW/IdXEW/ECT) | no | Rebuild with tantivy. One-off harvest of the alias/section-title data | S harvest, M new search |
| PFX word wheel | no | Harvest as title/alias/autocomplete source → `fst` | S |
| ITIR `$WW`/`$FX`/`$OBJINST` | no | Ignore. Regenerate from XML. Harvest the MSWORLD `$WW$CTCATALOG` strings only if the credits are missing elsewhere | S–M (optional) |
| catalog2 ECN/ECS + guid.lst | conversion-time only | Port the parser. Export to SQLite | S |
| LEX lexicons | no | Hunspell/SymSpell/fst | S |
| TIMELINE.DAT | yes (data) | Finish decoding the record fields and dates | M |
| MINDMAZE.DB/IDX | optional | Decode to JSON | S |
| P2MTBL.DAT | if atlas | Decode to JSON | S–M |
| GPD, D*.TXT | no / optional | — | – / S |
