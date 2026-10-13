# Atlas (Virtual Globe), gazetteer side files, and 3D Virtual Tours

Status: **triaged.** The container layer is solved for every format below. The MAD payload codec has been
identified from the decompilation but has not been re-implemented yet.

Evidence comes from hex inspection plus Ghidra output for MOBB200.DLL in
`analysis/decompiled/ghidra/MOBB200/`. Helper scripts used during triage (not part of the repo) did the following:
parse the MAD TOC, check entropy per section, walk the RMAP/RAST chunks, and check the layouts of
the 3D walkmap and point-cloud files.

## 0. Which binaries handle what

| Data | Reader | Evidence |
|---|---|---|
| `*.MAD` (atlas DB) | **MOBB200.DLL** ("Map Object", MapPoint common code; PDB `e:\MapPointCommon\main\Common\source\MapObject\bin\Release\Mobb200.pdb`), hosted by ENCARTA.EXE (`CEVGMapControl`, `EncMOBBThread`, EVG = Encarta Virtual Globe) | strings `\c_CD.MAD`, `_HD.MAD`, `Invalid/corrupt MAD-file`, `Unsupported Mad-file version`, the table names. ENCARTA.EXE has `MAD\%s_CD.MAD` (%s = WLD or SWLD). |
| `MSWORLD.SDB`, `D%d.TXT`, `D%dPLACES.TXT`, `MSWorld.eit` | ENCARTA.EXE | strings `D%d.TXT`, `D%dPLACES.TXT`, `MSWorld.eit` |
| GEOCOMP.DLL | ENCARTA.EXE only (`GCMDICreateDecompression`/`GCMDIDecompress`, plus `GCMCI*` compressors). **MOBB200 does not use it** | MOBB200 has its own codec (see 1.4) |
| 3D tours (`.x .3lt .3wm .3cl .3sc .32d .3vp .cur`) | **AREF/COMPNTS/ACTX3DX3.OCX**, "Microsoft 3DX Control" / "MS 3DX3 OCX" v11.0.0.806 (C) 2001. Uses DirectDraw/D3D immediate mode (D3DERR_* strings) and D3DX for .x files. Debug path `C:\3DX3\ActiveX3DX3\...` | strings `WALKMAP.LOAD(`, `LIGHTS.LOAD(`, `CLOUDSCALED=<filename>,<fScale>`, `LOADVP(`, file dialog filters `*.32d`, `*.3cl`, `*.3cp`, `*.3cm` |

---

## 1. MAD files: Encarta World Atlas database

| File | Size | Version string | Sections |
|---|---|---|---|
| CONTENT/MAD/WLD_CD.MAD | 306,507,800 | `WA.US.CD.0505` | 19 (bulk: terrain rasters, object, geometry and find grids) |
| EE/ENCARTA/WLD_HD.MAD | 7,034,784 | `WA.US.CD.0505` | 29 (indexes, styles, fonts, legends, symbols, entity tables) |
| EE/ENCARTA/SWLD_CD.MAD | 18,650,363 | `WA.US.CD.0205` | 10 (reduced atlas: HYPSO raster only, small grids) |
| EE/ENCARTA/SWLD_HD.MAD | 496,233 | `WA.US.CD.0205` | 27 |

**The HD and CD files are two halves of one logical database.** The installed `_HD` file holds the
lookups, indexes and styles. The `_CD` file (left on the disc) holds the bulk grids. For example,
`GeoObjLookup0409` and `GeoGemLookup` are in WLD_HD, while `GeoObjGrid0409` and `GeoGemGrid` are in WLD_CD.
SWLD ("limited atlas") matches the ENCARTAR.DLL text "This Trial Edition includes a limited world
atlas…". It is probably used by the KIDS/Student/Trial SKU.

### 1.1 Top-level header and TOC (solved, all four files parse; offset+len chain is exact)

```
0x00 u32  magic   = 0x00002711 (10001)   MOBB checks: != 0x2711 -> "Unsupported Mad-file version"
0x04 u32  nsect
0x08 char version[20]  "WA.US.CD.0505"  (NUL padded)
0x1C u32  0xABCD1234
0x20 u32  0x0900A105 (WLD) / 0x0900A102 (SWLD)   build/date stamp?
0x24 ...  zeros, 0x3C u32 0x003FCFF8 (constant in all four)
0x40 nsect x 0x30-byte entries:
       u32 offset (absolute), u32 length, char name[40] (NUL-terminated; the tail holds
       uninitialised heap garbage like 0x017969xx, so ignore it)
```
The first section starts right after the TOC (`0x40 + n*0x30`), and each later section starts at
the previous section's offset plus its length.
In the decompilation, table names come from a 62-entry name table (`PTR_s_PDCAttribute_63606330`).
Language-specific tables get `sprintf("%4.4hx", lcid)` appended (`0409` = en-US, `0009` = en neutral).

