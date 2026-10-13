# Atlas side files: `MSWORLD.SDB`, Geography Quiz `D*.TXT`, flags, and the `$WW$CTCATALOG` media catalog

Status:
- `MSWORLD.SDB`: **fully parsed**; two numeric item columns are not yet understood.
- `D1–D4.TXT`, `D1–D4PLACES.TXT`: **fully parsed**; one settings line is not yet understood.
- `$WW$CTCATALOG` leaf records (MSWORLD.ITR and TIMELINE.EIT): **fully decoded**. Every entry parses, and the DATA stream is consumed exactly.
- `dfg/*.dfg` flags: standard BMPs (`atlas-and-3d.md` §2); no decoding needed.

Source: observation of the files and cross-references (P2MTBL, `catalog2`, PFX titles, `baggage.ecs`, `MSWORLD.EIT`).
Readers: `encarta_formats::msworld` (SDB, quiz, catalog record fields) and `encarta_formats::itir_catalog`
(leaf scanner). Dumps: `smallfmt sdb`, `smallfmt quiz`, `smallfmt catalog`, `smallfmt media` (see the end).

---

## 1. `MSWORLD.SDB`: statistics database ("EVGSDB")

`EE/ENCARTA/MSWORLD.SDB`, 1,625,056 B. It is text in **Windows-1252** (`°C`, `Côte d'Ivoire`) with CRLF line ends
(the file ends with a final CRLF). It is used by the atlas statistics maps.

### Grammar
The file is a sequence of **sections**. Each starts with a line `// <name>`. Blank lines are separators only and carry no meaning.
Within a section, data lines are tab-separated.

| section | body |
|---|---|
| `Signature` | one line: `EVGSDB` |
| `Revision` | one line: `18051300` (looks like a date code; kept as a string) |
| `Countries` | count line (193), then `id <TAB> name` |
| `Indicator Category List` | count (12), then `bit <TAB> name`: 1 Basic Indicators, 2 Agriculture, 4 Energy and Minerals, 8 Transportation, 16 Economy, 32 Health, 64 Environment, 128 Population, 256 Trade, 512 Communications, 1024 Education, 2048 Geography |
| `Qualifier List` | count (10), then `id <TAB> text`: 2 N/A, 3 approximate, 4 estimate, 5 average, 6 N/A, 7 N/A, 8 preliminary, 9 Free, 10 Partly free, 11 Not free |
| `Unit List` | count (575), then `id <TAB> unit <TAB> altUnit <TAB> factor <TAB> offset` |
| `¶ Indicator Package List` | (363 times; the name starts with the pilcrow byte 0xB6) **the first occurrence only** has a count line (363) before its row; each section holds exactly one indicator row: `id <TAB> name <TAB> categoryMask <TAB> unitId <TAB> contentId <TAB> source` |
| `Items For The Above Indicator List` | (follows each package) count, then that many item rows (7 columns, below) |

- **Units:** `altUnit` is `N/A` when there is no conversion. The alternate value is `value × factor + offset`
  (°C → °F: 1.8, 17.777777778; km → mi: 0.6213712, 0).
  Unit 1 is a single space (unitless).
- **Indicators:** `categoryMask` is a bit set over the category list (3 = Basic + Agriculture; 66 = Agriculture + Environment).
  `unitId` refers to the unit list (all 363 resolve). `contentId` is a 6315xxxxx id, unique per indicator,
  probably the indicator's definition text (in `data.ecn`). `source` is free text ("United States Geological Survey (USGS)").
  The ids are unique and not dense (36 … 37x).
- **Items** (57,347 rows in all): `countryId <TAB> value <TAB> decimals <TAB> B <TAB> year <TAB> C <TAB> note`

| column | observed | meaning |
|---|---|---|
| countryId | always one of the 193 countries; unique within an indicator | country (also the atlas place id, see `p2mtbl.md`) |
| value | decimal number, possibly negative, never empty | the statistic, written with exactly `decimals` digits after the point |
| decimals | 0–3 | **number of decimal places**: it matches the digits after the point in all 57,347 rows |
| B | 1–20; 8 in 93 % of rows; varies within an indicator (Area: 6, 8, 7, 7, 5, …) | **unknown**. It is not the significant-digit count, and its values go beyond the qualifier ids. |
| year | 1990–2008 | data year |
| C | 0 in 90 %; constant in most indicators (deforestation rate: 15 in every row; compulsory education: 1) | **unknown**. One hypothesis is a year span (a 1990 base year with span 15 gives 1990–2005; 1 gives a "2002/03" school year), unconfirmed. |
| note | `N/A` or `estimate` | footnote text; the words are qualifier names (2, 4) |

