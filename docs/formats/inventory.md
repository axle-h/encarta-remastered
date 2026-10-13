# Format inventory

Source: `vendor/Microsoft Encarta Premium Edition 2009.iso` (2.9 GB DVD; Encarta Premium + Kids).
Extracted to `extracted/iso` (ISO), `extracted/eit` (all ITOLITLS containers), `extracted/finddlx` (search CAB).
No file anywhere on the disc is encrypted. Every opaque format is a compressor.

| Area | Files | Format | Status | Effort |
|---|---|---|---|---|
| Media containers | 80 × `.EIT`, `CATALOG.DLX` | LIT/ITOLITLS + LZX | **Done** (`eit` tool) | — |
| Images, audio, video, Flash, MIDI | inside EITs + `CONTENT/DSV/*.WMV` | JPEG/GIF/PNG/BMP, WAV/WMA/MP3, WMV1, SWF | Standard; transcode with ffmpeg | S |
| **Article bodies** | `CONT*.AKC` (93,872 records in DLX) | custom XML-token Huffman ("DicT") | Architecture known ([akc.md](akc.md)) | **L — critical path** |
| Article metadata, related articles | `DATA*.AKC`, `DATAF*.AKC`, `REL*.AKC` | same codec | Same decoder | (incl.) |
| Sidebars | `MD*.EIT/baggage/*.xml` | plain XML | Readable | S |
| Dictionary, thesaurus, translation | `EDICT/*.EIT` (~105k entries) | plain XML per id | Readable | S |
| Interactives | `*.iax` + `.swf` | XML + Flash | Readable; play via Ruffle | S–M |
| Catalog (id → package/record) | `catalog2/*.ecn/.ecs` | simple tables | Decoded ([search-catalog-misc.md](search-catalog-misc.md)) | S |
| Titles, aliases, section heads | `FINDDLX.CAB` PFX/IdXE | plain records | Decoded; export once | S |
| Search | `FINDDLX.CAB`, ITIR `$WW/$FX` | B-tree + postings | Not needed; rebuild with tantivy | M |
| Timeline | `TIMELINE.DAT` + `.EIT` | records + string pool | Partly decoded | M |
| Atlas rasters | `*.MAD` Terrain section | MOBB codec, palette rasters | Layout known ([atlas-and-3d.md](atlas-and-3d.md)) | M–L |
| Atlas vectors, places | `*.MAD` grids | ~30 undocumented tables | Biggest risk | XL (full decode — no substitution) |
| World stats, flags | `MSWORLD.SDB`, `MSWORLD.EIT` | text, BMP | Readable | S |
| 3D tours | `CONTENT/WORLD/7015/*.EIT` | DirectX `.x` + 6 proprietary files | Triaged | M (static) / L (interactive) |
| Director/Shockwave bits | `.dcr/.cct/.swa` | Shockwave | No modern player except emerging ones | decode via Shockwave player work (see plan) |
| Games (MindMaze, Geo Quiz) | `MINDMAZE.*`, `D*.TXT` | simple | Required | S each |
| Spelling lexicons | `*.LEX` | Megalith | Decode word lists; drive our own spell/fuzzy lookup | S–M |

Original UI: XML + XSL (`ENCXSL.EIT`, ~200 templates) rendered in embedded IE, with
`encarta://view=ERSArticleView/id=…` and `encdata:` URLs. Use them as a **reference for the
XML schema's meaning only**. Write our own templates and components rather than shipping theirs.

See [`docs/CLEAN_ROOM.md`](../CLEAN_ROOM.md) for the rules and [`docs/PLAN.md`](../PLAN.md) for the plan.
