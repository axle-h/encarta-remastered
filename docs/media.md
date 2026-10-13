# Media: inventory and conversion (Step 2, media half)

Status: **implemented** in `crates/encarta-convert` (`src/media.rs`, `src/media/`), run over the whole disc.
Command: `encarta-convert [--extracted DIR] [--out DIR] media` (defaults `extracted/`, `out/`).
Outputs: content-addressed files under `out/media/`, the manifest `out/media/manifest.sqlite`, and the
completeness report `out/report/media.json` plus `out/report/media-members.tsv` (one line per file on the disc).
Progress goes to stderr and `out/report/media-progress.log`.

The run is **idempotent and resumable**: re-running skips every output whose manifest row has the current
recipe and whose file still has the recorded size and SHA-256, so an interrupted run continues where it
stopped, and changing one recipe redoes only that derivative. Knobs (environment, so `main.rs` stays generic):
`MEDIA_JOBS` (worker pool size, default 8; video jobs use 2 ffmpeg threads each) and `MEDIA_ONLY`
(comma-separated substrings of `container/member`, for test subsets; use a separate `--out`).

Tools: the system `ffmpeg`/`ffprobe` (subprocesses, one per job step, with time-outs) and the `image` crate
(BMP/ICO decoding, PNG encoding, image headers). Encoders are detected at start-up and recorded in the
manifest's `run` table; on the reference machine (ffmpeg 8.1.2) they are libopus, libfdk_aac, libx264,
libvpx-vp9, libwebp/libwebp_anim and libaom-av1. A missing encoder drops that derivative from the plan.

---

## 1. Inventory

All 437,049 members of the 93 unpacked containers (`extracted/eit`) and all 726 loose ISO files were
classified; the media pipeline sniffs every file's leading bytes (`media/sniff.rs`) and **never trusts
the extension**. Media totals below are from the full run (§6).

### 1.1 What the odd extensions are

| ext | where | sniffed | what it is |
|---|---|---|---|
| `.jpg` | MD\*, SW\*, SWQUIZ, THUMBSTD, tours | JPEG | article photos, IAF slide images, quiz images, tour textures. One tour file (`05917/Kevin.jpg`) is a BMP |
| `.jsm` / `.gsm` | PICON\* | JPEG / GIF (16 `.gsm` are PNG) | "picons": gallery-size pictures of each media item (typically 216×192 / 192×192) |
| `.jtn` / `.gtn` | THUMB\* | JPEG / GIF | thumbnails, at most 96×96 (typically 64×64) |
| `.gmo` | THUMBDLX/STD/KDC | GIF (2 PNG) | 12 banner-like images (220–252 × 53–90); purpose unconfirmed |
| `.svj` | MDDLX01/02 | JPEG | 65 wide panoramas (1440–3120 × 308–500) |
| `.dib` | MDDLX01/02, MINDMAZE | BMP (4/8-bit, some RLE) | article charts/maps; MindMaze room art |
| `.dtn` | MSWORLD.EIT `dtn/` | BMP 64×64 8-bit | atlas "media map" pushpin thumbnails (msworld.md §3) |
| `.dfg` | MSWORLD.EIT `dfg/` | BMP 64×64 8-bit | country flags; background key colour **(0, 254, 0)** |
| `.bmp` | tours | BMP (one is a GIF: `05285/ioban3.bmp`) | 3D textures, HUD strips |
| `.cur` | tours | Windows cursor (ICO type 2) | 12 navigation cursors per tour (same 12 everywhere) |
| `.wav` | PRON (50,229), MINDMAZE (35) | RIFF/WAVE | pronunciations: **GSM 6.10, 11,025 Hz mono**; MindMaze ambience: MS ADPCM 22,050 Hz |
| `.wma` | MD\* | ASF | narration/music/speeches: WMA v2 (1,635 of 1,675 are 22,050 Hz mono), 8 files carry two bit-rate variants as separate streams, 1 is WMA Pro |
| `.wmv` | ISO `CONTENT/DSV`, `EE/KIDS/CONTENT/DSV` | ASF | 318 videos (215 unique; the Kids folder is a subset): **WMV1** (308) / WMV2 (10) 320×240 15 fps, WMA v2 audio |
| `.mp3` | SW\*, SWQUIZ, SWKDC | MPEG audio (1,277 with ID3, 432 bare) **or SWA (71)** | IAF slide-show narration and quiz audio. 71 `.MP3` files in `dswMedia/IAF/a/<id>/` are really Shockwave Audio (header length field 0x140, except 4 with 0x164) |
| `.swa` | SWALLDSK, SUPPORT | SWA | City Climate / Population Chart narrations (shockwave.md §4) |
| `.smi` | MD\* | SAMI | captions. Paired by stem with a `.wma` (1,497 stems), a `.wmv` (all 215), or a `.mid` (236) |
| `.mid` | MD\*, MINDMAZE | Standard MIDI (format 1, 480 ppq in the files sampled) | music |
| `.swf` | SW\*, SWQUIZ, SWSHARED | `FWS`/`CWS` | Flash interactives and the IAF player (`dswMedia/shared/players/IAFPlayer/`) |
| `.iax` | SW\*, SWQUIZ | UTF-8 XML (`<iai …>`) | interactive descriptors (§2.7) |
| `.dcr` / `.cct` | SWSHARED, SWALLDSK | Director Afterburner (`XFIR`) | shockwave.md |
| `.xml` | MD\*/SW\* `baggage/` | XML | sidebars (content converter); `_label.xml`/`_labelw.xml` in every EIT are build labels |