- Item rows are sorted by value, descending, within each indicator (all 363). That is the ranking order the UI shows.
- **Needs VM or decompiler analysis:** the meaning of item columns B and C, and how the qualifier list is used
  (its ids 2–11 never appear as numbers in the items). In the VM, compare a country's statistics page with its rows.

---

## 2. Geography Quiz: `D1–D4.TXT` and `D1–D4PLACES.TXT`

`EE/ENCARTA/`. ASCII, tab-separated, CRLF. `n` is the difficulty, 1 (easiest) to 4.

### `Dn.TXT` (35–36 B): settings
```
20                     questions per game
5  4  3  2  0          points for a correct answer at attempt 1, 2, 3, 4, and after that
0  0  0                unknown (D1 0 0 0, D2 1 1 1, D3 2 2 2, D4 4 4 4)
.70  .20  .10          share of questions of place type 0, 1, 2 (sums to 1)
```
| file | points | line 3 | mix |
|---|---|---|---|
| D1 | 5 4 3 2 0 | 0 0 0 | .70 .20 .10 |
| D2 | 10 7 4 2 0 | 1 1 1 | .60 .35 .05 |
| D3 | 15 10 5 2 0 | 2 2 2 | .50 .50 0 |
| D4 | 25 20 10 5 0 | 4 4 4 | .40 .60 0 |

Line 3 is one value per place type and grows with difficulty. It could be a zoom-level offset or a number of hints;
this is **unknown (VM observation)**. The mix maps by position to the place types below.
For example, D3 and D4 ask no continent questions.

### `DnPLACES.TXT` (74 / 203 / 406 / 689 rows): question pool
Each row is `type <TAB> group <TAB> entity <TAB> 0`. The last column is always 0.

| type | rows (D1…D4) | entity | group |
|---|---|---|---|
| 0 | 47 / 97 / 144 / 186 | a **country**: an `MSWORLD.SDB` country id (12 = Australia, 11 = Argentina). The one exception is a row in D4 with entity 2989, which is the city Vadodara (Baroda) according to its P2M article link; this is probably a data error | the region that contains it (27114, 31396, …) |
| 1 | 20 / 99 / 255 / 496 | a **city or other place** (atlas place id, e.g. 3359 Beijing, 5151 Buenos Aires) | its region |
| 2 | 7 / 7 / 7 / 7 | a **continent** (742 Africa, 2129 Asia, …) | 39070, the same for all seven (presumably "World") |

All ids are atlas place ids, the same id space as the `P2MTBL.DAT` keys: 73/74, 202/203, 404/406 and 684/689 entities
are keys there. They are resolved to names and coordinates through the MAD gazetteer (`atlas-and-3d.md` §1) or
through the P2M article links.

---

## 3. `$WW$CTCATALOG` leaf scanner (ITIR catalog B-tree)

Found in `MSWORLD.ITR` and in `TIMELINE.EIT` as a directory holding `BTREE`, `DATA` and `PROPERTY`. Only the catalog
is decoded here. The `$WW*`/`$FX*` word wheels and full-text indexes are not (see `search-catalog-misc.md` §2).

### BTREE file
- An `ITOLITLS` header (version 1, header length 0x28). The fields we use:
  - u32 at 0x124: **page size** (0x2000);
  - u64 at 0x144: **offset of page 0** (0x1DC in both catalogs);
  - u64 at 0x48: end of the page area (page 0 offset + page count × page size).
- Pages are numbered from 0 at that offset. A leaf page starts with `AOLL` and an index page with `AOLI`.
  The catalogs have 131 / 152 leaves and one index page each.
- **Leaf page header (0x30 bytes):**

| off | type | field |
|---|---|---|
| 0 | 4 B | `AOLL` |
| 4 | u32 | number of unused bytes at the end of the page (the free space plus the quick-reference table) |
| 8 | u64 | this page's number |
| 0x10 | u64 | previous leaf (all ones = none) |
| 0x18 | u64 | next leaf (all ones = none) |
| 0x20 | u64 | ordinal of the first entry on this page (running count) |
| 0x28 | u32, u32 | 1, 0 (constant) |

