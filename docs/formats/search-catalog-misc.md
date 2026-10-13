# Search indexes, catalog and misc files

Status: **search (FINDDLX) and catalog formats implemented in Rust and validated over the whole disc**
(Deluxe and Kids). ITIR indexes and misc files identified.

- Rust: `crates/encarta-formats/src/catalog.rs` (§3) and `src/search.rs` (§1).
- CLIs: `catalog [--extracted DIR] [--kids] <resolve <id|key>|dump|packages|check>` and
  `findidx [--dir DIR] [--kids] <pfx|idxe|idxe-walk|idxw|idxew|idxcm|word KEY|stats>`; dumps are JSON lines.
- Whole-disc tests: `crates/encarta-formats/tests/catalog_search.rs` (reads `ENCARTA_EXTRACTED`, skipped when absent).
- Earlier exploration parsers (Python, superseded): `tools/explore-py/` (`idxe.py`, `idxw.py`, `idxew.py`, `pfx.py`, `cat.py`).
  Note that `idxe.py` predates the trailing-u32 finding below and walks into the 4-byte gaps.

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
- Version 1.6.0 in both sets. The header is the common header (0x2C bytes); the first record is at the next 16-byte boundary (0x30, entry id 3).
- Records start on **16-byte boundaries**, and `indexEntryId = file_offset / 16`.
- Record: `u8 type`, `i32 refId`, [`i32 dupeId` if type&0x40], [`u32 sectPara` if type&0x20], `u8 wordCount`,
  NETSTR `displayText` (UTF-8 with Encarta inline markup, e.g. `<it>…</it>` and `<entity tag="eacute"/>`), `u8 nParents`, `nParents × i32 parentRefId`,
  **`u32 dualPara`**.
- **Correction (from the data): the trailing `u32 dualPara` is present on every record of these 1.6 files**, not only in a 1.7 format.
  It is a second `paraEnd<<16 | paraStart` range. It is 0 when the record has no paragraph range. When `sectPara` is a paragraph range,
  it is usually the same range or one paragraph lower (e.g. `sectPara` 0x90009, `dualPara` 0x80008).
  Evidence: with it counted as part of the record, the next record always starts at `align16(end)` (all 3,034,468 Deluxe and 50,257 Kids records).
  Without it, the space left after records ending at offsets ≡ 0, 13, 14 or 15 (mod 16) parses as an empty pseudo-record. That is the "junk" seen earlier.
  The rest of the alignment padding is zero.
- `type & 0x1F` = content type 0–27. Bit 0x80 = "list", 0x40 = has dupeId, 0x20 = has section or paragraph.
- `sectPara`: if the high u16 is 0, the low u16 is a section number. Otherwise it is (paraEnd<<16 | paraStart).
- Content types, observed: 0 article and its sections, 1 tables or lists, 2–11 dictionary or bilingual headwords,
  12 images, 13 maps, 14 video, 15 audio, 16 web links, 17 quotations, 18 (930 records), 19 "on this day", 21 timelines.
  SEARCH.DLL hard-codes the type→bucket and filter table (`ContentTypes` ctor).
- Live record counts by type (Deluxe): 0: 1,372,501 · 1: 124,035 · 2: 234,354 · 3: 96,305 · 4: 42,858 · 5: 39,372 · 6: 44,699 · 7: 43,389 ·
  8: 443,559 · 9: 133,690 · 10: 69,520 · 11: 86,739 · 12: 35,322 · 13: 29,445 · 14: 390 · 15: 1,750 · 16: 94,308 · 17: 122,669 · 18: 930 · 19: 5,546 · 21: 13,087.
  Kids: 0: 28,437 · 1: 5,437 · 12: 12,975 · 13: 2,088 · 14: 798 · 15: 126 · 16: 396.
- **Counts (measured)**:

  | set | records posted (live) | sequential walk | walk records nothing posts | distinct refIds | distinct 761/762 ids |
  |---|---|---|---|---|---|
  | Deluxe | 3,034,468 | 3,034,472 | 4 | 656,581 | 53,039 |
  | Kids | 50,257 | 50,257 | 0 | 4,728 | 0 |

  The earlier figures (4.08 M Deluxe, 62.5 k Kids, "about 2% junk") came from a walk that did not know about `dualPara`: 1,044,321 and 12,306 of those were gap pseudo-records.
  The 4 unposted Deluxe records are real (text `dictionary` ×3 and `info`, types 4, 5, 6, 9). The walk ends exactly at EOF, and every posted record lies on the walk path.
  Every posting's refId equals the refId of the record it points to (0 mismatches).
