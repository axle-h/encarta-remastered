# 3D Virtual Tours: data formats and ACTX3DX3 runtime behaviour

Status: **implemented** (plan item 1.7, Step 1 decoders): every file kind parses with zero
unexplained bytes in all 7 tours, and the self-check values below are asserted by the integration
tests (see §12). Items still marked **[VM]** need dynamic confirmation in the Windows XP reference VM.
They are collected in §10. Items marked **(VM)** were measured there (black-box, see
[vm-findings.md](../vm-findings.md)); those measurements override the static reading where they disagree.

This document is the only input a converter or runtime implementer needs. It is written from
hex inspection of the 7 tour containers, trial parsing of every file, public documentation of the
DirectX `.x` format, and clean-room reading (per [CLEAN_ROOM.md](../CLEAN_ROOM.md)) of the
Ghidra output for `AREF/COMPNTS/ACTX3DX3.OCX` ("Microsoft 3DX Control", v11.0.0.806) and for
the host `EE/ENCARTA/ENCARTA.EXE`. Function addresses such as `FUN_10040990` are provenance pointers
into those two binaries (OCX addresses are `0x1000xxxx`–`0x100fxxxx`, EXE addresses are `0x004xxxxx`–`0x006xxxxx`).

The summary in [atlas-and-3d.md §3](atlas-and-3d.md) is superseded by this file wherever they disagree.

Conventions: all binary integers and floats are little-endian; `f32` is IEEE single precision. "Name
compare" means case-insensitive ASCII unless stated otherwise. Units are the scene's own model units, and the scale differs between
tours (see §6.1).

<!-- TOC is maintained by hand -->
1. Tours, containers and file catalog
2. Runtime architecture (control, host, frame tick)
3. `.3sc` script language
4. Host integration (ENCARTA.EXE) and events
5. DirectX `.x` subset, textures and the container catalog
6. Camera, input, walk maps (`.3wm`), collision and gravity
7. Lights (`.3lt`), flicker, point clouds (`.3cl`), `.3vp` and per-object 3D properties
8. 2D layers (`.32d`), HUD, captions and hotspots
9. Validation: parse coverage and self-check values
10. Checklist for the XP reference VM
11. Remaining unknowns
12. Implementation (Rust reader and CLI)

---

## 1. Tours, containers and file catalog

