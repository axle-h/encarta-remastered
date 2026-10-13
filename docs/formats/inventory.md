# Format inventory

Source: `vendor/Microsoft Encarta Premium Edition 2009.iso` (2.9 GB DVD; Encarta Premium + Kids).
Extracted to `extracted/iso` (ISO), `extracted/eit` (all ITOLITLS containers), `extracted/finddlx` (search CAB).
No file anywhere on the disc is encrypted. Every opaque format is a compressor.

| Area | Files | Format | Status | Effort |
|---|---|---|---|---|
| Media containers | 80 × `.EIT`, `CATALOG.DLX` | LIT/ITOLITLS + LZX | **Done** (`eit` tool) | — |
| Images, audio, video, Flash, MIDI | inside EITs + `CONTENT/DSV/*.WMV` | JPEG/GIF/PNG/BMP; WAV (GSM 6.10, MS ADPCM), WMA v2 (+1 Pro), MP3, SWA; WMV1 (308) + WMV2 (10); SWF | **Done** ([media.md](../media.md); `encarta-convert media`) | — |
| **Article bodies** | `CONT*.AKC` (ContDlx 69,949 + ContStd 44,055 records) | custom XML-token Huffman ("DicT") | **Done** ([akc.md](akc.md); `akc` tool): 634,248 records / 2.14 GB decode to well-formed XML, spec hashes reproduced | — |
| Article metadata, related articles | `DATA*.AKC`, `DATAF*.AKC`, `REL*.AKC` (+ Kids `*KDC.AKC`) | same codec | **Done** (same decoder) | — |
| Sidebars | `MD*.EIT/baggage/*.xml` | plain XML | Readable | S |
| Dictionary, thesaurus, translation | `EDICT/*.EIT` (~105k entries) | plain XML per id | Readable | S |
| Interactives | `*.iax` + `.swf` | XML + Flash | Readable; play via Ruffle | S–M |
| Catalog (id → package/record) | `catalog2/*.ecn/.ecs` + `guid.lst` (Deluxe + Kids) | simple tables | **Done** (`catalog` tool; whole-disc tests) | — |
| Titles, aliases, section heads | `FINDDLX.CAB` PFX/IdXE/IdxW/IdXEW/IdXCM (DLX + KDC) | plain records, B-tree, postings | **Done** (`findidx` tool, JSON-lines export) | — |
| Search | `FINDDLX.CAB`, ITIR `$WW/$FX` | B-tree + postings | Not needed; rebuild with tantivy | M |
| Timeline | `TIMELINE.DAT` + `.EIT` | records + string pool | **Done** ([timeline.md](timeline.md); `smallfmt timeline`) | — |
| Atlas rasters | `*.MAD` Terrain section | MOBB codec, palette rasters, octahedral projection | **Done** ([mad.md](mad.md); `mad` tool): all 445,176 tiles | — |
| Atlas vectors, places | `*.MAD` grids | MOBB codec, ~30 tables | Geometry and gazetteer **done** ([mad.md](mad.md); `mad geom/entity`); styling **done** ([mad-styling.md](mad-styling.md)); converted to `out/atlas/` ([atlas-output.md](../atlas-output.md)) | — |
| World stats, flags | `MSWORLD.SDB`, `MSWORLD.EIT`, `MSWORLD.ITR` `$WW$CTCATALOG` | text, BMP, ITIR leaf pages | **Done** ([msworld.md](msworld.md); `smallfmt sdb|media`) | — |
| 3D tours | `CONTENT/WORLD/7015/*.EIT` | `.x`, `.3sc`, `.3wm`, `.3lt`, `.3cl`, `.32d`, `.3vp`, images, cursors | **Decoded** ([tours-3d.md](tours-3d.md); `tour` tool); runtime behaviour specified, VM checks open | L (interactive runtime) |
| Director/Shockwave | 35 unique (×2 Kids copies) `.dcr/.cct` + 2 `.swa` in SWSHARED/SWALLDSK.EIT | Director 8.5 Afterburner + SWA (MP3) | **Done** ([shockwave.md](shockwave.md); `director` tool): City Climate and Population Chart data → JSON, SWA → MP3; the rest are orphaned | S–M (rebuild the charts, Step 7) |
| Games (MindMaze, Geo Quiz) | `MINDMAZE.*`, `D*.TXT` | simple | **Done** ([mindmaze.md](mindmaze.md), [msworld.md](msworld.md)) | — |
| Place → media links | `P2MTBL.DAT` | block table | **Done** ([p2mtbl.md](p2mtbl.md); `smallfmt p2m`) | — |
| Spelling lexicons | `*.LEX` | Megalith byte-coded DAWG | **Done** ([lex.md](lex.md); `smallfmt lex`) | — |

Original UI: XML + XSL (`ENCXSL.EIT`, ~200 templates) rendered in embedded IE, with
`encarta://view=ERSArticleView/id=…` and `encdata:` URLs. Use them as a **reference for the
XML schema's meaning only**. Write our own templates and components rather than shipping theirs.

See [`docs/CLEAN_ROOM.md`](../CLEAN_ROOM.md) for the rules and [`docs/PLAN.md`](../PLAN.md) for the plan.