- **It does contain titles**: every article title, its synonyms and alternate titles ("Paper Chase, The" / "The Paper Chase"),
  section titles (`Lepcis Magna : Roman Period`), and dictionary headwords.

### IdxW (IDXWDLX.EFF, 26 MB): the vocabulary B-tree (BTREE.DLL `BTreeFile`)
- Header: common header, version 1.8.0, NETSTR encoding (`"UTF8"`), then `u32 lastUpdate`, `u8 valid`, `u32 freeListHead`, `u32 iter`,
  `u16 maxKeysPerNode` (201), NETSTR key-factory class name (`Microsoft.Encarta.BTree.BTreeKey_String`), and `u32 rootNodeOffset` (near EOF).
- Node: `u16 nKeys`, then nKeys × (`u32 dataOffset`, `u8 deleted`, `u16 keyLen`, UTF-8 key), then `u16 nChildren`, nChildren × `i32 childOffset`.
  This is a classic B-tree (in-order walk: child[i], key[i], …, child[n]).
- `dataOffset` → WordEntry `{u32 wordId, u8 altId, u8 stopWordType}`. **wordId is a byte offset into IdXEW.**
  The WordEntry records sit right after the header (from 0x6C), and the nodes follow them. The root is the last node.
- 1,120,732 normalised words (Deluxe) and 55,261 (Kids). The keys are lower-cased and accent-folded according to the ECT map.
  The in-order walk gives keys in strictly ascending **ordinal (byte) order**. No key is marked deleted. Tree shape: Deluxe 8,055 nodes (7,990 leaves), Kids 412 (408), depth 3.
  Header values on both: `lastUpdate 0, valid 1, freeListHead 0, iter 0`.
- **wordId groups variants and synonyms**: keys sharing a wordId form one group, each with its own `altId` (1…n), e.g.
  `0`/`zero`/`zeroes`/`zeroing`/`zeros`, or `novelist`/`writer`/`wrote`/`written by`. Deluxe: 984,105 distinct wordIds; Kids: 17,809.
- **wordId 0 = no postings.** Kids has 177 such keys, all stop words (`stopWordType` 1 or 2, e.g. `also`, `an`, `because`). Deluxe has none.
  Each set has 336 keys with a non-zero `stopWordType` (116 × type 1, 220 × type 2).

### IdXEW (IDXEWDLX.EFF, 498 MB): postings
- Version 1.11.0, followed by 6 bool flags: `paraInT3, threeTiered, shortRank, paraInT2, skipOffset, refIdInT3` = `1,1,1,1,0,1`.
- At `wordId`: `u8 nAlt`. For each alt (word form, e.g. photosynthesize/-s/-d share one set):
  - `u8 altId`
  - `u16 n1` × T1 (title hit): `u24 indexEntryId, u8 wordOrder, u16 rank, u32 refId`
  - `u16 n2` × T2 (heading or alias hit): `u24 indexEntryId, u16 rank, u32 refId (bit31 = flag), u32 paraEnd<<16|paraStart`
  - `u16 n3` × T3 (body full-text hit): `u24 indexEntryId, u8 wordOrder, u32 refId (bit31 flag), u16 para`
- Example: "zebra" → T1 `Zebra` (761575094), T2 `Equus zebra` (same article, para 0x20), T3 `Perissodactyla` para 4.
  T3 lists are moderate (about 200 for "photosynthesis"), so they are curated or capped rather than exhaustive.
- The postings blocks, taken in wordId order, **tile the file exactly**: the first starts right after the flags (0x32), each starts where the previous one ends,
  and the last ends at EOF (no gaps or overlaps in either set). So the IdxW vocabulary reaches every block.
- A block lists **only the alts that have hits**. In Deluxe, 83,664 keys have an altId with no entry in their block (Kids: 33,960).
  **altId 0 never appears on a key but does appear in blocks** (Kids: in 3,786 of 17,809 blocks, the only alt in 1,597).
  Presumably these are hits for the whole group.