WLD_CD sections: `Terrain` (164.6 MB, RMAP), `MillerTerrainLku`+`MillerTerrainGrid` (17.9 MB),
`LangId` (4 B: 09 04 09 00), `GeoObjGrid0409/0009` (14 MB each), `Find Ndx/Grid/Key/Data` and
`FindGECContextLookup/Grid` for 0409 and 0009, and `GeoGemGrid` (66.6 MB).
WLD_HD sections: `GeoObjLookup0409/0009`, `GeoTrans`, `GeoLabel`, `G1G2 to Unicode`, `Unicode Attributes`,
`ShieldId`, `Label Font` (UTF-16 "Times New Roman"…), `GeoFaceScale`, `GeoPenDef`, `MapStyle` ("Atlas Default",
"Atlas Tectonic", … each naming a raster and a LOOK style), `BitmapPalette/Color/DimDir/Lookup`, `Bitmap`
(point-symbol icons), `PDCAttribute` (feature-class names such as "PPL - sovereign capital million+ city",
"Road - text label - minor", "Seamount"), `PDCAltitudeLookup/Grid`, `GeoGemLookup`, `MinRect`,
`EntityCenter`, `EntityConvKey/Lookup`, `Font Conv`, `LegendLookup/Grid`, `MillerStyles`, `EntityConv`.

MOBB also knows table names that are absent here (`GeoStreetBodyGrid`, `GeoStreetNPGrid`, `GeoTagLookup`,
`GridIndex*`). These belong to MapPoint/Streets and are not shipped.

### 1.2 Section kinds

1. **`INDX` index** (`*Lookup`, `*Lku`, `Find Ndx`, `FindGECContextLookup`; also inside RMAP)
   ```
   "INDX", u32 npages, u32 key[npages] (ascending: last/first key per page, binary-searched
   by FUN_63532eb7), then npages x 2048-byte pages
   ```
   Sizes check exactly: 2060 = 8+4+2048, 4112 = 8+8+4096, 26684 = 8+13*4+13*2048, HYPSO 410408 = 8+200*(4+2048).
   The page contents are bit-packed key→(offset,len) records and have not been decoded.
2. **"Grid" sections** (`*Grid*`, `EntityConv`, `Bitmap`, `MillerTerrainGrid`). These are blobs of records,
   each compressed with the MOBB codec (1.4). Entropy is 7.99 bits/byte and zlib cannot shrink them.
   Small grids still show repeated structure, so they are compressed, not encrypted. Typical header:
   `u16 nfields, u32 0x1a, u32 4, u32 -1, …, ff ff fe ff` (`LegendGrid`: `02 00 1a 00 00 00 04 00 00 00 ff ff ff ff 00 00 00 00 01 00 00 00 02 00 00 00 ff ff fe ff`).
   The serialized per-field models are probably next. Not yet decoded.
