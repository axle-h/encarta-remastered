# Shockwave / Director (`.dcr`, `.cct`, `.swa`)

Status: **identified, triaged and extracted (plan item 1.8)**. Reader, data converters and SWA → MP3 are
implemented in `encarta_formats::director` (§8); our JSON shapes are in §7. The scope is tiny: two live interactives, both
data-driven charts. **Recommendation: convert the data and reimplement both charts in the web client
(option C). Do not port a Shockwave player.**

Sources: observing the files (hex dumps, plus a throwaway Python inspector written for this triage),
and the public Director/Afterburner format notes in ProjectorRays (MPL-2.0) and in the ScummVM
Director engine and dirplayer-rs. Each structure below was confirmed against every file on the disc.
Our scripts list only handler *names* and string *literals*, never decompiled bytecode. Their dumps are in
`analysis/decompiled/director/` (gitignored).

---

## 1. Inventory

Detection used file extensions and file magic over all of `extracted/iso` and `extracted/eit`.
Magic: `XFIR` (a little-endian RIFX) with form type `MDGF` (an Afterburner movie, read reversed as "FGDM") or `CDGF` (an Afterburner
cast, "FGDC"). No file has a plain `RIFX`/`MV93` (unprotected Director) header, and none is a `.dir/.dxr/.cst/.cxt/.x32/.w32`.
Four other files contain the bytes `XFIR`/`RIFX`, at random offsets inside compressed data in `PICONSTD.EIT`, `T627511A.WMV` (both copies) and `WLD_CD.MAD`.
None of those hits has a valid form type, so they are coincidences.

| Container | Files | Bytes | Contents |
|---|---|---|---|
| `EE/ENCARTA/SWSHARED.EIT` → `dswMedia/` | 21 `.cct` + 1 `.dcr` | 354,438 | shared Lingo libraries, UI media, `players/LOC_vwr/LOC_vwr.dcr` |
| `EE/ENCARTA/SWALLDSK.EIT` → `dswMedia/ia/clim_CCL/` | 5 `.cct` + `clim_ccl.dcr` + `clim_in.swa` | 152,570 | **City Climate** interactive |
| `EE/ENCARTA/SWALLDSK.EIT` → `dswMedia/ia/pop_CPO/` | 6 `.cct` + `pop_CPO.dcr` + `pop_in.swa` | 124,439 | **Population Chart** interactive |
| `EE/KIDS/SWSHARED.EIT`, `EE/KIDS/SWALLDSK.EIT` | same 37 files | — | byte-identical copies (`diff -r` clean) |
| ISO `SUPPORT/DIRHTML/` | `DIAGNOSE.DCR`, `DSWMEDIA/MEMO_IN.SWA`, `DEFAULT.HTM` | 178,916 | support-desk Shockwave diagnostic page from 1999 (Encarta 2000). Not part of the app |

Totals: 7 `.dcr`, 64 `.cct` and 5 `.swa`, of which 35 Director files and 2 SWA files are unique within the product.
Besides these, 71 `.MP3`-named files in `dswMedia/IAF/a/<id>/` are really SWA. Their header length field L is
usually 0x140 (0x144 bytes to strip), but 0x164 on 4 of them, so strip L + 4 bytes.
The catalog's `generic.ecs` maps every `dswmedia/...` path to `SwShared.eit` or `SwAllDsk.eit`. Those paths are the
keys the app uses to load the files (`msencdata://~catalog~/dswMedia/...`).

**Versions.** Every product file has `Fver` version string `8.5.1#102` and config `directorVersion` 1850,
which is **Director 8.5.1**. `DIAGNOSE.DCR` has config version 1406 (**Director 7**, `Fver` 0x500, no version string).

**Not on the disc:**
- No Shockwave Player, no Xtras (`.x32`) and no `swdir.dll`. `REDIST/` holds only the MFC71/MSVC71 runtimes.
- The MSI checks only for Flash. `ENCARTAR.DLL` shows "requires a Shockwave player … available online from Adobe"
  (Shockwave 10), and `ENCARTAU.DLL` has a `teleport…tname=shockwave` download link.
- The 2,794 `.swf` files are all genuine Flash (`FWS`/`CWS` magic), served by the IAF Flash player
  (`shared/players/IAFPlayer/*.swf` plus the `.iax` descriptors). **None of the 4,343 `.iax` files references a Director file.**
  Ruffle's Flash scope is unaffected by this item.

### Per-file summary (unique files)

`fld` = field (`STXT`) members, `bmp` = bitmaps, `scr` = scripts (Lscr / handlers), `xtra` = Xtra-type members.