- Bit 31 of the refId: never set in T1, set on 341,472 T2 hits (Kids 41,397), never set in T3. Meaning unknown.
- Totals: Deluxe 1,045,325 alts, T1 8,788,447, T2 2,606,782, T3 36,819,536. Kids 24,910 alts, T1 174,037, T2 54,798, T3 313,362.
  Every posted indexEntryId is a valid IdXE record.

### IdXCM (IdxCMDLX.ECT, 2.3 KB): normalisation character map
`BinaryReader(Encoding.Unicode)`: NETSTR culture (`"en-US"` as UTF-16), `u8 answers`, `i32 n`=383, then n × (`char ch, char normal, char browse`), all UTF-16.
Folding table for accents and case. Replace with Unicode NFKD plus case-folding.
The NETSTR length counts bytes (10 for `en-US`). `answers` = 0. The file is exactly `1 + 10 + 1 + 4 + 383×6` = 2,314 bytes, and the Deluxe and Kids files are byte-identical.
`browse` is often U+0000 (no browse form), e.g. `Ó → O, \0`, while `s → s, S`.

### PFX (PFXDLX.DAT, 22 MB): prefix-match "word wheel" (autocomplete), in SEARCH.DLL `Microsoft.Encarta.PrefixMatch`
- Two collections back to back: `i32 count, i32 LCID (0x409)`, then count × (`u32 refId, u16 classId, NETSTR sortTitle, NETSTR displayTitle`).
- No EFF header. The file is exactly the two collections, with nothing after them.
- Collection 0 = full titles (198,701), sorted by sortTitle. Collection 1 = partial or suffix keys (289,044), e.g. `burchelli` → `Equus burchelli`.
  Kids: 5,749 and 8,333.
- The sort is **culture-aware, not ordinal**. Hyphens and apostrophes are nearly ignored, so `12-step program` comes before `12 Apostles` and `1920’s` before `1920s`.
  Measured inversions under plain ordinal order: 15,383 (collection 0) and 12,627 (collection 1); under lower-casing: 6,271 and 1,045.
  A reimplementation should re-sort with its own collation rather than rely on this order.
- classId: **1 = encyclopedia article** (41,235 distinct ids in collection 0; 33,793 in collection 1), 203–370 = media or sidebar kinds, 2020/2021 dictionary/quotations,
  2214 = Kids article, 16960 = index terms. Deluxe uses 51 distinct classIds. Kids has no classId 1. Its biggest classes are 2214 (Kids article, 2,519 in collection 0), 256 (2,109), 2215 and 2216.
- Distinct refIds: Deluxe 82,034 (collection 0) and 70,654 (collection 1). Kids 3,513 and 3,112. `sortTitle == displayTitle` for 160,060 of the 198,701 Deluxe titles.
- **This is the cleanest title list.** It has article titles with display markup, sort keys, aliases and media titles.

### Implementation (`search.rs`, `findidx` CLI)
- `Pfx::parse`, `IdxE::{record, walk}`, `IdxW::walk` (in-order key list plus tree stats), `IdxEw::postings`, `CharMap::parse`,
  `live_entry_ids` (every IdXE entry id that a posting references) and `FindFiles::locate(dir, "DLX"|"KDC")`.
  `locate` accepts both the CAB names (`IDXEDLX.EFF_E_A_09_XL_R_D`) and the installed names (`IDXEDLX.EFF`).
- `findidx idxe` enumerates records through the postings. `findidx idxe-walk` walks sequentially; with `dualPara` parsed, the two agree apart from the 4 unposted records.
  `findidx word zebra` shows one word's hits with their display text. `findidx stats` runs every count and cross-check quoted here
  (about 2 s and 0.9 GB RSS for Deluxe in a release build).
- The IdXEW reader decodes only the observed flag combination `1,1,1,1,0,1` and rejects any other.

### Verdict
- **Needed?** No. Everything here is derived from the content. Rebuild it with tantivy over the decoded article XML:
  titles boosted, a headings field, and the body (paragraph ids kept as stored fields).
