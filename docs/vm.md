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
  `tours-ready` (live: home page, the 7 tours extracted to `C:\tours\<id>\`, our tour host built in `C:\tools`).
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
- Not yet available: sound in the guest; Flash (no official installer any more); 3D hardware (the Cirrus card forces
  the tours' software renderer, so gravity mode can't be observed); a debugger workflow (OllyDbg 1.10 is installed, unused).
