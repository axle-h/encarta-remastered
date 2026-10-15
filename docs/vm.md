# Windows XP reference VM

Encarta 2009 does not run under Wine, so ground truth comes from a QEMU/KVM Windows XP Pro SP3 guest
with Encarta Premium 2009 installed from the user's own disc. Everything lives in `vm/` and
`fixtures/`, both gitignored; nothing from the VM is committed. The Windows licence key is typed
into setup straight from the user's own media and is never stored (see [CLEAN_ROOM.md](CLEAN_ROOM.md)).

- Guest: XP Pro SP3, i440fx/IDE, 2 vCPU, 1.5 GB, Cirrus 1024x768, usb-tablet, user networking with
  `restrict=on` (no internet; one forwarded localhost port for our in-guest agent). The guest clock is
  pinned to 2009-03-01 at every boot, so keep the `-rtc` option.
- Encarta: full "Copy To Hard Drive" install (all content local), Encarta Kids included. No product key was needed.
- Snapshots in `vm/xp.qcow2`: `xp-base` (disk), `clean-install` (disk), `encarta-home` (live, Encarta on its home page),
  `tours-ready` (live: home page, the 7 tours extracted to `C:\tours\<id>\`, our tour host built in `C:\tools`),
  `flash-ready` (live: `tours-ready` plus Flash Player 10.3 and cdb; use this one).
- Helpers: `vm/start.sh`, `vm/stop.sh`, `vm/savevm.sh`, `vm/vmctl.py` (QMP: screenshots, keys, mouse),
  `vm/agent.py` + `vm/guest/encagent.cs` (our own small agent: run commands, move files, dump the rendered
  DOM of Encarta's HTML view), `vm/xchg.sh` (FAT32 exchange disk via mtools). Only one QMP client can
  connect at a time, so only one person or tool should drive the VM at once.
- Opening content: `ENCARTA.EXE msencarta://view=ERSArticleView/id=<id>[/display=article]` (forwarded to a
  running instance). Other views seen: `ERSMediaView`, `VisualBrowserView`, `ERSHomescreenView`, `-q <term>` search.
- Capturing: `vm/start.sh --loadvm encarta-home && vm/capture-article.sh <id> [--outline]` writes
  `fixtures/articles/<id>/` with a screenshot, a stitched full-length image, per-screen images, the rendered
  HTML, the text, and metadata. These are private fixtures for checking our decoders and renderer against.
- Other helpers: `vm/mapshot.py` (atlas URL → screenshot) and `vm/tour.py` + `vm/guest/tourhost.cs`, our own small host
  for the tour control at Encarta's size, which renders the same as Encarta's to within 0.1 % of pixels and reads
  back camera state (run `vm/tour.py setup` after each start).
- Results: [vm-findings.md](vm-findings.md), grouped by spec. Captures are indexed in `fixtures/{articles,tours,atlas}/INDEX.md`.
- Full instructions: `vm/README.md` (local only).
- Sound: `vm/start.sh --loadvm encarta-home --wav out.wav` hot-plugs a USB audio device (XP's own usbaudio.sys, ~20 s to
  install) and QEMU records everything the guest plays to the WAV (it only grows while sound plays). usb-audio cannot be
  migrated, so no live snapshot holds it; `vm/scratch/sndmatch.py` matches a slice against the ENCARTAR.DLL sounds.
- Flash: Adobe Flash Player 10.3.181.16 ActiveX (Adobe's archive zip, via the Internet Archive; downloaded on the host
  into `target/dl-flash/`, moved in through the exchange disk). Restart Encarta once after installing it.
- Debugger: Debugging Tools for Windows 6.12 (`dbg_x86.msi` from download.microsoft.com, `target/dl-debugger/`),
  installed at `C:\Program Files\Debugging Tools for Windows (x86)\`. `vm/dbg.sh peek <image> "<cdb commands>"`
  reads memory without stopping the process; `vm/dbg.sh attach <image> <script.cdb>` attaches cdb with a script of
  breakpoints that log to `C:\inst\*.log` (`vm/dbg.sh log`). Killing cdb also ends the debuggee. Module bases
  differ per process (the tour control sits at 10000000 in tourhost, relocated in ENCARTA.EXE). Breakpoint addresses
  come from `analysis/decompiled/` and stay there or in docs as provenance pointers only (CLEAN_ROOM.md).
- Not available: 3D hardware (the Cirrus card forces the tours' software renderer, so the hardware cloud step and
  MaxView can't be observed). OllyDbg 1.10 is also installed, unused.