| File | Members | Notes |
|---|---|---|
| `ia/clim_CCL/clim_ccl.dcr` | 9 scr, 4 fld, 1 bmp, 1 shape, 1 palette | Stage 608×424, 45 fps, 32-bit. 15 cast libraries (MCsL) |
| `ia/clim_CCL/cscripts.cct` | 9 scr / 47 handlers | the chart logic |
| `ia/clim_CCL/cmedia.cct` | 12 bmp, 4 shape, 1 SWA-stream member | radio/button art; the `swa file` member links `@\Clim_in.swa` |
| `ia/clim_CCL/a/lclData.cct` | 168 fld | **the dataset** (see §2) |
| `ia/clim_CCL/a/lclMedia.cct` | 66 fld, 36 bmp, 3 film loops, 1 Text-Xtra member | labels and slots, launch filter |
| `ia/clim_CCL/a/lclstrng.cct` | 1 fld | title `"City Climate"` |
| `ia/pop_CPO/pop_CPO.dcr` | 9 scr, 3 fld, 5 shapes, 4 vector shapes | Stage 608×424, 30 fps. 16 cast libraries |
| `ia/pop_CPO/cscripts.cct` | 7 scr / 41 handlers | the chart logic |
| `ia/pop_CPO/cmedia.cct` | 16 bmp, 2 shape, 1 SWA-stream member | links `@\Pop_in.swa` |
| `ia/pop_CPO/dtus.cct`, `dtcanada.cct` | 55 / 17 fld | **the datasets** |
| `ia/pop_CPO/a/lclMedia.cct` | 111 fld, 20 bmp, 2 film loops | axis labels, slots, launch filter |
| `ia/pop_CPO/a/lclstrng.cct` | 1 fld | title `"Population Chart"` |
| `shared/code/corescpt.cct` | 26 scr / 257 handlers | movie framework: host communication, navigation, audio layer, focus, stage, keys, tabs, debug, engines, windows |
| `shared/code/coreUtil.cct` | 37 scr / 189 handlers | sprite pools, layers, broadcast/subscribe, engines |
| `shared/code/widgets1.cct`, `scrMenu.cct`, `optMan1.cct` | 44 / 7 / 8 scr | buttons, scrollbars, pop-up menus, cursors, copy/paste, "T-actions" |
| `shared/media/{cursors,stdsnd,uigrfx}.cct` | 10 bmp / 13 sounds / 64 bmp | cursors, UI clicks, UI bitmaps |
| `shared/players/LOC_vwr/LOC_vwr.dcr` | 53 scr, 35 bmp, 6 fld, 3 sounds | tiled "locator" map viewer (see §2.3). **Orphan** |
| `shared/code/{animscpt,animscpt2,animTips,chrt,entrtxt,linkScpt,scrolTxt,tooltip}.cct`, `shared/ia/{buildit,IAshared,seganim,sldfade}.cct`, `dummy.cct` | — | **Orphans**: libraries for older Director interactives (animation player with transport and captions, jigsaw "build-it", layer slider/fade, segment animation, tooltips, text entry). No movie on the disc links them |

No cast member keeps its Lingo source text (`srcTextLen` is 0 everywhere), so only bytecode is present (published/protected).

---

## 2. What they drive

### 2.1 How the app launches them
- Media records carry a `group`. The templates `mediacommon.xsl` and `kidsmediacommon.xsl` (both in ENCARTA and KIDS `ENCXSL.EIT`) treat
  `group = 'cityclim'`, `'citypop'` and `'director'` as launchable media, alongside `interact` (Flash IAF) and `virttour`.
  A click navigates to `msencarta://id=<refID>`. For `director` it adds `/forceCD=1`.
  `artmain.xsl` leaves `citypop`/`cityclim` out of the inline media strip, as it does for `interact` and `flash`.
- `ENCARTA.EXE` hosts the Shockwave ActiveX (`CShockwaveCtl`, `CMaxShockwaveWnd` "MaxShockwave Director Control",
  `DirectorView`, `macromed\director\swdir.dll`) and passes the movie a `swURL` parameter.
  The movie talks back through `EvalScript` / `externalEvent` (copy, print, save BMP, resize, focus, key forwarding).
  This is all handled by `corescpt`.
- `ENCARTA.EXE` also holds **dead legacy tables** that point to Director titles which are not on this disc: Africana (`afrhome\*.dcr` …),
  `hibrow`, `hwkctr\hc.dcr`, `rptguide\rg.dcr`, `protours\pt_ee.dcr`, `shared/players/launcher.dcr`, and 9 entries of the form
  `msencdata::dswmedia/ia/tNNNNNNa/tNNNNNNa.dcr` paired with media ids (461527279, 461548584, 461545281, 461549568, 461519363,
  461548587, 461549569, 701508650, 461549296). None of those files exists. None of the ids is in `content.ecn`/`relart.ecn`, and only 701508650 is in `data.ecn`.
  The `pop_CPO` scripts embed `631722`, which suggests that `t631722a` was the earlier name of the Population Chart.