Other members (dictionary XML, XSL templates, ITIR indexes, tour data files, catalog tables, MindMaze lists)
are not media; §4 lists who owns them.

### 1.2 Media totals by kind

See `media_by_kind_extension_format` in `out/report/media.json` for the full table (kind × extension ×
sniffed format, files and bytes). Summary of the full run:

| kind | files | unique sources | source bytes (unique) | notes |
|---|---|---|---|---|
| image | 76,669 | 66,989 | 881 MB | 56,568 JPEG, 6,362 GIF (10 animated), 16 PNG, 4,043 BMP |
| audio | 53,723 | 53,138 | 261 MB | 50,264 WAV, 1,675 WMA, 1,709 MP3, 75 SWA (71 named `.MP3`); 50,229 GSM pronunciations |
| video | 318 | 215 | 263 MB | 5,940 s of 320×240 video |
| captions | 2,198 | 1,883 | 1.9 MB | SAMI |
| flash | 2,794 | 2,240 | 144 MB | SWF |
| interactive | 4,345 | 2,514 | 18.5 MB | 4,343 `.iax` + 2 `flashvr.xml` |
| midi | 267 | 244 | 2.0 MB | |
| director | 42 | 21 | 0.3 MB | live City Climate / Population Chart files (orphans are omitted) |
| cursor | 84 | 11 | 3.6 KB | the same 12 names in every tour (one pair identical) |
| **total** | **140,440** (1.65 GB with copies) | **127,255** | **1.57 GB** | |

Largest image 3,120×2,055; no WebP size limit was hit.

---

## 2. Conversion rules

Every unique byte string (SHA-256) is one **source**; the 140,440 media files collapse to 127,255 sources
(Kids containers repeat Deluxe ones byte for byte). Each source gets the outputs of its kind:

| kind (rule) | sources | original | derivatives |
|---|---|---|---|
| image: JPEG | jpg/jsm/jtn/svj | kept | WebP (lossy q80, method 4), AVIF (libaom, crf 30, cpu-used 6, still picture) |
| image: GIF / PNG | gif/gsm/gtn/gmo | kept | WebP **lossless**; animated GIFs → animated WebP (loop forever) |
| image: BMP | dib/dtn/dfg/bmp | kept | PNG (lossless; RGBA only when keyed), WebP lossless from that PNG |
| cursor | cur | kept | PNG of the largest image; hotspot in `meta` |
| audio: WAV/WMA | wav/wma | kept | Opus (Ogg, VBR) + AAC (`.m4a`, faststart) |
| audio: SWA | swa and SWA-in-`.MP3` | kept | MP3 (header stripped, bytes unchanged) + Opus + AAC |
| audio: MP3 | mp3 | kept | none: every browser plays MP3, and the Flash players load these files by name |
| video | wmv | kept | MP4 (H.264 High, CRF 23, preset slow, AAC), WebM (VP9 CRF 34, Opus), poster WebP (frame at min(1 s, half the length)) |
| captions | smi | kept | WebVTT, one track per caption class |
| MIDI | mid | kept | none (played with a soundfont synth in the browser) |
| Flash | swf | kept | none (Ruffle) |
| interactive | iax | kept + root attributes in `meta` | none; file references → `iax_ref` |
| IAF player config | `flashvr.xml` | kept | none |
| Director | live `.dcr`/`.cct` | kept | none per file; the two chart datasets are exported as JSON (§3) |

