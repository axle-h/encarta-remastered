# encarta

A clean-room resurrection of **Microsoft Encarta Premium 2009**: an offline converter turns the disc's
data into modern formats, a Rust server serves them, and the encyclopedia runs in a web browser.

This repository contains **no Microsoft code or content**. You supply your own disc image.

- [Plan](docs/PLAN.md): the step-by-step roadmap.
- [Clean-room policy](docs/CLEAN_ROOM.md): how we learn the formats without copying code.
- [Format inventory](docs/formats/inventory.md): every data format on the disc and its status.

## Layout

```
crates/encarta-formats   readers for the disc formats (+ `eit` CLI)
docs/                    plan, clean-room policy, format specifications
tools/                   exploration scripts
vendor/                  your disc image                  (gitignored)
extracted/               unpacked disc data               (gitignored)
analysis/decompiled/     Ghidra / ILSpy output            (gitignored, never committed)
out/                     converter output                 (gitignored)
vm/  fixtures/           reference VM, private captures   (gitignored)
```

## Getting started

```sh
mkdir -p extracted/iso && 7z x -oextracted/iso vendor/*.iso
cargo build --release
target/release/eit ls   extracted/iso/EE/ENCARTA/CATALOG.DLX
target/release/eit extract extracted/iso/CONTENT/MDSTD01.EIT extracted/eit/CONTENT/MDSTD01.EIT
```
