# TIMELINE.DAT: the Encarta Timeline

Status: **structure fully decoded; every byte of the file is accounted for.** Dates are decoded and
cross-checked. A few semantic labels are inferred, not confirmed (listed at the end).
Reader: `crates/encarta-formats/src/timeline.rs`. Exploration notes: this file only.

Source of knowledge: observation of `EE/ENCARTA/TIMELINE.DAT` (2,523,290 bytes), cross-referenced
with `catalog2/content.ecn` / `data.ecn`, the PFX title list (`tools/explore-py/pfx.py`) and the
keys of `TIMELINE.EIT/$WW$CTCATALOG`. All integers are little-endian. Strings are UTF-16LE.

## 1. Layout

| offset | size | content |
|---|---|---|
| 0x00 | 24 | zero bytes (reserved) |
| 0x18 | 28 | **section descriptor**: 7 × u32 (below) |
| 0x34 | 4 | u32 category count (6) |
| 0x38 | 4 | u32 region count (9) |
| 0x3C | var | category list, then region list (§2) |
| after lists | `eventBytes` | **event table**: fixed 70-byte records (§3), 3,404 records |
| next | `itemBytes` | **item table**: same 70-byte record layout, 13,061 records (§4) |
| next | `stringBytes` | **title pool**: UTF-16LE strings addressed by the records |
| next | `termBytes` | **term table**: 16-byte entries (§5), 22,749 entries |
| next | `termRefBytes` | **term reference pool**: u32 refIds, 190,993 values |

The last section ends exactly at end of file.

Section descriptor at 0x18 (values from the disc):

| field | value | meaning |
|---|---|---|
| u32 | 0x1C (28) | byte length of this descriptor, itself included (7 × 4) |
| u32 eventBytes | 0x3A2C8 = 238,280 = 3,404 × 70 | event table size |
| u32 itemBytes | 0xDF35E = 914,270 = 13,061 × 70 | item table size |
| u32 stringBytes | 0x3B248 = 242,248 | title pool size |
| u32 | 1 | **unknown** (a flag or version; always 1) |
| u32 termBytes | 0x58DD0 = 363,984 = 22,749 × 16 | term table size |
| u32 termRefBytes | 0xBA844 = 763,972 = 190,993 × 4 | term reference pool size |

## 2. Category and region lists

Each list entry is `utf16z name` followed by `u32 bit`. The bit is a single flag that the records
use in their category and region masks. All categories come first, then all regions.

| category | bit | | region | bit |
|---|---|---|---|---|
| Arts | 0x08 | | Africa | 0x040 |
| Geologic Time | 0x20 | | East Asia | 0x004 |
| People and Society | 0x10 | | Europe | 0x001 |
| Philosophy and Religion | 0x04 | | Global | 0x010 |
| Political History | 0x02 | | North America | 0x002 |
| Science and Technology | 0x01 | | Oceania | 0x100 |
| | | | South America | 0x080 |
| | | | South and Central Asia | 0x020 |
| | | | Southwest Asia | 0x008 |

Check: the union of every record's category mask is 0x3F and of every region mask 0x1FF, so the
masks use exactly these bits. Examples: Battle of Hastings → region 0x001 (Europe);
Japanese Attack Pearl Harbor → 0x106 (Oceania + East Asia + North America);
Spinoza Describes a Rational Universe → category 0x04 (Philosophy and Religion).

## 3. The 70-byte record

| off | type | field | notes |
|---|---|---|---|
| 0x00 | u32 | **date flags** | bit meanings below |
| 0x04 | u32 | title offset | byte offset into the title pool |
| 0x08 | u32 | title length | bytes, **including** the 2-byte NUL terminator |
| 0x0C | 24 | zero | always zero |
| 0x24 | u32 | start year magnitude | 0 = no date (only in the item table) |
| 0x28 | u16 | start month | 1–12, 0 = unknown |
| 0x2A | u16 | start day | 1–31, 0 = unknown |
| 0x2C | u32 | end year magnitude | 0 = no end date |
| 0x30 | u16 | end month | 0 = unknown |
| 0x32 | u16 | end day | 0 = unknown |
| 0x34 | u16 | importance | 1 (most prominent) … 5; 0 on four records. VM: importance 1 (First Moon Landing) has a larger title than 3 (Battle of Hastings) |
| 0x36 | u16 | category mask | bits from §2 |
| 0x38 | u16 | region mask | bits from §2 |
| 0x3A | u32 | sequence number | unique across both tables (1…16,465) |
| 0x3E | u32 | refId | content id of the event body (174…, 461…, 701…) |
| 0x42 | u16 | record kind | 1 = event, 3 = historical essay (VM: a two-tone striped bar with a thumbnail and the label "HISTORICAL ESSAY: ‹TITLE›", whatever flag 0x01 says), 0 = item-table entry |
| 0x44 | u16 | zero | always zero |