- **Harvest once** (S): dump PFX and IdXE to JSON/SQLite as an alias, synonym and sort-title table.
  These editorial aliases ("Paper Chase, The", species names) and section anchors are hard to regenerate.
  **Done**: `findidx pfx` and `findidx idxe` emit them as JSON lines. The IdxW wordId groups (`findidx idxw`) are a ready-made synonym list.
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
- **`$WW$CTCATALOG` is the exception that carries data.** Decoded: see [msworld.md](msworld.md) §3. Each key is a u32 (MSWORLD: the media content id; TIMELINE: the item number), and the title is a property (property 1 in MSWORLD, property 0 in TIMELINE). An MSWORLD value looks like
  `i;L36640;B14632.jpg;P14632.JSM;U0;M631500205;CPaul A. Souders/Corbis`
  (type; L = place id; B = image file; P = thumbnail; U = paired item; M = legacy id; **C = credit line**). 5,413 records (4,775 images, 330 sounds, 308 animal images).
  TIMELINE.EIT's catalog is the event-title list.

**Dictionary keying (DICTA.EIT, about 105k entries).** File name = decimal refId: 81,810 × 186… (English),
17,933 × 561…, 5,289 × 701…, plus `_labelw.xml` and the `$…` items.
Content is UTF-8 XML `<DICTIONARY><HOMOGRAPH ID="US…GUID" Variety="US"><HEAD><HEADWORD><ENTRY>401(k)</ENTRY>…<DisplayHeadword>`.
The headword → id mapping can be regenerated from `<ENTRY>` and `<DisplayHeadword>`, plus the inflections (`<ONE-INFLECTION><ENTRY>`).
IdXE type 2–11 entries give the same mapping. The `$_EDictSignature2007_$` XML gives the dictionary name, GUID, `UniqTag` and `LexFile` (e.g. `DICTA` → `EDICT/DICTA.LEX`).

**Verdict:** not needed. Rebuild headword lookup, fuzzy lookup and full-text search with tantivy and an `fst` from the XML.
The exception is the `$WW$CTCATALOG` records, which are decoded (`encarta_formats::itir_catalog`, `smallfmt catalog|media`) and carry the photo credits.
A minimal leaf-page scanner (UTF-16 key + record) for that one index is **S–M**. A full ITIR B-tree/FTI decoder would be **L** and is not worth it.

---

## 3. Catalog: `CATALOG.DLX/catalog2` (MSENCDAT.DLL `HECatalog`)

Two catalogs, same format: Deluxe `EE/ENCARTA/CATALOG.DLX` with `EE/ENCARTA/DELUXE.INI`, and Kids `EE/KIDS/CATALOG.KDC` with `EE/KIDS/KIDSCD.INI`.
Each is an ITOLITLS container holding `catalog2/{guid.lst, content.ecn, data.ecn, relart.ecn, baggage.ecs, generic.ecs}`. None is compressed.

**guid.lst**: UTF-16LE text without a BOM, CRLF line breaks, 91 lines, each ending in CRLF. **Package index = 0-based line number.** Blank lines are unused slots
(Deluxe has 51 GUIDs, Kids 35; the first Kids line is blank). Every INI package has a slot and every slot GUID is in the INI.
Each GUID maps to a file through the `[Packages]` section of the INI (`File.ext={GUID}`, ASCII; GUID case and order may differ from guid.lst).
Examples: line 4 `{0EA8A288…}` = ContDlx.akc, 69 = ContStd.akc, 47 = DataDlx.akc.
A GUID has **the same slot in both catalogs** (e.g. `{162B29C8…}` DataESK.akc = 7, `{9CA61D5E…}` MdEDK00.eit = 53). The slots are in GUID order with gaps,
so the slot is probably derived from the GUID. We don't need to recompute it, because guid.lst lists it.
The INI's other sections (`[.AKC]`, `[.EIT]` with values 0/2, `[.WMV]` …) are install-location hints and are not needed for lookup.

**Packed location** (u32, same in ECN and ECS): byte0 = package index, byte1 = local-id bits 16–23, u16 at +2 = local-id bits 0–15.
In u16 pairs that is `(pkgLo | localHi<<8, localLo)`. The high byte really is used (DataDlx has 175,793 records).
**Local ids are dense per package across all five tables**: for each package the ids are exactly 0…n−1, with no duplicates (true for all 38 Deluxe and 20 Kids packages).
A Sw*.eit package is shared by baggage.ecs and generic.ecs: only the union of the two is dense (e.g. SwSTD03: 49 baggage + 178 generic = ids 0…226).
For EIT packages, the **local id is the key's rank in sorted order among all of that package's keys** (baggage and generic merged).
For AKC packages, the local id order is usually not refId order. It is refId order only in DataFStd, DataFDlx, DataFKdc and ContKdc.