3. **Plain tables**, low entropy and readable directly. `Find Data0409` (778 KB) is a plain NUL-separated
   string pool. It holds the feature-class taxonomy ("Populated Place", "second-order administrative
   division", …) and place-name strings ("Remscheid, North Rhine-Westphalia, Germany", ISO codes like `RU-MOW`).
   Other plain tables: `MapStyle`, `PDCAttribute`, `GeoPenDef`, `Label Font`, `MillerStyles`, `G1G2 to Unicode`
   (8-bit legacy glyph set → Unicode), and the palettes.
4. **`RMAP` raster container** (`Terrain`), see 1.3.

### 1.3 `Terrain` = RMAP raster pyramid (layout fully verified for all 12 rasters)

```
RMAP header (0x54 bytes, constructor FUN_63547e37 writes 'RMAP', 0x00020001, 0x54):
 +0x00 "RMAP" +0x04 u16 1,u16 2 +0x08 u32 0x54 (hdr len) +0x0C u32 4
 +0x10 f32 0.00175 (~0.1 deg in radians)
 +0x14 u32 TESS off (0x54)   +0x18 u32 TESS len (0x14854)
 +0x1C u32 RAST off (0x148A8)+0x20 u32 RAST len (0x9C8)
TESS chunk ("TESS", u32 0x34, then (off,len) sub-tables): f32 unit-circle sin/cos tables plus
   subdivision flags. These are globe-tessellation lookup tables and can be regenerated.
RAST chunk: "RAST", u32 n (=12), n x 0xD0 entries. All offsets are relative to the RAST chunk start:
   char name[32]; u32 levels; u32 ?; u32 look_off; u32 look_len; u32 ?;
   f32 scale; u32 indx_off; u32 indx_len; u32 data_off; u32 data_len; u32 model_off; u32 model_len
   (Decompilation: FUN_63571be5 picks a raster by name with _strnicmp over 0xD0 strides and reads
   the model at +0x50/+0x54 into memory. +0x40 is passed to the INDX loader FUN_63532e17.)
Order on disk for each raster: LOOK | nstyles x (RCT 0x408 + CMT 0x804) | INDX | model | tile data
LOOK: "LOOK", u32 nstyles, nstyles x {char name[32]; u32 rct_off,rct_len(0x408); u32 cmt_off,cmt_len(0x804)}
   RCT = "RCT\0", u32 ncolors, 256 x RGBx palette. CMT = "CMT\0" + 256 x 8-byte colour-map entries.
   MOBB checks magics 0x544352/0x544D43 (FUN_6357185a).
Model block: u32 0x1234FB00, u32 len, u32 0x1C, u32 ?, u32 127, u32 127, then u16 tables
   (likely Huffman models plus per-tile fill codes).
```

| Raster | Levels | Scale (f32) | Styles (LOOK) | Tile data |
|---|---|---|---|---|
| HYPSO | 8 | 150 | HYPSO, HYPSOFLAT, POLITICAL, STATISTICAL, GENERALFLAT, GENERAL, LBU_BORDERS | 139.1 MB (200 INDX pages) |
| SATELLITE | 5 | 2000 | SATELLITE | 16.4 MB |
| MOON | 3 | 14000 | MOON | 2.3 MB |
| NIGHT | 4 | 4000 | NIGHT | 1.8 MB |
| TECTONIC | 3 | 8000 | TECTONIC | 1.3 MB |
| RAINJAN/RAINJULY/RAINAVG, TEMPJAN/TEMPJULY, POPULATION | 2–3 | 8000–14000 | own | 0.3–0.5 MB each |
| LOCATOR | 0 | 60000 | LANDWATER | ~2 KB |

SWLD_CD has only HYPSO (7 levels, 9.5 MB, style HYPSO) and LOCATOR.

The raster pixels are **palette or class indices**, not RGB. The colour comes from a LOOK style, so the
same HYPSO grid is drawn as physical, political, statistical and other map looks. `scale` is probably the
ground resolution at the finest level (units not verified). The `MillerTerrainGrid` (17.9 MB) is a
separate set of terrain tiles for flat Miller-cylindrical maps.

### 1.4 Compression: "MOBB codec" (identified, not yet re-implemented)

MOBB200 strings: `Compressed byte-section corrupt?` (xref 0x6352520A) and
`Compressed word-section corrupt?` (xref 0x63524D4C). The decoders are FUN_63524E50 (bytes) and FUN_635249F0 (16-bit words).
- The bitstream is MSB-first, read from little-endian u32 words (FUN_635248F0 = getbits).
- Symbols are decoded with **canonical Huffman** through a `{limit, base, len}` triplet table (FUN_6352487D).
  Each field has 4 tables (literal/symbol, distance, length, run), selected by a context index read
  from the stream (FUN_6354964B).
- Symbol alphabet:
  - `< 0xFEFB`: literal value
  - `0xFEFB..`: entry from a per-field phrase dictionary (each entry is 0x1C bytes: ptr, count)
  - `0xFFFA`/`0xFFFB`: run of the value 2-back or 1-back
  - `0xFFFC`: LZ copy, *backwards*
  - `0xFFFD`: LZ copy, forwards (distance and length from their own Huffman tables)
  - `0xFFFE`/`0xFFFF`: escapes for a raw 16-bit or 11-bit value
- Post-filters per field (field descriptor stride 0x48): mode 2 = **delta coding with a −32000 bias**
  (word streams, typical for elevation or coordinates); mode 1 = "sentinel means repeat previous".
- **No encryption anywhere**: no key schedule or XOR in the path, and the string pools are plaintext.
  The high entropy comes only from the Huffman coding.

Work still needed: the on-disk serialization of the per-field models (the Grid section header and the RMAP model block), the INDX page
record format, and the record schemas of each grid type (geometry coordinate encoding, class and style ids,
label text in the G1G2 code page). Reading the decompilation and reimplementing it in Rust is
feasible because the codec functions are small (about 300 lines). The schema semantics are the hard part.

### 1.5 Suggested modern targets

| Content | Target |
|---|---|
| Terrain / thematic rasters | Decode tiles → index arrays → apply the LOOK palette → **WebP/PNG XYZ tiles in PMTiles** (or COG). Keep the raw index tiles too, so styles can be recoloured client-side. |
| GeoGem (geometry) + GeoObj (features) | **MVT vector tiles in PMTiles** + MapLibre style JSON generated from MapStyle/GeoPenDef/PDCAttribute |
| Find* / EntityCenter / EntityConv | **SQLite (FTS5) gazetteer** (name, class, hierarchy, centroid, article link) |
| Symbol bitmaps, fonts, legends | PNG sprite sheet + MapLibre glyphs generated from the disc's own fonts (local use) |

### 1.6 Effort and risks
- TOC/container: **S** (done).
- Codec reimplementation (shared by rasters and grids): **M**.
- Rasters → tiles: **M–L**. Main risk: working out the tile addressing and projection. The INDX keys
  must be mapped to (level, x, y), and the globe uses its own tessellation (TESS), so reprojecting to
  Web Mercator may be needed.
- Vector and gazetteer grids → MVT + SQLite: **XL**. About 30 table types with undocumented schemas,
  multi-scale generalisation, label placement rules and 1.8 M places. This is the riskiest part of the project.
  **Decision (project owner):** no substitution with open data. The MAD vector, gazetteer and style
  tables will be fully decoded so that the atlas is reproduced with Encarta's own content.
- Only MOBB200.DLL documents this format, so all knowledge must come from clean-room reading of the
  decompilation (see the project rules).

---

## 2. Atlas side files

| File | What it is | Structure | Target / effort |
|---|---|---|---|
| **EE/ENCARTA/MSWORLD.SDB** (1.6 MB, CRLF text, cp1252) | "EVGSDB" = Virtual Globe **statistics database**, used by the "Atlas Statistical" map style (STATISTICAL LOOK) | `// Section` headers. `Signature` (EVGSDB), `Revision` (18051300), `Countries` (193 rows `id<TAB>name`), `Indicator Category List` (12 bit-flag categories), `Qualifier List` (10), `Unit List` (575: id, unit, alt unit, factor, offset; e.g. °C→°F 1.8/17.78), then **363 x** (`Indicator Package List`: id, name, category, unit, ?, source) + `Items For The Above Indicator List` (count; rows `country_id value qualifier unit year ? note`) | JSON/SQLite. **S** (½ day). The country ids are the same ids used by the `dfg` flags. |
| **D1..D4.TXT** (35 B each) | Geography Quiz difficulty presets | e.g. D1: `20` questions / `5 4 3 2 0` points per attempt / `0 0 0` / `.70 .20 .10` question-type mix. Format strings in ENCARTA.EXE: `%d %d %d %d %d`, `%s %s %s` | JSON. **S** |
| **D1..D4PLACES.TXT** (74–689 rows) | Quiz question pools per difficulty | `type group entity 0`. type 0/1/2. group = 265 or 27082..47610. entity up to 10039884. These are **MAD entity references**, so resolving them needs EntityCenter/GeoObj decoding (or a manual mapping) | **S** to parse. Resolution depends on MAD (§1). |
| **CONTENT/MSWORLD.EIT** → `dfg/*.dfg` (191) | **Country flags**, 64x64 8-bit BMPs with a pure green (0,255,0) key background. Named by SDB country id (9 = Angola, 100 = Guinea, 101 = Guyana, which matches the SDB) | Standard BMP | PNG with alpha (key out green). **S** |
| MSWORLD.EIT → `dtn/*.dtn` (3587) | **Media thumbnails** for map pushpins / "Media Maps" (photos), 64x64 8-bit BMPs. Named by 5-hex media id or `T<id>A` | Standard BMP | WebP/PNG. **S** |
| MSWORLD.EIT → `map/map.dat` (5.4 KB) | HTML/CSS/JScript fragment for the map pane UI (MapStyles/Legend/MapMaker/MediaMaps buttons; `MapObject.Zoom(...)`, `msref:pid=vg99&cmd=article&id=`) | Text | UX reference only. **S** |
| **EE/ENCARTA/MSWORLD.ITR** (2.9 MB, 2001) | ITOLITLS store of InfoTech **full-text (`$FX*FTI`) and word-wheel (`$WW*KIndex`) indexes**: Main, Titles, Images, Audio, Video, MapTreks, WldTours, Stats, … plus `$WW$CTCATALOG`. Already unpacked to `extracted/eit/EE/ENCARTA/MSWORLD.ITR/` | Container solved. The BTREE/DATA/PROPERTY formats inside are MS IT search internals | **Do not convert.** Rebuild search from the content (e.g. tantivy/SQLite FTS). No binary references the `.itr` name directly, so it may be legacy. |
| EE/ENCARTA/PUSHPINS/*.GPD | Empty user pushpin DB templates (20–44 B) | — | Ignore |

---

## 3. 3D Virtual Tours: CONTENT/WORLD/7015/*.EIT (7 tours)

| Tour | Id | Main mesh | Notes |
|---|---|---|---|
| Acropolis | 04257 | acr.x 2.1 MB | 6 `.3cl` statue point clouds |
| Roman Forum | 05285 | rom.x 4.9 MB | 3 floor walkmaps |
| Beaumaris Castle ("Gate Next the Sea") | 05916 | bea.x 5.9 MB | has `.3vp` |
| Persepolis | 05917 | per.x 5.1 MB | |
| Colosseum | 05918 | col.x 5.8 MB | has `.3vp` |
| Abu Simbel | 05919 | abu.x 2.9 MB | has `.3vp` |
| Azuchi Castle | 05920 | azuchi.x 7.9 MB | 7 floors |

Each tour also contains 489 JPG / 151 BMP / 41 GIF textures and icons, 84 `.cur`, and stray authoring files
(`Thumbs.db`, `.psd`, `error.log` with `\\phoenix\media\...` paths).

| Ext | Standard? | Structure (evidence) | Target | Effort |
|---|---|---|---|---|
| `.x` | **Standard DirectX .x, binary, uncompressed** (`xof 0302bin 0032`, 32-bit floats). Templates used: Header, Material, TextureFilename, Mesh, MeshNormals, MeshTextureCoords, MeshMaterialList, plus Frame (rom.x only). No animation. 2k–30k Mesh objects, named (e.g. `Cloud05`, `n_00statue`) because scripts refer to them by name | Write a clean-room binary-.x tokenizer in Rust, or use assimp (BSD) as an offline tool | **glTF 2.0 (.glb)**, keeping node names. Merge or instance the meshes for performance. | **S–M** |
| textures | Standard JPG/BMP/GIF | `.x` files often say `foo.bmp` while only `foo.jpg` ships. The engine falls back by file stem (29/38/13/66 refs in bea/col/abu/azuchi). Only 1 texture is truly missing (per.x) | KTX2 or WebP/JPG | **S** |
| `.cur` | Standard Windows cursor (32x32) | — | PNG + CSS `cursor:` | **S** |
| `.3sc` | Proprietary **text script** (CRLF, `'` comments): `CAMERA.*`, `SCENE.*`, `3D.LOAD(acr.x)`, `LIGHTS.LOAD`, `WALKMAP.LOAD/ON`, `3D.<mesh>.SPRITE/CLOUDSCALED=x.3cl,0.88/NOILLUMINATION/SKY/WATER`, `LIGHTS._TORCH1.FLICKER=.1`, `2D._Hnn.TEXT=^…^;ANCHORPOINT=x,y,z,yaw;MACRO.MOUSEUP=SCRIPT.EVENT(HotSpotClicked:hotSpotID=701504853)` (links to Encarta media/article ids) | Small interpreter in TS, or convert offline to a JSON scene description (camera presets, hotspots, per-node flags) | **M** |
| `.3wm` | Proprietary **walk map** (collision/floor cells) | `u32 ver=3; f32 bbox[6]; u32 ncells`. Each cell: `u32 kind, u16 ?, u16 nverts, nverts x f32 xyz`, then either a plane `f32 a,b,c,d` (kind 4) or `u32 count` + a packed visibility/adjacency list (kind 2/3). Not fully decoded | Navmesh JSON or glTF collision mesh. Port the walk and collide logic to JS | **M** |
| `.3lt` | Proprietary **baked light influence** file | `u32 ver=3; u32 nlights (acr: 71)`. Per light: `f32 x,y,z,radius; u32 BGRA colour; char name[]` (`_TORCH4`, `SUNFILL`, …; a fixed buffer with uninitialised junk after the NUL), then lists of u32 indices (meshes/vertices lit by the light). Used for flickering torches | Bake into glTF `COLOR_0`, or KHR_lights_punctual plus a flicker animation | **M** |
| `.3cl` | Proprietary **voxel point cloud** ("Cloud") used for statues placed onto `*.sprite` placeholder nodes | `u32 ver (3, or 2 for wordcld); u32 n; n x {i16 x,y,z, u16 0; f32 sortkey (ascending); u16 RGB565 colour; u16 0}`, plus 4 trailing bytes in v3. Size formula `8+16n(+4)` matches all 7 files (6.3k–186k points) | glTF `POINTS` primitive with colours, or three.js `Points`/instanced splats | **S–M** |
| `.32d` | Proprietary **2D overlay layer** (HUD icons, label styles: "ERS Label"/Tahoma, icon strip `3DVTicon.bmp`, frame animation) | Binary, not decoded | Decode, then render as an HTML/CSS overlay | **M** |
| `.3vp` | Proprietary, loaded with `LOADVP/APPENDVP/PROCESSVP`. Probably visibility/view points | `u32 n` + an f32 xyz stream (many repeated points). Only 3 files | Probably drop (modern GPUs do not need a PVS) | **S** to ignore |

**Decision (project owner): full interactive parity is required** (walking with collision, flickering lights, point-cloud statues, HUD, hotspots). The static-only option below is a milestone, not the target.

**Overall 3D effort:** **M** for static glTF scenes with textures, clouds and hotspots in three.js.
**L** for full interactive parity (walkmaps, gravity, flicker lighting, script events, 2D layers).
Risks:
- Per-mesh draw calls (up to 30k meshes) need merging or instancing.
- The `.x` uses D3D (left-handed) conventions, so the handedness and V coordinate must be flipped for glTF.
- `.3wm`, `.32d` and `.3vp` are not fully decoded.
- ACTX3DX3 behaviour details (camera speed units, gravity) may need decompilation to reproduce.

---

## 4. Summary table

| Format | Purpose | Compressed / encrypted | Binary | Target | Effort |
|---|---|---|---|---|---|
| MAD container/TOC | Atlas DB | — | MOBB200.DLL | — | S (done) |
| MAD RMAP rasters | Globe terrain and thematic maps | MOBB Huffman+LZ codec / no | MOBB200.DLL | PMTiles (WebP) + palettes | M–L |
| MAD vector/object/find grids | Geometry, features, labels, gazetteer | MOBB codec / no | MOBB200.DLL | MVT PMTiles + MapLibre style + SQLite FTS | XL |
| MSWORLD.SDB | Country statistics | plain text | ENCARTA.EXE | JSON/SQLite | S |
| D*.TXT, D*PLACES.TXT | Geography Quiz | plain text | ENCARTA.EXE | JSON (needs MAD entity ids) | S (+MAD) |
| MSWORLD.EIT dfg/dtn | Flags / media thumbnails | none (BMP) | ENCARTA.EXE | PNG/WebP | S |
| MSWORLD.ITR | Legacy FTI/word-wheel search | LZX container (solved) | (ITSS) | rebuild search | ignore |
| 3D `.x` | Tour geometry | none | ACTX3DX3.OCX | glTF | S–M |
| 3D `.3sc/.3wm/.3lt/.3cl/.32d/.3vp` | Scripts, collision, lights, statues, HUD | none | ACTX3DX3.OCX | JSON scene + glTF extras | M–L total |

Next steps, in priority order:
1. Reimplement the MOBB codec from the FUN_635249F0/FUN_63524E50 logic. Test it on the TECTONIC raster
   (1.3 MB, 3 levels) and on `LegendGrid` (363 bytes).
2. Decode the INDX page records to recover tile keys.
3. Dump the HYPSO and SATELLITE pyramids.
4. Only after that, start on GeoObj/Find (the gazetteer) before GeoGem (the geometry).