- Inside a movie, a "required" internal cast works out the base path from `swURL` (`msencData:` versus `http:`).
  It then downloads and links the shared casts and the `ia\<name>\` casts with NetLingo (`vDownLoadCast`, `vLinkCast`).
  The locale folder is `a` (American English) and the product code is `EE`.
  The placeholder casts `lclstrng`, `lclMedia` and `lclData` point at `dummy.cst` at authoring time and are re-linked to `ia\<name>\a\*.cct` at run time.
- **Open item (needs 1.3 AKC):** the media records whose `group` is `cityclim`/`citypop`, and the articles that link them, live in
  `DATA*.AKC`. Expect one or a few media ids per interactive, possibly with a state, province or city preselection (see "launch filter").

### 2.2 The two live features

**City Climate** (`clim_CCL`) is a bar chart that compares climate statistics for cities that the user picks.
- Up to **8 cities** (slots `select1..8`, `chartdisp1..8`), with "< add a city >" and "< remove a city >" pop-up menus.
- Categories: Temperature (Annual Average, January High, January Low, July High, July Low) and Precipitation (Annual Average,
  Days with Precip., Avg. Snowfall). The units toggle between imperial and metric (default imperial). Each sub-category carries its own axis
  `low`/`high` per unit system.
- Data (`a/lclData.cct`): a spec field, a categories field, a "location list" of **165 cities worldwide**, and one record per city
  with every measure pre-computed in both unit systems (values are strings, `"na"` where missing).
- Launch filter (`a/lclMedia.cct` "data filter"): maps media refIds to a
  state/province code, optionally with a city. **Correction (1.8 extraction):** 227 entries, not 213. 63 have a region only
  (62 × 461535xxx state/province codes such as `WY`, `ONT`, plus `461544027` → `["Nunavut"]`). 164 name a city:
  85 × 461549xxx `[code, city]` for US/Canadian cities, 78 × 461579xxx `[country, city]` for the rest of the world, and one 461535xxx.
  One entry is irregular: `"461549787": ["D.C., United States", "Washington"]`. Together they cover 164 of the 165 cities. This preselects cities when the chart is opened from a place's media.
  None of these ids is in the 2009 catalog, so the filter may be vestigial. Confirm with the AKC media records.
- An intro narration plays from `clim_in.swa` (8.70 s).
- Also in `a/lclMedia.cct`: a Text Xtra member (no name) holding the legend `na = Data not available`, 36 label bitmaps
  (category names, "Display", title art), and runtime-state fields (`chartdispN`, `chrt N.M`, `colLabelN`, `incrementN`,
  `units`, `initial measures` = `us`, `pophilite` = `Washington, D.C., United States`). Those fields hold whatever the author
  last left in them, so the converter does not export them.
- Data quirks (**found during extraction**): metric annual precipitation is written with digit grouping (`"1,290"`) for 62 cities;
  the record for "Belfast, Great Britain" says `#state: "Ireland"` (the list label wins, since that is what the menu shows);
  156 values are `"na"`.

**Population Chart** (`pop_CPO`) is a line chart of historical census population.
- **US**: 51 entries (50 states + DC), 1800 to 2000, chart axis 1790–2010.
  **Canada**: 13 provinces and territories, 1861–2001, axis 1841–2021 (credit: "© Statistics Canada").
  **Correction (1.8 extraction):** the series are sampled every **20** years, not 10: `#xAxisSpecs` has `#increment: 20`, so each
  US series holds 11 values (1800, 1820 … 2000) and each Canadian series 8 (1861, 1881 … 2001). Years before a region's first
  census are `""`. The Canadian footnote says "first Canadian census (1851)", but the data starts at 1861.
  The first field of each dataset carries the footnote ("Data begins with … census …").
- Measures: Total Population, Population density (people per sq mi or per sq km), Percentage Urban. Each has a list of
  preset y-axis scales. The chart picks the smallest scale that fits the selection (`pollForChartRange`).
- Up to **4 series** (four column labels and four vector-shape lines `vector1..4`), chosen through "< add a state/province >" menus.
  A US/Canada switch picks the dataset. Defaults are imperial for the US and metric for Canada.
- Launch filter (`a/lclMedia.cct` "filter data"): 64 lines of `refId,country,state/province` (plain text, not a Lingo list).
  One line has no region: `461575121,Canada`.
- An intro narration plays from `pop_in.swa` (8.59 s).
- Data quirks to keep: the US spec title reads `"US Climate"` and the Canadian one `"Canadian Climate"` (copy-paste), and
  `"Percantage Urban"` is misspelt. These are internal names. The user-visible title comes from `lclstrng`.