Seven tours live in `CONTENT/WORLD/7015/<nnnnn>.EIT` (ITOLITLS containers, see [itol-eit.md](itol-eit.md)).
The Encarta content id of a tour is `7015` followed by the 5-digit container number. For example, content id
`701504257` is container `7015/04257.EIT`. The host builds the path by formatting
`world\%u.eit` with the 9-digit id and then inserting a `\` after the 4th digit.

| Content id | Container | Tour | Model | Walk maps | Notes |
|---|---|---|---|---|---|
| 701504257 | 04257.EIT | Acropolis | acr.x | 1 | 7 `.3cl` files; 5 are used, on 12 statue placeholders (`floral` only in commented-out lines, `wordcld` v2 unreferenced) |
| 701505285 | 05285.EIT | Roman Forum | rom.x | 7 | 3 floors + 4 stairs |
| 701505916 | 05916.EIT | Beaumaris Castle | bea.x | 9 | `level2.3vp` |
| 701505917 | 05917.EIT | Persepolis | per.x | 2 | map rotated −90° |
| 701505918 | 05918.EIT | Colosseum | col.x | 10 loaded (11 files) | `level3.3vp`, `STAIR5.3WM` (32 B, not loaded) |
| 701505919 | 05919.EIT | Abu Simbel | abu.x | 1 | `abu.3vp` |
| 701505920 | 05920.EIT | Azuchi Castle | azuchi.x | 13 | 7 floors + 6 stairs; map rotated −90° |

The full per-extension catalog of all 892 files (textures, cursors, scripts, binary formats and authoring junk) is in §5.7.

---

## 2. Runtime architecture

### 2.1 The control's COM interface

`ACTX3DX3.OCX` is an MFC ActiveX control. Its dispatch map (table at `0x100fbbe0`) has, in dispid order:

| Dispid | Member | Kind | Signature | Purpose |
|---|---|---|---|---|
| 1 | `Timer` | property | `BOOL` (get `FUN_1000d1d0`, set `FUN_1000d1e0`) | Enables the per-tick update (alarms, input, rendering). Same flag as script `TIMER.ON/OFF`. |
| 2 | `Do` | method | `BSTR Do(LPCSTR command)` (`FUN_1000d0f0`) | Runs one command string through the script interpreter (§3) and returns its result string. **This is the only way the host drives the control.** |
| 3 | `Try3DHardware` | method | `BOOL ()` (`FUN_1000dde0`) | Selects a hardware Direct3D device. Same as `SCENE.TRYHARDWARE`. |
| 4 | `TryFullScreen` | method | `void ()` | Same as `SCENE.TRYFULLSCREEN`. |
| 5 | `Force3DSoftware` | method | `BOOL ()` (`FUN_1000e7d0`) | Selects the software rasteriser. |
| 6 | `AbortLoad` | method | `void ()` (`FUN_1000e960`) | Abandons a load in progress. |
| −552 | `AboutBox` | method | stock | |

The control has one event, **`Event3DX(LPCSTR text)`** (event dispid 1). It is fired by
`SCENE.EVENT(...)`/`SCRIPT.EVENT(...)` (§3.6) and once, by the control itself, with the text
`FinishedInitializing` on its first paint. Events are suppressed while a full-screen mode switch is in progress.

A modern runtime does not need COM. It needs: a `do(command) -> string` entry point, an event callback, and a
tick. The rest of this document describes what `do` does.

### 2.2 Start-up handshake (host ↔ control)

1. The control paints for the first time. It fires `Event3DX("FinishedInitializing")` and creates an
   internal alarm named `STARTUP` (interval 0.3 s) whose macro is `SCENE.EVENT(STARTUP)`. The host
   ignores the resulting `STARTUP` event.
2. On `FinishedInitializing` the host (`FUN_004d4a10`):
   1. calls `Try3DHardware` if the user setting "hardware acceleration" (host field `+0xcd8`) is on,
      otherwise `Force3DSoftware`;
   2. sends `SCENE.HALFREZDRIVING=1` or `=0` (host field `+0xcdc`). **The control does not recognise this
      spelling** (it expects `HALFRESDRIVING`), so the command has no effect. A faithful port ignores it.
   3. normal case: sends `script.itsfileurl=<full path of world\7015\nnnnn.eit>` and then `script.load=load.3sc`.
      Development override: if a folder `_3DVTContent\<id>` exists it sends `script.load=<folder>\load.3sc`
      instead, and files are read from disk.
3. `load.3sc` runs (§3.7). At its end it fires `WORLDLOADED:...` (§4.1) and starts a 1-second alarm that fires
   `USERLOCUPDATE`. **(VM)** `USERLOCUPDATE` arrives every 1.00–1.02 s while a tour is loaded.
4. **(VM)** After `WORLDLOADED` the host sends the camera command of the tour's **default Tour Stop** (§4.3), then
   shows the site map and the stop's text. In the Colosseum the scene appeared 2.65 s after the request and the
   captions, site map and text 0.65 s later.

**(VM) Replies.** The reply of `Do()` is not a reliable success signal: `script.load=…\load.3sc` returns `Failed`
although the load then runs (`WORLDLOADED` 1.6–2.3 s later), `SCENE.DISPLAYFPS=1` returns `Failed` but switches the
overlay on, and `SCENE.HALFREZDRIVING=0` returns `Failed` (the misspelling of step 2.2). `script.itsfileurl=<path>`
returns `1` when the container is found and `-2` for a path containing spaces (they do not survive, §3.2), so the
host must pass an 8.3 path. A camera command pair such as `CAMERA.MOVETO=…;CAMERA.PITCH=…` returns `Success;Success`.

### 2.3 The tick

The control has a tick routine (`FUN_1000d3c0`, entered through `FUN_1000cc80` once initialised). On every tick:

1. **Progressive loading** (while the "loading" flag is set, which happens at the end of every `3D.LOAD`):
   - if textures remain, load **one** texture and end the tick;
   - otherwise, if a script was suspended (§3.7), resume it up to and including the next line that starts with
     `SCENE.DRAW` or `SCRIPT.DRAW`, then end the loading step for this tick;
   - when nothing is left, clear the loading flag.
2. If `Timer` is on (and the control is active): poll the alarms (§3.5), process keyboard/mouse input and move
   the camera (§6), then render. The 3D scene is drawn only when `3D.ON` is in effect **and** no load is in
   progress. The 2D layer is always drawn, which is how the `begin.32d` loading animation stays visible
   while the model streams in.

The tick is driven by a 1 ms Windows timer (`SetTimer`). On XP the timer resolution and the render time
limit the real rate, and movement is scaled by real elapsed time (§6.9). **(VM)** `SCENE.DISPLAYFPS=1` shows
`SW 3 65.83` standing still and `SW 3 47.08` while walking (Acropolis start, 616×420, software device): about
64 ticks/s idle (the 15.6 ms system tick), fewer while the rasteriser is busy. The `3` is unexplained.

---

## 3. `.3sc` script language

### 3.1 File loading (`SCENE.LOAD`, `SCENE.RUN`, `script.load`; `FUN_10044790`)

1. Resolve the name:
   - **ITS mode** (`ITSFILEURL` set, which is always the case in the shipped product): strip any directory
     part and read the file by its basename from the tour container. Names are matched case-insensitively.
   - **Disk mode:** a name that starts with `\` or has a drive letter (`X:`) is used as is. Any other name is
     appended to the *working path*.
2. Set the working path to the directory part of the resolved name (the text up to the last `\`).
3. Split the bytes into lines. A line ends at CR, and an LF right after the CR is swallowed too. A bare LF does
   **not** end a line, but every shipped file uses CRLF. **TAB characters are deleted** wherever they occur.
   No other trimming takes place.
4. Skip empty lines and lines whose **first character** is `'` (a comment). A `'` anywhere else is ordinary
   text. For example `'3d.statue_plan08...` is commented out, but ` '...` with a leading space would not be.
5. **Macro blocks:** when a line, upper-cased, starts with `BEGIN(`, start collecting. While collecting, each
   line (after comment skipping) is appended to a buffer **with no separator**. When a line starts with `END(`,
   append it too, stop collecting, and pass the whole buffer to the interpreter as one command. That command
   defines the macro (§3.4).
6. Every other line is passed to the interpreter as one command, in order.
7. After the last line, the loader discards all keyboard and mouse messages queued during the load.
8. While a file executes, `HALFRESDRIVING` is forced off and then restored.

A block still open at the end of the file does not occur on the disc; our loader delivers it as it is.

Consequence of step 5: a statement inside a block that lacks its trailing `;` merges with the next line. This
happens once on the disc, in Acropolis `lhfuncs.3sc` `HIDEALL`:
`3d.n_00water.WATER=0` + `3d.Box23a.VISIBLE=0` + `2D._dot.VISIBLE=0;` becomes the single statement
`3D.N_00WATER.WATER=03D.BOX23A.VISIBLE=02D._DOT.VISIBLE=0`. That statement sets WATER from the value text
`03D...`; flag properties look only at the first character after `=`, so this is `WATER=0` and the rest is discarded (see §7.4). `Box23a` is **not** hidden by `HIDEALL`
(`load.3sc` hides it directly), and `_dot` is not hidden by that statement either (it is hidden elsewhere).
Reproduce this quirk by running the scripts through the same algorithm, rather than special-casing the line.

### 3.2 Command strings and statements (`FUN_10040990`)

A *command* is what one `Do()` call or one script line delivers. Processing order:

1. If the command is exactly `?`, return the help text of every subsystem.
2. Upper-cased, `STARTMACROLOG` / `STOPMACROLOG` open or close a log file `macro.log` that records every
   command and event. This is a debugging aid and can be ignored.
3. If the upper-cased command starts with `RUN(`: see §3.4. If it starts with `BEGIN(` or `DELETE(`: see §3.4.
4. Otherwise split the command at **every `;`**. This is a plain split: parentheses and `^…^` text give no
   protection. Each non-empty piece is a *statement*:
   1. **Case folding:** if the statement contains `^`, upper-case only the part before the first `^`. Otherwise
      upper-case the whole statement. So object names, keywords and file names are case-insensitive, and
      only `^…^` text keeps its case.
   2. Split at the first `.` into *subsystem* and *rest*. Dispatch on an exact match of the subsystem:

      | Subsystem | Handler | Section |
      |---|---|---|
      | `SCENE`, `SCRIPT`, or empty (a statement starting with `.`) | scene/script | §3.3 |
      | `CAMERA` | camera | §6 |
      | `WALKMAP` | walk maps | §6 |
      | `LIGHTS` | lights | §7 |
      | `3D` or `MODEL` | model objects | §7.4, §7.5 |
      | `2D` | 2D layer | §8 |
      | `TIMER` | `ON`/`OFF` set the Timer flag; `ON?`/`OFF?` query it | |

      A statement with any other subsystem is ignored and produces no result.
   3. Each handler returns a result string (`Success`, `Failed`, a value for `?` queries…). The results of the
      statements are joined with `;` and returned from `Do()`.

**Argument conventions** used by every handler. `NAME=value`, `NAME(value)` and `NAME(value` (with no closing
parenthesis) are interchangeable. The unclosed form exists because the `;` split often cuts statements just
before their `)`. Handlers trim leading `=`/`(` and trailing `)`. Booleans accept `1`, `TRUE`, `ON` (and for
some properties `ACTIVE`) for true, and `0`, `FALSE`, `OFF` for false. Numbers are parsed like C `atof`/`atoi`:
leading whitespace is skipped, the longest numeric prefix is used, and a non-numeric value gives 0. `NAME?`
queries return the current value. Floats are formatted `%f`, so `CAMERA.POS?` returns `x,y,z,yaw`.
**(VM)** Spaces in values are not preserved: `script.itsfileurl=C:\Program Files\…` fails (`-2`) where the same
file's 8.3 path works, so the text loses its spaces (or is cut at the first one) before the path is used.

### 3.3 `SCENE.` / `SCRIPT.` commands (`FUN_10041f80`)

The *rest* (the text after `SCENE.`) is matched by **prefix** in this order. The first match wins.

| Command | Seen on disc | Semantics |
|---|---|---|
| `WORLDINFO?` | – | Returns `<largest bbox extent>;<bbox centre x,y,z>` of the loaded model. |
| `FORCESOFTWARE` / `TRYHARDWARE` / `TRYFULLSCREEN` / `TRYHARDWAREFULLSCREEN` / `KILLFULLSCREEN` | `TRYHARDWARE` (Colosseum macro `LARGE`, never run) | Device selection. A web runtime treats these as no-ops that return `Success`. |
| `DELETEALL` | yes, 7 tours (3 as `SCRIPT.DELETEALL`) | Full reset: aborts any suspended load; deletes the model, lights, point clouds, walk maps (a fresh empty walk-map set is created), 2D layer, camera state, all alarms and all macros; sets 3D drawing on; resets fog to colour `0x4B96E6`, start 0, end 20, fog off. |
| `FOGCOLOR=r,g,b`, `FOGCOLOR?` | – | Fog colour (integers 0–255). |
| `FOGSTART=f`, `FOGEND=f` | – | Linear fog range. Fog is enabled iff `FOGEND > 0.01`. |
| `HALFRESDRIVING=bool` | – | Render at half resolution while moving. See §6. |
| `SPECULAR=bool`, `HIGHSPECULAR=bool` | `SPECULAR=0` (all tours) | Specular lighting toggles. See §7. |
| `DISPLAYFPS=0/1` | `=0` | Frame-rate overlay. The 2nd character of the rest is tested: `=1` turns it on, anything else turns it off. |
| `LOAD(file)` / `LOAD=file` | yes | Run a script file (§3.1). Returns `Success` or `Failed`. |
| `GRAVITY=f`, `GRAVITY?` | yes | Gravity constant (§6). |
| `LOADFILESFROMITS=bool` | – | Flag. |
| `ITSFILEURL=path`, `ITSFILEURL?` | via host | Sets the container to read files from. The file must exist. Returns 1, or a negative code (−1 empty, −2 not found, −3 is a directory). |
| `CURSORS=bool`, `CURSORS?` | `=TRUE` | Enables the custom cursors set by `CURSOR`. |
| `CURSOR=index,file` | 7 per tour | Cursor slot `index` (0–6) gets the `.cur` file. Slot meaning: §6.9. |
| `EVENT(text)` / `EVENT=text` | yes | Fires `Event3DX(text)` to the host (§3.6). |
| `WORKINGPATH=path`, `WORKINGPATH?` | – | Working path for disk-mode file names. |
| `RUN(file)` / `RUN=file` | – | Same as `LOAD`. Note that the **top-level** `RUN(` form is different: it runs macros (§3.4). |
| `DRAW` | yes (55×) | Render a frame now. During a suspended load it also marks a yield point (§3.7). |
| `SHOWCOLLISION=bool`, `SHOWCOLLISION?` | – | Debug drawing of walk-map geometry. |
| `AMBIENT=r,g,b`, `AMBIENT?` | yes | Ambient light colour (integers 0–255), applied to every mesh of the loaded model. Ignored (returns `Failed`) when no model is loaded. See §7. |
| `ALARM.…` | yes | §3.5. |
| `CLEARSEQUENCE`, `ADDSEQUENCE` | – | Alarm sequences, as `ALARM.<name>.ADDSEQUENCE=…` (§3.5). |

*Implementation note:* where one keyword is a prefix of another, the order is not certain. Our parser
tests `TRYHARDWAREFULLSCREEN` before `TRYHARDWARE`, `HIGHSPECULAR` before `SPECULAR`, `LOADFILESFROMITS`
before `LOAD`, and `CURSORS` before `CURSOR` (as in the table). Of these, only `TRYHARDWARE`, `SPECULAR`,
`LOAD` and both `CURSOR` forms occur on the disc, so the choice has no effect there. Prefix matching is
what makes the `SCENE.DRAW)` piece of the `LOADALARM` line (§3.5) draw.

### 3.4 Macros: `BEGIN`, `END`, `RUN`, `DELETE`

- **Define:** `BEGIN(name);body…END(name);`. The text between `BEGIN(` and the first `);` is the name. The
  body is the text after that `);` up to the first occurrence (case-insensitive) of `END(name`. Re-defining
  an existing name replaces the body. Names are stored upper-cased.
- **Run:** a command that starts with `RUN(` is split at `;`. Each `RUN(name)` piece runs the named macro's
  body as a command (recursively, through §3.2). An unknown name yields `Failed`.
- **Delete:** `DELETE(name)`.
- The macro table is cleared by `SCENE.DELETEALL`.

Macros on the disc: `HIDEALL` and `SHOWALL` in every tour, plus `LARGE` in the Colosseum
(`SCENE.TRYHARDWARE; camera.backgroundcolor=0,0,0`). `load.3sc` runs `RUN(HIDEALL)`. **Nothing runs `SHOWALL`
or `LARGE`.** Captions and hotspots are instead made visible **by location**: the walk-map hotspot cells'
enter macros set `2D._Ln.VISIBLE=1` / `2D._Hn.VISIBLE=1` for the items near that spot, and their exit
macros run `RUN(HIDEALL)` (§6.5, §8.6). The host's menu item "Show Labels and Hot Spots" sends `2D.ON` /
`2D.OFF`, which gates the whole layer.

### 3.5 Alarms (timers that run macros)

`SCENE.ALARM.NEW(active, maxExecutions, interval, name, macro)` creates an alarm:

| Arg | Type | Meaning |
|---|---|---|
| `active` | int | 1 = start active |
| `maxExecutions` | int | Number of firings before the alarm deactivates itself. −1 = unlimited. |
| `interval` | float | Seconds between firings, measured on a high-resolution real-time clock (`QueryPerformanceCounter`) |
| `name` | text | Upper-cased name used by the other `ALARM.` commands |
| `macro` | text | Everything after the 4th comma. Trailing `)` characters are trimmed. |

Other forms: `ALARM.ACTIVATE` / `ALARM.DEACTIVATE` (global enable for all alarms, which also restarts every
alarm clock), `ALARM.<name>.ACTIVATE` (sets the alarm active and resets its count), `.DEACTIVATE`,
`.ACTIVE.TOGGLE` / `.ACTIVE=TOGGLE`, `.DELETE`, `.ADDSEQUENCE=text`, `.CLEARSEQUENCE`. A sequence is a list
of strings. On each firing, if the sequence is not exhausted, every `@` in the macro is replaced by the
sequence entry whose index is the current firing count. No tour uses sequences.

**Polling** happens once per tick (§2.3), and only while alarms are globally active. For each alarm: if its
clock is not running and the alarm is active, start the clock and reset the count. If the elapsed time is at
least `interval` and the alarm is active, run the macro, increment the count, deactivate the alarm once
`count >= maxExecutions` (when `maxExecutions >= 0`), and restart the clock. An alarm therefore fires at most
once per tick, and late firings are not caught up.

**Worked example (the loading animation).** The `load.3sc` line
`SCENE.ALARM.NEW(1,-1,0.1,LOADALARM,2d._0.nextframe(16);scene.draw)` is split at its `;` into two statements:

1. `SCENE.ALARM.NEW(1,-1,0.1,LOADALARM,2D._0.NEXTFRAME(16)`, which becomes alarm `LOADALARM`, every 0.1 s,
   unlimited, with macro `2D._0.NEXTFRAME(16` (the trailing `)` is trimmed). The 2D handler accepts the
   unclosed form.
2. `SCENE.DRAW)`, which matches `DRAW` and draws once immediately.

`SCENE.ALARM.ACTIVATE` then enables alarms globally. The animation advances while the model streams in, and
`SCENE.ALARM.LOADALARM.DELETE` removes it at the end of `load.3sc`.

The other alarm on the disc is `SCRIPT.ALARM.NEW(1,-1,1.0,USERLOCUPDATE,SCRIPT.EVENT(USERLOCUPDATE))`. Its
macro becomes `SCRIPT.EVENT(USERLOCUPDATE` (both trailing `)` are trimmed), and it fires `Event3DX("USERLOCUPDATE")`
once per second for the rest of the session.

### 3.6 Events

`SCENE.EVENT(x)`, `SCENE.EVENT=x` and `SCRIPT.EVENT(x)` all fire `Event3DX(x)`. The handler strips one
leading `(` or `=` and one trailing `)`. Because of case folding (§3.2), event text without `^` arrives
**upper-cased**. Hotspot macros (`2D._Hn.MACRO.MOUSEUP=SCRIPT.EVENT(HotSpotClicked:hotSpotID=701504853)`)
therefore deliver `HOTSPOTCLICKED:HOTSPOTID=701504853`. The host compares names case-insensitively.
**(VM)** Confirmed: `HOTSPOTCLICKED:HOTSPOTID=…`, `USERLEVELCHANGE:QTSREFID=…` and
`WORLDLOADED:XOFFSET=-1.00,…` arrive upper-cased; the control's own `FinishedInitializing` keeps its case.

Event payload syntax (host side, §4): `NAME` or `NAME:key=value,key=value,…`.

### 3.7 Suspended loading

`3D.LOAD(file.x)` parses the mesh and sets the *loading* flag. Textures are then loaded one per tick (§2.3). When
the script loader (§3.1) finishes a line and finds the loading flag set, it **saves the rest of the file and
returns**. From then on, each tick first loads the remaining textures. After that, it executes saved lines
up to and including the next line that starts with `SCENE.DRAW`/`SCRIPT.DRAW`. When the saved text is
exhausted, the loading flag is cleared and pending input is discarded. Nested `SCENE.LOAD`s that run during the
resumed part are executed synchronously.

This mechanism only affects timing. A port may run the script to completion as soon as all assets are
fetched, as long as (a) the 3D view stays hidden until the end of `load.3sc` (`3D.OFF` … `3D.ON` brackets the
load in every tour), and (b) the `begin.32d` animation plays meanwhile.

### 3.8 Grammar summary (EBNF, after loading)

```
command    = "?" | macro-def | run-list | statement { ";" statement } ;
macro-def  = "BEGIN(" name ");" body "END(" name ");"        (* body: any text, statements joined by ";" *)
run-list   = "RUN(" name ")" { ";" "RUN(" name ")" } [";"] ;
statement  = subsystem "." rest ;
subsystem  = "SCENE" | "SCRIPT" | "" | "CAMERA" | "WALKMAP" | "LIGHTS" | "3D" | "MODEL" | "2D" | "TIMER" ;
rest       = [ object-id "." ] property [ op value ] ;
object-id  = "_" ident | ident ;     (* 2D ids start with "_" (_H26, _L17, _0, _dot); LIGHTS ids too (_TORCH1);
                                         3D ids are mesh names from the .x file *)
op         = "=" | "(" | "?" ;
value      = text-up-to-end ;         (* comma-separated lists; ^...^ quotes literal text *)
```

**Statement inventory across all 36 `.3sc` files** (after folding, with object ids generalised, counting
the statements of every macro body once). These are the counts a re-implementation must handle. Total:
**3,554 statements** (confirmed by the Rust reader). *Correction (implementation):* the earlier table gave
2D `VISIBLE` 681, 3D `VISIBLE` 300 and `SCENE.LOAD` 36. After loading, the glued `HIDEALL` statement
(§3.1) swallows one 3D and one 2D `VISIBLE`, so the counts are 680 and 299; and there are 29
`SCENE.LOAD`s (5 in Acropolis, 4 in each other tour). The 7 `SCENE.DRAW)` pieces left over from the
`LOADALARM` split (§3.5) are counted separately from the 55 `SCENE.DRAW`.

| Statement shape | Count | Statement shape | Count |
|---|---|---|---|
| `2D.<id>.VISIBLE=n` | 680 | `3D.<id>.SPRITE=1` | 304 |
| `3D.<id>.VISIBLE=n` | 299 | `3D.<id>.FILMSTRIP=n,f,file` | 269 |
| `2D.<id>.ANCHORED=1` | 254 | `2D.<id>.ANCHORPOINT=x,y,z,yaw` | 252 |
| `2D.<id>.TEXT=^…^` | 224 | `3D.<id>.APPLYTEXTURE=file` | 203 |
| `LIGHTS.<id>.FLICKER=f` | 198 | `3D.<id>.NOILLUMINATION=1` | 163 |
| `2D.<id>.ICON=n` | 119 | `2D.<id>.MACRO.MOUSEUP=SCRIPT.EVENT(HotSpotClicked:hotSpotID=n)` | 119 |
| `SCENE.DRAW` (+ 7 `SCENE.DRAW)`) | 55 | `WALKMAP.LOAD(file)` | 43 |
| `SCENE.CURSOR=n,file` | 49 | `CAMERA.BACKGROUNDCOLOR=r,g,b` | 15 |
| `3D.<id>.CLOUD.OFFSET=f` | 12 | `3D.<id>.CLOUDSCALED=file,f` | 12 |
| `CAMERA.NEAR=f` | 10 | `CAMERA.CLEARBACKGROUND=0` | 8 |
| `2D.<id>.NEXTFRAME(16)` | 7 | `CAMERA.{POS,PITCH,FOV,HEIGHT,SPEED.FLY,SPEED.TURN,PITCHSPEED,COLLISIONRADIUS,CLIMBRATE}=…` | 7 each |
| `SCENE.{LOAD×29,DISPLAYFPS,SPECULAR,AMBIENT,GRAVITY,CURSORS,ALARM.NEW,ALARM.ACTIVATE,ALARM.LOADALARM.DELETE}` | 7 each (LOAD 29) | `CAMERA.FAR=-1` | 6 |
| `3D.{OFF,ON,LOAD,CALC64BIT}` | 7 each | `2D.{ICONFILE,ICONS,LOAD(lh.32d),LOAD=begin.32d}` | 7 each |
| `LIGHTS.LOAD(file)` | 7 | `LIGHTS.<id>.OFF` | 3 |
| `WALKMAP.ON` | 7 | `SCRIPT.EVENT(WORLDLOADED:…)` | 7 |
| `SCRIPT.ALARM.NEW(…USERLOCUPDATE…)` | 7 | `SCENE.DELETEALL` / `SCRIPT.DELETEALL` | 4 / 3 |
| `BEGIN`/`END` (macros) | 15 | `RUN(HIDEALL)` | 7 |
| `2D.<id>.ANCHORPOINT=a:b` (two-point) | 2 | `3D.<id>.WATER=0` | 1 |
| `SCENE.TRYHARDWARE` (in macro `LARGE`) | 1 | | |

### 3.9 The common `load.3sc` sequence

All seven `load.3sc` files follow the same template. They differ only in camera constants (§6.11), file
names and the `WORLDLOADED` payload:

1. `SCENE.DELETEALL` (or `SCRIPT.DELETEALL`), `SCENE.DISPLAYFPS=0`, a black background, and
   `CAMERA.CLEARBACKGROUND=0`.
2. Initial `CAMERA.POS=x,y,z,yaw` and `CAMERA.PITCH`, then `SCENE.DRAW`.
3. `2D.LOAD=begin.32d`, `2D._0.NEXTFRAME(16)`, the `LOADALARM` alarm, and `SCENE.ALARM.ACTIVATE`
   (the loading animation).
4. Camera physics constants, then `SCENE.GRAVITY`.
5. `3D.OFF`, `3D.LOAD(model.x)`, `SCENE.SPECULAR=0`, `SCENE.AMBIENT`, `LIGHTS.LOAD(model.3lt)`,
   one `WALKMAP.LOAD` per walk map (top floor first), `WALKMAP.ON`.
6. `2D.ICONFILE=3DVTICON.bmp`, `2D.ICONS=20`, `2D.LOAD(lh.32d)`.
7. `SCENE.LOAD` of `geo.3sc`, `lights.3sc`, `setText.3sc` and `lhfuncs.3sc` (the order varies; Acropolis also
   loads `statues.3sc` first).
8. Seven `SCENE.CURSOR` slots, `SCENE.CURSORS=TRUE`, `RUN(HIDEALL)`, `3D.CALC64BIT=0.05`, and
   `SCENE.ALARM.LOADALARM.DELETE`.
9. `3D.ON`, `SCRIPT.EVENT(WORLDLOADED:…)` and the `USERLOCUPDATE` alarm.

---

## 4. Host integration (ENCARTA.EXE) and events

The tour window classes are `C3DVirtToursWnd`, `CMax3DVTWnd`/`CMax3DVTView` (the maximised view),
`C3DVTBannerWnd`, the site-map window `EncartaVTSitemap`, and `CVTCreditsWnd`. The event handler is
`FUN_004d4a10`. It matches event names by case-insensitive prefix:

| Event text (as fired) | Host action |
|---|---|
| `FinishedInitializing` | Start-up handshake (§2.2). |
| `WORLDLOADED:<pairs>` | Marks the world as loaded, enables the tour toolbar buttons, and configures the site map from the pairs below. |
| `USERLOCUPDATE` | Posts itself a message, then queries `camera.pos?` and updates the "you are here" marker on the site map (§4.2). |
| `HOTSPOTCLICKED:HOTSPOTID=<n>` | Looks `n` up in the tour's hotspot list and **replaces the whole main view** with the media page of that hotspot's `destmedia` (§4.3); the Back button returns to the tour at the same pose (VM). If no entry matches, the debug text `NO CALLBACK: Hotspot to ` applies and nothing happens. |
| `USERLEVELCHANGE:QTSREFID=<n>` | Changes the floor shown on the site map to `n` (the first pair's value). Fired by the enter macros of the walk-map hotspot cells at the top and bottom of stairs (§6.4). For example, Roman Forum uses `QTSRefID=701505289`. |
| `STARTUP` | Ignored. |

*Correction (converter):* `QTSRefID=701505289` is a Colosseum id (sent by cells of `level2.3wm`),
not Roman Forum's. Roman Forum sends 701505293 (`1stFloor`), 701505292 (`2ndFloor`) and 701505286
(`3rdFloor`). Not every `UserLevelChange` string also loads a walk map: some fire only the event,
for the map the cell belongs to (e.g. Roman Forum `3rdFloor.3wm` cell 43, `2ndFloor.3wm` cell 224).

### 4.1 `WORLDLOADED` payload

Keys are case-insensitive. Missing keys keep these defaults:

| Key | Default | Meaning |
|---|---|---|
| `XOFFSET`, `ZOFFSET` | 0 | World X/Z that maps to the **centre** of the site-map rectangle |
| `XEXTENT`, `ZEXTENT` | 0 | World size covered by the rectangle's width/height. If either is 0, the marker is not drawn. |
| `MAPHEIGHT` | – | Used as the rectangle top when `SM_RECTTOP` is 0 or absent. Not used on the disc. |
| `FIELDOFVIEW` | 120 | Width of the view wedge drawn on the map, in degrees, clamped to 30…120. All tours send 80. |
| `SM_ROTATEFRAME` | 0 | Map rotation: 0, 90, −90, 180 or −180 |
| `SM_RECTLEFT/TOP/RIGHT/BOTTOM` (alias `SMRECTBOTTOM`) | client rect | Pixel rectangle of the map image inside the site-map window |

Values on the disc:

| Tour | XOFFSET | ZOFFSET | XEXTENT | ZEXTENT | SM rect (L,T,R,B) | Rotate |
|---|---|---|---|---|---|---|
| Acropolis | −1.00 | 0.65 | 10.00 | 18.30 | 41,7,130,190 | 0 |
| Roman Forum | 6.560451 | 3.50 | 18.671543 | 12.00 | 0,102,170,187 | 0 |
| Beaumaris | −0.108095 | −3.370864 | 49.102837 | 49.102837 | 32,49,141,193 | 0 |
| Persepolis | 21.526201 | 22.00 | 59.439548 | 49.00 | 26,21,156,184 | −90 |
| Colosseum | 5.686062 | 2.584099 | 18.986126 | 18.986126 | 39,108,142,175 | 0 |
| Abu Simbel | −0.179946 | −0.895949 | 21.028120 | 21.028120 | 22,5,150,189 | 0 |
| Azuchi | 1.624422 | −1.050000 | 20.685574 | 20.685574 | 46,108,139,185 | −90 |

### 4.2 Site-map marker (`FUN_005031b0`, `FUN_00503390`)

Given the camera position `(x, z)` and yaw `θ` (degrees, from `camera.pos?`):

1. Rotate the frame by `SM_ROTATEFRAME`: for 90, `(x,z) ← (z, −x)`; for −90, `(x,z) ← (−z, x)`;
   for ±180, `(x,z) ← (−x, −z)`. The offsets are rotated the same way, and for ±90 the extents are swapped.
2. With `W = right − left` and `H = bottom − top` of the map rectangle:
   `px = left + round(W/2 + (x − XOFFSET)·W/XEXTENT)`, `py = top + round(H/2 − (z − ZOFFSET)·H/ZEXTENT)`,
   each clamped to the rectangle. World +Z points up the map.
3. Marker heading `= round(360 − θ + 90 − SM_ROTATEFRAME) mod 360`, in degrees, for drawing the view wedge.
4. The map is redrawn only when the heading changes by more than 2° or the position by more than 2 px.
5. The host also keeps a list of named map *locations*. The nearest location within 8 px (squared
   distance < 64) whose floor matches the current floor is highlighted.

**Site-map artwork and locations (data, confirmed by the captures).** Each Tour Stop's `children/site` record
(group `vtour`, §4.3) is the map shown while at that stop:

- Its image (`item_file` role `image`) is a baggage DIB of 344×194, two 172×194 halves: the plain map, and the
  same map tinted (peach) as it appears inside the view wedge. The site-map window is 172×194 (screen (1,129)–(173,323)
  in the VM captures): the host draws the left half, then the right half clipped to the wedge (apex at the marker,
  `FIELDOFVIEW` wide, reaching the window edges), then the marker, a dark blue (0x103463) four-way arrow. The
  location boxes are part of the artwork.
- Its `children/hotspot` records are the named locations: `title`, `destmedia` = the stop, `x`, `y` = the top-left
  pixel of the location's 11-px box in the window (box centre ≈ x+5, y+5). The loading screen tells the user to
  "click the highlighted points in the map" to jump to a stop; a click on a box moves to its `destmedia` stop.
- Single-floor tours give every stop the same site; multi-floor tours have one site per floor (Roman Forum 3,
  Beaumaris 5, Colosseum 4, Azuchi 7). `USERLEVELCHANGE:QTSREFID=<n>` names a **stop id**, and the map switches to
  that stop's site. Stops whose site does not list them (the "Level n" stops, Roman Forum Third Floor) have no box.
- Check against the data: for every stop that has a box, the marker of steps 1–2 at the stop's `coord3d` position
  lands within 13 px of the box centre (within 5 px for most), including the −90 frames of Persepolis and Azuchi
  (within 9 px), which supports the rotation rule of step 1. At the Temple of Athena Nike stop the capture's marker
  is drawn in a lighter blue (0x6371B5) with a dotted box: that is one phase of the marker's blink (below).

**(VM)** Confirmed for an unrotated map: at the six Acropolis stops the marker sits exactly where step 2 puts it
(site-map window origin at screen (1,129)), +Z points up the map, and the wedge (about 80° wide) points up the map
for yaw 3.4° and 19°, down for 197° and 204°, down-right for 155°. **[VM]** The rotated maps (Persepolis, Azuchi,
`SM_ROTATEFRAME=-90`) were checked later, see below.

**(VM)** Rotated maps confirmed (Persepolis `fixtures/tours/701505917/sitemap/`, Azuchi `fixtures/tours/701505920/sitemap/`,
window coordinates = screen − (1,129)). Persepolis Apadana Stairs: marker exactly at the predicted (96,118), wedge
headings 102.2/177.5/71.2/194.5° against 102/180/72/196°. Azuchi (rect 46,108,139,185), marker centre measured / predicted
by `siteMapMarker`: Grounds (start and via Tour Stops) (139,157)/(139,157) (x clamped to the rect right edge; unclamped
141), Donjon (100,153)/(100,153), Living Quarters (102,139)/(102,139), Octagonal Chamber (88,153)/(89,153) (1 px). Wedge
centre (middle of the two edges) 183.5/177.5/122.5/171.5° against 185/179/124/172°. The floor follows the stop's site:
"Level 1" for Grounds and Donjon, "Level 3" Living Quarters, "Level 6" Octagonal Chamber, with the matching elevation
highlight (as the client does with `stop.site`). Colour: the marker **blinks** between dark blue 0x103463 and light
blue 0x6371B5, each phase about 0.715 s, on a box or off it (VM, sampled every 6 ms); single screenshots catch either
phase.

### 4.3 Hotspots and tour content

The hotspot ids in the scripts (`701504853` …) are Encarta content ids. The host matches them against the
tour's content records: DATA records of group `virttour` in the Premium catalog (`DataDlx`), decoded by the
content converter ([content-schema.md](../content-schema.md) §3). From the data, confirmed by Encarta's menus (VM):

| Record | Field | Meaning |
|---|---|---|
| tour (`7015nnnnn`, `ticon="vtour"`, `type="3DVT"`) | `children/media` (in order) | the **Tour Stops**, in the order of the Tour Stops menu |
| | `default/@refid` | the **default stop**: the one the host moves to after `WORLDLOADED` (§2.2 step 4) |
| | `children/article` | the tour's article |
| stop | `title` | the menu entry |
| | `default/@coord3d` | the camera command string the host sends (`CAMERA.MOVETO=…;CAMERA.PITCH=…;`, disc coordinates; multi-floor tours first move far away, e.g. to `160,…` or `350,…`, then `WALKMAP.LOAD(…)`) |
| | `children/hotspot` (in order) | the stop's hotspots: the submenu entries after "go to the stop" |
| | `children/label`, `children/site`, `children/article` | caption ids, the site-map location, the stop text |
| hotspot | `title` | the menu entry and caption text |
| | `@destmedia` | **the content id the click opens** (a `media` item, or a `flash` interactive) |
| | `@x`, `@y` | not used by the tour runtime |

**(VM) On `HOTSPOTCLICKED`** the host replaces its whole main view with the media page of `destmedia`
(e.g. hotspot 701504869 → `msencxml://content/681500403`: picture, caption and credit), not the tour's side pane.
The Back button returns to the tour at the pose it had when the hotspot was clicked. Several destinations are
`group="flash"` interactives (e.g. "Propylaea, Now and Then"), which need Flash (not installed in the VM).
Tour credits use `display=article_vtcredits`.

There are 119 hotspot macros across the 7 tours (§8), and every hotspot id they fire has a `destmedia`.

---

## 5. Geometry (`.x`), textures, and the container catalog

Sources: observation of the 7 `.x` files and all images (prototype parser
`analysis/decompiled/tours-proto/xfile.py` plus helper scripts in the same folder), the public description
of the DirectX `.x` binary format, and the ACTX3DX3 loader (provenance: FUN_1005d7c0 load entry,
FUN_1005db40 top-level loop, FUN_1005ae60 token reader, FUN_1005b0f0 template dispatch, FUN_1005c800
Mesh, FUN_1005c620 MeshNormals, FUN_1005c480 MeshMaterialList, FUN_1005bf10 Material, FUN_1005bdb0
MeshTextureCoords, FUN_1005bc60 TextureFilename, FUN_10033b50 texture file loading, FUN_100333a0 texture
surface creation, FUN_10035a20 colour key, FUN_1003b670 LOD pairing, FUN_100508a0 LOD switch at draw time).

**Note on the earlier triage in atlas-and-3d.md §3:** it says "2k–30k Mesh objects" and "Frame (rom.x only)".
Both are wrong. The true counts are 531–7,315 meshes per file, and **no file contains a Frame**. The
corrected figures are below.

### 5.1 File header

| Offset | Bytes | Value in all 7 files | Loader check |
|---|---|---|---|
| 0 | 4 | `xof ` | first three bytes must be `xof` |
| 4 | 4 | `0302` | parsed as a decimal number, which must be ≥ 302 ("File version not >= 302") |
| 8 | 4 | `bin ` | first three bytes must be `bin` ("File not bin") |
| 12 | 4 | `0032` | **not checked**; the loader always assumes 32-bit floats |

The token stream starts at offset 16 and runs to the end of the file. There is no trailing data. In all 7 files
every byte is accounted for.

### 5.2 Binary token stream

Every token starts with a little-endian `u16` id. The payload depends on the id. (Token counts in §5.3 count
each id once; the `;` that terminates a STRING belongs to the STRING token. Counting it separately gives
39,445 for acr.x, i.e. 39,357 plus its 88 STRINGs.)

| Id | Token | Payload | Seen in tour files |
|---|---|---|---|
| 0x01 | NAME | `u32 len`, `len` bytes (no NUL; cp1252) | yes |
| 0x02 | STRING | `u32 len`, `len` bytes, then a `u16` terminator token that must be 0x14 `;` (0x13 `,` is legal in the public format; the OCX accepts only `;`) | yes (texture file names only) |
| 0x03 | INTEGER | `u32` | no |
| 0x05 | GUID | 16 bytes (the OCX's reader actually reads u32,u16,u16,8 bytes) | no |
| 0x06 | INTEGER_LIST | `u32 n`, `n × u32` | yes |
| 0x07 | FLOAT_LIST | `u32 n`, `n × f32` | yes |
| 0x0A / 0x0B | `{` / `}` | none | yes |
| 0x0C–0x14 | `( ) [ ] < > . , ;` | none | no (apart from the STRING terminator) |
| 0x1F, 0x28–0x34 | `template` and type keywords | none | **no** (no template declarations) |

**No templates are declared inline.** The files use the standard templates by name only. The OCX does not
parse template declarations at all.

An object is written as `NAME(type) [NAME(instance name)] [GUID] { body }`. Inside a body, `{ NAME }` (three
tokens) is a **reference** to a previously defined named object. The OCX uses references only inside
MeshMaterialList.

The OCX reader expects a **fixed token shape** for each template, as shown below. Any deviation stops the load:
an error flag is set, a debug string `loadbin-<ProcName><code>` is logged, and parsing stops. Meshes that were
already built stay loaded. Every tour file uses exactly these shapes:

```
Header            { INTLIST[3] = major, minor, flags }              (1,0,0 in all files)
Material  <name>  { FLTLIST[11] = faceRGBA(4) power specRGB(3) emisRGB(3)
                    [ TextureFilename { STRING } ] }                (template name also accepted as "TextureFileName")
Mesh      <name>  { INTLIST[1] = nVertices
                    FLTLIST[3*nVertices] = x,y,z ...
                    INTLIST = nFaces, then per face: count, idx0..idx(count-1)
                    child objects in any order:
                      MeshNormals       { INTLIST[1]=nNormals  FLTLIST[3*nNormals]
                                          INTLIST = nFaces, per face: count, nidx... }
                      MeshTextureCoords { INTLIST[1]=nVertices FLTLIST[2*nVertices] = u,v ... }
                      MeshMaterialList  { INTLIST = nMaterials, nFaceIndexes, idx[nFaceIndexes]
                                          then nMaterials × ( {ref} | inline Material ) } }
```

The OCX dispatcher also recognises Frame, FrameTransformMatrix, Animation, AnimationKey, AnimationOptions,
AnimationSet, ProgressiveMesh, Boolean, Boolean2d, MeshFace, MeshFaceWraps, MeshVertexColors, IndexedColor,
ColorRGB(A), Quaternion and Matrix4x4. Most of these are stubs ("need to add this"). **None of them occur in
the tour files**, so a converter can treat any of them as unsupported.

### 5.3 What the files contain (self-check values)

| File | Bytes | Tokens | Top-level objects | Material | TextureFilename | Mesh (= MeshNormals = MeshTextureCoords = MeshMaterialList) | Vertices | Faces (tri / quad) | Triangles after fan | Max verts in one mesh |
|---|---|---|---|---|---|---|---|---|---|---|
| acr.x | 2,082,194 | 39,357 | 1,632 | 89 | 88 | 1,542 | 40,777 | 20,059 (6,342 / 13,717) | 33,776 | 1,694 |
| rom.x | 4,937,381 | 93,221 | 3,799 | 109 | 108 | 3,689 | 102,233 | 41,956 (8,354 / 33,602) | 75,558 | 1,716 |
| bea.x | 5,928,261 | 59,249 | 2,456 | 134 | 133 | 2,321 | 139,074 | 59,640 (17,644 / 41,996) | 101,636 | 2,951 |
| per.x | 5,116,191 | 37,545 | 1,512 | 167 | 166 | 1,344 | 118,050 | 52,269 (18,721 / 33,548) | 85,817 | 1,260 |
| col.x | 5,770,619 | 79,217 | 2,978 | 134 | 133 | 2,843 | 123,015 | 63,108 (53,204 / 9,904) | 73,012 | 908 |
| abu.x | 2,860,300 | 14,333 | 646 | 114 | 112 | 531 | 45,976 | 42,477 (29,460 / 13,017) | 55,494 | 5,054 |
| azuchi.x | 7,938,192 | 184,585 | 7,441 | 125 | 124 | 7,315 | 173,080 | 63,513 (12,819 / 50,694) | 114,207 | 582 |

Top-level order in every file: `Header`, then all `Material`s, then all `Mesh`es. Further facts:

- **Faces are only triangles and quads.** Every Mesh has all four child templates, exactly once each.
- **Every mesh is named, and the names are unique when compared case-sensitively.** Some names differ only
  in case: rom 26 groups (e.g. `front6`/`Front6`, `mesh01`), per 15, bea 1 (`ground`), col 1. Scripts
  address meshes by name, so the script section must define whether name matching ignores case.
  The loader's own comparisons (material references, `lod_` pairing) are **case-sensitive**.
- **Normals are indexed separately.** The normal count equals the vertex count in only 191 of 19,585
  meshes. The per-face normal index lists always have the same sizes as the faces (0 mismatches). glTF
  needs one normal per vertex, so the converter must split vertices on the (position index, normal index)
  pair.
- Texture coordinates are per vertex (count = nVertices in every mesh). Values tile far outside [0,1]
  (−106…168 is typical), so the sampler must use REPEAT. One anomaly: rom.x mesh `o2138` has 4 non-finite
  UV components (±inf, in 2 UV pairs). Replace them with 0. Degenerate faces (a repeated index) occur only rarely: acr 4, bea 1.
- MeshMaterialList takes one of two forms. Either `nMaterials=1, nFaceIndexes=1` for a mesh of two or more
  faces (14,659 meshes; the whole mesh uses the material), or `nFaceIndexes = nFaces` with one index per face
  (4,926 meshes: 3,596 single-face meshes with one material, and 1,330 meshes with 2–7 materials, plus
  per.x `lounge` with 39). *Correction (implementation):* the earlier figures 12,659 / 6,926 and "up to 5
  materials" were wrong; the totals still add up to 19,585. Materials are **always references** `{name}` to top-level Materials. No inline materials occur.
  Every reference resolves.
- **Material values.** Alpha is 1.0 in all 972 materials, so materials carry no translucency. Emissive is
  zero everywhere except one col.x material. Power is 0, 2, 3, 4, 6, 12, 16, 31 or 32. Every material except
  `xof_default` (and abu's unused `Sphere01`) has exactly one TextureFilename. faceColor is mostly grey 0.5
  or 1.0.
- **`lod_` meshes:** acr 116, rom 320, per 64. Each pairs with a mesh whose name is the rest after the
  `lod_` prefix. per's `lod_strim16` has no partner. No names start with `frm-`.
- Bounding boxes (x,y,z min → max): acr (−59.3,−5.7,−61.8)→(56.6,31.0,53.7); rom (−34.6,−8.7,−38.7)→(54.0,35.6,50.0);
  bea ±127 in x/z, −18.8…61.3 in y; per (−619,−49,−634)→(677,362,658) (a large sky/terrain surround);
  col (−186,−9,−190)→(206,187,202); abu (−112.5,−18.5,−105.4)→(113.0,104.4,95.5); azuchi ±119 in x/z, −17.4…104.1 in y.

### 5.4 How the OCX interprets the data

**Load sequence.** The loader reads the whole file. When the host has set ITSFILEURL, it reads the file from
the tour container by its basename instead. It then checks the header (5.1) and parses the top level:

1. The first object is parsed (it is the Header).
2. The second object, **if it is a Material, is parsed and thrown away.** This is the `xof_default`
   material in every file. It is never referenced, so the effect is invisible here, but a converter can
   simply skip it.
3. All remaining objects are parsed in file order. Top-level Materials are registered by name for later
   `{ref}` lookup.

After parsing, the loader computes the scene bounding box over all meshes, and the largest of its three
extents (the "world size"). It then pairs the `lod_` meshes (see below). Textures are not decoded during
parsing. They load afterwards, one per timer tick, and script execution waits until all of them have loaded
(§3.7).

**Material table.** Every Material that a mesh uses is stored in one global table, and **identical
materials are merged**. Two materials are identical when all 11 floats are equal and the texture name is equal
(case-sensitive). Self-check: the merging collapses 10/11/23/27/28/4/17 groups of byte-identical Materials in
acr/rom/bea/per/col/abu/azuchi. A material named exactly `NULL` is ignored. A material with no name and no
texture gets its table index as its name. A converter can keep one glTF material per (texture, flags) pair.

**Mesh build.** Each face becomes one polygon. Each polygon vertex gets position, normal (looked up through
the face's normal index list) and UV (from the vertex index). Faces with ≥3 vertices are rendered as a
**triangle fan from the first vertex**. Only triangles and quads occur, so fan triangulation is exact. The
engine stores vertices in "lit vertex" form: position, a colour (initially opaque white), a specular colour
(initially black) and UV. It computes vertex colours itself from the `.3lt` light data (see §7)
and does not use D3D's lighting. As a result, how faceColor, specular and power affect the final colour is
decided by the engine's lighting code, not by D3D. **(VM)** faceColor is ignored: with `LIGHTS.OFF` surfaces whose
material faceColor is 0.22 or 0.29 show their texels unchanged. Treat the texture as the base colour and take the
colour from the lighting section. The material's specular value probably drives the `SCENE.SPECULAR`/`HIGHSPECULAR` behaviour, which also
belongs to the lighting section.

Polygon material selection:
- If `nFaceIndexes == nFaces`, each face uses its own index into the reference list.
- If `nMaterials == 1`, every face uses that single material.
- A reference whose name is not found leaves the face with the default material.

**Vertex limit.** The engine stops with "Error -- Vertex Table >= 65535 (Direct3Ds limit)" when one internal
vertex table would pass 65,534 entries. The limit applies to a batch, not to a whole file (every file has more
than 40k vertices before it is expanded), so a converter does not need to reproduce it. glTF should use
`uint32` indices, or split primitives.

**LOD (`lod_` prefix).** A mesh named `lod_<base>` is the low-detail stand-in for mesh `<base>` and is
never drawn on its own. The switch is on the **horizontal (XZ) distance** from the eye to the base mesh's
centre, against a threshold of **13.0 units**, a constant set when the renderer is created (details in §7.6).
The `.x`-side reading also saw `3D.LODDISTANCE` added to a distance test. That parameter defaults to 0 and no
tour sets it, so the effective threshold on the disc is 13.0 either way (§7.5 thinks it only affects picking).
A three.js port can map this onto `THREE.LOD` with an XZ-distance switch.

**`frm-`** is used only by the Frame code path, which never runs for these files. **`loadbin-`** is only the
prefix of debug messages.

### 5.5 Texture resolution (exact rule)

The texture name is the STRING inside TextureFilename, taken byte for byte. To resolve it:

1. If the name ends in `.ppm` (case-insensitive), replace the last three characters with `bmp`. This never
   happens in the tours.
2. If the name contains no backslash, put the loader's base path in front of it. When ITSFILEURL is set, the
   base path is dropped again: files come from the tour EIT by basename through a
   `msencdata:<eit>*<name>` URL. Otherwise the base path is the working path, i.e. the directory of the last
   loaded `.3sc`.
3. Try to open the name. If that fails, **replace the last three characters with `jpg`** and try again. If
   that also fails, **replace the last three characters with `gif`** and try again. If all three fail, the
   material has no texture. Note that this replaces the last three *characters*, not the extension, so it
   assumes a 3-letter extension.
4. Lookup is case-insensitive, because the Windows file system and the ITS store both ignore case. The
   converter should match case-insensitively.

Per-file resolution counts (distinct names; exact match / `.jpg` fallback / missing):

| acr | rom | bea | per | col | abu | azuchi |
|---|---|---|---|---|---|---|
| 72 / 0 / 0 | 91 / 0 / 0 | 31 / 29 / 0 | 106 / 1 / 1 | 48 / 38 / 0 | 95 / 13 / 0 | 18 / 66 / 0 |

Every fallback is `X.bmp → X.jpg`. The `.gif` fallback is never needed. The only missing texture is per.x's
`roooof.jpg`. per's `error.log` names the same file (`\\phoenix\...\Persepolis\inbox\roooof.jpg`), so the
original also rendered it untextured.

**Decoding is chosen by content, not by extension.** The engine looks at the first two bytes. `BM` means
its own BMP decoder: uncompressed only, 8-bit paletted or 24-bit. Anything else goes through the OLE picture
loader (JPEG/GIF), which draws into the texture surface. Two files have the wrong extension: per's `Kevin.jpg`
is a BMP, and rom's `ioban3.bmp` is a GIF. A converter must sniff the content too.

**Colour-key transparency (cut-outs).** This matters for parity: trees, statues, fences, windows, railings
and fire are cut-outs. The engine converts each pixel to 16-bit. The format is RGB555, or RGB565 when the
display mode is 565. If any pixel quantises to **pure magenta** (R≥248, B≥248, and G<8 in 555 or G<4 in
565), the texture is marked as colour-keyed. The engine then sets a source colour key of magenta, so those
texels are not drawn. Keyed textures are not mipmapped. Only the **BMP path** can produce a key. JPEG and GIF
textures are always opaque, so any GIF transparency is ignored (to be confirmed in the VM). On disc, 113 BMP
files contain key pixels (acr 18, rom 4, bea 21, per 50, col 10, abu 4, azuchi 6; including `fireAnm.bmp`
in every tour that has it; Persepolis has no `fireAnm.bmp`). *Correction (implementation):* the total was
given as 112; the per-tour counts are right and add up to 113. Because the stem fallback points `X.bmp` references at JPEGs, those surfaces stay opaque.
**Conversion:** write keyed BMPs as PNG with alpha 0 where the 555 test passes (the few near-magenta pixels
differ between 555 and 565 only by a handful of texels), and set glTF `alphaMode: MASK`.

**Sizes.** The engine records whether both dimensions are powers of two (2…4096), but creates the texture at
its native size either way. Every 3D texture on disc is a power of two (16…1024). The only exceptions are
the four 2D/HUD images `3DVTicon.bmp` (420×22), `dir.gif` (541×381), `dirTxt.gif` (286×295) and
`loadtxt.gif` (86×29). Textures that are not colour-keyed get mipmaps, with linear-mip-linear filtering where
the hardware supports it.

`3D.CALC64BIT=<float>` (default 0.1; every tour sets 0.05) only stores a value. The loader does not read it,
so it is not a texture-size switch. Its use (if any) belongs to the renderer analysis.

### 5.6 Coordinate system and glTF conversion

- `.x` uses the Direct3D convention: left-handed, +Y up, and the camera looks along +Z at yaw 0. The stored
  normals agree with `cross(v1−v0, v2−v0)` for 98–99.99 % of faces (acr 19,520 vs 536; rom 41,938 vs 18;
  bea 58,516 vs 118 with 1,006 degenerate; per 51,657 vs 612; col 61,870 vs 1,237; abu 42,473 vs 4;
  azuchi 63,126 vs 374). This is D3D's clockwise front-face winding.
- **To glTF (right-handed):** negate Z for positions and normals, and reverse each triangle's index order
  (so (0,i,i+1) becomes (0,i+1,i)). Apply the same Z negation to all other tour data in world coordinates
  (camera, walk maps, lights, anchor points, clouds), so the whole scene uses one convention.
- **Texture surfaces are bottom-up, so V flips: `v' = 1 − v`.** The engine fills every texture surface with the
  image's *bottom* row first: its own BMP decoder copies the file's rows in file order (a bottom-up BMP, which every
  tour BMP is, none has a negative height), and the OLE picture path used for JPEG and GIF gives the same order (the
  2D GIFs, stored upside down, show upright, §8.4.1). Texture coordinates address the surface with v = 0 at its first
  row, so v = 0 samples the image's bottom row, for every format. glTF puts v = 0 at the image's top row, hence
  `v' = 1 − v` (equivalently: keep the UVs and flip every image vertically). With `REPEAT` sampling this is exact
  for tiled coordinates too. **(VM)** Confirmed by rendering all 40 Tour Stops at their camera commands and comparing
  with the captures: without the flip the Acropolis Propylaea frieze and coffers, the Athena Nike pediment, the
  landscape backdrop and the railing spikes (`fence4.bmp`, pointing down), the Persepolis gate frieze and Gate of
  Xerxes bulls (`bullf12.jpg`), and the Abu Simbel Osiris statues showed the wrong part of their images; with it they
  match (mean absolute pixel difference at the six Acropolis stops 19 → 9 grey levels; what remains is filtering, the
  software rasteriser samples without mipmaps). Most other surfaces are tiling or symmetric textures, which is why the
  error was not visible everywhere. *Correction:* earlier versions said UVs need no change.
- Keep the mesh names as glTF node names, so that `3D.<name>.*` script commands can find them. Keep the
  `lod_` pairs as metadata.

### 5.7 Container catalog (all 7 tour EITs, 892 files)

| Ext | acr 04257 | rom 05285 | bea 05916 | per 05917 | col 05918 | abu 05919 | azu 05920 | Total | What it is |
|---|---|---|---|---|---|---|---|---|---|
| `.jpg` | 36 | 83 | 37 | 63 | 90 | 108 | 73 | 490 | textures (baseline or progressive JPEG; per `Kevin.jpg` is really a BMP) |
| `.bmp` | 27 | 14 | 27 | 56 | 13 | 5 | 9 | 151 | textures, mostly 24-bit (per has 13 8-bit), plus the HUD icon strip `3DVTicon.bmp` and `fireAnm.bmp` film strips; rom `ioban3.bmp` is really a GIF |
| `.gif` | 16 | 5 | 4 | 4 | 4 | 4 | 4 | 41 | GIF89a. The 4 in every tour are 2D/HUD art (`dir`, `dirTxt`, `loadanim`, `loadtxt`); acr also has texture GIFs |
| `.cur` | 12 | 12 | 12 | 12 | 12 | 12 | 12 | 84 | Windows cursors, 32×32. The same 12 in every tour: `sv`, `svdl`, `svdown`, `svdr`, `svleft`, `svright`, `svul`, `svup`, `svur`, `vthot360`, `vthotaud`, `vthotpho`. **`grip.cur` is referenced by every load.3sc but is not present** |
| `.x` | 1 | 1 | 1 | 1 | 1 | 1 | 1 | 7 | geometry (this section) |
| `.3sc` | 6 | 5 | 5 | 5 | 5 | 5 | 5 | 36 | scripts (acr has an extra `statues.3sc`) |
| `.3wm` | 1 | 7 | 9 | 2 | 11 | 1 | 13 | 44 | walk maps |
| `.3lt` | 1 | 1 | 1 | 1 | 1 | 1 | 1 | 7 | lights |
| `.32d` | 2 | 2 | 2 | 2 | 2 | 2 | 2 | 14 | 2D layers (`begin.32d`, `lh.32d`) |
| `.3cl` | 7 | – | – | – | – | – | – | 7 | point clouds |
| `.3vp` | – | – | 1 | – | 1 | 1 | – | 3 | view/visibility points |
| `.log` | – | 1 | – | 1 | 1 | – | – | 3 | authoring junk `error.log`: textures that were missing on the build share (`\\phoenix\media\AEncProd_02\Targets_mac\3DVT\<tour>\inbox\…`): rom `Horiz19.bmp`, per `roooof.jpg`, col `Cloth297.jpg`, `Hllnd009.jpg` |
| `.db` | – | – | 1 | – | 1 | – | 1 | 3 | `Thumbs.db` (Explorer thumbnail cache, OLE compound file), junk |
| `.psd` | – | – | 1 | 1 | – | – | – | 2 | Photoshop sources (`Tower17.psd`, `Sky_019.psd`), junk |

col also has a 32-byte `STAIR5.3WM` that no script loads (see §6).

**Images referenced by nothing.** Each image was checked against `.x` texture names, `.3sc`/`.32d` file-name
strings (including walk-map hotspot strings and 2D element macros), and the resolution rule of §5.5. The
following are referenced by none of them (safe to drop, but keep them in the completeness report).
*Correction (implementation):* the list now also has the eight files that share a stem with a texture
reference that resolves exactly, so the engine never opens them: rom `Blue_98.bmp`, `sky_0105.bmp`
(references are `.jpg`); bea `First8.jpg`; per `Kevin.jpg` (the reference `Kevin.bmp` exists); col
`elev46.jpg`, `elev4956.jpg`; azuchi `BLue_130.bmp`, `Surro124.jpg`. Note also that rom `aslar46.jpg` is
named only by a Material that no mesh uses (counted in the §5.5 table, never loaded), and that six cursors
(`svdl`, `svdr`, `svul`, `svur`, `vthotaud`, `vthotpho`) are in every tour but no `SCENE.CURSOR` slot names
them. Original list:
acr `cloud91.jpg`; rom `bas_02.jpg`, `basalt2.jpg`, `wai02.jpg`; bea `Broun92.bmp`, `Broun93.bmp`, `Wall 9.jpg`,
`Water120.jpg`, `Water121.jpg`, `Water5.jpg`; per `Cynthia.bmp`, `Nanae2.bmp`, `adam1.bmp`, `armor94.jpg`, `k.bmp`,
`l3257.jpg`, `l451a.jpg`, `psurr01.bmp`, `psurr02.bmp`, `sky_009A.jpg`; col `Cllnd007.jpg`, `Cllnd018.jpg`,
`Hllnd001–006.jpg`, `Hllnd008.jpg`, `Hllnd020.jpg`, `col_dot.bmp`, `fireAnm.bmp`, `fireStrip.bmp`, `hallw972.jpg`,
`sky_0105.jpg`, `standin.bmp`, `travnew.jpg`; abu `Blue_98.jpg`, `abu_dot.bmp`, `outside.jpg`; azuchi `Top_g68.jpg`.
`*_dot.bmp` may still be loaded by the host page (e.g. the site-map position dot), so check the host before dropping it.

### 5.8 Recommended converter output

For each tour, write one `.glb`:
- one node per `.x` Mesh, named after it;
- primitives grouped by merged material, with expanded vertices (position/normal split) and fan triangulation;
- textures sniffed by content, keyed BMPs converted to PNG with alpha and `alphaMode: MASK`, JPEGs kept as they are;
- extras carrying `lod_` pairs, the world bounding box and world size, and the original texture name together with the resolved file.

### 5.9 VM confirmation checklist (geometry/textures)
- [x] Back-face culling: single-sided faces are invisible from behind (VM: from below the Acropolis terrain with the
      walk map off, terrain and floors vanish and only walls facing the eye show).
- [x] faceColor (0.5 grey on most materials) is ignored (VM, §5.4).
- [x] The LOD switch distance: 13.0 XZ units (VM, §7.6).
- [ ] GIF textures with transparency: shown opaque? (No tour GIF texture was checked for a transparent index.)
- [x] Display pixel format of the reference VM: RGB 565 (VM: caption colour 0xC0D1E3 reads back with green 0xD3, which needs 6-bit green). [ ] Which key test the engine uses on that surface for the few near-magenta texels.
- [ ] per's untextured `roooof.jpg` surface: what does it look like (white, or material colour)?
- [x] How the missing `grip.cur` behaves: it never shows (VM: no grip cursor while pressing or dragging in the view
      or on a caption, §6.9).

### 5.10 Unknowns
- How material colours, specular and power combine with `.3lt` lighting (§7).
- What `3D.CALC64BIT` does (not read by the loader).
- Whether colour-keyed textures are filtered bilinearly in hardware mode, which would give magenta fringes in the original.


---

## 6. Camera, input, walk maps (`.3wm`), collision and gravity

Status: **format fully decoded** (44/44 files parse to the last byte), **runtime behaviour specified
from static analysis**. Items marked **[VM]** need dynamic confirmation in the Windows XP reference VM.

Provenance: ACTX3DX3.OCX (Ghidra project in `analysis/decompiled/ghidra/ACTX3DX3/`). Function addresses
below are pointers into that binary. Prototype parser: `analysis/decompiled/tours-proto/walkmap.py`
(with the helper `wm_xmesh.py` that lists the `.x` meshes in file order).

### 6.1 Coordinate conventions (shared with `.x`, `.3sc` and anchors)

* World space is the `.x` space as stored: Direct3D style, **left-handed, +Y up**. Units are arbitrary
  "model units". Tours use different scales: eye heights range from 0.1 (Acropolis) to 0.5 (Beaumaris, Azuchi).
* All walk-map tests work in the **XZ plane**. Y comes only from ground planes.
* **Yaw** θ is in degrees. The look direction is `(sin θ, 0, cos θ)`, so θ = 0 looks along +Z and θ = 90 looks
  along +X. Positive turning increases θ, which is a turn to the right in a left-handed view.
  `CAMERA.POS?` reports θ as `atan2(dx, dz)` in degrees, normalised to [0, 360).
  This is the same convention as the 4th value of `CAMERA.POS=x,y,z,yaw` and of `2D._x.ANCHORPOINT=x,y,z,yaw`.
* **Pitch** φ is in degrees, and positive means looking **up** (`PITCH?` returns the elevation angle of the look
  vector). It is applied as a rotation of the look vector about the horizontal axis perpendicular to the yaw
  direction. **(VM)** Confirmed: pitch +30 shows sky, −30 the ground.
* The camera stores an eye point E and a target point T (the look direction is T − E, normally unit length).
  Turning rotates T about E around +Y. Moving translates E and T together.

### 6.2 `.3wm` file layout (little-endian)

```
u32   version            3 in every shipped file (the loader accepts 2 and 3; 1 is an old CArchive form, rejected)
f32   bbox[6]            minX, minY, minZ, maxX, maxY, maxZ   (a grid domain, see §6.3; it is not tight)
u32   ncells
ncells x CELL
```
There are no trailing bytes. The 32-byte `05918/STAIR5.3WM` is an empty walk map: bbox = (+3.4e38 ×3, −3.4e38 ×3),
ncells = 0. No script loads it.

```
CELL :=
  u32 kind                2 = visibility, 3 = collision, 4 = ground, 5 = hotspot (no other values occur)
  u16 npolys
  npolys x { u16 nverts; nverts x f32[3] xyz }     // convex polygons; every poly has 3 or 4 verts
  kind-specific tail:
    kind 4 (ground):     f32 a, b, c, d           // plane n·p + d = 0, n = (a,b,c) unit, b ≈ 1
                         (version 2 files: a 4-byte placeholder, no plane)
    kind 5 (hotspot):    u32 len1; char enter[len1];  u32 len2; char exit[len2]
                         // two script strings, not NUL-terminated (the runtime adds the NUL)
    kind 3 (collision):  f32 level;  u32 nseg;  nseg x { f32 x1, z1, x2, z2, top; u32 climbable }
                         (version 2 files: f32 level; u32 m; u32[m] (ignored); u32 nseg; segments)
    kind 2 (visibility): u16 nentries;
                         nentries x { u16 mesh_index; u8 bits[ceil(F(mesh_index)/8)] }
```
* **Polygons** carry real Y values (ground cells lie on their floor plane). The runtime only uses their XZ
  projection to decide *where* a cell applies (§6.3).
* **Visibility entry sizes depend on the `.x` file.** `mesh_index` is the 0-based index of a `Mesh` data
  object in the order the tour's `.x` file declares them (nested meshes inside `Frame`s count in document order).
  F(i) is that mesh's face count, which is the first integer of its face list (`nFaces`, polygons, not triangles).
  The runtime rejects an index ≥ the mesh count ("Walkmap illegal index to geometry", and the walk map is
  turned off). Bit *j* of the mask is `bits[j >> 3] & (1 << (j & 7))`, LSB first. A set bit means face *j*
  is potentially visible from this cell. Entries are in ascending mesh order. 3 visibility cells have no polygons.
* **Collision segments**: the 2-D walls are `(x1,z1)–(x2,z2)`. In all 7,549 shipped segments, `top` = 1.0
  and `climbable` = 0. `level` is a Y value close to the floor height of the cell's region, and is informational
  (the runtime does not use it, as far as the analysis found).
* **Hotspot strings** are ordinary `.3sc` statement lists (`;`-separated), for example
  `WALKMAP.LOAD=level2.3wm;SCRIPT.EVENT(UserLevelChange:QTSRefID=701505289);` or
  `2D._H18.VISIBLE=1;`. The exit string is often `RUN(HIDEALL);` or empty.
  **Authoring junk:** 701 of the 4,850 strings are non-empty and do not end in `;` (816 strings are empty).
  Most of these carry uninitialised buffer bytes after the authored text (e.g. `RUN(HIDEALL);\tè\n…ýýýý`).
  The original passes the whole buffer to the interpreter, so the junk becomes malformed statements that fail
  silently. **(VM)** No visible effect: cells with junk (acr cell 1055, the Roman Forum stair bands) behave as their
  clean scripts say.
  *Correction (implementation):* "cut each string after its last `;`" is not quite right, for two reasons
  found in the data. (a) Junk can contain `;` bytes and printable characters, e.g. Roman Forum `stair2.3wm`
  cell 15 `WALKMAP.LOAD(stair1.3wm););\x06è£‹?…\n;`, so the last `;` can lie inside the junk. (b) Four strings
  have no junk at all and end with a genuine statement without `;`: Roman Forum `3rdFloor.3wm` cells 42 and 48
  (`CAMERA.HEIGHT=.16000`, `=.20000`) and Azuchi `Floor3.3wm` cells 207 and 208 (`WALKMAP.LOAD=STAIR4.3WM`).
  These run in the original. **Cleaning rule:** a string made only of printable ASCII (0x20–0x7E) is kept
  whole. Otherwise the authored text is its printable prefix up to and including the prefix's last `;`, and
  the rest is junk. This finds junk in 721 strings. What remains are genuine authoring slips that fail
  silently, and a port should keep them: `)` pieces from `WALKMAP.LOAD(x.3wm););` (Roman Forum `stair2.3wm`,
  8), a stray `1` statement (acr 1, abu 4), `'2D._L8.VISIBLE=1` (a commented-out statement, Persepolis 8),
  `3D. .VISIBLE=1` (abu) and `2D._L2H.ANCHORED=1` (Roman Forum `1stFloor.3wm` cell 654).
* **What hotspot strings do** (all 44 maps, 6,397 statements after cleaning): `2D.<id>.VISIBLE=` 3,717,
  `WALKMAP.LOAD` 1,497, `3D.<id>.VISIBLE=` 676 (Abu Simbel swaps sky and backdrop meshes by location),
  `SCRIPT.EVENT(UserLevelChange…)` 418, `2D.<id>.ANCHORPOINT=` 31 and `ANCHORED=` 19 (Roman Forum moves
  captions), `3D.N_00WATER.WATER=1/0` 8 (Acropolis cells 1145–1148 turn the water ripple on when entered and
  off when left, see §7.4.6), `CAMERA.HEIGHT=` 4 (Roman Forum), `LIGHTS.OFF` / `LIGHTS.ON` and
  `SCENE.TRYHARDWAREFULLSCREEN` (Persepolis `stair1.3wm` cell 1255), and `SCENE.ALARM.LITEANIM.ACTIVATE` 3 (Abu
  Simbel cells 519–521; no alarm `LITEANIM` is ever created, so it does nothing). Plus 21 statements with
  no known subsystem (the `)`, `1` and `'2D…` slips above).

Worked example: `04257/acr.3wm`
```
03 00 00 00 | 3c 38 25 c1 d4 b7 09 c0 e4 ea 90 c1 67 4b ba 40 8b 76 f2 3f 98 3b 4c 41 | b1 06 00 00
version 3     bbox = (-10.3262, -2.1518, -18.1147) .. (5.8217, 1.8942, 12.7645)          ncells 1713
cell 0:   04 00 00 00 01 00 04 00 …  kind 4 (ground), 1 poly, 4 verts:
          (1.2058,0.381,1.1629) (1.2077,0.381,0.7304) (0.8832,0.381,0.7373) (0.8831,0.381,1.1629)
          plane (1.5e-9, 1.0, 6.9e-8, -0.38102)  -> floor at y = 0.381
cell 1055 (first hotspot): enter = "2D._L21.VISIBLE=1;2D._L4.VISIBLE=1;2D._H1.VISIBLE=1;…;2D._L1.VISIBLE=1;"+junk
          exit = "RUN(HIDEALL);"+junk
cell 1273 (first collision): 43 polys, level 0.2100, 67 segments, the first being (3.7684,4.5519)-(4.2424,9.3670), top 1.0, flag 0
cell 1288 (first visibility): 2 polys, 0 entries (draws nothing from the mesh list; see §6.6)
```
`acr.x` has 1,542 meshes and 20,059 faces.

Self-check counts (cells per kind; every file is consumed exactly, 0 unexplained bytes):

| File | Bytes | Cells | Polys | ground | hotspot | collision | visibility |
|---|---|---|---|---|---|---|---|
| 04257/acr.3wm | 955,538 | 1713 | 3465 | 1055 | 237 | 16 | 405 |
| 05285/1stFloor.3wm | 754,320 | 673 | 1090 | 372 | 109 | 3 | 189 |
| 05285/2ndFloor.3wm | 141,679 | 225 | 377 | 143 | 8 | 3 | 71 |
| 05285/3rdFloor.3wm | 70,930 | 49 | 77 | 23 | 7 | 1 | 18 |
| 05285/stair1.3wm | 9,943 | 35 | 141 | 27 | 6 | 1 | 1 |
| 05285/stair2.3wm | 5,464 | 21 | 45 | 13 | 6 | 1 | 1 |
| 05285/stair3.3wm | 13,742 | 41 | 119 | 30 | 6 | 1 | 4 |
| 05285/stair4.3wm | 5,286 | 25 | 61 | 17 | 6 | 1 | 1 |
| 05916/level1.3wm | 256,273 | 856 | 1652 | 714 | 68 | 2 | 72 |
| 05916/level2.3wm | 363,525 | 1455 | 2820 | 1244 | 110 | 2 | 99 |
| 05916/level3.3wm | 343,189 | 995 | 1895 | 828 | 71 | 2 | 94 |
| 05916/level4.3wm | 81,540 | 362 | 689 | 302 | 14 | 1 | 45 |
| 05916/level5.3wm | 84,296 | 387 | 784 | 368 | 8 | 1 | 10 |
| 05916/stair2.3wm | 12,267 | 59 | 213 | 41 | 13 | 1 | 4 |
| 05916/stair3.3wm | 21,485 | 92 | 306 | 63 | 24 | 1 | 4 |
| 05916/stair4.3wm | 11,168 | 52 | 174 | 35 | 12 | 1 | 4 |
| 05916/stair5.3wm | 10,673 | 50 | 173 | 35 | 12 | 1 | 2 |
| 05917/per.3wm | 925,691 | 1254 | 1838 | 818 | 190 | 1 | 245 |
| 05917/stair1.3wm | 928,096 | 1257 | 1842 | 819 | 191 | 1 | 246 |
| 05918/STAIR5.3WM | 32 | 0 | 0 | 0 | 0 | 0 | 0 |
| 05918/level1.3wm | 713,151 | 1117 | 1571 | 715 | 74 | 1 | 327 |
| 05918/level2.3wm | 618,323 | 849 | 1128 | 454 | 187 | 1 | 207 |
| 05918/level3.3wm | 493,026 | 491 | 683 | 277 | 89 | 1 | 124 |
| 05918/level3b.3wm | 58,987 | 189 | 447 | 134 | 25 | 1 | 29 |
| 05918/level4.3wm | 232,046 | 426 | 777 | 360 | 12 | 1 | 53 |
| 05918/stadium2.3wm | 417,740 | 614 | 1030 | 460 | 93 | 1 | 60 |
| 05918/stadium3.3wm | 887,606 | 1187 | 2013 | 905 | 148 | 1 | 133 |
| 05918/stair2.3wm | 159,548 | 585 | 1059 | 485 | 73 | 1 | 26 |
| 05918/stair3.3wm | 108,269 | 331 | 584 | 260 | 48 | 1 | 22 |
| 05918/stair4.3wm | 26,994 | 84 | 150 | 55 | 24 | 1 | 4 |
| 05919/abu.3wm | 140,035 | 618 | 861 | 267 | 319 | 7 | 25 |
| 05920/Floor1.3wm | 144,819 | 166 | 418 | 86 | 47 | 2 | 31 |
| 05920/Floor2.3wm | 124,300 | 217 | 475 | 128 | 23 | 1 | 65 |
| 05920/Floor3.3wm | 136,081 | 242 | 526 | 153 | 33 | 5 | 51 |
| 05920/Floor4.3wm | 61,904 | 112 | 277 | 74 | 23 | 3 | 12 |
| 05920/Floor5.3wm | 27,474 | 75 | 258 | 46 | 14 | 2 | 13 |
| 05920/Floor6.3wm | 29,705 | 107 | 227 | 61 | 32 | 2 | 12 |
| 05920/Floor7.3wm | 13,056 | 38 | 87 | 19 | 12 | 2 | 5 |
| 05920/Stair2.3wm | 2,711 | 18 | 18 | 10 | 6 | 1 | 1 |
| 05920/Stair3.3wm | 6,620 | 49 | 53 | 33 | 12 | 2 | 2 |
| 05920/Stair4.3wm | 17,145 | 91 | 145 | 72 | 12 | 2 | 5 |
| 05920/Stair5.3wm | 2,854 | 17 | 17 | 9 | 6 | 1 | 1 |
| 05920/Stair6.3wm | 3,592 | 22 | 22 | 13 | 7 | 1 | 1 |
| 05920/Stair7.3wm | 5,108 | 39 | 40 | 29 | 8 | 1 | 1 |

Totals: 44 files; 17,285 cells (ground 12,052, visibility 2,725, hotspot 2,425, collision 83).
Mesh counts used for the visibility masks: acr 1,542; rom 3,689; bea 2,321; per 1,344; col 2,843; abu 531; azuchi 7,315.

### 6.3 Spatial lookup: which cell the camera is "over"

Each loaded walk map rasterises every polygon of every cell into a **4096 × 4096 grid** over the file's
bbox in X and Z (FUN_10054a80 rasterises; the grid has 4,096 rows of span lists):
```
gx = trunc((p.x - minX) / (maxX - minX) * 4095)      gz = trunc((p.z - minZ) / (maxZ - minZ) * 4095)
```
(If either extent is ≤ 0.001, or gx/gz falls outside 0..4095, the point is "nowhere". FUN_10024ff0.)
Each row gz holds spans `[gx_lo, gx_hi] → cell`. These come from a scan-line fill of the polygon
with vertices quantised to the grid. After loading, consecutive spans of the same kind in one row that are separated
by a gap of 1–3 grid units are stretched to close the gap (FUN_10055840), so seams between neighbouring cells
do not leak.
**Query** "cell of kind K at P" (FUN_10024f50): it scans row gz in insertion order and returns the first span
of kind K whose range contains gx. Insertion order is file order (cell, then polygon). Kinds are looked up
independently, so a point is usually over one ground cell, one visibility cell, one collision-region cell and
at most one hotspot cell at the same time.

Reimplementation: a point-in-convex-polygon test in XZ, taking the first matching cell in file order,
is equivalent except within one grid unit (bbox/4095: about 0.004 units for Acropolis, 0.06 for Azuchi)
of polygon edges. For exact parity, reproduce the grid quantisation and the 1–3-unit gap closing.

### 6.4 Several walk maps: floors, stairs and switching

* `WALKMAP.LOAD(name)` / `WALKMAP.LOAD=name` keeps a **cache keyed by file name (case-insensitive)**
  (FUN_1004d6a0). If the name is already cached, it only switches the *active* walk map to it.
  Otherwise it loads and rasterises the file and makes it active. **Exactly one walk map is active at a time.**
  Ground, collision, visibility and hotspot tests only consult the active map.
* `load.3sc` preloads every floor and stair map. The **last** one loaded is the starting map
  (Acropolis `acr`, Forum `1stFloor`, Beaumaris `level1`, Persepolis `per`, Colosseum `level1`, Abu Simbel `abu`,
  Azuchi `Floor1`).
* **Floor changes are driven by hotspot cells.** A stair has bands of hotspot cells at each end, three
  strips in a row. The strip nearest the floor runs `WALKMAP.LOAD=<floor>.3wm;SCRIPT.EVENT(UserLevelChange:QTSRefID=<id>);`
  on enter, the middle strip has enter = floor map and exit = stair map, and the strip nearest the stair runs
  `WALKMAP.LOAD=<stair>.3wm` (Beaumaris `stair2.3wm` cells 28–37 and 56–58; `level2.3wm` cells 1321–1415).
  The floor map and the stair map overlap geometrically in these bands, so switching is seamless.
  The `UserLevelChange` event tells the host which floor the user is on (used by the site map).

### 6.5 Hotspot cells (enter/exit macros)

After every camera move (end of the step move, `MOVETO`, POS changes), FUN_10015bb0 runs:
1. If the active walk map differs from the one seen last time, run the **exit** macro of the old map's
   current hotspot cell (if any) and clear "current" for both maps.
2. Find the hotspot cell at the eye's XZ.
   * The same cell as current: do nothing.
   * A different cell: run the current cell's **exit** macro (if any), make the new cell current, then run its
     **enter** macro.
   * No cell: run the current cell's exit macro and clear current.
3. Loading a walk map initialises "current" to the hotspot cell under the camera **without** running its enter macro.

**(VM)** When an enter macro switches the walk map, the old map's exit macro does **not** switch back on the next step.
Crossing the Roman Forum stair-1 band (three strips; the middle strip's enter loads the other map and its exit reloads
its own) switches the active map exactly once in each direction, in the middle strip, and it stays switched. A port
treats a switch to a cached map like a first load: the new map's current cell is the one under the eye, without
running its enter macro, and the change of map is taken as seen.

### 6.6 Visibility cells (precomputed potentially-visible set)

In the render pass (FUN_100659e0), when the walk map is on and the eye is over a visibility cell:
* **Only faces whose bit is set are drawn.** For each listed mesh that is itself visible (`3D.<mesh>.VISIBLE`),
  only its flagged faces are drawn. Meshes not listed are not drawn. Then the always-drawn object list is drawn
  (that list is not part of the walk map; it presumably holds sprites, clouds and sky; see §7.4 and §7.6).
* On entering a new visibility cell, the face list is rebuilt (FUN_10051580 releases the old one, FUN_10051870 builds the new one).
* If the eye is over **no** visibility cell, or the walk map is off, every visible mesh is drawn (with the LOD
  swap by `3D.LODDISTANCE`, see §7.6).

Visible fractions are small: on average 1–22% of a tour's faces per cell, and 0 for some cells (for example
cell 1288 of `acr.3wm`). A modern renderer can ignore the PVS for performance, but **parity requires honouring
it**: faces that are culled in a cell (for example geometry behind the camera that the authors excluded) would
otherwise become visible. Suggested target: per-cell lists of (mesh, face ranges), or per-mesh draw ranges after
regrouping triangles by face.

**(VM)** At the six Acropolis stops the view with the PVS and after `WALKMAP.DELETE.VISIBILITY` is the same, except
for **LOD**: the lists name base or `lod_` meshes per cell (the authoring tool chose), and over a visibility cell
exactly the listed faces draw, `lod_` meshes included, with **no distance swap**. At the Erechtheum stop the far
Parthenon columns are `lod_` meshes with the PVS and full columns without it, although they are 5.2 units away. The
§7.6 distance swap applies only when the eye is over no visibility cell. **Sprites** in no PVS list are drawn
(Torch_19 from cell 1508, whose list lacks it), so the always-drawn list holds at least the sprites.

### 6.7 Collision (walls) and walkable region

Movement happens only in XZ. The per-step pipeline is in §6.9. Collision is resolved by FUN_10022000 with a
"walker" state object:
1. **Gather** (FUN_10023070): walk the grid with a Bresenham line from the old eye position to
   `new + dir·(R + |step|)` (dir = unit move direction, R = `CAMERA.COLLISIONRADIUS`) and collect every collision
   cell met. All their wall segments become the candidate set. Each segment is inflated by R into two parallel
   offset lines, one on each side.
   If the line crosses grid points with no collision cell, a flag is raised. **Collision-cell polygons cover the
   walkable area**: 85–100% of ground and hotspot cell centroids lie inside a collision polygon in every map
   (Acropolis 905/1055 is the lowest). **(VM)** The flag does not stop a move that leaves the covered region (see
   below).
2. **Filter** (FUN_10025520): keep segments whose distance from the path is less than |step| + R.
3. **Sweep and slide** (FUN_100239b0, FUN_100222e0, FUN_100227a0): find the earliest intersection of the move
   segment with any inflated wall line. On a hit, the remaining motion is projected onto the wall direction: the
   eye slides along the wall by the tangential component, choosing the wall direction that is closer in angle to
   the attempted move. This is repeated once more against the second-nearest wall (a corner). The target point
   moves with the eye, so the view direction is unchanged.
4. **Depenetrate** (FUN_100268e0, FUN_100267e0): any segment closer than R pushes the eye out perpendicular to
   the segment. Any segment *endpoint* within R pushes it out radially to distance R (rounded corners).
5. Climbable walls (`climbable` ≠ 0, which never occurs in shipped data): if feet height (eyeY − HEIGHT) is
   below `top`, the slide is reduced to 10%, the eye is raised by `CAMERA.CLIMBRATE`, and the fall speed is set
   to −GRAVITY. A camera standing inside the R-inflated rectangle of a climbable wall uses `top` as its ground
   height. **Because no shipped segment is climbable, `CLIMBRATE` has no effect in Encarta.** Steps are climbed
   through sloped or stepped *ground* planes (§6.8).

Simplified, parity-oriented model for the web runtime: a circle of radius R moving in XZ against the union of
all wall segments of the active map; slide along walls; push out of segments and endpoints. Moves that leave the
collision-cell polygons are not rejected (VM).
**(VM)** Walking at yaw 93.4 into the Propylaea side wall, X stopped at the wall (constant to 6 decimals) while Z
kept changing at 0.0124 units/s ≈ 0.2·sin 3.4°: the move is **projected onto the wall and the tangential part
kept**, with no bounce or jitter; at a corner the camera stopped completely. **(VM)** Open edges: walking out of
the collision coverage where no wall is near (Roman Forum 1stFloor, cell 9 ends at z 7.744, nearest wall 0.72 away)
goes on at full speed. Leaving the coverage does not block a move.

### 6.8 Ground following and gravity

After the move, FUN_10026320 runs **every tick, even when not moving**:
* It finds the ground cell under the eye. If there is none, the height is left unchanged and the eye keeps its Y.
* The ground point is the projection of the eye onto the cell plane along the plane normal
  (`g = E − (n·E + d)·n`). For the near-horizontal planes in the data this equals solving the plane for Y.
  `targetY = g.y + HEIGHT`.
* **Gravity mode** (walker state +0x54, set in FUN_10022000; **(VM, debugger)** it is 1 in Encarta's own control
  and in our host on the software device, so this is the mode every user saw):
  * If `E.y ≥ targetY`: `vfall += GRAVITY; E.y −= vfall; T.y −= vfall` (per tick, not time-scaled).
  * If `E.y < targetY` (also after the step above): `vfall = 0`, and the eye and target are raised to `targetY`
    (instant step-up).
* **Snap mode** (walker +0x54 = 0; not seen in the VM): `E.y = targetY` and T is shifted by the same amount every
  tick, so stepping down is instant too.
* Each fall and each landing writes E.y and T.y as float32 (the landing adds `targetY − E.y` to both). The rounding
  differs between the two values, so T.y − E.y, and with it the pitch derived from the look vector, drifts a little
  every tick even when standing still (§6.9, pitch keys).
* Encarta's Colosseum script has a `LARGE` macro that calls `SCENE.TRYHARDWARE`.
  **(VM)** `SCENE.TRYHARDWARE` and `Try3DHardware` return `Failed` in the VM (no D3D device), so Encarta runs the
  software device, **in gravity mode** (2026-10-15, cdb in the guest: walker +0x54 = 1 in ENCARTA.EXE's copy of the
  control at the Acropolis start, and a write breakpoint on the eye's Y shows every tick falling by
  `vfall = GRAVITY` = 0.003 and landing back in the same tick). An eye raised 0.6 units with `CAMERA.POS` falls
  0.003, 0.006, … 0.057 units on successive ticks and lands on tick 20 (about 0.4 s); the earlier reading "back on
  the ground at the next sample" came from samples too far apart. Walking down a stair therefore drops with
  `vfall` growing per tick, though steps shallower than a few ticks' fall look instant. The unused function
  FUN_10024940 implements the same ground rule without the mode switch.

### 6.9 Input, camera step and timing

**Tick.** The control calls `SetTimer(hwnd, 0, 1 ms)`. Every `WM_TIMER` runs the tick (FUN_1000cc80 →
FUN_1000d3c0), which reads the DirectInput keyboard, runs alarms and the step (FUN_10012280), and redraws.
Windows XP limits timers to about 10 ms, and in practice the tick rate is bounded by the 15.6 ms system tick and
the render time. **(VM)** About 64 ticks/s idle and 47 while walking (§2.3).
`WM_LBUTTONDOWN`/`UP` also run one step immediately.

**Time factor.** The time since the previous step is measured with QueryPerformanceCounter (or timeGetTime as
a fallback): dt in seconds.
`k = clamp(100 · dt, 1, 5)`. (The scale 100 is the default of a per-camera rate field, which no script changes.
If dt > 0.15 s the factor would be 15 and is clamped to 5.) So **speeds are "per 1/100 s"**, with at least one
nominal step per tick and at most five.

**Inputs** (walk/POV mode, the only mode the tours use):

| Input | Effect per tick |
|---|---|
| ↑ / ↓ (DIK_UP 0xC8 / DIK_DOWN 0xD0) | move input m = +1 / −1 |
| ← / → (DIK_LEFT 0xCB / DIK_RIGHT 0xCD) | turn input t = −1 / +1 |
| Page Up / Page Down (0xC9 / 0xD1) | pitch +PITCHSPEED / −PITCHSPEED degrees, **not time-scaled**, refused if \|pitch\| would reach 45° (VM: from −40 at 6°/tick: −34, −28, …, 44, then stops; from 44.998962 down it stops at −39.001038) |
| Home (0xC7) | executes `CAMERA.PITCH=0` (level the view) |
| Left-drag in the 3D view | m and t from the mouse offset (below); keys override if also held |
| (two further flags for vertical "fly" ±0.01 per tick, ×20 with a modifier, exist but no key sets them in this build) | — |

* Mouse vector (FUN_10012690): with half sizes (w/2, h/2), offsets `dx = x − w/2`, `dy = (h − y) − h/2` (up is
  positive) and dead zones `zx = 0.15·w/2`, `zy = 0.08·h/2`: `vx = sign(dx)·(|dx| − zx)/(w/2 − zx)` (0 inside
  the dead zone, clamped to ±1), and likewise vy. Response curve: `t = vx·|vx|^0.5`, `m = vy·|vy|^0.5`.
* The step (FUN_10011c50) first turns, then moves:
  `turn = 2·k·SPEED.TURN·t` degrees about +Y; `move = k·SPEED.FLY·m` model units along the horizontal look
  direction (pitch never tilts the motion). The move then goes through collision (§6.7), ground following (§6.8)
  and the hotspot check (§6.5).
  No acceleration or inertia: velocity is applied immediately and stops when the input is released.
* Worked numbers (Acropolis, `SPEED.FLY=.002`, `SPEED.TURN=.5`, assuming ~64 ticks/s, so k ≈ 1.56):
  walking 0.2 units/s (HEIGHT is 0.1 units, so this is about 2 eye-heights per second); turning
  100°/s; pitch 6° per tick (≈ 380°/s, reaching ±45° almost at once).
  **(VM)** Confirmed. **Walking is 100 × `SPEED.FLY` units/s and turning 200 × `SPEED.TURN` °/s, independent of the
  tick rate**: Acropolis 0.203 units/s and 101–103 °/s at ~47 ticks/s, Roman Forum (`.0035`) 0.347 units/s; the
  right arrow increases yaw and turning does not move the eye. The `k ≥ 1` floor never applied (dt was always
  > 10 ms). Pitch steps exactly `PITCHSPEED` per tick. Home sets pitch 0.
  **Pitch arithmetic (VM, debugger, 2026-10-15).** The camera keeps two pitches. The *stored* pitch (camera +0x264)
  is what `PITCH?` reports. The *look vector* is the eye (+0x114) and the target (+0x120), each three float32, with
  the target one unit from the eye. A pitch key step (FUN_10010f80, argument ±PITCHSPEED):
  1. derives the current pitch from the look vector: asin of the normalised eye→target rise, in degrees (FUN_10016840);
  2. refuses the step unless |derived + step| < the limit (+0x260, 45.0);
  3. stores derived + step as the new stored pitch, and rotates the normalised look vector by step × 0.017453292
     rad about the horizontal axis perpendicular to it; target = eye + rotated unit vector (float32).
  `CAMERA.PITCH=` (clamped to ±44.9) stores its value and levels the look vector, then rotates it by the full angle.
  The two pitches part because ground following moves eye and target in float32 every tick (§6.8). At the Acropolis
  start (eye Y −0.385825, GRAVITY 0.003) the derived pitch falls about 4.5·10⁻⁷° a tick: logged by a breakpoint on
  FUN_10010f80 after about a minute idle, the stored pitch was 21.000036 and the derived one 20.999359, and four
  6° steps were all accepted, the fourth at derived 38.999359 + 6 = 44.999359. The earlier log's 44.998962 is the
  same thing after a longer idle (about 2,200 ticks). A model of just the per-tick fall and landing in float32 (T.y
  and E.y each rounded after subtracting `vfall`, then after adding `targetY − E.y` computed in higher precision)
  reproduced 1,133 consecutive logged ticks bit for bit, and the drift rate (−1.5·10⁻⁵° over 31 ticks) to within
  10 %. So a step that lands on 45 in real arithmetic is accepted when the look vector has drifted below the stored
  value, which at the Acropolis start is always after the first tick.
* While the mouse is pressed on an active 2D item, no camera step happens (it is a HUD interaction).

**Cursors** (when `SCENE.CURSORS=TRUE`): over a 2D hotspot item the cursor is index 5 (`vthot360.cur`).
Otherwise the zone of the pointer relative to the view centre (FUN_10016ca0) picks the cursor:
inside |dx| < 30 px and |dy| < 60 px → 0 (`sv.cur`). Outside, if |dy| ≤ |dx|: right → 1 (`svright`), left → 3
(`svleft`); otherwise up → 2 (`svup`), down → 4 (`svdown`). **(VM)** Exactly so: dx 29 is the centre, 30 is right;
dy 59 is the centre, 60 is down; (100,100) is right and (99,100) down. Index 6 (`grip.cur`) never shows: pressing and
dragging in the view keeps the zone cursor, and a press on a caption keeps the hotspot cursor even when dragged off it. Each cursor shows the direction a drag from that point would move the camera.

### 6.10 Script-visible camera/scene/walkmap properties and defaults

The defaults are the values set at control initialisation (FUN_10012c60 and FUN_10010110) and apply when a script
does not set the property:

| Property | Meaning | Default |
|---|---|---|
| `CAMERA.SPEED.FLY` | move speed, units per 1/100 s | 0.03 |
| `CAMERA.SPEED.TURN` | turn speed: degrees per 1/100 s is 2× this value | 1.3 |
| `CAMERA.PITCHSPEED` | degrees per tick for PgUp/PgDn | 1.0 |
| (pitch limit) | \|pitch\| < 45°; `PITCH=` clamps to ±44.9 | 45 |
| `CAMERA.HEIGHT` | eye height above the ground plane | 0.05 (`HOME` resets HEIGHT to this) |
| `CAMERA.COLLISIONRADIUS` | wall clearance R | 0.05 |
| `CAMERA.CLIMBRATE` | rise per tick on climbable walls (none ship) | 0.01 |
| `SCENE.GRAVITY` | fall acceleration per tick² (gravity mode, which Encarta runs on the software device too, §6.8) | 0.01 |
| `CAMERA.FOV` | **horizontal** field of view in degrees (VM: a 5° yaw step moves the image centre 32 px, f = 366 px, so FOV 80 spans the 616-px width; vertical 59.7° at 420 px) | not determined; every tour sets 80 |
| `CAMERA.NEAR` | near clip | set by every tour (0.01–0.09) |
| `CAMERA.FAR` | far clip. **Negative means automatic: 2 × the largest extent of the scene's bounding box** (FUN_10065570). Every tour except Azuchi sets −1 | — |
| `CAMERA.POS=x,y,z,yaw` | sets the eye, keeps the look vector, then sets yaw and reapplies the current pitch. Y is overridden on the next tick by ground following if over ground | eye (0, 0.2, −3), looking +Z |
| `CAMERA.PITCH=deg` | sets pitch absolutely | 0 |
| `CAMERA.MOVE=dx,dy,dz` | relative move, no collision | — |
| `CAMERA.MOVETO=x,y,z,yaw` | absolute set (as POS), then hotspot check. **(VM) quirk:** a POS/MOVETO to the X and Z the eye already has makes `POS?`/`PITCH?` report `-1.#IND00` (NaN) until `CAMERA.HOMEPOV` (which also resets HEIGHT to 0.05): the look vector degenerates. This is probably why the multi-floor stop commands first move far away. A port need not reproduce the NaN | — |
| `CAMERA.HOME` / `HOMEPOV` / `HOMEOMNI` | restore the home pose (POV home = the defaults above) | — |
| `CAMERA.POV` / `OMNI`, `VIEWMODE?` | walk mode (0) vs orbit mode (1); tours never use orbit mode | 0 |
| `SCENE.HALFRESDRIVING=1` | render at half resolution while a movement key or drag is active | 0 |
| `SCENE.SHOWCOLLISION=1` | debug overlay of the gathered collision data | 0 |
| `WALKMAP.ON` / `OFF` / `ON?` / `OFF?` | enable all walk-map behaviour (collision, ground, hotspots, PVS) | off until `WALKMAP.ON` |
| `WALKMAP.TOGGLE` | negates the flag, so 1 becomes −1, which still counts as on. **Toggle cannot switch it off** (a quirk) | — |
| `WALKMAP.CELL?` | "gx,gz" grid coordinates of the eye | — |
| `WALKMAP.VISIBILITY?` / `GROUND?` / `COLLISION?` / `HOTSPOT?` | name of the cell of that kind under the eye (names are always empty in shipped files) | — |
| `WALKMAP.[n].OVER?` | whether the eye is over cell n (by index, or by name in brackets) | — |
| `WALKMAP.DELETE.VISIBILITY` / `.GROUND` / `.COLLISION` / `.HOTSPOT` / `.VISIBILITY.DATA` | drop cells of a kind, or only the PVS masks | — |

With **walk map off** there is no collision, no ground following, no hotspots and no PVS. Movement stays
horizontal, and Y never changes (there is no fly up/down binding).

Note: the host (ENCARTA.EXE) sends `SCENE.HALFREZDRIVING=1/0` (spelled with Z). The control's keyword is
`HALFRESDRIVING`, so **the host's command does not match and has no effect**. **(VM)** The reply is `Failed`.

### 6.11 Per-tour values (from each `load.3sc`)

| Tour | HEIGHT | SPEED.FLY | SPEED.TURN | PITCHSPEED | COLL.RADIUS | CLIMBRATE | GRAVITY | NEAR (final) | FAR | FOV | Start POS (x, y, z, yaw) | Start PITCH | Walk maps, load order (last = start) |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| 04257 Acropolis | .1 | .002 | .5 | 6 | .015 | .0045 | .003 | .01 | −1 | 80 | −2.260526, −0.385825, −7.827163, 3.398490 | 21.000036 | acr |
| 05285 Roman Forum | .16 | .0035 | .6 | 7 | .022 | .0045 | .008 | .02 | −1 | 80 | 15.015445, 0.184222, −0.180281, 318.391469 | 22.000025 | 3rdFloor, stair4, 2ndFloor, stair3, stair2, stair1, 1stFloor |
| 05916 Beaumaris | .5 | .005 | .6 | 7 | .010 | .0045 | .100 | .01 | −1 | 80 | −5.405047, 0.818306, −24.242380, 5.675681 | 10.999681 | level5, stair5, level4, stair4, level3, stair3, level2, stair2, level1 |
| 05917 Persepolis | .47 | .01 | .6 | 6 | .02 | .03 | .5 | .02 | −1 | 80 | 1.570536, 0.526654, 0.441017, 78.446045 | 17.999920 | stair1, per |
| 05918 Colosseum | .145 | .004 | .5 | 7 | .010 | .0045 | .038 | .01 (.09 before the walk maps load) | −1 | 80 | 13.087535, 0.175660, 2.531180, 271.003505 | 22.998180 | level4, stair4, stadium3, level3, level3b, stair3, level2, stadium2, stair2, level1 |
| 05919 Abu Simbel | .37 | .008 | .6 | 6 | .03 | .03 | .5 | .01 | −1 | 80 | −0.056055, −0.230000, −12.275214, 1.189002 | 20.999550 | abu |
| 05920 Azuchi | .5 | .005 | .6 | 7 | .010 | .0045 | .048 | .01 (.02 first) | (default) | 80 | −1.273148, −3.578547, −11.928087, 355.251404 | 19.012995 | Stair7…Stair2, Floor7…Floor1 |

### 6.12 Suggested modern target

Walk maps → JSON (or compact binary) per file: bbox; ground cells (XZ polygons plus a plane); hotspot cells (XZ
polygons plus cleaned enter/exit statement lists); collision cells (XZ region polygons plus 2-D wall segments);
visibility cells (XZ polygons plus mesh/face lists remapped to the converter's glTF primitive/triangle ranges).
The three.js runtime should then implement §6.3–§6.9 as written: tick-based input with the k-factor, collision as
in §6.7, ground following in gravity mode as in §6.8 (what the VM runs), hotspot enter/exit
as in §6.5, PVS as in §6.6.

### 6.13 VM confirmation checklist (this section)

- [x] Tick rate of the control inside Encarta (ticks/s while walking); verify `k = clamp(100·dt,1,5)` by timing a walk over a known distance (`CAMERA.POS?` before/after). (VM: ~64 idle, ~47 walking; speeds time-scaled.)
- [x] Walk speed, turn speed and pitch rate for one tour against the numbers in §6.9.
- [x] Pitch sign (positive = up) and the 45° limit; Home levels the view.
- [x] Ground mode in Encarta: the software device (no D3D device in the VM) runs gravity mode, walker +0x54 = 1 (debugger, §6.8). The maximised view remains unobserved.
- [x] Wall sliding and corners. [x] Open edges: leaving the collision coverage does not block the move (§6.7).
- [x] Hotspot enter/exit order when a macro switches walk maps: one switch per crossing, no switch back (§6.5).
- [x] PVS: the same with `WALKMAP.DELETE.VISIBILITY` except the per-cell LOD choice; unlisted sprites draw (§6.6).
- [x] Trailing junk in hotspot strings causes no visible effect.
- [x] `SCENE.HALFREZDRIVING` from the host returns "Failed". [ ] Whether half-res driving is ever active.
- [x] Cursor zones (30×60 px centre box, diagonals); `grip.cur` never appears (§6.9). [ ] Right-button behaviour (WM_RBUTTONDOWN/UP are handled, effect not analysed).
- [x] FOV is horizontal (VM). [ ] The default FOV, and whether NEAR/FAR reach the projection.

### 6.14 Remaining unknowns

* The exact edge rule of the scan-line rasteriser (which grid cells on a polygon boundary are included). Only relevant within one grid unit.
  One VM data point: the Colosseum stop "Seating Areas" stands on a thin slanted seating step whose left edge crosses
  the eye's grid row 0.44 units left of the eye's (truncated) grid column; the original puts the eye on that step
  (read-back Y = its plane + HEIGHT), whereas "the grid point lies inside the quantised polygon" puts it on the
  neighbouring step 0.044 units lower. The rule "a column belongs to a polygon when the polygon's span on that row
  overlaps the column" reproduces it and all 46 other Tour Stop read-backs.
* The second gravity-mode flag (0x1011c958): the decompilation shows no direct writes to it. It is probably part of the hardware/fullscreen device state.
* The meaning of the collision cell's `level` float and of the ignored u32 list in version-2 collision cells (no v2 files ship).
* Orbit (OMNI) mode details: never used by the tours, so not specified.
* Who sets the two vertical-fly flags (no keyboard binding was found).


---

## 7. Lights (`.3lt`), point clouds (`.3cl`), `.3vp` and per-object 3D properties

Scope: `.3lt` (light files), the `LIGHTS.*` commands and flicker, `.3cl` point clouds (v3 and v2) and
`CLOUD*` placement, `.3vp`, the per-mesh `3D.<id>.*` properties (SPRITE, NOILLUMINATION, WATER,
FILMSTRIP, APPLYTEXTURE, PLAYAVI, STRING, VISIBLE, CLOUD…), the scene-wide `3D.*` switches
(ON/OFF, CALC64BIT, LODDISTANCE, TOGGLEHIDDEN, UNHIDEALL, DELETE, LOADVP…), the `lod_` naming rule,
and how `SCENE.AMBIENT`, `SCENE.SPECULAR` and fog feed the lighting.

Provenance: static reading of ACTX3DX3.OCX (addresses given as pointers only), confirmed against the
data by the prototype parsers in `analysis/decompiled/tours-proto/` (`lights.py`, `clouds.py`,
`vp.py`, `crosscheck.py`, `placeholders.py`). Items marked **[VM]** need dynamic confirmation in the
Windows XP reference VM.

Conventions: all integers little-endian. "Object" means one `Mesh` of the tour's `.x` file. Objects
are numbered 0…N−1 in the order the `Mesh` templates occur in the `.x` file. "Face" means one polygon
of that mesh's face list (3 or 4 vertices in every shipped file), numbered in file order. A
D3DCOLOR is a u32 `0xAARRGGBB`, so its bytes on disk are B, G, R, A.

---

### 7.1 Name matching (applies to `3D.<id>` and `LIGHTS.<id>`)

The command dispatcher upper-cases the statement text before it reaches these handlers (up to the
first `^`). The handlers then compare as follows (3D: FUN_100316c0; LIGHTS: FUN_10030470).

* **Effectively case-insensitive.** Each stored name (the mesh name as written in the `.x` file, or
  the light name from the `.3lt` file) is copied, upper-cased, and compared for **exact byte
  equality** with the id from the statement. The comparison is a plain multibyte strcmp with no
  case folding of its own, so the result is case-insensitive only because both sides are upper-cased.
* **First match only.** The objects (or lights) are scanned in load order and the scan stops at the
  first equal name. When two meshes differ only in case, a statement affects only the earlier one in
  the `.x` file. Collision groups found by `crosscheck.py`: rom 26, per 15, bea 1, col 1, others 0.
  No tour has two meshes with byte-identical names.
* **No wildcards and no prefix matching.** The id is the text between the first and second `.`.
  Bracket form: `3D.[n].PROP` uses the decimal number *n* as a zero-based object index instead of a
  name (no shipped script uses it). The LIGHTS handler accepts the same bracket form for a light index.
* **Unknown id:** the statement fails and returns `Failed`, with no other effect. The shipped scripts
  refer to many ids that do not exist (see §7.8), and these are silently ignored.
* Internal lookups that do *not* go through a statement compare names case-sensitively. These are the
  `lod_` pairing (§7.6) and a cloud finding its own host object again (§7.3.5). The cloud stores the
  host's exact name, so in practice it always resolves to its original host.

---

### 7.2 `.3lt`: light file

Loaded by `LIGHTS.LOAD(<file>)`, `LIGHTS.LOAD=<file>` or `LIGHTS.LOAD(<file>` (FUN_1002e960). Loading
first deletes all existing lights and all "vertex paint" records (§7.2.4). Versions 1, 2 and 3 are
accepted. Any other version fails the load. **All 7 shipped files are version 3.**

#### 7.2.1 Layout

```
u32 version              1, 2 or 3
u32 nlights
nlights × LightRecord
if version == 3:
    u32 npaint
    npaint × PaintRecord
(end of file — all 7 files are consumed exactly)
```

LightRecord (280 bytes fixed part, then two lists):

| Off | Type | Field | Meaning |
|---|---|---|---|
| 0 | f32[3] | pos | light position (x, y, z), world units |
| 12 | f32 | range2 | **squared** range. A vertex is lit only if \|L−V\|² < range2 (see §7.2.3). Shipped values run from 0.0067 to 10000 (e.g. the "SUN" lights use 3887…10000, which is effectively unbounded) |
| 16 | u32 | colour | D3DCOLOR. Alpha is always 0xFF in the shipped files |
| 20 | char[256] | name | NUL-terminated name. The bytes after the NUL are uninitialised authoring-tool memory (non-zero in 100% of records). Ignore them |
| 276 | u32 | count | number of entries in each list |
| 280 | u32[count] | faceRef | `(objectIndex << 16) \| faceIndex`. 0xFFFF in either half means "skip this entry" (none in the shipped data) |
| … | u32[count] | vertexMask | **version ≥ 2 only.** Bit *k* set means vertex slot *k* (0-based position within that face's vertex list) receives this light. Only bits 0–31 are honoured. Version 1 has no mask list, and the loader treats every mask as all ones |

PaintRecord (v3 only, 268 bytes):

| Off | Type | Field |
|---|---|---|
| 0 | u32 | colour (D3DCOLOR) |
| 4 | i32 | vertex slot within the face |
| 8 | i32 | face index within the object |
| 12 | char[256] | object (mesh) name, NUL-terminated, junk after NUL |

#### 7.2.2 Self-check values

| File | ver | nlights | Σcount (list entries) | npaint | size (all consumed) |
|---|---|---|---|---|---|
| 04257 acr.3lt | 3 | 71 | 132,071 | 145 | 1,115,320 |
| 05285 rom.3lt | 3 | 105 | 937,788 | 0 | 7,531,716 |
| 05916 bea.3lt | 3 | 228 | 523,424 | 0 | 4,251,244 |
| 05917 per.3lt | 3 | 80 | 689,154 | 0 | 5,535,644 |
| 05918 col.3lt | 3 | 299 | 881,205 | 0 | 7,133,372 |
| 05919 abu.3lt | 3 | 128 | 371,703 | 0 | 3,009,476 |
| 05920 Azuchi.3lt | 3 | 203 | 433,395 | 3,291 | 4,406,000 |

Cross-checks against the `.x` files (`crosscheck.py`) all pass with 0 failures. Every
`objectIndex` is less than the mesh count (e.g. acr: max 1541 with 1542 meshes). Every `faceIndex`
is less than that mesh's face count. Every mask fits that face's vertex count (all masks ≤ 15, all
faces have 3 or 4 vertices). Every PaintRecord resolves to an existing mesh, face and slot.

Worked example: the first record of acr.3lt is `_TORCH4`. Its bytes start with
`a47abd3f 80426e3e 25854fc0` = pos (1.480, 0.233, −3.243), then `f1c1073f` = range2 0.5303
(reach √0.5303 ≈ 0.728), then `011e53ff` = colour 0xFF531E01 (R=0x53, G=0x1E, B=0x01, a dim orange),
then `"_TORCH4\0…"`, count 693. Light `_TORCH1` entry 0 is 0x033D0025 with mask 6. That means object
829 (`Box02zvc`, 100 faces), face 37 (a quad), vertex slots 1 and 2.

#### 7.2.3 Lighting model (per-vertex, software)

The lists are a **precomputed visibility set**. They record which face corners each light can see.
The light's influence itself is computed at run time from position, range2 and colour
(per-face routine at 0x10037D80; the list is attached to faces in FUN_100659E0).

Each face keeps a small list of (lightIndex, vertexMask) pairs, built from all lights' faceRef lists
whenever the "lights dirty" flag is set. The flag is set by LOAD, COLOR, ON/OFF, FLICKER on/off and
AMBIENT. The colour of each face corner is then:

```
if object.SPRITE or object.NOILLUMINATION or face is excluded:
    colour = 0xFFFFFFFF (full bright, texture unmodulated); specular = 0
elif LIGHTS global switch is OFF, or there are no lights:
    colour = 0xFFFFFFFF
elif this face has no light list entries:
    colour = AMBIENT   (the per-material base colour, see §7.2.5)
else:
    c = AMBIENT (R,G,B bytes; the alpha byte is kept from AMBIENT)
    for each (light, mask) attached to this face:
        if light is OFF: skip
        if vertex slot k ≥ 32 or bit k of mask is clear: skip
        D  = light.pos − vertex.pos
        d2 = |D|²
        if d2 ≥ light.range2: skip
        ndotl = dot(vertex.normal, D / sqrt(d2))
        if ndotl ≤ 0: skip
        f = (1 − d2 / light.range2) × ndotl
        for ch in R,G,B:
            c.ch = min(255, c.ch + trunc(light.current.ch × f))   // clamp after each light
    colour = c
```

* The falloff is **linear in squared distance**: `1 − d²/range2`. It is not inverse-square.
* `vertex.normal` is the mesh's per-vertex normal as resolved by the `.x` loader (§5).
* Static and dynamic lights: a light is "static" unless flicker is on (§7.2.6). On the first evaluation
  after the lights become dirty, the sum of all *static* lights is cached per vertex. On later frames
  the cache is reused and only the *flickering* (dynamic) lights are added on top. The result is
  identical to evaluating every light every frame.
* Afterwards the v3 **PaintRecords** are applied. Each forces the colour of one face corner to the
  record's colour. In the shipped data these are the 145 acr records (e.g. object `Box08ff`) and
  3,291 Azuchi records (e.g. `intpanel23`, `intpanel19`).
* The resulting colour modulates the texture (Gouraud × texture). **(VM)** Confirmed, with no overbright and
  without the material faceColor: an unlit Acropolis floor shows texel × 60/255 (`AMBIENT` 60), and with `LIGHTS.OFF`
  texels are unchanged.
* Specular (HW mode only, a second colour per vertex) is computed only when the scene specular colour
  is non-zero. Every tour sets `SCENE.SPECULAR=0`, so specular is always zero. A port may skip it.
* Port recommendation: bake the static lights into a glTF `COLOR_0` at convert time with the formula
  above. That reproduces the original exactly, including its "visibility by list" shadowing, which
  real-time lights would not. Then add the flickering lights at run time as a per-vertex term. Keep,
  per flickering light, the list of (vertex, f) pairs computed with the same formula. The colour then
  only scales with the flicker factor.

#### 7.2.4 `LIGHTS` commands (FUN_10030470)

| Statement | Effect |
|---|---|
| `LIGHTS.LOAD(f)` / `LOAD=f` | load a `.3lt` file (replaces all lights) |
| `LIGHTS.DELETE` | delete all lights |
| `LIGHTS.ON` / `LIGHTS.OFF` | global lighting switch (default **ON**). OFF makes everything full-bright |
| `LIGHTS.TOGGLE` | intended to toggle, but implemented as a sign flip of the switch. So ON stays ON (1 → −1, still non-zero) and OFF stays OFF. Reproduce as a no-op |
| `LIGHTS.ON?` / `OFF?` | query |
| `LIGHTS.<id>.ON` / `.OFF` | enable or disable one light (marks lights dirty). Abu Simbel uses `_SUNANIM1..3.OFF`, but those names do not exist (§7.8) |
| `LIGHTS.<id>.COLOR=r,g,b` | set the light's base colour bytes |
| `LIGHTS.<id>.POS=x,y,z` | move the light and **recompute its faceRef/mask lists at run time** by rendering the scene from the light into six 90° views (±X, ±Y, ±Z) and recording the visible face corners (FUN_1002E230). Not used by any shipped script |
| `LIGHTS.<id>.STATIC=0/1` | static flag (1 = static). Not used by the shipped scripts |
| `LIGHTS.<id>.FLICKER=<seconds>` | see §7.2.6 |

#### 7.2.5 `SCENE.AMBIENT`, `SCENE.SPECULAR`, `HIGHSPECULAR`, fog

* `SCENE.AMBIENT=r,g,b` writes D3DCOLOR (r,g,b) into the base colour of **every material/texture
  record that exists at that moment**, and marks lights dirty. It is ignored if no model is loaded,
  so it must follow `3D.LOAD`, which every tour's load.3sc does. This base colour is the starting
  value of every lit vertex in §7.2.3. Shipped values: acr 60,60,60; rom 20,20,24; bea 32,32,32;
  per 20,20,20; col 32,32,32; abu **0,0,0**; azuchi 32,32,32. **(VM)** In Abu Simbel faces no light reaches are black;
  `SCENE.AMBIENT=60,60,60` after loading brightens every surface.
* `SCENE.SPECULAR=0` disables specular. `HIGHSPECULAR` is a separate flag (unused).
* Fog: `SCENE.DELETEALL` resets the fog colour to 0x4B96E6, fog start to 0, fog end to **20.0**, and
  fog to off. `FOGEND=` sets the end distance, and fog is enabled only if the value is > 0.01. No
  tour enables fog. **But fog end (default 20) is also the cloud re-render cut-off (§7.3.4).**

#### 7.2.6 Flicker (FUN_1002E870 per frame, setup in FUN_10030470)

`LIGHTS.<id>.FLICKER=p`, with p in seconds (shipped values: 0.1, 0.2, 0.4):

* **Off → on** (p > 1e−6): the light becomes dynamic (static = 0). Its current base colour is saved
  as float R,G,B in 0…1, and a timer starts.
* **On → on**: only the period is updated.
* **On → off** (p ≤ 1e−6): the light becomes static again, with base colour = the last saved float
  colour × 255 (truncated).
* **Every frame, for each flickering light:** add the elapsed wall-clock time to an accumulator. When
  the accumulator reaches p, reset it to 0 and draw a new intensity factor:

  ```
  k = (rand() & 0x1FF) / 511 × 0.6 + 0.4          // uniform in [0.4, 1.0], 512 steps
  current.R = trunc(k × saved.R × 255)  (same for G, B), alpha = 0xFF
  ```

  The light keeps this colour, unchanged, until the next draw. It is a **step function** with no
  interpolation. `rand()` is the MSVC C runtime LCG. Exact sequence parity is not needed. Use any
  uniform source.
* `FLICKER?` returns the period.
* Shipped pattern: `lights.3sc` alternates 0.1 and 0.4 (Persepolis and Abu Simbel use 0.2 and 0.4).
  **Because names must match, flicker is active only where the light exists** (see §7.8): Acropolis 19
  of 19 FLICKER statements, Roman Forum 9/9, Beaumaris 44/44, Persepolis **0/53**, Colosseum **0/7**,
  Abu Simbel 57/59 (`_TORCH36` is stored as `"_TORCH36 "` with a trailing space, and `_TORCH39` is
  missing), Azuchi 4/7.

**(VM)** At the Athena Parthenos stop the lit strip seen through the Parthenon doorway changes brightness between
frames sampled 105 ms apart, in jumps with no smooth transitions, while the rest of the view is static. `LIGHTS.<id>.COLOR?`
returns the current colour: sampled every 5 ms, a 0.1-s light changes every 100–118 ms (mean 109 ms) and a 0.4-s light
every 402–421 ms, i.e. after 7 and 26 ticks of 15.6 ms. The accumulator restarts at 0 when it fires, so a period
rounds up to whole ticks. Persepolis and Colosseum have no `_TORCH` lights (`COLOR?` answers `Failed`) and their views
are static; Abu Simbel `_TORCH36`/`_TORCH39` answer `Failed` as well.

---

### 7.3 `.3cl`: point clouds ("voxel statues") and `CLOUD`/`CLOUDSCALED`

#### 7.3.1 Layout (loader 0x10019570)

```
u32 version        must be ≥ 2 (version < 2 is rejected); shipped: 3 (6 files), 2 (wordcld.3cl)
u32 n
n × Point (16 bytes):
    i16 x, i16 y, i16 z     cloud-local integer coordinates (1 unit = 1 texel at scale 1)
    u16 pad                 always 0
    f32 key                 facing angle in DEGREES (0…360), or NaN — see §7.3.3
    u16 colour              RGB565 (r = bits 15–11, g = 10–5, b = 4–0)
    u16 pad                 always 0
v3 only: u32 trailer   ignored by the runtime (equals max y − 1 in 5 files, e.g. brzath 366 vs
                        max y 367; floral 58 vs 58). v2 has no trailer.
```

The v2 and v3 points are parsed identically. The only difference is the 4-byte trailer.

| File | ver | n | size | y range | key range | used by |
|---|---|---|---|---|---|---|
| brzath.3cl | 3 | 185,678 | 2,970,860 | 1…367 | 0…360 sorted | statue_plan02, scale 0.65 |
| cartidm.3cl | 3 | 86,971 | 1,391,548 | 1…285 | 0…360 sorted | n_00statue, 01, 04 (0.88) |
| carytid.3cl | 3 | 86,931 | 1,390,908 | 1…285 | 0…360 sorted | n_00statue02, 03, 05 (0.88) |
| chariot.3cl | 3 | 124,867 | 1,997,884 | 1…117 | many NaN | statue_plan03 (0.8) |
| nike.3cl | 3 | 7,378 | 118,060 | 1…82 | many NaN | statue_plan04–07 (0.35) |
| floral.3cl | 3 | 6,300 | 100,812 | 0…58 | has zero padding records | only in commented-out lines (unused) |
| wordcld.3cl | 2 | 31,425 | 502,808 | 0…162 | ≈ atan2(z,x) | not referenced (unused) |

Size formula: `8 + 16n (+4 for v3)`. It matches all 7 files exactly. floral and wordcld contain
all-zero records (position 0,0,0, colour 0). The runtime would draw them as black texels, but neither
file is used.

#### 7.3.2 Attaching a cloud (`3D.<id>.CLOUD=f`, `3D.<id>.CLOUDSCALED=f,s`)

* `CLOUD=f` is the same as `CLOUDSCALED=f,1.0`.
* Clouds can only attach after the renderer exists. Otherwise the statement is ignored.
* A new cloud is created and attached to the object. Its **target image is the texture of the
  object's first face** (the placeholder's own bitmap). Every placeholder in the shipped data is a
  single quad with UVs (0,0)…(1,1), with v = 0 at the *bottom* edge (lowest y) and v = 1 at the top.
  Placeholder bitmaps: `00STA*.bmp` 128×256 (caryatids), `Athen70.bmp` 256×256 (bronze Athena),
  `Chari69.bmp` 512×128 (chariot). The width used is the surface pitch ÷ 2 (16-bit surface), the
  height is the surface height.
* **Point loading and scale.** `s` is texels per cloud unit. If s = 0 it is treated as 0.5. The
  stored integer coordinates are: column offset u0 = x·s and depth term z0 = z·s, each carried with
  19 fractional bits; row = trunc(y·s).
  Example: a caryatid at 0.88 has rows up to 285·0.88 = 250 (fits in 256) and x ±45 → ±40 texels
  around column 64 (fits in 128).
* **Shared loads:** if a cloud with the same file name was already loaded, its already-scaled points
  are copied instead of re-reading the file. A later `CLOUDSCALED` of the same file with a
  *different* scale therefore silently keeps the first scale. In the shipped scripts each file has a
  single scale, so this never matters.
* The RGB565 colours are converted to RGB555 when the display surface is 15-bit.
* `3D.<id>.CLOUD.OFFSET=a` sets the cloud's yaw offset **in radians** (scripts comment `180=3.141592`,
  `90=1.570796`). It only works after the cloud is attached. Shipped: caryatids π, bronze Athena and
  chariot −π/2, Nikes 0, 0.001 and π.
* `3D.CLOUDS?` / `3D.CLOUD?` returns the number of clouds.

#### 7.3.3 Rendering a cloud into its texture (FUN_10019AB0, called from the object draw at 0x1003ADC0)

The cloud is an **impostor**. On each draw the points are re-projected *in software* into the
placeholder's texture, and the textured quad is drawn as a SPRITE billboard (§7.4.1). The algorithm,
in our words:

```
centre = object centre (mean of the object's per-face centres)
dx, dz = centre.x − eye.x, centre.z − eye.z
phi    = atan2(dz, dx) + offset, wrapped into [0, 2π]
viewDeg = phi × 57.29578 − 90

clear texture to the colour key (magenta: 0xF81F in 565, 0x7C1F in 555)
clear a per-texel u32 depth buffer to 0xFFFFFFFF

for each chunk of 500 consecutive points [i, j) with j = min(i+500, n−1):
    draw the chunk if angleDiff(viewDeg, key[i]) ≤ 95° or angleDiff(viewDeg, key[j]) ≤ 95°
    (angleDiff = min(|a−b|, ||a−b|−360|); a NaN key counts as "visible")
    for each point p in the chunk:
        col   = floor(x·s·cosφ + z·s·sinφ)            // signed texel offset from the centre column
        index = row(p)·W + W/2 + col                  // linear texel index; NO per-row clipping
        if 0 ≤ index < W·H:
            depth = (z·s·cosφ − x·s·sinφ)·64 + 65536   // unsigned compare, smaller is nearer
            if depth < zbuf[index]: zbuf[index] = depth; texel[index] = colour(p)
```

* One point is one texel. There are no splats, no lighting (the point colours are final), and the
  sprite's vertices are full-bright (§7.4.1).
* Row 0 is the surface's first row, which is where v = 0 samples (§5.6: surfaces are bottom-up). The
  placeholder UVs put v = 0 at the bottom of the quad, so cloud y = 0 lands at the bottom and the statue
  stands upright. **(VM)** Confirmed: the Bronze Athena (stop "Bronze Athena", seen from yaw 19°) and the
  Athena seen through the Propylaea are upright, not mirrored and show the same side as the captures
  (shield and spear on the viewer's right), with `phi = atan2(dz, dx) + offset` and the column formula above.
* Because only the linear index is range-checked, a point whose `col` runs past the left or right
  edge wraps into the neighbouring row. A faithful port can reproduce that, but clipping per row is
  visually equivalent for the shipped data (all clouds fit their bitmaps).
* The last point of the file (index n−1) is never drawn, because each chunk's end bound is exclusive.
* The **sort key is a facing angle in degrees.** Points are sorted by it, so each 500-point chunk
  covers a narrow angular band, and chunks facing more than 95° away from the viewer are skipped as a
  speed-up. A port that draws every point with a depth test gets nearly the same image. The culling
  only removes back-facing points that the depth test would hide anyway.
* Constants: 500 points per chunk, 95° acceptance, 19-bit fixed point (2¹⁹ = 524288), depth bias
  65536.

#### 7.3.4 When a cloud is re-rendered

Before each re-render (FUN_10019AB0):

1. Compute phi as above.
2. **Hardware mode only:** skip if |phi − lastPhi| < 0.09 rad (about 5°), so the old image is kept
   (a 0.002 threshold is also tested, which is redundant). In software mode the cloud is re-rendered
   on every draw.
3. Skip if the object centre is behind the eye: dot(normalise(centre − eye), normalise(lookAt − eye)) < 0.
4. After the first render, also skip if the 3-D distance eye→centre ≥ 0.9 × FOGEND (default 20, so
   **18 units**). The stale image remains visible. **(VM)** With `FOGEND=10` the Bronze Athena re-renders at 8.95 units
   and is frozen at 9.05, showing the view of its last render from any side; in software mode it re-renders on every
   draw (the image changes between 1° steps). Step 2 needs a 3D device and was not seen.
5. In hardware mode the image is drawn into a system-memory copy and then blitted to the video texture.

#### 7.3.5 Cloud lifetime

Clouds are freed by `3D.DELETE`, by `SCENE.DELETEALL` and by loading a new model. A cloud finds its
host again by the host's exact (case-sensitive) name.

---

### 7.4 Per-object properties `3D.<id>.<PROP>` (FUN_100316C0)

Value parsing for the flag properties (VISIBLE, SPRITE, NOILLUMINATION, WATER): only the **first
character after `=`** is examined. `0` means off, `1` means on, and anything else fails. Any text
after it is ignored. Consequence: in Acropolis HIDEALL, the BEGIN/END body is concatenated into
`3d.n_00water.WATER=03d.Box23a.VISIBLE=02D._dot.VISIBLE=0`. This executes only `WATER=0` on
`n_00water`, and the glued-on `Box23a`/`_dot` statements are lost. `3d.Box23a.VISIBLE=0` would be a
no-op anyway, because Box23a does not exist. `?` after the property name returns the current value.

#### 7.4.1 `SPRITE=1`: cylindrical billboard

* When set, every vertex colour of the object becomes white, so a sprite is never lit.
* At draw time (FUN_1003AB50) the object is rotated **about the vertical (Y) axis through its centre's
  (x, z)** by the camera's current yaw, taken from the view matrix. As a result the quad stays
  parallel to the view plane in yaw. Pitch is ignored, so it does not tilt up or down.
  Sprites are authored facing ±Z (all shipped placeholders and torch quads lie in a z = const plane).
* Used for: statue placeholders (with clouds), torch flames (`Torch_*`, `Flame*`, `n_00FLAME15`,
  with FILMSTRIP), and in Abu Simbel 69 sprite statements. **(VM)** A torch is face-on from every yaw (45° steps; the
  opposite sign would turn it edge-on at 45°), and at yaw 0, pitch −40, `SPRITE=1` and `SPRITE=0` render identically:
  rotation by the camera yaw only, no pitch.

#### 7.4.2 `NOILLUMINATION=1`

The object skips lighting and is drawn with white vertex colour (texture at full brightness, no
specular). It is used for skies, clouds and sky domes (`Sky*`, `Cloud*`, `SKYSURROUND`, `sky6_8` …).
For a port: use an unlit material.

#### 7.4.3 `VISIBLE=0/1`, `UNHIDEALL`, `TOGGLEHIDDEN`

* `VISIBLE` is a per-object draw flag (default visible).
* `3D.UNHIDEALL` sets every object visible.
* `3D.TOGGLEHIDDEN` inverts every object's flag. **(VM)** `VISIBLE?` flips 1 → 0 → 1 on every object, but the
  drawn result was not understood: a first toggle left the Parthenon view unchanged; a second one, after two objects
  had been hidden, showed those again and hid the ground. No shipped script uses it.

#### 7.4.4 `FILMSTRIP=framewidth,wait,path` (setter 0x1003B430, per-frame step in the object draw 0x1003ADC0)

* `framewidth > 0`:
  1. `path` is applied as the object's texture (as APPLYTEXTURE).
  2. N = textureWidth ÷ framewidth (integer division). For `fireAnm.bmp` 256×32 with framewidth 16,
     N = 16.
  3. The object's current U coordinates are saved and scaled by 1/N, so the quad shows one frame.
  4. A random starting frame is chosen: `rand() % N`.
  5. A timer starts.
* Every draw: add elapsed seconds to an accumulator. When it reaches `wait`, reset it, advance the
  frame (1…N, wrapping N → 1), and set U = savedU/N + (frame−1)/N. V is unchanged.
  Torches use `16,.07`, each torch starting at a random phase. **(VM)** The accumulator restarts at 0, so `wait`
  rounds up to whole ticks: frames change every 5 ticks (72–96 ms, mean 81 ms, about 12.4 fps at 64 ticks/s, not 14.3),
  on the same ticks for every torch, which show different frames.
* `framewidth = 0` turns the strip off and restores the saved U × N.
* The strip is horizontal: frames are side by side along U.

#### 7.4.5 `APPLYTEXTURE=file`

Replaces the bitmap of the object's **first face's** texture record with `file`, and reloads it into
video memory in hardware mode. Note that it changes the shared texture record, so every face or
object that uses that texture record changes too. `fireAnm.bmp` is applied to all torches.
**[VM: check whether other meshes share the placeholder texture.]**

#### 7.4.6 `WATER=0/1`: animated ripple texture (setters 0x10035DC0 / 0x10035C50, step 0x10035F10)

Turning water **on** for an object makes its first face's texture a 2-D ripple simulation:

* Two int height fields of W×H start at zero.
* **Step**, run once per rendered frame. If the frame rate exceeds 40 fps, steps are skipped
  proportionally **[VM: exact rate]**. For every interior texel:
  `new = (sum of the 8 neighbours in the current field) / 4 − new` (the previous-previous value),
  then `new −= new / 16` (damping ×15/16). Afterwards the two fields swap.
* **Drops:** at random intervals a drop is stamped at a random position at least 3 texels from the
  edges. The stamp is a 5×5 disc without its corners, with these heights:

  ```
       .   700  500  700   .
      700 1200 1400 1200  700
      500 1400 1500 1400  500
      700 1200 1400 1200  700
       .   700  500  700   .
  ```

  every value shifted right by `(rand() & 15) + 1`.
* **Display:** output texel (x, y) = source texel displaced by (h[x−1] − h[x]) columns and
  (h[x−W] − h[x]) rows, taken from the unmodified original bitmap. A displacement that falls outside
  the bitmap gives black.
* Turning water **off** restores the original texture.
* **Shipped use: Acropolis only, through the walk map.** *Correction (implementation):* this section
  used to say the effect is never on. The only WATER statement in the `.3sc` files is
  `3d.n_00water.WATER=0` (Acropolis HIDEALL, a no-op when water was never on), but `acr.3wm` hotspot
  cells 1145–1148 (XZ ≈ 0.46…1.35, 2.70…3.16, in front of the Athena Parthenos caption `_L17`) run
  `3d.n_00water.WATER=1;2D._L17.VISIBLE=1;2D._H28.VISIBLE=1;` on enter and the same with `=0` on exit.
  `n_00water` (mesh 1014) is a single quad with material `A_00water`. So the ripple shows while the
  visitor stands in that area. Walking from one of these cells into a neighbouring one runs the exit
  (water off, original restored) and then the enter (water on, height fields from zero), so the ripple
  restarts. Implementing the effect is needed for parity. **(VM)** In those cells `3D.n_00water.WATER?` is 1 and the pond
  ripples; just outside it is 0 and the pond is still.

#### 7.4.7 Other properties (not used by any shipped script)

* `PLAYAVI=file`: plays a video as the object texture.
* `STRING=` / `STRING?`: an arbitrary per-object string.
* `3D.<id>.SKY`: does not exist. `SKY` is a **2D-layer** property (`2D.[id].SKY=`, see the 2D
  section) and is not used by any script.

---

### 7.5 Scene-wide `3D.*` switches

| Statement | Effect |
|---|---|
| `3D.ON` / `3D.OFF` | master 3-D draw switch (default ON). While OFF, no 3-D object, cloud, water or filmstrip is updated or drawn, and only 2D layers show. load.3sc runs `3D.OFF` before `3D.LOAD` and `3D.ON` at the end, so only `begin.32d` (the loading animation) is visible meanwhile |
| `3D.LOAD(f)` / `LOAD=f` | load the `.x` model (§5) |
| `3D.DELETE` | delete all objects and textures |
| `3D.CALC64BIT=v` | default 0.1, every tour sets 0.05. Used only by the **software rasteriser**: when the eye is within `v` of a face's plane (signed plane distance ≤ v), that face is rasterised with 64-bit texture-gradient arithmetic to avoid precision artefacts. **No effect for a GPU port** |
| `3D.LODDISTANCE=v` | default 0, never set by a script. It is added to the distance threshold of the object-picking pass, not the visible LOD switch (§7.6) **[low confidence]** |
| `3D.LOADVP` / `APPENDVP` / `PROCESSVP` | listed in the control's help text but **not implemented**: there is no command handler. `LOADVP=x` fails like any other unknown statement |
| `3D.UNHIDEALL`, `3D.TOGGLEHIDDEN` | §7.4.3 |

---

### 7.6 Level of detail: `lod_` meshes (FUN_1003B670 at model load; draw loop in FUN_100659E0)

* At model load, each mesh whose name starts with `lod_` (lower-case, **case-sensitive**) is marked
  "LOD mesh" and is never drawn on its own. The mesh whose name equals the rest of the name
  (case-sensitive, first match) gets this LOD mesh as its low-detail partner.
* At draw time, for every visible non-LOD object that has a partner: if the **horizontal** (XZ)
  distance from the eye to the object centre is ≤ **13.0** units, the object itself is drawn.
  Otherwise the partner is drawn. 13.0 is a fixed constant set when the renderer is created. No script
  command changes it.
* Shipped LOD meshes: acr 116 (all with partners), rom 320 (all), per 64 (63 with partners; one
  orphan never draws), others 0.
* **(VM)** newcolumn07 turns into its `lod_` mesh between XZ distance 12.99 and 13.01 (eye 1.36 units above it, so the
  rule is horizontal). Over a visibility cell there is no distance swap: the PVS lists decide (§6.6).

---

### 7.7 `.3vp` (observation only; not used at run time)

```
u32 n;  n × f32[3] point
u32 m;  m × f32[3] unit vector       (m == n in all files; size = 8 + 24n exactly)
```

| File | n | size | point y range |
|---|---|---|---|
| 05916 level2.3vp | 7,685 | 184,448 | 0.880…4.552 |
| 05918 level3.3vp | 13,028 | 312,680 | 2.453…2.929 |
| 05919 abu.3vp | 2,863 | 68,720 | −0.500…2.130 |

Every vector has length 1.0000 with positive y (0.07…1.0). Most of them occur byte-for-byte as plane
normals in the same-named `.3wm` walk map (level2: 292 of 308 sampled, level3: 303/303,
abu: 165/319). The points do not occur in the walk map. This looks like sample viewpoints on the
walkable surface, paired with the surface normal, produced by an authoring "view point" tool. No
script loads them and the runtime has no handler, so **converters can skip `.3vp`**.

---

### 7.8 Shipped-script name problems (silent failures a port must reproduce)

From `crosscheck.py`, comparing the names case-insensitively:

* **LIGHTS ids missing from the `.3lt`**
  * per: all 53 `_TORCH*` (per.3lt has no `_TORCH` lights)
  * col: `_TORCH1..7`
  * abu: `_SUNANIM1..3`, `_TORCH36` (stored with a trailing space), `_TORCH39`
  * azuchi: `_TORCH5..7`
  * acr, rom, bea: none missing
* **3D ids missing from the `.x`**
  * acr: `Cloud50`, `Sky_Dome`, `PLANE01`, `cloud10/12/13/15/16/17/25/26`, `xxxxxxxx`,
    `statue_plan01/08/09`, `statue_plan`, `Plane05/06/07`, `Torch_17`, `Box23a`
  * rom: `Sky03`, `Sky04`
  * bea: 38 `Flame*` ids (`Flame121/122/141/142`, `227–246`, `251–254`, `257–266`; *correction:* was
    given as 34) plus `Sky03/04`, `SurroundBack/Front`, `skylast`, `sky6_09`, `sky05` (45 ids)
  * per: `sky6_8`, `cloud13`, `Sphere01`, `Box23a`
  * col: `Surround`, `sky6_8`, `Sphere01`, `Ground`, `Mesh157..164`, `cloud12/16/17/22/25`
  * abu: `NILE`, `SKYSURROUND`, `CYLINDER01`, `Plane32/33/34/45/46/91`
  * azuchi: none
* Statement counts per property (all scripts): SPRITE 304, VISIBLE 300, FILMSTRIP 269,
  APPLYTEXTURE 203, NOILLUMINATION 163, CLOUDSCALED 12, CLOUD.OFFSET 12, WATER 1. (VISIBLE is 299 after
  the `HIDEALL` merge, §3.8. Walk-map hotspot strings add 676 VISIBLE and 8 WATER statements, §6.2.)

---

### 7.9 Parse coverage (this area)

| Kind | Files | Parsed exactly (bytes consumed = size) | Unexplained bytes |
|---|---|---|---|
| `.3lt` | 7 | 7 | 0. The only ignored bytes are the junk after each name NUL, inside the fixed 256-byte buffers |
| `.3cl` | 7 | 7 | 4-byte v3 trailer (ignored by the runtime; ≈ max y) |
| `.3vp` | 3 | 3 | 0 (format by observation; not used at run time) |

---

### 7.10 VM confirmation checklist (this area)

- [x] Torch flicker is a step change of brightness every 7 ticks for 0.1 s and 26 ticks for 0.4 s, within 0.4–1.0
      of the base colour (§7.2.6).
- [x] Persepolis and Colosseum torches do not flicker, and Abu Simbel `_TORCH36`/`_TORCH39` stay steady.
- [x] Lit colour × texture: modulate with no overbright, faceColor ignored. Abu Simbel's ambient 0,0,0 makes unlit
      areas black.
- [x] Cloud statues: upright, not mirrored, correct side (Bronze Athena and the Propylaea view of it, §7.3.3).
      The Persepolis Gate of Xerxes bulls, once listed here as showing "another side", are not clouds: they are
      textured meshes (`bullf12.jpg` …) and were wrong because of the V flip (§5.6).
      [x] The image updates continuously in SW mode and freezes beyond 0.9 × FOGEND (§7.3.4). [ ] The 5° steps of
      HW mode (no 3D device).
- [x] Sprite billboards rotate with camera yaw only, same sign as the camera (§7.4.1).
- [x] Flame filmstrips: every 5 ticks (about 12.4 fps), random phase per torch (§7.4.4).
- [x] LOD pop at 13 units horizontal distance (Acropolis `newcolumn07`, §7.6).
- [x] The water ripple on `n_00water` appears in Acropolis while standing in `acr.3wm` cells 1145–1148
      (in front of the Athena Parthenos) and stops on leaving. [ ] Its step rate.
- [x] `LIGHTS.TOGGLE` does nothing (VM: replies `Success:-1`, `LIGHTS.ON?` stays TRUE, the view is unchanged).

### 7.11 Remaining unknowns

* Exact per-vertex normal source and face-centre definition (mean of face vertices vs. bounding-box
  centre). This depends on §5. Object centre = mean of its per-face centres.
* Water step throttling above 40 fps and drop interval (uses the measured frame rate). Irrelevant
  for the shipped content.
* What `TOGGLEHIDDEN` does to drawing (VM: it flips `VISIBLE?`, but the views did not follow; §7.4.3).
* Which texture records are colour-keyed (magenta transparency). This belongs to §5.5, but clouds rely on magenta being the transparent key.
* `LODDISTANCE` semantics (only used in the picking pass).


---

## 8. 2D layers (`.32d`), HUD, captions and hotspots

Provenance: static analysis of ACTX3DX3.OCX (2D layer object is a single global, ~0x1011ac20).
Key functions: file load FUN_10008880, file save FUN_100085d0, element insert FUN_10002800,
2D command dispatcher FUN_10005780, draw pass FUN_10001fa0, sky backdrop FUN_10001570,
mouse/hover/macro handler FUN_10004520, hit test FUN_10003960, label measure FUN_10009070,
label raster FUN_10007fe0 + FUN_1005a380, icon strip load FUN_100076b0, NEXTFRAME FUN_10009d10,
anchor cone test FUN_10009ba0, projection FUN_100025d0, viewport clip FUN_10003b60.
Validated by parsing all 14 `.32d` files exactly (see coverage at the end).

### 8.1 File layout

Little-endian. `lpstr` = `u32 length` + `length` bytes of cp1252 text, **no** terminator.

Header:

| Off | Type | Name | Notes |
|---|---|---|---|
| 0 | u32 | version | 4 in all files. Loader rejects < 4. |
| 4 | u32 | layer_on | 1 in all files. Restored into the layer's ON flag (same flag as `2D.ON/OFF`). |
| 8 | lpstr | icon_file | Authoring path, e.g. `\\phoenix\media\AEncProd_02\...\3DVTicon.bmp`; empty in `begin.32d`. Loader stores the string (it is what `2D.ICONFILE?` reports) but does **not** load the bitmap. Ignore for rendering. |
| . | u32 | icon_count | 0 in all files. Written by the saver, **skipped** by the loader. |
| . | i32 | element_count | 4 (`begin.32d`), 26–57 (`lh.32d`). |

Then `element_count` element records, back to back, nothing after the last one.

Element record (variable length; 382–401 bytes in practice):

| Rel off | Size | Field | Observed values / meaning |
|---|---|---|---|
| 0x00 | u32 | primitive | 6 everywhere (triangle fan). Hit testing only considers elements with 6. |
| 0x04 | u32 | kind | 0 = plain image or plain label; 1 = **hotspot** (hover-colour/icon-state behaviour, see §8.5); 2 = 4-state button strip (no file uses it). All `_H*` are 1, everything else 0. |
| 0x08 | u32 | additive | 0 everywhere. When non-zero the quad is alpha-blended with src = dest = `blend` (0x44). |
| 0x0C | u32 | (unused) | 0. |
| 0x10 | u32 | visible | 1 in every file (scripts hide them afterwards, §8.6). |
| 0x14 | u32 | textured | 1 everywhere. 0 = draw untextured. |
| 0x18 | u32 | captures_mouse | 1 for images, `_H*`, `_dot`; 0 for `_L*`. When the mouse is pressed on such an element the 3D navigation does not move and the "hot" cursor is shown (§8.5). |
| 0x1C | u32 | anchored | 0 in files; set by script `ANCHORED=1`. |
| 0x20 | u32 | color_key | 1 = magenta is transparent. 1 for `_L*` and `dir.gif` (`_1`), 0 otherwise. For labels it also selects the **outlined-caption** style (§8.4). |
| 0x24 | u32 | tex_blend | 1 (D3D "decal": texel colour replaces vertex colour). |
| 0x28 | u32 | diffuse | 0xFFFFFF; `_dot` 0x000000. Vertex colour written into all four vertices each frame. |
| 0x2C | u32 | shade_mode | 1 (flat). |
| 0x30 | u32 | ? | 0xFFFFFFFF everywhere; not read by the runtime. |
| 0x34 | u32 | junk | Uninitialised memory from the authoring tool (e.g. 0x6C371C73, 0x02010001). Ignore. |
| 0x38 | f32×3 | anchor_xyz | 0,0,0 in files; set by `ANCHORPOINT`. World coordinates (same space as the `.x` scene). |
| 0x44 | u32 | blend | 2 (= "one"). Only used when `additive`. |
| 0x48 | u32 | is_label | 1 = text caption rendered into an off-screen 512×32 surface; 0 = image from `image`. `_L*`, `_H*` are 1. |
| 0x4C | u32 | label_fill0 | 0xFF00FF (`_L`), 0xFF0000 (`_H`). Initial fill colour; always overwritten when the caption is rasterised (§8.4). |
| 0x50 | u32×2 | ? | 0xFF00FF/0xFF0000 copies; not read. |
| 0x58 | u32 | frame_color0 | Initial caption frame colour. `_H` 0xC0D1E3. |
| 0x5C | u32×2 | ? | 0xFFFFFF; not read. |
| 0x64 | i32 | icon_normal | Icon index (−1 = none). File: `_H` 0, `_L` −1, images −1. |
| 0x68 | i32 | icon_hover | = icon_normal+2 (recomputed on load). |
| 0x6C | i32 | icon_pressed | = icon_normal+1 (recomputed on load). |
| 0x70 | u16×4 | fan indices | 0,1,2,3. |
| 0x78 | u16 | pad | 0. |
| 0x7A | u32 | vertex_count | 4 everywhere. (The loader always reads exactly four vertices; the count is informational.) |
| 0x7E | 4×32 | vertices | Pre-transformed screen vertex: `f32 x, y, z, rhw; u32 diffuse; u32 specular; f32 u, v`. Order top-left, top-right, bottom-right, bottom-left. z = 1e-5, rhw = 1. x/y in client pixels. For labels `u_max = width/512`, `v_max = 22/32 = 0.6875`. |
| 0xFE | u32 | text_color0 | Caption text colour, 0x00RRGGBB. `_L` 0xFFFFFF, `_H` 0xC0D1E3. |
| 0x102 | u32×2 | ? | 0xFFFFFF; not read. |
| 0x10A | 12 B | zero | |
| 0x116 | 60 B | font | Win32 LOGFONTA: `i32 height, width, escapement, orientation, weight; u8 italic, underline, strikeout, charset, out_prec, clip_prec, quality, pitch_family; char face[32]` (face is NUL-terminated, tail is junk). All files: Tahoma, height −11, weight 400 (images, `_H`) or 700 (`_L`), charset 1, pitch_family 0x20. Effectively overridden at render time (§8.4). |
| 0x152 | lpstr | text | Caption text. Placeholders in files: `ERS Label` / `ERS HotSpot` (replaced by script `TEXT=`). |
| . | u32 | has_text | 1 for `_H` (enables the hover behaviour of kind 1), 0 otherwise. |
| . | lpstr | macro_mousedown | |
| . | lpstr | macro_mouseup | |
| . | lpstr | macro_mousein | |
| . | lpstr | macro_mouseout | All empty except `_dot.mousedown` (`SCENE.TRYHARDWAREFULLSCREEN[;]`, Persepolis `WALKMAP.LOAD=stair1.3wm;`, Azuchi empty). |
| . | lpstr | name | Element id, e.g. `_L1`, `_H26`, `_dot`, `_0`. |
| . | lpstr | image | Image file for non-labels (`dir.gif`, `loadanim.gif`, …); empty for labels and `_dot`. |

Offsets 0x00–0x79 and 0xFE–0x151 are raw copies of the runtime element struct, which is why
unused and junk fields exist. A writer should emit the junk fields as zero.

Worked example (Acropolis `lh.32d`): header occupies 0x00–0x5E (`04 00 00 00`, `01 00 00 00`,
icon path length 0x4B, icon_count 0, element_count 0x2D = 45). First record `_L1` at 0x5F, 382 bytes:
quad (22,34)–(111,56) i.e. 89×22 px, `u_max` 0.1738 = 89/512, `color_key` 1, `is_label` 1,
font Tahoma −11/700, text `ERS Label`. `_H26` at 0x37D2: 91×22 at (299,180), kind 1, has_text 1,
text colour 0xC0D1E3, text `ERS HotSpot`. `_dot` at 0x4259: 4×4 black square at (−1,−1) with
`mousedown = SCENE.TRYHARDWAREFULLSCREEN`.

### 8.2 Element classes found on the disc

| Class | Where | What it is |
|---|---|---|
| `_0` `_1` `_2` `_3` | `begin.32d` (identical file in all 7 tours, MD5 800c9356…) | Loading screen: `_1` dir.gif 541×381 at (45,25) colour-keyed; `_0` loadanim.gif 128×128 at (39,6); `_2` loadtxt.gif 86×29 at (62,72); `_3` dirTxt.gif 286×294 at (250,88). Drawn in that order (array order). |
| `_L<n>` | `lh.32d` | Place-name caption, anchored to a 3D point. Outlined bold white text, transparent background, not clickable (captures_mouse 0, no macros). |
| `_H<n>` | `lh.32d` | Hotspot: black box with icon + text, frame, hover highlight; `MACRO.MOUSEUP` raises the host event. |
| `_dot` | `lh.32d` | 3–7 px black square in the top-left corner; a hidden debug button (mouse-down switches to hardware full screen). Scripts hide it. |

### 8.3 Layer commands (`2D.<cmd>`)

The interpreter strips the `2D.` prefix and passes the rest (already upper-cased up to the first
`^`, see §3) to the 2D dispatcher. Return strings (`Success`, `Failed`, query
results) go back to the caller; scripts ignore them.

| Command | Effect |
|---|---|
| `ON`, `SHOW` | Layer ON flag = 1. |
| `OFF`, `HIDE` | Layer ON flag = 0. Nothing in the layer is drawn or hit-tested. Per-element `VISIBLE` flags are **not** touched. |
| `ON?`, `OFF?` | Query the flag. |
| `TOGGLE` | Reports the flag; does **not** change it (as implemented). |
| `LOAD=<f>`, `LOAD(<f>)` | Replace **all** elements with the contents of `<f>` (§8.1). Each element is then initialised: labels get their 512×32 caption surface and are rasterised; images are loaded (gif/jpg/bmp, native size; elements naming the same file share one texture). |
| `NEW` | Delete all elements. |
| `ICONFILE=<f>` | Load the icon strip (§8.4.1). Rasterises every label again. |
| `ICONS=<n>` | Number of icons in the strip (all tours: 20). Must follow `ICONFILE`; the strip is sliced at use time. |
| `ICONFILE?`, `ICONS?` | Queries. |
| `FONT=<face>` | Global caption face (default `Tahoma`, or `MS UI Gothic` in Japanese mode). |
| `FONTSIZE=<pt>` | Global caption height = abs(pt × dpiY / 72) pixels (default **13**, Japanese 14). |
| `CAPTIONMETRICS=<voff>,<scale>,<adder>` | Caption text y-offset (int, default **4**) and the width-correction constants (default **0.26**, **82**; Japanese 0.4, 68). `CAPTIONMETRICS?` reports them. |
| `JVERSION=TRUE/1/FALSE/0` | Japanese mode (font defaults, charset 0x80). |
| `HARDWARE=…` | Texture memory placement hint. No visible effect for a reimplementation. |
| `GRID=1/0/TOGGLE` | Debug ground grid (±5 units, 1-unit lines at y=0, colours 0x64C896 / axes). Not used. |
| `SIZE?` | "w,h" of the most recently created element. |

### 8.4 Element properties (`2D.<id>.<prop>` or `2D.[<index>].<prop>`)

`<id>` is matched case-insensitively against element names (the element name is upper-cased
before comparison and the statement is already upper-cased). `[n]` addresses by array index.
If no element matches, the statement fails silently — this happens a lot on the disc (§8.7).

| Property | Effect |
|---|---|
| `VISIBLE=TRUE/1/FALSE/0`, `VISIBLE.TOGGLE`, `VISIBLE=TOGGLE`, `VISIBLE?` | Per-element visible flag. |
| `TEXT=<s>`, `TEXT(<s>)`, `TEXT?` | Caption text (the `^…^` quoting is resolved by the interpreter), then re-layout + re-raster. |
| `ANCHORPOINT=x,y,z[,…]`, `ANCHORPOINT?` | Anchor world point. **Only x, y, z are used**: the value is split on the first two commas and the third number is read with `atof`, which stops at the next comma. The 4th value seen in every script (a yaw in degrees, e.g. `183.640060`) and the `:`-separated second point in Roman Forum `_L9` are ignored by this version of the control. |
| `ANCHORED=TRUE/1/FALSE/0`, `ANCHORED?` | Enable per-frame re-positioning at the anchor (§8.5.2). |
| `ICON=<n>` | Icon indices: normal = n, pressed = n+1, hover = n+2; −1 = no icon. Re-raster. Scripts use 6 (picture), 3 (TV/video), 9 (four-way arrows / panorama), 12 (globe). |
| `FONT=`, `FONT?`, `FONTSIZE=`, `FONTSIZE?` | Per-element LOGFONT face / height. The caption raster overwrites them with the global face and height (§8.4.2), so they have no lasting effect. |
| `REDRAWLABEL` | Re-raster the caption. |
| `MACRO.MOUSEDOWN=`, `.MOUSEUP=`, `.MOUSEIN=`, `.MOUSEOUT=` (and `?`) | Store a script string run on that event (§8.5.3). The stored string is the upper-cased statement text, e.g. `SCRIPT.EVENT(HOTSPOTCLICKED:HOTSPOTID=701504853)`. |
| `NEXTFRAME(<n>)`, `NEXTFRAME=<n>` | Advance a vertical film strip of n frames (§8.4.3). |
| `SKY=TRUE/1`, `SKY=FALSE/0`, `SKY?` | Make this element the sky backdrop (§8.4.4) / clear it. Not used by any tour. |
| `MOVE…` | Accepted, does nothing. |

#### 8.4.1 Icon strip

`2D.ICONFILE=3DVTICON.bmp` (identical file in all tours, 420×22, 24-bit, uncompressed) followed
by `2D.ICONS=20`. Cell width = strip width / ICONS = **21 px**, cell height = strip height = **22 px**.
Cell i covers x ∈ [21·i, 21·i+21).

The loader copies pixel rows **in file order as top-to-bottom** (no bottom-up flip). The shipped
bitmap is stored upside down relative to normal BMP convention, so this yields upright icons. A
converter using a standard BMP decoder must flip the image vertically. Only 24-bit BMPs whose
row length is a multiple of 4 bytes work (no padding handling); the shipped file qualifies.

Strip contents (file-order orientation), in triples normal/pressed/hover:
0–2 loudspeaker (audio), 3–5 TV (video), 6–8 picture frame (image), 9–11 four-way arrows,
12–14 globe, 15–17 clapperboard, 18–19 empty. "Normal" icons are light blue-grey, pressed/hover
white. Background of every cell is black.

#### 8.4.2 Caption rendering (`is_label` = 1)

Each caption owns an off-screen **512×32** surface; the quad samples `u ∈ [0, w/512]`,
`v ∈ [0, 22/32]`, with clamp addressing and point (nearest) filtering, so texels map 1:1 to pixels.

Two styles, selected by `color_key`:

* **Outlined caption** (`color_key` = 1, all `_L*`):
  1. Fill the surface with magenta (transparent key).
  2. If `icon_normal ≥ 0` and a strip is loaded, blit the current icon cell at (0,0).
  3. Text at x = 6 (or cell width if an icon is drawn), y = 4 (`voff`), font = global face,
     height 13 px (cell height, i.e. LOGFONT height +13), **weight 700**, transparent background.
     First the glyph outlines are stroked with a **2-px black pen** (path → widen → stroke), then
     the same text is drawn in **white** on top. Net effect: white bold text with a ~1–2 px black outline.
  4. No frame.
* **Hotspot box** (`color_key` = 0, all `_H*`):
  1. Fill the surface with the colour of the **top-left pixel of the current icon cell**
     (black for the shipped strip). Drawn opaque.
  2. Blit the current icon cell (21×22) at (0,0).
  3. Text at x = 21 (cell width; 6 if no icon), y = 4, global face, 13 px, **weight 400**,
     colour = current text colour (initially `text_color0` 0xC0D1E3; hover/pressed 0xFFFFFF).
  4. 1-px frame along the four edges of the w×22 rectangle in the current frame colour
     (initially `frame_color0` 0xC0D1E3; hover/pressed 0xFFFFFF).

Colours are stored 0x00RRGGBB and converted to Win32 BGR at draw time; treat them as RGB.

**Caption width.** Height is always **22 px**. Width:

```
w = Wm + 8 − trunc((Wm − adder) × scale)      // adder 82, scale 0.26 (defaults)
if color_key: w += 10
```

where `Wm` is the text width measured by the original **in the device context's default font**,
not in Tahoma: the measuring code creates the element's font but passes the wrong handle to
`SelectObject`, so the selection fails and the window DC's stock *System* font is used. The
`scale`/`adder` correction exists to bring System-font widths down to roughly Tahoma width.
Confirmed against the files: the authoring tool saved `ERS Label` boxes 89 px wide and
`ERS HotSpot` boxes 91 px, which matches System-font widths of ~68 and ~83 px (≈7.5 px per
character, the XP System font average) and does not match Tahoma (~50/~60 px).
The quad's right edge is set to `left + w`, bottom to `top + 22`.

**(VM)** The System font advance widths (GetCharWidth32 on the control's window DC, face "System", 0x20–0xFF)
were captured; with them the formula gives all 55 hotspot-box widths seen in the VM captures. The scale is a
float32: `trunc((Wm − 82) × 0.26f)` with `0.26f` = 0.25999999, so "Titus" (Wm 32) is 52 px wide, not 53. Labels
(`_L`) are white bold text with a 1-px black rim, drawn aliased. Icons show upright (TV, four-way arrows).
The original clips text that overflows `w`.

#### 8.4.3 Film strips (`NEXTFRAME`)

The element's image is treated as n frames stacked vertically. With `f = 1/n` and `v_top`
the current top texture coordinate:

```
k = trunc(v_top / f) + 1
if |(v_bottom − v_top) − f| ≥ 0.001: k = 0      // not yet showing a single frame
if k ≥ n: k = 0
v_top = k·f ;  v_bottom = (k+1)·f ;  if k == n−1: v_bottom = 1.0
```

`begin.32d` `_0` = `loadanim.gif`, 128×2048 = 16 frames of 128×128. `load.3sc` calls
`2D._0.NEXTFRAME(16)` once directly and then every 0.1 s from the alarm `LOADALARM`
(`SCENE.ALARM.NEW(1,-1,0.1,LOADALARM,2d._0.nextframe(16);scene.draw)`), i.e. a 10 fps
spinner (cadence subject to how the alarm/`;` parsing works, see §3). After
`2D.LOAD(lh.32d)` replaces the elements, `_0` no longer exists and the alarm's `NEXTFRAME` fails
silently until `SCENE.ALARM.LOADALARM.DELETE`.

**(VM) Loading screen.** Encarta's control is **616×420** (at screen (289,195), with a 1-px black frame drawn by the
host on a (49,69,107) background); the `begin.32d` rectangles are laid out for that size. While a tour loads the
control shows a black screen with the ring icon labelled "LOADING" on the left and the framed "How to navigate a 3D
Ancient Site" text on the right, for about **2 s** (Colosseum: 0.67 s to 2.65 s after the request; harness loads
took 1.6–2.3 s). The per-tour spread is not explained by the data: against the texture count (one texture
per tick, §5.4), the `.x` size, the texture pixels or bytes, the seven single loads (0.1 s resolution, disk mode,
not Encarta's ITS mode) leave residuals of 0.2–0.3 s for any one factor and still 0.19 s (Persepolis) for a
three-parameter fit, so a port keeps one minimum of about 2 s. **(VM)** Spinner: dumps every 15 ms during an Acropolis
load show the dot step 16 times, 90–120 ms apart, mean 110 ms: the 0.1-s alarm fires after 7 ticks (§3.5 restarts the
clock at each firing), about 9 frames per second.

#### 8.4.4 2D sky backdrop (unused by the tours)

If an element is flagged `SKY`, it is drawn first, before the 3D scene, as a full-viewport quad
at depth 0.9999 and excluded from the normal 2D pass. Texture coordinates follow the camera:
`u_c = yaw_deg/360`, `v_c = 1 − ((sin(elevation) + 1)·0.25 + 0.25)` (horizon at 0.5, zenith 0,
nadir 1), half-extent `h = FOV_deg/720` in both u and v; corners (u_c±h, v_c±h). Listed for
completeness; no `.3sc` uses `2D.*.SKY`.

### 8.5 Per-frame behaviour

#### 8.5.1 Draw pass

After the 3D scene, if the layer is ON, elements are drawn **in array order** (later on top),
skipping invisible ones and the sky element. 2D vertices sit at z = 1e-5, so they are always
in front of the scene. Images: wrap addressing, normal filtering, colour key if `color_key`.

Clipping against the viewport (width W, height H, in pixels): a quad is rejected if its right
edge ≤ 0, bottom ≤ 0, left ≥ W−1 or top ≥ W−1 (sic: the top edge is compared with the width).
Images that straddle an edge are clipped (texture coordinates interpolated). **Captions that
straddle any edge are not drawn at all.** **(VM)** A caption is drawn only strictly inside the view: left > 0,
top > 0, right < W − 1 and bottom < H − 1 (last drawn at left 1, right 614 and bottom 418 in the 616×420 control).

#### 8.5.2 Anchored elements

For each visible anchored element, every frame:

1. **Cone test** (FUN_10009ba0): let `d` = unit vector camera→anchor and `f` = unit view direction.
   The element is shown only if `dot(d, f) > FOV_deg × π/180 × 0.5`. Note the original compares a
   cosine with an angle in radians; for the tours' FOV of 80° the threshold is 0.698, i.e. the
   anchor must be within **≈45.7°** of the view axis. There is **no distance limit, no use of the
   anchor yaw, and no occlusion test**: captions show through walls. **(VM)** Confirmed with `CAMERA.FOV=100`
   (threshold 0.8727, 29.2°): a caption is shown at dot 0.8729 and hidden at 0.8692. At FOV 80 the 45.7° cone lies
   just outside the view's corners (45.4°), so only the edge rule of §8.5.1 is visible.
2. If shown: transform the anchor into camera space and project
   `sx = trunc(K·x/z + cx)`, `sy = trunc(cy − K·y/z)` (K = projection scale in pixels, cx, cy =
   viewport centre; same projection as the 3D renderer). Translate the whole quad so that its
   **top-left corner** is at (sx, sy). Size is unchanged.
3. If not shown, the quad keeps its last position but is not drawn.

#### 8.5.3 Mouse: hit test, hover, macros, cursors

Hit test: walk the elements from **last to first**; the first one with primitive 6, visible, layer
ON, and mouse inside [left, right] × [top, bottom] (inclusive) wins. Captions only count when they
were drawn unclipped this frame. Invisible elements are not hit; an anchored element that failed
the cone test but is still flagged visible keeps its stale rectangle and **can still be hit**. **(VM)** Confirmed:
hidden by the cone, a caption's last rectangle gives the hotspot cursor and a click fires `HOTSPOTCLICKED`. A
caption dropped at an edge cannot be hit.

Each input update (mouse position, button state) produces up to four events, run in this order:

| Event | Fires when | Hotspot (kind 1, has_text) side effect |
|---|---|---|
| MOUSEDOWN | button goes down over element E and E is not already the pressed element; E becomes "pressed" | icon = pressed (n+1), text+frame white |
| MOUSEUP | button is up, an element P is "pressed", and the element under the mouse is P; then pressed is cleared. Releasing elsewhere fires nothing. | icon = hover (n+2), white |
| MOUSEIN | hovered element changed to a new element | icon = hover (n+2), white |
| MOUSEOUT | hovered element changed away from a previous element | icon = normal (n), text+frame 0xC0D1E3 |

A side effect re-rasterises the caption. The macro string is executed by the normal script
interpreter (same as a `.3sc` line). For kind 2 (unused) the texture v-range switches between
quarter strips: normal 0–0.25, hover 0.25–0.5, pressed 0.5–0.75.

On disc the only macros are `_H*.MACRO.MOUSEUP=SCRIPT.EVENT(HotSpotClicked:hotSpotID=<id>)`
(119 statements) and the `_dot` debug mouse-down.

Navigation interplay: while the button is held on an element with `captures_mouse` = 1 the
camera does not move, and the hotspot cursor (`SCENE.CURSOR` slot 5, `vthot360.cur`) is shown.
When hovering an element with `has_text` = 1 (the `_H` boxes) the cursor is slot 5; otherwise
the normal navigation cursor logic applies (slots 0–4, §6).

### 8.6 Visibility on the disc

* `2D.LOAD(lh.32d)` leaves every element visible; `load.3sc` then runs `RUN(HIDEALL)` from
  `lhfuncs.3sc`, which sets `VISIBLE=0` on every `_L`/`_H` (and `_dot`).
* In the OCX nothing else writes the per-element visible flag: only `[id].VISIBLE…`, a `.32d`
  load and element deletion. `2D.ON/OFF/SHOW/HIDE` gate the whole layer; `TOGGLE` only reports.
  Anchoring does not recompute visibility (only the cone test above, which hides but never shows).
* No file on the disc and no string in ENCARTA.EXE or the resource DLLs contains `SHOWALL`, and the host only
  sends `2D.ON` / `2D.OFF`. **Visibility is driven by the walk maps instead.** Each walk-map *hotspot cell*
  (§6.2, §6.5) carries an enter macro such as `2D._L21.VISIBLE=1;2D._L4.VISIBLE=1;2D._H1.VISIBLE=1;…` and an
  exit macro, usually `RUN(HIDEALL);`. So captions and hotspot boxes appear while the visitor stands in the
  matching area and disappear on leaving it. Labels are therefore hidden right after load, until the first
  move enters a hotspot cell (loading a walk map does not run the enter macro of the cell under the camera,
  §6.5). The host's "Show Labels and Hot Spots" menu item toggles the whole layer with `2D.ON`/`2D.OFF`.
  **(VM)** No captions at the Acropolis start; captions after choosing a stop. But the host sends the default
  stop's command after loading (§2.2 step 4): where that command first moves far away and back (Colosseum, and
  every multi-floor tour), the start cell is entered again and its captions show at once. So "no labels until
  the first step" holds only where the default stop is a plain MOVETO to the start pose (Acropolis, Persepolis,
  Abu Simbel). **(VM)** Encarta 2009 has no "Show Labels and Hot Spots" item: the tour menu bar is Music, Tour Stops,
  Start Over, Appears in, Credits, the control has no context menu, and the File and Edit menus lack it. Only the
  strings (`&Show Labels and Hot Spots`, `Tour &Settings`) remain in ENCARTAR.DLL.
* Block macros: `.3sc` lines between `BEGIN(NAME);` and `END(NAME);` are concatenated with no
  separator (tabs removed, comment lines skipped), so statements without a trailing `;` run into
  the next one. In Acropolis `HIDEALL` this garbles `3d.n_00water.WATER=0` + `3d.Box23a.VISIBLE=0`
  + `2D._dot.VISIBLE=0;` into one failing statement; the 2D lines before them are unaffected.

### 8.7 Self-check values

| Tour | lh.32d elements | `_H` | `_L` | other | script 2D ids missing from lh.32d | of those given `TEXT` |
|---|---|---|---|---|---|---|
| 04257 Acropolis | 45 | 26 | 18 | `_dot` | 17 (incl. `_ZEUS2`) | `_L13` |
| 05285 Roman Forum | 26 | 16 | 9 | `_dot` | 13 | – |
| 05916 Beaumaris | 30 | 14 | 15 | `_dot` | 17 | – |
| 05917 Persepolis | 29 | 19 | 9 | `_dot` | 28 | – |
| 05918 Colosseum | 26 | 13 | 12 | `_dot` | 21 | – |
| 05919 Abu Simbel | 57 | 34 | 22 | `_dot` | 0 | – |
| 05920 Azuchi | 39 | 16 | 22 | `_dot` | 15 | – |

`begin.32d`: 4 elements (`_1`, `_0`, `_2`, `_3`) in every tour, byte-identical.
Every element in every `lh.32d` is referenced by at least one script. Statements naming missing
ids (almost all in the `HIDEALL`/`SHOWALL` lists, plus Roman Forum anchor edits) are no-ops; in
particular Acropolis `_L13` ("Mark of Poseidon's Trident") never exists, so that caption never
appears. Hotspots that exist but never receive `TEXT` (05285 `_H11`, `_H16`; 05917 `_H6`;
05919 `_H18`, `_H20`–`_H34`, `_L20`–`_L22`) keep the placeholder text, are at their file
positions (unanchored) and are hidden by `HIDEALL`.
Widths in files: all `_L` 89×22, all `_H` 91×22 (placeholder-text widths, §8.4.2).

Parse coverage: 14/14 `.32d` files parsed with zero unexplained bytes (begin.32d 1548 B ×7;
lh.32d 17386, 10104, 11631, 11250, 10093, 21994, 15037 B). Prototype:
`analysis/decompiled/tours-proto/layer2d.py`.

### 8.8 Needs dynamic confirmation in the XP VM

- [x] Captions appear per walk-map hotspot cell and, on load, only when the default stop command re-enters a cell (§8.6). [x] The "Show Labels and Hot Spots" item does not exist in Encarta 2009 (§8.6).
- [x] Caption widths: System font widths captured, the formula matches all 55 boxes in the captures (§8.4.2).
- [x] Hotspot box colours (VM: black fill, text and frame 0xC0D1E3, read back as (198,211,231) on the RGB 565 screen; white on hover; hand cursor). [x] `_L` captions: white bold, 1-px black rim, aliased; icon at x 0–20.
- [x] Icon strip orientation on screen: upright (TV, arrows), the icon each `ICON=` names.
- [x] Cone test (confirmed at FOV 100, §8.5.2); captions show through walls.
- [x] Captions touching or crossing the viewport edge disappear entirely (§8.5.1).
- [x] A hidden (out-of-cone) hotspot can still be clicked at its last position (§8.5.3).
- [x] MOUSEUP fires only when released over the same hotspot; the hotspot cursor shows over hotspots and stays during a press.
- [x] Event string case received by the host: upper-cased (VM).
- [x] Loading-screen layout and control size (616×420, VM). [x] Loading spinner cadence: 7 ticks (110 ms) per frame (§8.4.3).




---

## 9. Validation: parse coverage and self-check values

Every file of every tour was parsed by throwaway prototypes. They live outside the repository, in
`analysis/decompiled/tours-proto/` (`xfile.py`, `script.py`, `walkmap.py` + `wm_xmesh.py`, `lights.py`,
`clouds.py`, `vp.py`, `layer2d.py`, `crosscheck.py`), and are run with `python3 -I <script> extracted/eit/CONTENT/WORLD/7015`.

| Kind | Files | Parsed | Unexplained bytes | Notes |
|---|---|---|---|---|
| `.x` | 7 | 7 | 0 | 34,633,138 bytes, every token accounted for (§5.3) |
| `.3sc` | 36 | 36 | – | 3,554 statements, 15 macro definitions, 7 `RUN`. Every statement has a known subsystem. One merged statement caused by a missing `;` (§3.1) |
| `.3wm` | 44 | 44 | 0 | 17,285 cells: ground 12,052, visibility 2,725, hotspot 2,425, collision 83. Visibility masks are sized from the `.x` face counts. 721 hotspot strings carry authoring junk (§6.2 cleaning rule) |
| `.3lt` | 7 | 7 | 0 | 4.0 M list entries, all resolving to a valid mesh, face and slot. Junk after the name NUL is ignored (§7.2) |
| `.3cl` | 7 | 7 | 0 | v3 has a 4-byte trailer that the runtime ignores (§7.3) |
| `.32d` | 14 | 14 | 0 | Authoring junk in a few struct fields (§8.1) |
| `.3vp` | 3 | 3 | 0 | Meaning by observation only; unused at run time (§7.7) |
| images | 682 | 682 | – | 490 JPG, 151 BMP, 41 GIF. Content-sniffed. One texture reference has no file (per `roooof.jpg`, missing in the original too) |
| `.cur` | 84 | 84 | – | Standard cursors. `grip.cur` is referenced but absent |
| junk | 8 | – | – | 3 `error.log`, 3 `Thumbs.db`, 2 `.psd`: drop them, and list them as intentional omissions |

Total: 892 files in 7 containers, all classified (§5.7).

Quick self-check values for a Rust reader:

| Tour | meshes | faces | lights | `.3wm` cells (all maps) | lh.32d elements | hotspot macros |
|---|---|---|---|---|---|---|
| Acropolis | 1,542 | 20,059 | 71 | 1,713 | 45 | 26 |
| Roman Forum | 3,689 | 41,956 | 105 | 1,069 | 26 | 14 |
| Beaumaris | 2,321 | 59,640 | 228 | 4,308 | 30 | 14 |
| Persepolis | 1,344 | 52,269 | 80 | 2,511 | 29 | 18 |
| Colosseum | 2,843 | 63,108 | 299 | 5,873 | 26 | 13 |
| Abu Simbel | 531 | 42,477 | 128 | 618 | 57 | 18 |
| Azuchi | 7,315 | 63,513 | 203 | 1,193 | 39 | 16 |

Hotspot macros are `MACRO.MOUSEUP=SCRIPT.EVENT(HotSpotClicked:…)` statements, 119 in total.

---

## 10. Checklist for the XP reference VM

Ordered by how much parity depends on each item. Every section also has a detailed list (§5.9, §6.13,
§7.10, §8.8). Resolved items were measured in the reference VM ([vm-findings.md](../vm-findings.md)) and are
folded into the sections named; they are marked (VM) there.

Resolved:

- [x] **Tick rate and speeds.** ~64 ticks/s idle, ~47 walking; walk 100 × `SPEED.FLY` units/s and turn 200 × `SPEED.TURN` °/s whatever the tick rate (§2.3, §6.9).
- [x] **Pitch.** Positive looks up; exactly `PITCHSPEED` per tick; a step reaching |pitch| ≥ 45 is refused (tested on the look-vector value); Home sets 0 (§6.1, §6.9).
- [x] **Pitch limit arithmetic** (debugger): the test is on the pitch derived from the float32 look vector, which ground following's per-tick fall and landing make drift away from the stored pitch `PITCH?` reports (§6.8, §6.9).
- [x] **Gravity mode** on the software device: walker +0x54 = 1 in Encarta; falls grow by GRAVITY per tick (§6.8).
- [x] **Collision.** Wall sliding by projection, full stop in corners (§6.7).
- [x] **FOV.** Horizontal: FOV 80 spans the 616-px control width (§6.10).
- [x] **Hotspot click.** `destmedia` of the hotspot record replaces the main view; Back returns at the same pose; events arrive upper-cased (§4, §4.3, §3.6).
- [x] **Tour content record.** Stops, default stop, `coord3d`, hotspots and `destmedia` (§4.3).
- [x] **Label visibility on load.** Captions appear on load only when the host's default-stop command re-enters a hotspot cell (§8.6).
- [x] **Captions colours.** Black fill, 0xC0D1E3 text and frame, white on hover, hand cursor (§8.5.3).
- [x] **Site map** for an unrotated map (§4.2).
- [x] **Loading.** Control size 616×420, loading-screen layout, about 2 s on screen (§8.4.3).
- [x] **Leftovers.** `SCENE.HALFREZDRIVING` returns `Failed`; `Do()` replies (§2.2); spaces in values are lost (§3.2); the MOVETO-to-the-same-position NaN quirk (§6.10).
- [x] **Texture orientation.** Surfaces are bottom-up for every image format: `v' = 1 − v` in glTF (§5.6), checked at all 40 Tour Stops.
- [x] **Cloud statue orientation.** Upright, not mirrored, correct side (§7.3.3).
- [x] **Site map** with the rotated frames of Persepolis and Azuchi (§4.2): marker within 1 px and wedge within 2° at all 8 stops, floor images per stop confirmed. The marker blinks dark/light blue every 0.715 s (§4.2).
- [x] **Floors.** An enter macro that switches walk maps switches once; the old map's exit macro does not switch back (§6.5).
- [x] **Flicker.** Steps after 7 ticks (0.1 s) and 26 ticks (0.4 s): the accumulator restarts at 0; Persepolis and Colosseum torches and Abu Simbel `_TORCH36`/`39` are steady (§7.2.6).
- [x] **Lighting.** faceColor is ignored; texel × lit colour with no overbright; Abu Simbel's ambient 0 leaves unlit faces black (§7.2.3, §7.2.5, §5.4).
- [x] **Point-cloud statues.** Re-rendered on every draw in software mode; frozen at 0.9 × FOGEND (§7.3.4). Orientation is resolved (§7.3.3).
- [x] **Sprite billboards.** Rotation by the camera yaw only, same sign; flame strips step every 5 ticks (about 12.4 fps) from a random frame (§7.4.1, §7.4.4).
- [x] **Captions.** System font widths captured and the width rule confirmed (float32 scale); `_L` outline 1 px; icons upright; the cone (at FOV 100); dropped unless strictly inside the view; a cone-hidden caption stays clickable; no "Show Labels and Hot Spots" item in Encarta 2009 (§8.4.2, §8.5, §8.6).
- [x] **LOD.** The pop at 13.0 XZ units; none over a visibility cell, where the PVS lists choose (§7.6, §6.6).
- [x] **Water ripple** on `n_00water` in cells 1145–1148 only (§7.4.6). Its step rate is not measured.
- [x] **PVS.** Same view with `WALKMAP.DELETE.VISIBILITY` apart from the per-cell LOD choice; sprites in no list are drawn (§6.6).
- [x] **Back-face culling.** Back faces are culled (§5.9).
- [x] **Cursor zones** as specified; `grip.cur` never appears (§6.9).
- [x] **Open edges.** Leaving the collision coverage does not block a move (§6.7).
- [x] **Loading spinner cadence** (7 ticks per frame, §8.4.3). Junk in hotspot strings has no visible effect (§6.2).

Still open:

- [ ] The hardware 5° cloud re-render step and the maximised (MaxView) window (§7.3.4): no 3D device in the VM.
- [ ] **Water ripple step rate** (§7.4.6) and **`3D.TOGGLEHIDDEN`**'s effect on drawing (§7.4.3).
- [ ] **ITS-mode loading** in the harness.

How to capture: a debugger breakpoint on the control's `Do` method (`FUN_1000d0f0`) and on the event fire
(`FUN_1000e720`) logs every command and event (cdb is in the VM's `flash-ready` snapshot; `vm/dbg.sh`, docs/vm.md;
the control is `ACTIVEX3DX3`, at 10000000 in tourhost and relocated in ENCARTA.EXE). Alternatively, the control's own `STARTMACROLOG` writes
`macro.log`, but it must be issued through `Do`.

---

## 11. Remaining unknowns

- *Resolved:* the site-map artwork and its named locations come from the stops' `children/site` records (§4.2).
- Per-vertex normal source and face-centre definition used by lighting and sprite pivots (§7.11).
- The control's default FOV when a script sets none (every tour sets 80, which is horizontal, §6.10).
- The exact edge rule of the walk-map grid rasteriser (only matters within one grid unit) (§6.14).
- The second gravity-mode flag and the vertical-fly flags (§6.14).
- `3D.LODDISTANCE` (picking-only per §7, additive switch distance per §5). It is 0 on the disc, so there is no visible effect.
- `3D.CALC64BIT` (software-rasteriser precision only, per §7.5; the `.x` loader never reads it).
- A few `.32d` struct fields that are never read (§8.1).
- Colour-key filtering artefacts in hardware mode (§5.10).

---

## 12. Implementation

Module `encarta_formats::tours` (`crates/encarta-formats/src/tours/`), written from this document:

| Module | Contents |
|---|---|
| `xfile` | `.x` token stream → generic object tree → `XFile { header, materials, meshes }` with the fixed template shapes of §5.2; stats for §5.3, `lod_pairs`, material lookup, script-name lookup, face counts |
| `script` | `load_lines` (§3.1 loader: CR line ends, TAB removal, column-1 comments, glued `BEGIN`/`END` blocks), `parse_command` (§3.2–3.4: help, macro log, define, delete, run list, statements), `Statement` (folded text, subsystem, object id, property path, op, value) with helpers for scene keywords (prefix match), C-style numbers, flags, `^…^` text, events, alarms and file references |
| `walkmap` | `.3wm` v2/v3, sized by the model's face counts; hotspot strings split into script and junk (§6.2 rule); XZ point-in-polygon lookup and the 4096-grid coordinates |
| `lights` | `.3lt` v1–3, face references, masks, paint; the §7.2.3 light factor |
| `cloud` | `.3cl` v2/v3 |
| `layer2d` | `.32d` header and element records, including the unread fields |
| `viewpoints` | `.3vp` |
| `textures` | content sniffing, image sizes, uncompressed BMP decoding, the 555/565 magenta key and keyed RGBA, the §5.5 resolution rule, `.cur` directories |
| `tour` | `Tour::load(dir)`: runs `load.3sc` in order (nested `SCENE.LOAD`, `RUN`, macros, alarms), parses each referenced file once, then checks deferred strings (unrun macros, alarm macros, walk-map hotspot strings, 2D element macros) against the final state, sniffs every image and cursor, and cross-checks lights, paint and visibility masks against the model. Problems are `Issue`s: dangling files, silent no-ops (unknown mesh/light/element/macro/alarm ids, statements with no subsystem) and structural errors |

CLI `tour` (`src/bin/tour.rs`): `tour info <dir>` (counts), `tour check <dir>` (all issues; exit 1 on a
structural error), `tour dump <dir> <kind|file> [--pretty]` (JSON; kinds `summary files scripts executed
deferred macros events model model-stats materials textures images cursors lights walkmaps layers clouds
placements viewpoints issues unreferenced inventory`, or any file of the container). `<dir>` is one
`…/7015/<id>.EIT/` folder, or `…/7015/` for `info` and `check`.

Integration tests `tests/tours.rs` assert, over all 7 tours: the §5.7 catalog (892 files), §5.3 geometry
table and extras, §5.5 texture resolution and keyed BMPs, the two dangling file references (`grip.cur` in
every tour, Persepolis `roooof.jpg`) and the unreferenced images, every row of the §6.2 walk-map table and
its totals, §7.2.2 lights, §7.2.6 flicker hits, §7.3.1 clouds and placements, §7.7 view points, §8.7 layers,
the §3.8 statement totals (3,554 statements, 15 macros, 7 `RUN`, 119 hotspot macros), the §7.8/§8.7 missing
ids, and the hotspot-string findings of §6.2.