Derivatives keep the source's pixel size (the disc already has its own thumbnail sizes). WebP has a
16,383-pixel limit; nothing on the disc reaches it.

### 2.1 Colour keys (BMP → PNG with alpha)

- **Tour BMP textures** (`CONTENT/WORLD/7015/*.EIT`, sniffed BMP, including `Kevin.jpg`): a pixel is
  transparent when it quantises to magenta in RGB555, i.e. R ≥ 248, B ≥ 248 and G < 8 (tours-3d.md §5.5).
  Only BMPs are keyed; tour GIFs and JPEGs stay opaque (`ioban3.bmp` is a GIF and is not keyed).
  Result: 113 keyed tour texture files (101 unique), exactly the per-tour counts of tours-3d.md (acr 18, rom 4,
  bea 21, per 50, col 10, abu 4, azuchi 6).
- **Flags** (`MSWORLD.EIT/dfg/*.dfg`): the background is pure green. On the disc it is **(0, 254, 0)**, not
  (0, 255, 0) as atlas-and-3d.md §2 says; it is the only near-green colour in any flag, so the rule is the same
  RGB555 test (R < 8, G ≥ 248, B < 8). 189 of 191 flags have key pixels (`223.dfg` and `253.dfg` are full-bleed).
- Unkeyed BMPs become opaque RGB PNGs. The key and the number of keyed pixels are in the `png` row's `meta`.

### 2.2 Audio rates ("rate-rule-1")

The stream with the most channels, then the highest sample rate, is used (some WMAs carry two variants).
More than two channels are downmixed to stereo.

| source | Opus | AAC |
|---|---|---|
| ≤ 16 kHz, mono (GSM pronunciations) | 24 kbit/s | 48 kbit/s |
| ≤ 16 kHz, stereo | 32 | 64 |
| > 16 kHz, mono | 40 | 64 |
| > 16 kHz, stereo | 80 | 128 |

ASF packet timestamps jitter by about a millisecond, which ffmpeg reports as non-monotonic DTS; audio-only
outputs therefore regenerate timestamps from the sample count (`asetpts=N/SR/TB`), and video outputs use
`aresample=async=1` to keep A/V sync.

### 2.3 SWA

A big-endian u32 header length *L*, then u32 3, the sample rate, the bit rate and the tag `MACR`; MPEG audio
frames start at *L* + 4. 76 files on the disc are SWA: the 4 `.swa` in SWALLDSK (Deluxe + Kids), 71 `.MP3`-named
files in IAF folders, and `SUPPORT/DIRHTML/DSWMEDIA/MEMO_IN.SWA` (omitted with the rest of that folder). Of the 50
unique SWA sources, 46 have the usual *L* = 0x140 (0x144 bytes stripped) and are stripped with
`encarta_formats::director::swa_file_to_mp3`; 4 IAF files have *L* = 0x164 (0x168 bytes), which that function rejects,
so the converter's own rule (`media::sniff::swa_audio_offset`: *L* + 4, and a frame sync must follow) handles them.
The `mp3` row's `meta.stripper` records which was used. The remainder is stored as the `mp3` derivative and is the
input for Opus/AAC.

### 2.4 Video

All 215 unique WMVs decode with ffmpeg's `wmv1`/`wmv2` and `wmav2` decoders. Frame size is kept (320×240);
odd sizes would be rounded down to even. Captions for each video are the `.smi` with the same stem (§2.5).

### 2.5 SAMI → WebVTT (`media/sami.rs`)