**ECN** (numeric key), used by content.ecn, data.ecn and relart.ecn:
```
"ECN\0" | N × u32 packedLocation | N × u32 refId (sorted ascending, parallel array) | u32 N
```
File size = 8 + 8N exactly. The refIds are strictly ascending (no duplicates). Lookup: binary-search refId, then take location[i].

| file | N (Deluxe) | packages (Deluxe) | N (Kids) | packages (Kids) | meaning |
|---|---|---|---|---|---|
| content.ecn | 114,004 | ContDlx 69,949 / ContStd 44,055 | 637 | ContKdc 637 | article and sidebar body XML (761 36k, 186 17.5k, 174 16k, 701 14k, 561 12k, 461 8k…) |
| data.ecn | 477,393 | DataDlx 175,793 / DataFStd 152,433 / DataStd 121,055 / DataFDlx 28,112 | 5,974 | DataKdc 5,874 / DataFKdc 100 | per-item data or metadata records (210 index terms 180k, 701 134k, 761 38k, media…) |
| relart.ecn | 35,576 | RelStd 34,606 / RelDlx 970 | 663 | RelKdc 663 | related-articles lists |

Example: 761575094 (Zebra) → content ContStd #22865, data DataStd #81969 (pkg byte 0x0A, localHi 1), relart RelStd #18691. 761561691 → ContStd #11333, DataStd #70437, RelStd #8667.
The local id is the record key inside that AKC (`akc.md` §3). The record's root `refid` equals the content id for every entry (verified for all 626,973 Deluxe and 7,274 Kids entries by `akc check --all`). `akc::Library` joins the catalog to the AKC files (`akc id <refId>`).

**ECS** (string key), used by baggage.ecs and generic.ecs:
```
"ECS\0" | N × u32 packedLocation | N × fixed-width UTF-16 key, space-padded | u32 N
width = (filesize − 8 − 4N) / N   → baggage 34 B (17 chars), generic 144 B (72 chars), in both catalogs
```
Keys are all lower-case ASCII, unique and strictly ascending in ordinal order. Lookup is by binary search, lower-casing the query.
A 17-character baggage key fills its slot with no padding.
- baggage.ecs (Deluxe 65,892, Kids 9,905): media "baggage" file names, e.g. `0000ea10.gsm` → PiconStd.eit, `.gtn` → ThumbStd.eit, `.iax` → SwSTD02.eit.
  **The file is the EIT member `baggage/<key>`**, case-insensitive (the EIT may store `0000EC5A.iax`).
- generic.ecs (Deluxe 6,408, Kids 6,671): path keys (`dswmedia/iaf/a/0000eebb/t304800a1_globe.swf`, `list/expert.xml`) → Sw*.eit / SwQuiz / SwShared / SwAllDsk / DataESK.
  **The key is the EIT member path itself**, case-insensitive (`dswMedia/…`). The one AKC target is `list/expert.xml` → DataESK.akc.
- Verified: every one of the 72,299 Deluxe and 16,575 Kids string keys that point into an EIT exists as a member of that EIT.
  Each EIT has exactly its catalog entries plus `_label.xml`/`_labelw.xml` (e.g. PiconStd: 7,683 + 2 files).

**Package files on the disc** (all present: 38 used by Deluxe, 20 by Kids). AKCs and the Thumb/Sw-shared EITs sit next to the INI (`EE/ENCARTA`, `EE/KIDS`).
Md*/Picon*/Sw* EITs are in `CONTENT/` (Deluxe) or `EE/KIDS/CONTENT/` (Kids). The disc uses upper-case names, so match ignoring case.
Neither catalog has entries in ContESK/ContEDK/DataEDK (nor Kids in ContECK/DataECK), although those AKCs are on the disc and listed in the INIs. DataESK is used only for `list/expert.xml`. Both catalogs share the ESK/EDK media packages (PiconESK, MdESK00, ThumbEDK, …).

**How a content id resolves**: `<x id="761561691">` → content.ecn → (ContStd.akc, local 11333) → AKC record → article XML.
Its metadata comes from data.ecn → Data*.AKC, and related links from relart.ecn → Rel*.AKC. Media files referenced by name use baggage.ecs → EIT package → `baggage/<name>`.
Kids uses the same scheme in `EE/KIDS/CATALOG.KDC`.