Both datasets are written as **Lingo property-list literals** stored in field text, for example
`[#City: "Albany", #state: "United States", #imperial: [#temperature: [#AnnualAverage: "47.4", …]], #metric: […]]`.
The grammar is small: `[` `]` lists, `#symbol: value` pairs, quoted strings, integers and floats.
**Lingo literal syntax as the parser accepts it:** a list is `[]`, `[:]` (empty property list), `[v, v, …]` (linear) or
`[k: v, k: v, …]` (property; mixing both forms is an error). Keys are `#symbols`, strings (the climate filter's `"461535120":`)
or numbers. Strings are `"…"` with no escapes (Lingo has none). Numbers are optional sign, digits, optional `.` fraction and
exponent. The constants `VOID`, `TRUE`, `FALSE` and `EMPTY` are accepted. Symbols compare case-insensitively, and the data relies on this
(`#Temperature` in `categories` and `#temperature` in the records, `#Imperial` and `#imperial`). Whitespace includes CR/LF.
Some fields end with blank lines. Every value in the datasets is a string, even the numbers (`"77.4"`).
The `cscripts` casts also contain authoring-only routines (`buildData`, `readAfile`) that imported the data from
tab-separated text (`cityclim.txt`, `usdata.txt`, `candata.txt`) through the FileIO Xtra. FileIO is not in the movies' Xtra list,
so those routines never run at play time.

### 2.3 Orphans (convert nothing, list as intentionally omitted)
- `LOC_vwr.dcr`: a tile-scrolling map "locator" viewer (`scpTile`, `scpLocator`, `scpViewer`, virtual grid, 4-way scroll buttons,
  intro screen, preview frames, JPG tiles). Its content casts (`gen_media`, `LOC_bmps`, `LOC_strs`) are `dummy.cst` placeholders, and the only
  default it names is `#internal,T631788a`. No content cast for it exists anywhere on the disc. It is listed in `generic.ecs` but nothing references it.
- The 12 shared library casts with no movie (§1).
- `SUPPORT/DIRHTML/*` is a 1999 support diagnostic (Director 7, "Encarta 2000 shipped with 7.0.0.204") that the app never opens.

---

## 3. Container structure (Afterburner, Director 8.5)

All Director files are **Afterburner** (compressed, published) archives. The layout below comes from the public notes and was
confirmed on all 36 unique files (35 product files plus `DIAGNOSE.DCR`). Every file parsed and every zlib stream inflated without error.

**Varint** = unsigned base-128, most significant group first. The high bit of each byte (0x80) means "more bytes follow".
Fixed-width integers in the outer container are little-endian (`XFIR`). Four-character codes are stored byte-reversed (`revF` = `Fver`).

| Offset / order | Field |
|---|---|
| 0 | `XFIR`, u32 LE length, codec `MDGF` (movie) or `CDGF` (cast) |
| `Fver` | varint length, then varint version (0x501 here). If ≥ 0x401: varint imap version and varint Director version (0x73A). If ≥ 0x501: u8 length plus a version string (`8.5.1#102`) |
| `Fcdr` | varint length of a zlib block (**correction:** product files list only "ziplib", the Director 7 file lists ziplib, Fontmap and SWA Compressor). The inflated block is a u16 count (LE), `count` × 16-byte GUIDs, then `count` NUL-terminated names. Names seen: "Macromedia ziplib compression", "SWA Decompressor Xtra", "SWA Compressor Xtra", "Macromedia Fontmap compression" |
| `ABMP` | varint length (covering everything that follows), varint compression type, varint uncompressed length, then zlib data. Inflated: varint, varint, varint `count`, then `count` × {varint resourceId, varint offset (signed; −1 means "inside the ILS"), varint stored size, varint unpacked size, varint compression index into `Fcdr`, 4-byte fourCC (LE, reversed)} |
| `FGEI` | varint, then the **ILS** (initial load segment). This is resource id 2 and a zlib stream. Inflated, it repeats {varint resourceId, then that resource's bytes}. All other resource offsets are relative to the first byte after this varint. Resources inside the ILS have offset −1 and stored size = unpacked size |

Resources stored outside the ILS are zlib streams when their compression is "ziplib" (`BITD`, `CASt`, `Lscr`, `STXT`, `VWSC`, `XMED`, `ediM`).
**Correction (1.8 extraction):** "Macromedia Fontmap compression" is **not** zlib. It is used only for `FXmp` in `DIAGNOSE.DCR`
(4 bytes stored, 6,848 unpacked), which stands for the built-in default font map. The reader keeps those bytes as stored.
The KEY* table can name a resource that the published file left out (`DIAGNOSE.DCR` bitmaps 18–21 have a `BITD` key but no `BITD` resource).
`snd ` resources use the SWA compression (MP3, see §4).
Fourcc counts across the unique files: `CASt` 1131, `STXT` 458, `Lscr` 383, `BITD` 230, `Cinf`/`CAS*` 43, `Lnam` 38, `LctX` 37 (+1 `Lctx`),
`KEY*`/`DRCF`/`Fmap`/`VERS`/`ILS ` 36 each, `snd ` 23, `ccl ` 14, `SCVW` 7, `CLUT` 6, `XMED` 6, `ediM` 5, and `MCsL`/`Sord`/`VWFI`/`VWSC`/`VWLB`/`XTRl` 4 each.

The inner chunks are **big-endian** unless noted:
- `KEY*` (LE): u16 entry size, u16, u32 capacity, u32 used. Then entries of {i32 child resource id, i32 owner id (a cast id such as 1024, or a member's `CASt` id), fourCC}.
  This links each `CAS*`/`LctX` to its cast library, and each member to its `STXT`/`BITD`/`snd `/`XMED`.
- `DRCF` (config): stage rect at offset 4 (top, left, bottom, right as i16), min/max member at 12 (i16 each: the member range of the
  file's **own** cast; `coreUtil`, `widgets1` and `scrolTxt` start at **2**, so a `.cct`'s `CAS*` slot 0 is member 2), bit depth at 28,
  **directorVersion i16 at 36**, frame rate i16 at 54.
- `MCsL` (cast list): a list chunk. A "list chunk" is a header that ends at its u32 data offset (at 0). At the data offset come a u16 count,
  `count` × u32 item offsets, a u32 total length and then the items. In `MCsL` the header is {u32 data offset = 12, u16, u16 cast count,
  u16 items per cast (4), u16}. Item 0 is empty. Each cast then has 4 items: name (Pascal string), file path (Pascal string, authoring paths such as `C:\dirsrc\dswMedia\shared\code\corescpt.cst`),
  preload flags (u16), and {u16 min member, u16 max member, i32 cast id}. **Correction:** only internal casts (empty path) have a real
  cast id: `Internal` is 1024 and `required` is 0x10404, and these match the `KEY*` owners of their `CAS*`/`LctX`. External casts carry the placeholder
  1024 and a stale member range (`dtus` 1..107 in the movie, 55 members in the file). The reader therefore looks up `CAS*` only for internal casts.
- `CAS*`: an array of i32 `CASt` resource ids, one per member slot.
- `CASt`: u32 type, u32 info length, u32 specific-data length, then the info and the specific data. The info is a list chunk whose header is
  {u32 data offset = 20, u32, u32, u32 flags, **u32 script id at +16**}. Item 0 is the script source (empty here). Item 1 is the name: a Pascal string,
  sometimes followed by a NUL. Items 2 and up, when present, hold the authoring folder, the `.ls` file name and behaviour property defaults.
  The script id is a 1-based index into the cast's `LctX` entries. Fields can have one too, for an attached script.
  Types seen: 1 bitmap (specific data holds the rect at +2), 2 film loop, 3 field, 4 palette, 6 sound, 8 shape, 11 script
  (u16 at specific data +0: 1 = score/behaviour, 3 = movie, 7 = parent), 15 Xtra member (the specific data starts with the Xtra name:
  `vectorShape`, `text`, or SWA `MACR`).
- `STXT`: u32 header length (12), u32 text length, u32 format length, then text and style runs. **Field text is Windows-1252.**
  Member names are **Mac Roman** (`Bogot\x87`), so the casts were authored on a Mac.
- An empty `Lscr` (92 bytes, handler count 0) occurs four times: `sldfade` "empty class" and `LOC_vwr` members 63–65.
- `LctX`: a 96-byte header with entry count at +8, entries offset (u16) at +16 and **the `Lnam` resource id (i32) at +32**.
  Entries are 12 bytes: {i32, i32 `Lscr` resource id, u16, u16}.
- `Lnam`: the u16 name offset and u16 count sit at +16. The names are Pascal strings.
- `Lscr`: Lingo bytecode (D8.5 format). Handler table: count u16 at +72, offset u32 at +74, with **46-byte records** (D8.5 adds a u32 stack height).
  Each record starts with an i16 name id into the context's `Lnam`. Literal table: count at +78, offset at +80, data offset at +88.
  Each literal is 8 bytes {u32 type, u32 offset}; type 1 = string (u32 length, then bytes).
- `XTRl`: the Xtra list. Each movie lists SWA Streaming, SWA Decompression, NetLingo, INetURL, NetFile, Mix Services, Sound Import
  Export, TextXtra/Text Asset, Font Xtra/Font Asset, MacroMix, **Flash Asset**, QuickTime Asset, DirectSound, Sound Control and Script Agent
  (plus AVI Agent in `LOC_vwr`). **No third-party Xtras and no FileIO.**
- `XMED` for the 4 vector-shape members in `pop_CPO.dcr` is a tiny **SWF v1** (`FWS\x01`, about 100 bytes): the Flash Asset Xtra
  stores each vector shape as a one-shape SWF.

**Lingo:** Director 8.5 bytecode, with dot syntax and parent scripts, behaviours and movie scripts (no JavaScript syntax, which arrived in DMX 2004).
The architecture is object-oriented: managers created at `prepareMovie`, `sys*` facades over `m*` methods, behaviours with `getPropertyDescriptionList`,
and `new`/`mDestroy`. **External casts** are always linked at run time over NetLingo from `msencdata://~catalog~/dswMedia/…`.

---

## 4. Media inside

- **`.swa` files**: a 0x144-byte header, then raw **MPEG-1 Layer III** frames. Header observations: BE u32 0x140 (length of the rest
  of the header), u32 3, u32 44100 (sample rate), u32 48000 (bit rate), u32 0x571 (the same constant appears in `snd ` headers), u32 that is
  probably the sample count (377,697 for `clim_in`, which is 8.56 s, against 333 frames × 1,152 samples), 8 × 0xFF, u16 2, u16 1, then `MACR`, a GUID and
  `©1997 Macromedia Inc.` at 0x40. Frames run back to back from 0x144 to the end of the file. They are 48 kbit/s mono at 44.1 kHz: 333 frames
  (8.70 s) for `clim_in`, 329 (8.59 s) for `pop_in` and 495 (12.93 s) for `MEMO_IN`. The converter checks the header length, `MACR`, the frame sync at
  0x144, that at least two frames are chained and that their rate matches the header. Then it keeps the MP3 bytes as they are. ffprobe on `tail -c +325`: mp3, 44.1 kHz, mono, 48 kbit/s.
  Conversion: strip 0x144 bytes, then transcode to Opus (or keep the MP3).
- **Embedded `snd `** (`stdsnd.cct` UI clicks, `LOC_vwr`): SWA-compressed. A header carrying a sample rate (22,050 or 44,100), then MPEG audio frames.
  The header length varies (0x52 in `01down`). The reader takes the first offset from which three frames chain. In `DIAGNOSE.DCR`, sound member 45
  has no `snd ` and keeps a whole `.swa` stream (0x140 header, 16 kHz) in `ediM`.
  Only UI feedback sounds (`01down`, `01over`, `select`, `error`, `rightSound` …). They are optional for our UI.
- **Bitmaps** (`BITD`): UI chrome only (buttons, radio buttons, backgrounds, cursors, label art). Our UI is written from scratch and does not
  need them. They decode for VM side-by-side checks:
  - `CASt` specific data (big-endian): u16 row bytes (bit 15 set = extended header; low 14 bits = bytes per row, even-padded for 8-bit),
    rect (i16 top, left, bottom, right) at 2, 8 bytes, reg point (i16 y, x) at 18, u8 flags at 22, **u8 bit depth at 23**,
    i16 palette cast library at 24 and i16 palette member at 26. Without bit 15 (22-byte form) the bitmap is 1-bit.
    Depths on the disc: 1 (16 bitmaps), 8 (209), 16 (4, `DIAGNOSE` only), 32 (5).
  - Palette: a member number > 0 (cast library −1 = the bitmap's own cast; 0 = the movie's cast, for example `cmedia` bitmaps name
    member 3 in `clim_ccl.dcr` and member 4 in `pop_CPO.dcr`, which is "RefBU Palette"). A number ≤ 0 is built-in (−101 = System Win, used only by 1-bit cursors).
    Every "RefBU" `CLUT` on the disc is byte-identical. `CLUT` = 256 × {u16 r, g, b}, and the high byte is the 8-bit value.
  - `BITD`: raw when it is `row bytes × height` long, otherwise run coded: a control byte `c` < 0x80 copies `c + 1` bytes; any other value
    repeats the next byte `257 − c` times. A 32-bit row unpacks to `width × 4` bytes as four planes (alpha, red, green, blue). Alpha is all zero
    when the image is opaque. A 16-bit row is a high-byte plane then a low-byte plane (1-5-5-5).
  - `ediM` on a bitmap without `BITD` is the imported JPEG (`DIAGNOSE.DCR` members 18–21).
- **Text Xtra** (`XMED`, member type 15 named `text`): ASCII sections, each introduced by 20 hex digits (u16 id, u32 length, u32 count).
  Section `0002` holds the characters as `<hex length>,<text>`, for example `17,na = Data not available`.
- **Vector shapes / Flash** (`XMED` of `vectorShape` and `flash` members): u32 length, u32 1, u32 SWF length, then the SWF (`FWS`).
- **SWA streaming member** (`cmedia` "swa file", Xtra `swa`): specific data = u32 name length, `swa`, u32 block length, then `MACR`, u32 path length,
  3 × u32 and the path (`@\Clim_in.swa`, where `@` means the movie's folder).

---

## 5. Options

| Option | What it takes | Licence | Fit | Effort |
|---|---|---|---|---|
| **A. dirplayer-rs** (Rust → WASM Shockwave emulator, igorlira/dirplayer-rs) | Bundle its polyfill, serve the 35 files at their `dswMedia` paths, emulate the `swURL` parameter and `msencdata://~catalog~/` URLs, answer `EvalScript`/`externalEvent` | **GPL-3.0-only** (vm-rust `Cargo.toml`). Shipping it with our client makes the combination GPL, or forces us to keep it as a strictly separate program | Active: v0.8.1 (Aug 2026), about 200k lines of Rust. It has Afterburner, Lingo VM, MP3/SWA sound, vector shapes, Flash members via Ruffle, text, and NetLingo-style net tasks. It targets web games (Habbo and others). No Encarta-specific testing, and how well it handles D8.5 host/ActiveX integration is unknown | spike S (2–3 d), integration M, plus open-ended fidelity bugs |
| **B. ScummVM Director** | Emscripten build of ScummVM inside the page | GPL-3.0+ | Targets D2–D5 CD-ROM titles. ScummVM enables an engine version only after testing, and D6+ titles "do not open at all" (Dec 2023 news). Not designed to embed in a host page or talk to it | XL. Not viable |
| **C. Convert + reimplement** | Converter: Afterburner reader (§3), list the `STXT` fields, parse the Lingo property-list literals into JSON, strip the SWA header and transcode. Client: two chart components (8-city bar chart, 4-series census line chart, unit toggle, add/remove menus, intro narration) | ours | Exact data parity. Accessible, responsive, themable, no plugin and no GPL. The logic is simple and fully documented above | **S–M**: reader + spec + data export ~4–5 d; two chart components ~5 d; VM parity check ~1 d |
| D. Ruffle | n/a: none of these are SWF. Only the four tiny `XMED` vector shapes are SWF-encoded, and they are just line art for the chart | — | — | — |

## 6. Recommendation

**Option C.** Only two features use Director, and both are thin UIs over about 230 records of tabular data. A player port would carry
a GPL dependency and unknown fidelity risk for roughly 150 KB of content. Plan:

1. Converter (`encarta-formats::director`): read the Afterburner archive, then the `KEY*`/`CAS*`/`CASt`/`STXT` chain, then member name → field text.
   This reader works on all 35 files and is enough to drive the completeness report.
2. A Lingo property-list literal parser (strings, symbols, numbers, nested lists) → JSON:
   `city_climate.json` (165 cities × 8 measures × 2 unit systems, plus axis specs) and `population.json` (both datasets; §7)
   (51 + 13 regions × 3 measures, census years, axis scales, footnotes). Keep the launch-filter maps as well.
3. SWA → Opus for the two narrations. UI clicks and bitmaps are optional.
4. Web client (Step 7): `CityClimateChart` and `PopulationChart` components, opened from media of group `cityclim`/`citypop`.
   Check them against VM screenshots.
5. Completeness report: list `LOC_vwr.dcr`, the 12 orphan shared casts, `dummy.cct` and `SUPPORT/DIRHTML/*` as intentionally omitted
   ("no content references them"). Revisit `LOC_vwr` only if AKC media records turn out to reference group `director`.

Keep dirplayer-rs as a **dev-only** fallback for side-by-side behaviour checks. Never ship it.

---

## 7. Our JSON (1.8 output)

Produced by `director climate <dir>` / `director population <dir>` (and the `interactive` module). All strings are Unicode
(field text is decoded from Windows-1252). Data values are JSON numbers written with the source precision (`"2.0"` stays `2.0`,
`"1,290"` becomes `1290`). `"na"` and `""` become `null`. Ids are the Lingo symbols in snake case (`#DaysWPrecip` → `days_w_precip`).

**`city_climate.json`**
```json
{
  "id": "clim_CCL", "title": "City Climate", "dataset_title": "Climate",
  "unit_systems": ["imperial", "metric"], "default_units": "imperial", "max_selection": 8,
  "strings": {"add": "< add a city >", "remove": "< remove a city >", "not_available": "na = Data not available"},
  "categories": [{"id": "temperature", "label": "Temperature"}, {"id": "precipitation", "label": "Precipitation"}],
  "measures": [{"id": "temperature.annual_average", "category": "temperature", "label": "Annual Average",
                "units": {"imperial": {"name": "Degrees Fahrenheit", "abbr": "ºF", "axis_min": 20, "axis_max": 90},
                          "metric":   {"name": "Degrees Celsius",    "abbr": "ºC", "axis_min": -10, "axis_max": 35}}}],
  "cities": [{"name": "Albany", "country": "United States", "label": "Albany, United States",
              "values": {"imperial": {"temperature.annual_average": 47.4, "precipitation.avg_snow": 63.8},
                         "metric":   {"temperature.annual_average": 8.6}}}],
  "launch_filter": [{"ref_id": 461535120, "region": "WY"},
                    {"ref_id": 461549846, "region": "WY", "city": "Cheyenne", "city_label": "Cheyenne, United States"}],
  "narration": {"file": "clim_in.swa", "duration_secs": 8.7}
}
```
There are 2 categories and 8 measures (temperature: annual average, January high/low, July high/low; precipitation: annual average,
days with precipitation, average snowfall), and 165 cities in list order. Each city has 8 values per unit system. The chart label is
`"<label> (<abbr>)"`, which is how the movie builds its `chrt N.0` fields. `max_selection` counts the `selectN` slots. `city_label` is the
`cities[].label` that a filter entry selects.

**`population.json`**
```json
{
  "id": "pop_CPO", "title": "Population Chart", "unit_systems": ["imperial", "metric"], "max_selection": 4,
  "datasets": [{
    "id": "us", "country": "US", "spec_title": "US Climate",
    "footnote": "Data begins with United States census of 1800 or census prior to statehood.",
    "default_units": "imperial", "strings": {"add": "< add a state >", "remove": "< remove a state >"},
    "category_label": "Population",
    "years": [1800, 1820, 1840, 1860, 1880, 1900, 1920, 1940, 1960, 1980, 2000],
    "axis": {"chart_start": 1790, "chart_end": 2010, "label_step": 20},
    "measures": [{"id": "total", "label": "Total Population",
                  "units": {"imperial": {"name": "", "scales": [[0, 1000000, 2000000, 3000000, 4000000], [0, 3000000, …]]},
                            "metric": {…}}},
                 {"id": "density", "label": "Population density", "units": {"imperial": {"name": "People per sq mi", "scales": […]}, …}},
                 {"id": "urban", "label": "Percantage Urban", "units": {…}}],
    "regions": [{"name": "Alabama",
                 "values": {"imperial": {"total": [null, 127901, 590756, …], "density": [null, 2.4, …], "urban": [null, 0.0, …]},
                            "metric": {…}}}]
  }, {"id": "canada", "country": "Canada", "spec_title": "Canadian Climate", "default_units": "metric", "years": [1861, …, 2001], …}],
  "launch_filter": [{"ref_id": 461537355, "country": "US", "region": "District of Columbia"}, {"ref_id": 461575121, "country": "Canada"}],
  "narration": {"file": "pop_in.swa", "duration_secs": 8.59}
}
```
There are 51 US regions and 13 Canadian ones. Each series has one value per year (`null` before the first census). Totals are the same in both unit systems.
Density is per sq mi (imperial) or per sq km (metric). `scales` are the preset y-axis tick lists, and the chart uses the smallest one whose
top fits the selection. `max_selection` counts the `colLabelN` fields (and the `vectorN` lines in the movie). `spec_title` and
`"Percantage Urban"` are kept as they are in the data. The UI title is `title`.

## 8. Implementation (`encarta_formats::director`)

| Module | Does |
|---|---|
| `container` | `XFIR` → `Fver`/`Fcdr`/`ABMP`/`FGEI`; splits the ILS; inflates every resource whose compressor is ziplib |
| `cast` | `KEY*`, `MCsL`, `CAS*`, `CASt`, `DRCF`, `LctX`/`Lnam`, handler names from the `Lscr` handler table (names only, never bytecode) → `DirectorFile { casts: [CastLib { members }] }` |
| `text`, `bitmap`, `sound`, `extract` | `STXT` → text; `BITD` + palette → PNG (or the `ediM` JPEG); `CLUT` → hex list; `snd `/`ediM` → MP3; `XMED` → SWF or Text Xtra text; SWA member → linked path |
| `proplist` | the Lingo literal parser (§2.2) → `Lingo` values and `serde_json` |
| `interactive` | §7 converters, which return the dataset and a list of data issues (empty on this disc) |
| `locate` | `msencdata://~catalog~/dswMedia/…` → `dswmedia/…` key → `generic.ecs` package → unpacked file; finding `ia/<name>` folders |

For the media converter: `encarta_formats::director::swa_file_to_mp3(&bytes) -> Result<Vec<u8>>` (header checked and stripped). Transcoding to Opus is left to the converter.

CLI `director`: `ls <file> [--json]`, `member <file> <cast> <n> [--out P]`, `chunk <file> <id> [--out P]`, `fields <file>`,
`climate <dir> [--pretty]`, `population <dir> [--pretty]`, `swa <in.swa> <out.mp3>`, `check <dir>`. `<dir>` is the interactive's
folder or any folder above it (`SWALLDSK.EIT`). Bitmaps in external casts take their palette from a `.dcr` next to them or one folder up.

Whole-disc results (`tests/director.rs`): all 71 Director files open (7 `.dcr`, 64 `.cct`). They hold 907 fields, 447 decodable bitmaps,
720 scripts and 46 sounds (counting the Kids copies). The Kids copies are byte-identical, and every file's key resolves through `generic.ecs` to
its own path. The 5 `.swa` files yield valid MP3, which ffprobe and ffmpeg decode cleanly. Members with nothing to extract are shapes (28) and film loops (7, `SCVW`).