Written from the public SAMI 1.0 description and the files on the disc:
- Text is UTF-8 when valid, otherwise Windows-1252 (50 of 2,198 files have bytes above 0x7F).
- `<STYLE>`: each `.CLASS { … }` rule gives a class; its `Name:` (underscores read as spaces) becomes the
  track label and `lang:` the language (`en-US-CC` → `en-US`). `#Source` is the style of the speaker line.
  Every file on the disc has the single class `ENUSCC`.
- `<SAMIParam Length=ms>` gives the media length. The body is a list of `<SYNC Start=ms [Duration=ms]>`
  blocks holding `<P Class=… [ID=Source]>` paragraphs. `</P>`, `</SYNC>` and `<BODY>` are optional
  (half the files have no `<BODY>`; 1,490 wrap the body in a `<TABLE><TD>` that is ignored).
- Inline markup: `<i>`, `<it>` (a typo for `<i>` in one file) and `<em>` → `<i>`; `<br>` → line break;
  every other tag is dropped. Entities: named Latin-1 and punctuation entities plus numeric references.
- A `<P>` without a class takes the class of the previous paragraph in its SYNC (one file, `MDDLX02/18346.smi`,
  writes `<P Class=ENUSCC ID=Source>…</P><P>caption</P>`); a classless paragraph with no predecessor belongs to every class.
- Each SYNC replaces the screen for its class. A cue runs from its SYNC to the next SYNC of that class, or for
  `Duration` when given; the last cue ends at `Length`, or 5 s later. A paragraph that is only `&nbsp;`
  clears the screen and produces no cue.
- The source line (`ID=Source`) becomes the first cue line as `<c.source>…</c>`, with a `::cue(.source)` style
  block (silver, as in the SAMI style). Text is escaped for WebVTT.
- One track per class with text: the first is the `vtt` derivative, any further class would be `vtt-<class>`.

### 2.6 Cursors

The `image` crate's ICO reader decodes cursors (type 2). The PNG is the largest image; the hotspot (x, y)
from the directory entry is in `meta` so CSS can use `cursor: url(…png) x y`.

### 2.7 `.iax` references (`media/iax.rs`)

The root `<iai>` attributes (`id`, `friendlyname`, `width`, `height`, `basecontroltype`, `skinlib`,
`hasaudio`, …) are stored in the original's `meta`. Every element text or attribute value that looks like a
media file name (an extension in swf/jpg/gif/png/bmp/mp3/wav/wma/wmv/mid/smi/xml/iax/flv/txt, no markup)
is a reference. Seen on the disc: `<image>`, `<imagefile>`, `<altimage>`, `<audiofile>`, `<audio>`,
`<pipgraphic>`, `<bgimage>`, `<picon>`, `<playergraphic>`, `<animationout>`, and the attributes
`pipfct@pipgraphic`, `event@audio`, `boardfct@animout`. A reference resolves, in order:
1. relative to the `.iax` file's own folder (SWQUIZ `dswMedia/quiz/<n>/<n>.iax`);
2. `iaf-folder`: a baggage `.iax` named `<id>.iax` keeps its assets in `dswMedia/IAF/<lang>/<id>/` of the same EIT;
3. the same container, then the same edition (Deluxe or Kids), then any container, by file name
   (shared player art such as `button_picon.swf` lives in `SWSHARED.EIT`).
When several candidates differ in content the first is taken and the row is marked `ambiguous`.

---

## 3. Store layout and manifest

```
out/media/o/<h0h1>/<sha256>.<ext>          original bytes (ext from the sniffed format: jpg gif png bmp wav asf mp3 swa swf mid smi xml dcr cur)
out/media/d/<h0h1>/<sha256>.<suffix>       derivatives of that source: webp avif png opus m4a mp3 mp4 webm poster.webp vtt
out/media/interactives/city_climate.json  City Climate dataset (from the Director casts)
out/media/interactives/population.json    Population Chart dataset
out/media/manifest.sqlite
```
`<sha256>` is always the **source's** hash, so all outputs of one source sit together and the server can
build any path from `file.sha256`. Files are written to `out/media/.tmp/` and renamed into place.

`manifest.sqlite` (rebuilt each run except `derivative`, which persists and drives resumption):