**Implementation**: `catalog.rs` (`Catalog::open(catalog2_dir, ini)`, `resolve_id`, `resolve_key`, `Ecn`/`Ecs` iteration, `package_extents`, `member_path`).
`catalog check` runs every check above. `catalog resolve 761575094` prints the three locations, and `catalog dump` prints one JSON line per entry
(Deluxe 699,273 lines, Kids 23,850).

**Verdict:** needed at conversion time only. Not compressed. Implemented. Modern replacement: SQLite table `item(ref_id PK, kind, package, local_id)`
plus `file(name PK, package)`. Or skip it entirely once AKC records are exported keyed by refId.

---

## 4. Misc files

| file | what | structure / evidence | read by | need? | replacement | effort |
|---|---|---|---|---|---|---|
| EDICT/DICTA.LEX, THESA.LEX, BIL??.LEX | Lexicon for dictionary/thesaurus/bilingual **fuzzy or spelling lookup**. Named by each EIT signature `<LexFile>` | **Decoded**: byte-coded DAWG, see [lex.md](lex.md) | EDICT.EXE / ENCARTAU.DLL with MSSPELL3.DLL | Word lists only | `fst` or SymSpell built from the decoded word lists | done |
| EESPLA01.LEX (748 KB, also in KIDS) | English **speller lexicon** (331,198 words; Kids 104,370) used for search spelling suggestions | **Decoded**, see [lex.md](lex.md) | ENCARTAU.DLL + MSSPELL3.DLL | Word list | SymSpell/fst over the decoded list | done |
| TIMELINE.DAT (2.5 MB) | **Timeline**: 6 categories, 9 regions, 3,404 events, 13,061 items, terms | **Fully decoded, including dates**, see [timeline.md](timeline.md) | ENCARTA.EXE (`CTimelineView`) | **Yes** | JSON/SQLite events | done |
| MINDMAZE.DB (1.4 MB) / .IDX (64 KB) | **MindMaze** question bank: 8,020 questions | **Fully decoded**, see [mindmaze.md](mindmaze.md). IDX = 36 group sizes (9 categories × 4 levels) + (id, offset) pairs; text is CP437 | ENCARTA.EXE (`CMindmazeView`) | Optional (game) | JSON | done |
| P2MTBL.DAT (227 KB) | **Place-to-media table**: atlas place id → content links (articles, flags, Facts and Figures, music, Media Maps photos) | **Decoded**, see [p2mtbl.md](p2mtbl.md) | AREF/COMPNTS/ATCONT.DLL | Yes (atlas) | SQLite `place → [(kind, content id)]` | done |
| PUSHPINS/*.GPD | Empty **user pushpin DB templates** (20–44 B, magic `PPIN..e\0` repeated at the end) | — | AREF/COMPNTS/PPDBMGR.DLL, PPMGR.DLL | No | User data in browser storage or the server DB | – |
| D1–D4.TXT, D1–D4PLACES.TXT | Geography Quiz difficulty settings and place pools (see `atlas-and-3d.md`) | text: `20 / 5 4 3 2 0 / 0 0 0 / .70 .20 .10`. Places rows `type group entity 0` → MAD entity ids | ENCARTA.EXE (`D%d.TXT`, `D%dPLACES.TXT`) | Optional | JSON | S (+MAD) |

---

## Summary of recommendations

| item | need at runtime? | plan | effort |
|---|---|---|---|
| FINDDLX index (IdXE/IdxW/IdXEW/ECT) | no | Rebuild with tantivy. Harvest of the alias/section-title data implemented (`search.rs`, `findidx` CLI) | harvest done, M new search |
| PFX word wheel | no | Harvested (`findidx pfx`) as title/alias/autocomplete source → `fst` | done |
| ITIR `$WW`/`$FX`/`$OBJINST` | no | Ignore, except `$WW$CTCATALOG` (decoded; photo credits) | done |
| catalog2 ECN/ECS + guid.lst | conversion-time only | Implemented (`catalog.rs`, `catalog` CLI). Export to SQLite | done |
| LEX lexicons | no | Decoded; word lists feed our own spell/fuzzy lookup | done |
| TIMELINE.DAT | yes (data) | Decoded | done |
| MINDMAZE.DB/IDX | optional | Decoded | done |
| P2MTBL.DAT | if atlas | Decoded | done |
| GPD, D*.TXT | no / optional | — | – / S |
