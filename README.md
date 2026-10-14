# encarta

A clean-room resurrection of **Microsoft Encarta Premium 2009**: an offline converter turns the disc's
data into modern formats, a Rust server serves them, and the encyclopedia runs in a web browser.

This repository contains **no Microsoft code or content**. You supply your own disc image.

- [Plan](docs/PLAN.md): the step-by-step roadmap.
- [Clean-room policy](docs/CLEAN_ROOM.md): how we learn the formats without copying code.
- [Format inventory](docs/formats/inventory.md): every data format on the disc and its status.

## Layout

```
crates/encarta-formats   readers for the disc formats (+ `eit`, `catalog`, `akc`, `findidx`, `smallfmt`, `tour`, `mad`, `director` CLIs)
crates/encarta-convert   one-time converter from the disc to out/ (`content`, `media`, `tours`, `atlas`, `search`, `soundfont`, `all`)
crates/encarta-search    tantivy search index, autocomplete and spelling
crates/encarta-server    axum server: the API, media, tiles, tours and the web client
web/                     React + Vite + TypeScript client (3D tours in web/src/tours)
k8s/                     raw Kubernetes manifests for the k3s deployment (see k8s/README.md)
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
target/release/catalog resolve 761575094      # content id → AKC package + record
target/release/akc id 761575094               # decoded article XML (Zebra)
target/release/akc check --all                # decode and verify every AKC record
target/release/findidx stats                  # search index exports
target/release/smallfmt timeline extracted/iso/EE/ENCARTA/TIMELINE.DAT
target/release/tour check extracted/eit/CONTENT/WORLD/7015/

target/release/encarta-convert all            # extracted/ → out/ (content, media, tours, atlas, search)
target/release/encarta-convert soundfont      # GM soundfont for the MIDI synth → out/soundfont/ (needs an SF2,
                                              #   default /usr/share/soundfonts/FluidR3_GM.sf2; --sf2 FILE)
(cd web && npm ci && npm run build)
target/release/encarta-server --out out --static web/dist   # http://localhost:8080
```

Server settings (flags win over env): `--out`/`ENCARTA_OUT` (default `out`), `--static`/`ENCARTA_STATIC`
(default `web/dist`), `--listen`/`ENCARTA_LISTEN` (default `0.0.0.0:8080`), `--extracted`/`ENCARTA_EXTRACTED`
(off; serves media straight from the disc when there is no media manifest). The API is in [docs/api.md](docs/api.md).

The reference VM is described in [docs/vm.md](docs/vm.md).
