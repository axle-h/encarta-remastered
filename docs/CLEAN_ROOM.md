# Clean-room policy

This project rebuilds Encarta 2009 on a modern stack by **understanding** its data formats,
then writing original code that reads them. We never copy Microsoft code into the project.

## What belongs to whom

| Microsoft's (never committed, never shipped) | Ours (this repository) |
|---|---|
| The disc image and every file on it: executables, DLLs, data packages, media, fonts, XSL/JS templates | Format specifications written in our own words (`docs/formats/`) |
| Anything derived mechanically from their code: decompiler or disassembler output | Rust readers, converters, server and web client written from those specifications |
| Content converted from the disc (articles, media, maps, tours) | Exploration scripts written from observing the files (`tools/`) |
| Captures from the original app (screenshots, exported articles) | Synthetic test data we create ourselves |

Each user supplies their own copy of the disc. The project distributes code only.

## Where knowledge may come from

1. **Observing the data**: hex dumps, statistics, trial parsing, comparing files.
2. **Public documentation** of shared formats (Microsoft Reader `.lit`, CHM, LZX, DirectX `.x`,
   SAMI, SWF, ASF) and open-source libraries under compatible licences.
3. **Black-box behaviour** of the original app, running in the Windows XP reference VM
   (it does not run under Wine): what it displays and how it responds.
4. **Static and dynamic analysis** (Ghidra, ILSpy, a debugger in the VM) to learn *what* a format
   contains and *how* an algorithm behaves. This is allowed only for understanding, under the rules below.

## Rules

1. **Decompiled and disassembled output lives only in `analysis/decompiled/`**, which is gitignored.
   Its contents never leave that folder, are never pasted into the repo, and are never machine-translated into Rust.
2. **Specification first.** Before a format is implemented, its behaviour is written up in
   `docs/formats/<format>.md` in our own words: field tables, offsets, bit layouts, algorithm
   descriptions in prose or short pseudocode, and worked examples taken from the data.
   - Allowed: function addresses (e.g. `FUN_4549EE70`), as provenance pointers.
   - Allowed: constants and small tables that interoperability depends on (magic numbers, field
     widths, code-length tables), recorded as facts, with how they were confirmed against the data.
   - Not allowed: decompiled function bodies or copied code structure.
3. **Implement from the specification.** Rust code is written from `docs/formats`, not with the
   decompiler output side by side. It uses our own names, types and structure. If the spec turns out
   to be wrong or incomplete, fix the spec first, then the code.
4. **Validate against the data, not the code.** Readers are checked by decoding the whole disc
   (structural invariants, well-formed output, cross-references that agree), and by comparing with
   private captures from the reference VM.
5. **No Microsoft bytes in git.** `.gitignore` excludes the ISO, extracted and converted data,
   decompiler output, VM images and private fixtures. Unit tests use synthetic data we build ourselves.
   Integration tests read the user's local disc through `ENCARTA_ISO` / `ENCARTA_EXTRACTED`
   and are skipped when it is absent.
6. **Never link, load or redistribute Microsoft binaries.** The original DLLs run only inside the
   reference VM, and only as a source of ground truth.
7. **Original templates are reference only.** The XSL/JS UI templates on the disc tell us what the
   content schema means. Our renderer and UI are written from scratch.
8. **Secrets stay out.** The Windows XP licence key is read from the NAS at VM install time and is
   never written into this repository.

## Review checklist (every change touching a decoder)
- [ ] The spec in `docs/formats/` was updated in the same change, or before it.
- [ ] No decompiler identifiers (`FUN_`, `DAT_`, `LAB_`, `uVar`, `piVar` …) in code.
- [ ] Control flow and naming are our own, not a transliteration.
- [ ] No disc-derived bytes in tests or fixtures.