Titles are UTF-16LE and may contain inline markup (`<it>…</it>` for italics). Each of the 16,465
records points at its own string. The pool is exactly the concatenation of them (the last record's
string ends at `stringBytes`). Item-table records mostly point at an empty string (just the NUL).

### 3.1 Date encoding (solved)

A date is `(year magnitude, month, day)` plus four flag bits from the record's `date flags`:

| bit | meaning | evidence |
|---|---|---|
| 0x01 | **span/period**: drawn as a horizontal orange bar from start to end with a boxed upper-case label ("WORLD WAR II"). **Confirmed in the VM** (World War II, Han Dynasty) | set on dynasties, empires, wars, eras: Han Dynasty, Roman Empire, World War II, Paleozoic Era |
| 0x02 | **start circa** (approximate). **Confirmed**: the pop-up prints a trailing "?" on the year | 98% of these have a year-only start ("Maya Use Symbol for Zero", 50) |
| 0x04 | **start is BC** | Battle of Actium 31, Horace Writes *Odes* 23 |
| 0x08 | **end circa** (approximate). **Confirmed** (as 0x02) | almost always paired with an end year |
| 0x10 | **end is BC** | Ptolemaic Dynasty 323–30 BC, Qin Dynasty 221–206 BC. Bantu Expansion has 0x04 but not 0x10: 2000 BC – AD 1000 |
| 0x20 | **start magnitude is in thousands of years** | Solar Nebula Forms: 4,700,000 → 4.7 billion years BC |
| 0x40 | **end magnitude is in thousands of years** | Archean Eon (0x75): 3,800,000 k – 2,500,000 k = 3.8–2.5 billion BC. Proterozoic Eon (0x35): 2,500,000 k BC – 570,000,000 BC |

Decoded signed year: `y = magnitude × (1000 if thousands bit) × (−1 if BC bit)`. There is no year 0
(no record has magnitude 0 together with a date), so −1 means 1 BC.

Confirmation: with this decoding the event table is sorted ascending by `(signed start year, month, day)`
with no exceptions over 3,404 records. Also, `end ≥ start` holds for every event that has an end date,
except one data-entry error (below). Worked examples:

| title | raw (flags; start; end) | decoded |
|---|---|---|
| Solar Nebula Forms | 0x24; 4,700,000 | 4.7 billion BC |
| Dinosaurs Become Extinct | 0x04; 65,000,000 | 65,000,000 BC |
| Battle of Hastings | 0x00; 1066/10/14 | 14 Oct 1066 |
| First Moon Landing | 0x00; 1969/7/20 | 20 Jul 1969 |
| Japanese Attack Pearl Harbor | 0x00; 1941/12/7 | 7 Dec 1941 |
| World War II | 0x01; 1939/9/1; 1945/9/2 | period, 1 Sep 1939 – 2 Sep 1945 |
| Jesus Christ | 0x0E; 4; 29 | c. 4 BC – c. AD 29 |

### 3.2 How the Timeline view shows a record (reference VM, `fixtures/timeline/`)

- Point events (no 0x01, kind 1) are a square and the title; Jesus Christ (flags 0x0E, has an end date but
  no 0x01) is a point at 4 BC. The point labels show no circa mark.
- Periods (0x01) and essays (kind 3) are bars; their label is the title in upper case, essays prefixed
  "HISTORICAL ESSAY: ".
- The event pop-up date line: circa is a trailing "?" on the year and a range that starts BC and ends AD
  writes "AD" on the end: **"4? BC - AD 29?"**. Full dates read "June 16, 1963" and ranges
  "October 12, 1964 - October 13, 1964" (VM 2026-10-14). Years before 8000 BC are shown counted back from 2000,
  without the circa "?": 9000? BC → "11,000 years ago", 118000? BC → "120,000 years ago", 938000 BC - 878000 BC →
  "940,000 years ago - 880,000 years ago"; from a million on with two decimals: "2.50 million years ago",
  "540.00 million years ago", "4.60 billion years ago". 8000 BC and later stay "8000 BC", "7000? BC" (no event lies
  between 9000 and 8000 BC, so the converter cuts at "before 8000 BC"). Captures: `fixtures/timeline/dates/`.
