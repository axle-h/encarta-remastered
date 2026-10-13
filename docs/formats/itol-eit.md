# ITOLITLS containers (.EIT, CATALOG.DLX/.KDC, MSWORLD.ITR)

Status: **solved** — `crates/encarta-formats/src/itol.rs`, CLI `eit`.

Same container family as Microsoft Reader `.lit` (and a cousin of CHM/ITSF):

- Header `ITOLITLS`, version 1, header length 0x28, 5 "pieces", secondary header length 0xE8,
  GUID `{0A9007C1-4076-11D3-8789-0000F8105754}`.
- Pieces are `(u64 offset, u64 size)`. Piece 1 is the directory (`IFCM` + `AOLL` chunks).
- Secondary header holds `CAOL` and `ITSF` blocks; `ITSF+16` = content offset for section 0.
- `AOLL` chunk: 0x30-byte header (`free_space` at +4), then entries
  `encint name_len, name, encint section, encint offset, encint size` until `chunk_size - free_space`.
  encint = big-endian base-128, high bit = continue.
- `::DataSpace/NameList`: section names (UTF-16): `Uncompressed`, `MSCompressed`.
- Only transform seen anywhere on the disc: LZX `{0A9007C6-4076-11D3-8789-0000F8105754}`.
  **No DRM/DES transform in any file.**
- LZXC control data: version 3, reset interval 4 frames, window 4 x 32 KiB = 128 KiB.
  ResetTable v3: `n, entry_size=8, hdr_len=0x28, u64 uncompressed_len, u64 compressed_len, u64 frame=0x8000`, then n u64 offsets.
- Quirk: some streams pad the final frame to a full 32 KiB and end with a match that runs past
  the real end; others do not pad. The reader tries the exact length and falls back to a full frame.

Result: all 80 containers on the disc extract cleanly — 437,049 files, 3.0 GB.