```sql
file(container, package, member_path, name, sha256, size, mime, kind, format, disposition)
  -- one row per media file on the disc (EIT member or loose ISO file); PK (container, member_path)
  -- container: path under extracted/eit ("CONTENT/MDDLX01.EIT") or "ISO"; package: its file name ("MDDLX01.EIT")
  -- name: lower-case basename: the join key for media records ("00012b7d.jpg", "00045123.wmv")
  -- kind: image | cursor | audio | video | captions | midi | flash | interactive | director
  -- format: sniffed format (jpeg gif png bmp wav asf mp3 swa swf midi sami xml director cursor); disposition: converted | kept
derivative(sha256, format, path, width, height, duration_ms, bytes, out_sha256, recipe, meta)
  -- PK (sha256, format); format 'original' is the stored original. path is relative to out/media.
  -- width/height for images, cursors, video and posters; duration_ms for audio, video and captions.
  -- recipe names encoder and settings; meta is JSON (GIF frames/transparency, PNG alpha and keyed pixels,
  --   cursor hotspot, audio codec/rate/channels, SWA header length, video codecs, .iax root attributes,
  --   caption class/lang/label/cues, non-benign ffmpeg warnings)
catalog_key(edition, tbl, key, package_index, package_file, local_id, container, member_path, sha256)
  -- every baggage.ecs / generic.ecs key of both catalogs (search-catalog-misc.md §3), resolved to the
  -- EIT member and its sha256 (NULL for non-media members such as sidebars, and for the one AKC key)
iax_ref(iax_sha256, element, ref, resolution, container, member_path, sha256)
  -- one row per distinct (iax content, element, reference)
member(container, member_path, size, format, status, owner, reason, sha256)
  -- every file on the disc with its completeness status (§4)
failure(sha256, format, error)      -- outputs that failed in the last run
run(key, value)                     -- ffmpeg version, encoders, filter
```

Typical joins for the content database: media record file name → `file.name` → `derivative` by `sha256`;
catalog key → `catalog_key.sha256` → `derivative`; captions for a WMA/WMV/MIDI → the `.smi` with the same stem.

---

## 4. Completeness report

`out/report/media.json` has `totals` (files and bytes per status, unique sources by kind, outputs per format,
failures), `by_rule` (status × owner × rule with counts and an example), `by_container`, `catalog`, `iax`,
`failures`, `not_applicable`, `unexplained` and the full `omitted` list. `media-members.tsv` and the
`member` table list every file. Statuses:

| status | meaning |
|---|---|
| converted | media file stored with all derivatives of its kind |
| kept | media file stored as-is (MP3, MIDI, SWF, `.iax`, `flashvr.xml`, live Director files) |
| delegated | not media: owned by another converter (`owner` = content, tours, atlas, search) |
| omitted | intentionally not converted; `reason` says why |
| container | a loose ISO container file whose members are classified individually |
| failed | a derivative failed (none in the full run) |
| unexplained | no rule matched (**zero** in the full run) |

Rules (`media/rules.rs`):
- **delegated → content**: `EE/ENCARTA/EDICT/*.EIT` (dictionary/thesaurus/bilingual XML), `CATALOG.DLX/KDC`
  (`catalog2/`), `$WW$CTCATALOG` in `TIMELINE.EIT`/`MSWORLD.ITR`, MD\* `baggage/*.xml` (sidebars),
  `MINDMAZE.EIT/*.lst`; loose AKC, `MSWORLD.SDB`, `TIMELINE.DAT`, `P2MTBL.DAT`, `MINDMAZE.DB/IDX`,
  `D*.TXT`, LEX lexicons, `DELUXE.INI`/`KIDSCD.INI`.
- **delegated → tours**: `.x .3sc .3wm .3lt .32d .3cl .3vp` in the tour EITs (their images and cursors are
  converted here; the tours converter can reference them by hash or name).
- **delegated → atlas**: `*.MAD`, disc fonts. **delegated → search**: `FINDDLX.CAB`.
- **omitted**: `_label.xml`/`_labelw.xml` (EIT build labels); `ENCXSL.EIT` and `MSWORLD.EIT/map/map.dat`
  (original UI templates, CLEAN_ROOM rule 7); ITIR full-text/word-wheel indexes (search is rebuilt);
  tour `error.log`, `Thumbs.db`, `.psd`; the 14 orphan Director files (shockwave.md §2.3) and
  `SUPPORT/DIRHTML/*`; executables, DLLs, installers and side-by-side runtimes (CLEAN_ROOM rule 6);
  `EE/WORKS/*` (Works Task Launcher integration); CHM help for the Windows UI; empty pushpin `.GPD`
  templates; setup/support INI/REG/URL/HTM/TXT files.