- Entries run from page offset 0x30 to `pageSize − unused` with no gaps. The leaves form a chain: start at the leaf
  whose previous field is "none" (page 0 in both files) and follow the next pointers. The index page is not needed.
- **Entry:**

```
u32     key                      (ascending over the whole chain)
varint  entryLength              (7-bit groups, least significant first)
entryLength bytes:
  u8      1                      (constant)
  u8      inline                 (1 = property block inline, 0 = property block in DATA)
  encint  inlineLength           (0 when inline = 0)
  encint  dataLength             (bytes owned in DATA; 4 when inline = 1)
  encint  dataOffset             (into DATA)
  inlineLength bytes             (the property block when inline = 1)
```
  `encint` is the CHM-style big-endian base-128 integer (7-bit groups, **most** significant first, high bit = more).
  Note that `entryLength` uses the opposite, little-endian, order.
- **DATA**: entries own consecutive byte ranges (`dataOffset` of entry *i*+1 = `dataOffset + dataLength` of entry *i*),
  and the last range ends exactly at the end of DATA. Each range starts with a u32 that is always 0. For entries
  that are not inline, the property block follows that u32 within the range. A property block goes to DATA exactly when it
  (counting its own length field) is longer than 255 bytes: the largest inline block is 255 B and the smallest external one 257 B.
  MSWORLD has 789 such entries; in TIMELINE all are inline.
- **Property block:**
```
u32  blockLength                 (bytes after this field)
u8   presentMask                 (bit 7 = property 0, bit 6 = property 1, …)
for each set bit, from high to low:  u32 byteLength, UTF-16LE text including its terminating NUL
```
  `blockLength` equals the bytes consumed in every entry of both files.

| catalog | entries | key | mask | properties |
|---|---|---|---|---|
| MSWORLD.ITR | **5,413** | the media content id (631…), the same ids P2MTBL links to | 0xC0 | 0 = media record string, 1 = title |
| TIMELINE.EIT | **22,749** | 0 … 22,748 (dense) | 0x80 | 0 = event title (sorted by title; SGML entities such as `&Omacr;` kept; see `timeline.md`) |

### MSWORLD media record string (property 0)
The string is `type;` followed by `;`-separated fields. Each field is one letter followed by its value.

| type | count | field order | P2MTBL kinds that link it |
|---|---|---|---|
| `i` image | 4,775 | `L B P U M C` (3,191) or `L B P U C` (1,584) | photo kinds 5, 37, 69, 101, 133 |
| `w` sound | 330 | `L B C U` | 12 (music) |
| `m` animal image | 308 | `L M B P U C` | 165 (animals) |

| field | meaning | evidence |
|---|---|---|
| `L` | atlas place id where the item is pinned | 3,828 of 5,413 are P2MTBL keys. For animals it is a continent or region (742 Africa) |
| `B` | main file name (`14632.jpg`, `T165193A.jpg`, `T090206A.wav`) | 3,279 / 4,775 image names and 252 / 308 animal names are in `baggage.ecs` (for example `14632.jpg` is in MDDLX01.EIT); the sound files are not on the disc under these names |
| `P` | thumbnail name (`14632.JSM`) | in `baggage.ecs` (3,292 + 252); the stem names `CONTENT/MSWORLD.EIT/dtn/<stem>.dtn` for 4,765 + 308 records |
| `U` | paired item: a sound links to the photo shown while it plays, and vice versa; 0 = none | 656 of 709 non-zero values are keys of this catalog (i→w 326, w→i 330) |
| `M` | a further 631… id, distinct from the key; it does not occur in the 2009 catalogs | probably a legacy (2001) media id. Kept, but unresolved |
| `C` | **credit line** ("Paul A. Souders/Corbis"); for sounds, a full recording credit | always the last field except in sounds, where `;U…` follows it |

Parsing rule for `C`: the value runs to the end of the string, or, if the rest of the string ends with `;U<digits>`,
up to that suffix. No image credit contains `;`.

Property 1 is the display title ("Legislative Building in Winnipeg").

---

## CLI
```
smallfmt sdb      EE/ENCARTA/MSWORLD.SDB
smallfmt quiz     EE/ENCARTA/D1.TXT EE/ENCARTA/D1PLACES.TXT
smallfmt catalog  <dir containing BTREE and DATA>          # generic: key + property strings
smallfmt media    extracted/eit/EE/ENCARTA/MSWORLD.ITR     # MSWORLD catalog, record fields split out
```