- A larger importance number is less prominent: the title of importance 1 is drawn largest.

The converter stores these as `timeline_event.display` (`point` | `period` | `essay`), `label`,
`date_text`, `start_text`, `end_text` and `prominence` (6 − importance; 0 when importance is 0)
(`docs/content-schema.md` §9).

Known data errors (kept verbatim by the reader):
- *Columbus Reaches the New World*: end fields are `(10, 12, 1492)`. The editor entered the end
  as month/day/year, so it means 12 Oct 1492.
- *Václav Havel Elected Czech President*: start day field is 1989.

So month and day are kept as raw u16 values. A consumer should treat month > 12 or day > 31 as invalid.

The geologic "thousands" magnitudes are years **before present** in the original sense ("4.7 billion
years ago"). They are encoded as BC, which at that scale makes no difference.

## 4. Item table

Same 70-byte layout. Every date field, flag, importance and mask is zero, and the kind is 0. Only the
title, the sequence number and the refId carry data.
- The first 28 items have titles (Sumerian Civilization, French Revolution, Internet, …). Their
  refIds are the **topic timelines** (PFX title "Sumerian Civilization Topic Timeline", "Timeline: 14th Century").
  Their sequence numbers are exactly the 28 numbers in 1…3,432 that the event table does not use.
- The other 13,033 items have empty titles, sequence numbers 3,433…16,465 (sequence = record index + 1),
  and 701… refIds. Their titles are in the PFX word wheel (class 203): event titles ("Dodo Becomes Extinct"),
  topic timelines, and search terms ("1 million years ago", "Aborigines").
  This is the list of timeline search targets. Nothing else in the file points at it.

## 5. Term index (term table + reference pool)

Each 16-byte term entry is:

| off | type | field |
|---|---|---|
| 0 | u32 | **unknown offset A**: cumulative, non-decreasing |
| 4 | u32 | **term text offset**: cumulative byte offset of this term's text (UTF-16Z) in a term-text pool |
| 8 | u32 | reference offset: byte offset into the reference pool |
| 12 | u32 | reference count |

- Reference offsets are dense. `refOffset[k+1] = refOffset[k] + 4 × count[k]`, starting at 0 and ending at `termRefBytes`.
- References are event refIds. All but 3 of the distinct values (0, one 461… and one 174…) are refIds of event-table records.
- **Term k is item number k of `TIMELINE.EIT/$WW$CTCATALOG`.** The catalog key for item k is the
  term's text (entity-encoded, e.g. `&Idot;znik Ware Thrives in Turkey`, `10th Century BC`, `Gold Rush`).
  Confirmed by the event lists: term 7 "10th Century" lists AD 901–1000 events, term 128 "4 - 1 million years ago" lists *Australopithecus*.
  Also, `textOffset[k+1] − textOffset[k]` equals the UTF-16Z byte length of catalog item k's title for all 22,748 consecutive pairs.
  This was checked with the exact leaf scanner (`msworld.md` §3: the catalog key is the item number 0…22,748 and the title is property 0).
  The term-text pool itself is not in TIMELINE.DAT. The catalog is the source of the text.
- Unknown offset A advances by the length of a *second* UTF-16Z string per term. It differs from
  the term text when terms repeat (e.g. five "Aachen" entries advance A by 40, 32, 56, 68, 62 bytes).
  It is not in the catalog record: the record holds only the title. It matches the title length for 13,577 of 22,748 terms.
  It is not the referenced event's title either. A likely source is the `$WWTimelineWW` word wheel's display strings, which are not decoded yet. **Unresolved.**

## 6. Unknowns (need decompiler analysis or VM observation)

1. Descriptor field 5 (always 1).
2. Term entry field 0 ("offset A"): which string pool it addresses. Not visible in the UI (VM 2026-10-15: the
   timeline search lists event titles; "charities" finds the `charity` events, so the strings may be the searchable
   word forms of each term, e.g. A advances by 9 characters for `charity`); decoding `$WWTimelineWW` would tell.
3. ~~Inferred labels~~: confirmed in the VM (§3.2): 0x01 = period bar, 0x02/0x08 = circa ("?"), kind 3 =
   essay, importance 1 = largest title. Still open: importance 0 on 4 records, and how importance maps
   to zoom levels.

## 7. Modern target

JSON or SQLite rows `{seq, refId, kind, title, start:{year,month,day,circa}, end:…, span, importance,
categories:[…], regions:[…]}` plus a `term → [refId]` table (texts from the TIMELINE.EIT catalog).
The term index can also be regenerated with tantivy.