---

## 5. Notes and open items

- **AVIF** is produced for JPEG sources only; GIF/PNG/BMP sources are palette art, where lossless WebP is
  the better modern format. libaom at cpu-used 6 costs about 0.2 s per typical image, which is affordable.
- WebP from an already-compressed JPEG at q80 is often **not smaller** than the JPEG (see §6). The server
  should pick the smallest acceptable representation rather than always preferring WebP.
- ffmpeg decodes everything on the disc, including WMV1/WMV2, WMA v2/Pro, GSM 6.10 and MS ADPCM.
- `grip.cur` is referenced by every tour `load.3sc` but is not on the disc (tours-3d.md §5.7).

## 6. Full-run results

First full run (2026-10-13, 8 workers, `nice 5`, on a shared 24-core machine): **3,341 s (56 min)**,
127,255 jobs, **0 failed outputs, 0 unexplained files**. Wall time was dominated by the 50k pronunciation
WAVs (≈30 jobs/s) and the 56k JPEG → AVIF encodes. A no-op re-run takes **13 s** (survey + re-hashing every
stored output). Changing a recipe redoes only that output (re-running after the SAMI fix redid 1,883 VTTs in 28 s).
A one-off backfill measured the real duration of the 104k audio/video outputs of that first run (30 min, only
needed once: newer runs measure each output as it is written).

| status | files | bytes |
|---|---|---|
| converted | 131,283 | 1,646 MB |
| kept | 9,157 | 216 MB |
| delegated | 296,633 | 1,343 MB |
| omitted | 609 | 147 MB |
| container (loose ISO containers) | 93 | 1,463 MB |
| failed / unexplained | 0 / 0 | |
| **total** (437,049 EIT members + 726 ISO files) | **437,775** | **4,815 MB** |

| output | files | bytes | vs source |
|---|---|---|---|
| original | 127,255 | 1,572 MB | |
| webp | 66,989 | 737 MB | JPEG 0.87× (smaller for 33,133 of 56,568), GIF 0.78×, BMP 0.38× |
| avif | 56,568 | 457 MB | JPEG 0.59× |
| png | 4,054 | 40 MB | 290 with alpha (113 tour files, 189 flags) |
| opus | 51,861 | 407 MB | WAV 1.74×, WMA 1.81× (the GSM/WMA sources are very low bit rate) |
| aac | 51,861 | 721 MB | |
| mp3 (SWA stripped) | 50 | 5.5 MB | |
| mp4 / webm / poster | 215 / 215 / 215 | 310 / 198 / 1.6 MB | |
| vtt | 1,883 | 0.6 MB | |
| **all derivatives** | | **2.88 GB** | `out/media` is 5.2 GB in all |

Decoding: ffmpeg decoded every source; the only decoder message left after filtering benign ones is
`wmav2: nb_frames is N bits left M` on 366 outputs (trailing bits in a WMA superframe; audio is complete).
Note that the ASF `duration` of many WMA files overstates the decoded length by 0.3–0.9 s; the `original`
row keeps the container's value, derivative rows carry the measured one.

Catalog: all 72,299 Deluxe and 16,575 Kids EIT keys resolve to a member (the one AKC key, `list/expert.xml`,
has no file); 68,658 + 16,561 resolve to media files, the rest are sidebar XML.

`.iax` references: 14,329, resolved as relative 4,685, iaf-folder 9,023, same-container 80, same-edition 480,
any-container 1, ambiguous 47, **unresolved 13** (files missing from the disc, e.g.
`SWDLX02/…/001ade9b.iax` → `BelaRUSSIAn_CaptionImage.jpg`, `SWEDK00/…/001714ab.iax` → `001714abPip.swf`,
`001714abBoard.swf`; listed in `media.json` → `iax.unresolved_sample`).

Director datasets: `encarta_formats::director::interactive::{city_climate, population_chart}` →
`out/media/interactives/city_climate.json` and `population.json` (no data issues reported); listed under
`interactives` in `media.json`.
