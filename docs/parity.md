# Parity audit (plan Step 8)

Feature-by-feature checklist of Encarta Premium 2009 against this project, and the "no content lost"
check of every data family in [formats/inventory.md](formats/inventory.md). Audit of 2026-10-14 against the
converted disc (`out/`, content schema 2) served by `encarta-server`, with the production build of `web/`.

**Sources of the feature list.** The VM home screen (`fixtures/vm/home.png`: menu bar File / Edit / Favorites / Help;
tool bar Back, Forward, Home, Features ▾, search box; nine home links); the 27 feature records (`group="feat"`:
Dictionary, Thesaurus, Translate a Word ×8, Translations, Timeline ×2, College Prep Online, Geography Quiz, Encarta
Kids, Quick Search ×6, Encarta on the Web ×2); [vm-findings.md](vm-findings.md); the article, tour, atlas, geoquiz and
timeline captures (indexes in `fixtures/*/INDEX.md`, private); the specs ([web-client.md](web-client.md),
[atlas-client.md](atlas-client.md), [games.md](games.md), [tour-runtime.md](tour-runtime.md), [api.md](api.md),
[content-schema.md](content-schema.md)). The Features menu itself and the dialogs (Options, Help) were never
captured, so features known only from the data are marked as such.

**Status.** **done**: present and behaves as the VM or the data shows. **partial**: present, with a gap written next
to it. **missing**: not present. **n/a**: not applicable to Encarta 2009 (not reachable in 2009, or an online service
that closed in 2009). *(ours)* marks behaviour chosen here where the original is unknown.

## Summary

| | done | partial | missing | n/a |
|---|---|---|---|---|
| Features (134 rows below) | 106 | 16 | 4 | 8 |

Fixed in this audit: 25 items (see [Fixes made](#fixes-made-in-this-audit)). The remaining gaps, in
priority order, are the "Next" list in [PLAN.md](PLAN.md).

---

## Home, menus and tool bar

| Feature | Status | Evidence | Gap |
|---|---|---|---|
| Home screen: logo, butterfly, background, nine links in three columns | done | `/`, data item 701712539; compared with `fixtures/vm/home.png` | — |
| Home links → category screens (History … Sports & Recreation, Games & Fun Stuff) | done | `/browse/{id}`; 7,888 `home-page` items, 50/50 sampled render links | — |
| Home links Dictionary, Encarta Kids | done | `/dictionary`, `/kids` | — |
| Menu bar File / Edit / Favorites / Help | done | `MenuBar` in `app/Shell.tsx`; entries and keyboard shortcuts (`app/shortcuts.ts`) per the VM (2026-10-14) | File › Go to is a flat group (Back / Forward / Home Page), not a submenu; Edit › Read Aloud enabled, Pause becomes Continue (UI pass 2); File › Exit left out (a page cannot close its window) |
| File › Print, Edit › Copy | done | Encarta's scope dialog (whole text / with media / selection), `app/ScopeDialog.tsx`, `commands.ts`; a copyright line is always appended (VM 2026-10-14) | — |
| Edit › Copy (with a source line) | done | `app/commands.ts`; VM line "Microsoft® Encarta® 2009. © 1993-2008 Microsoft Corporation. All rights reserved." on the next line after a selection, after a blank line for a whole article or entry; Ctrl+C in articles adds it too (UI pass 2) | the full-article dialog offers Whole Article With Media: HTML of the text and side columns (pictures, absolute URLs) plus the text and source line (not run in a browser) |
| Edit › Find in Article | done | menu entry → find bar (this audit); Ctrl+F | — |
| Help (Encarta's own help files) | partial | `/help`: about + keyboard shortcuts (this audit) | the disc's three CHM files (Windows UI help) are omitted by design (media.md §4) |
| Back / Forward orbs with history state | done | `NavButtons` | — |
| Home orb | done | | — |
| Features ▾ menu | done | `FEATURES`, keyboard menu (this audit) | — (order matches the VM capture) |
| Search box with autocomplete in the tool bar | done | `SearchBox`, `/api/suggest` | — |
| The ▾ tab under the search box, docked results panel (VM home) | done | `app/SearchPanel.tsx`, `Shell.tsx` (2026-10-14, vm-findings › Search box): Enter opens the right-docked panel with Best Matches / Articles / Photos & More / Web Links / Dictionaries / Maps / Quotations bars, one open at a time (the last one stays open for the next search), counts, "Search all" links, the two check boxes; the tab toggles it; a page click hides it unless "keep open"; opening a hit leaves it open | colours and geometry as measured (378 px, 308–498 px splitter, 32 px bars, first hit green), Articles titles only, empty categories show only a message; Quotations is not in the VM's panel, so only in Quick Search (UI pass 2) |
| Window title / document title per page | done | `useDocumentTitle` | — |

## Features menu (feature records)

| Feature | Status | Evidence | Gap |
|---|---|---|---|
| Dictionary (701712323, 701767247) | done | `/item/{id}` → `/dictionary` (this audit routes all 27 records) | — |
| Thesaurus (701712324, 701767248) | done | → `/thesaurus` | — |
| Translate a Word ×8 (`BA2E` … `BI2A`) | done | → `/translate?pair=ENG_SPA` etc. (this audit) | — |
| Translations (701767016) | done | → `/translate` | — |
| Timeline (701712334, 701779281) | done | → `/timeline` | — |
| Geography Quiz (701712336) | done | → `/games/geoquiz` | — |
| Encarta Kids (701712340) | done | → `/kids` | — |
| Quick Search: Articles / Maps / Photos & More / Quotations / Dictionary / Thesaurus | partial | → `/search?kind=article`, `?kind=map`, `/media`, `?kind=quotation`, `/dictionary`, `/thesaurus` (this audit) | Maps and Photos & More open the VM's small modal "Quick Search" dialog (one box with the word wheel; Enter opens the panel with only that category, `app/QuickSearchDialog.tsx`); the other entries still open the scoped search pages |
| World Atlas, Media Gallery, 3D Virtual Tours, Games & Quizzes | done | the Features menu now follows the VM's groups and content ids; no menu lists Games & Quizzes in the VM; they are under Home › Games & Fun Stuff, and our extra `/games` pages are linked from the bottom of that page (UI pass 2) | — |
| College Prep Online (701712335) | n/a | online service | page says so (`OnlineFeatureView`) |
| Encarta on the Web (701835573, 701882633) | n/a | online service | page says so |
| Extras: MindMaze, City Climate, Population Chart | n/a | not reachable in 2009 (vm-findings.md); rebuilt as extras | — |

## Article reader

| Feature | Status | Evidence | Gap |
|---|---|---|---|
| Encyclopedia layout: title, sections with numbers, (−)/(+) toggles | done | `ArticlePage`; 50/50 sampled articles render | — |
| Show Outline / Show Full Article | done | VM `761575094-outline`, `761563737-outline` | — |
| Find in Article | done | CSS Highlight API | — |
| Media strip, Show All Media | done | | — |
| Inline media placement (text column, margin, clusters) | done | schema 2; 100 % paragraph, order and cluster match (1,660 of 1,660 items) against the 25 encyclopedia article captures (`placement_matches_vm_captures`, exact); country introductions and overflow into the next section reproduced; the reader's reserved Archives and Further Reading slots (rule 5, VM pass 3) explain the last four sections | needs `encarta-convert content` re-run |
| Quick Facts / Facts and Figures panel | done | | — |
| World Atlas panel → map at the place | done | | — |
| Related Articles tree | done | | — |
| Further Reading | done | | — |
| Web Sites panel (right column, VM `761568934`) | done | `LinksPanel` with 8 shown + "Show all" (this audit); 31,976 links on 17,612 items | — |
| Contributors ("Contributed By") | done | | — |
| Sidebar teasers and sidebar layout | done | VM `631522251` | categories `SBA`/`SBP` unconfirmed |
| Inline tables (`tableinline`) | done | box border and header fill from the capture CSS | |
| "X or Y" alternative titles | done | VM `461510929` | — |
| Specialty and private-use glyphs | done | content-schema §7 | — |
| Pronunciation speaker | done | | — |
| Quick-verify links (blue #314dce) | done | | — |
| Short articles inside longer ones (open the containing article) | partial | VM: Al-Karaji → Mathematics | not verified in the client (S) |
| Quotations (29,705) | done | 50/50 render | — |
| Quotation authors (6,196) | done | `AuthorView`: dates, nationality, designation, their quotations (this audit; was "Nothing to display") | — |
| Contributors as items (2,034) | done | `ContributorView` (this audit; was empty) | 9 % have no articles in this edition |
| Yearbook (archive) articles (18,707) and Homework Starter intros | done | 50/50 render | how 2009 browsed them was not captured |
| Citation guides (`quickstart`, 2) | done | render as articles | — |
| Essay bars' picture on the timeline | done | 24 px thumbnail of the essay's first media child (`picture` in `/api/timeline?text=1`) | VM draws it 64 px; ours is smaller to fit the lanes |

## Media

| Feature | Status | Evidence | Gap |
|---|---|---|---|
| Images (media 30,122, composites 425) | done | 50/50 each; AVIF/WebP by `Accept` | — |
| Audio (Opus/AAC), audio sets with picture | done | 50/50 audio sets | — |
| Captions under audio (SAMI → WebVTT) | done | | — |
| Video with captions (414) | done | 50/50; VM `461546454` | — |
| MIDI music (236) | done | own SMF reader and soundfont synth over Fluid R3 GM (media.md §8); offline renders of 20 disc songs follow FluidSynth's loudness envelope within about 1 dB (web-client.md) | compared with the GS Wavetable synth in the VM (media.md §8): tempo, onsets, programs, velocity, bend, volume/expression match; pan (balance law), mod wheel (6 Hz on held notes), pedal threshold, release by family and cymbal/kick/snare levels now follow GS. Still different: Fluid's stereo sample pairs, velocity above 96, timbre, timpani length; our reverb and 0.3 s ring-out kept by choice |
| Interactives (860, Flash IAF player under Ruffle) | done | 50/50 open a player; Ruffle test pass: 46 of every control type and both editions draw and respond, disc fonts as device fonts (web-client.md) | Trebuchet MS approximated by Tahoma (VM, Flash 10.3, seven items: drawing matches, some paragraphs wrap one line differently, nothing extra clipped); Vasco da Gama's ocean fill missing once in an early headless run, drawn by Flash and in every run since |
| Match games natively | done | games.md | — |
| Panoramas (151, `surround`) | done | native cylinder player (`web/src/panorama/`) in place of the Flash `vrslide` viewer, which is blank under Ruffle | — |
| Maps as pictures (99,707) → "Open in the World Atlas" at the place | done | links to `/atlas/place/{geocode}` (this audit; was `/atlas`) | — |
| Charts, tables (2,508 + 266 facts) | done | 50/50 | — |
| Media viewer: prev/next, strip, Esc, appears-in, focus trap | done | `MediaViewer.tsx`, `components.test.tsx` | — |
| Media Gallery by type | done | `/media` | — |

## Dictionary, thesaurus and translation window

| Feature | Status | Evidence | Gap |
|---|---|---|---|
| English dictionary, word wheel, homograph numbers | done | VM `dict-paris`, `dict-encyclopedia` | — |
| Thesaurus | done | VM `dict-thes-happy` | — |
| Eight bilingual dictionaries, reverse links | done | VM `dict-trans-house` | — |
| Pronunciation audio | done | | — |
| Copy | done | `EntryTools` (this audit): entry or selection with source line | the dictionary's Copy copies a selection alone, without the line (VM) |
| Print | done | prints the entry alone (print style sheet) (this audit) | — |
| Parental Controls (hide offensive words) | n/a | dropped by choice (PLAN.md): nothing is hidden, as on Wikipedia | — |
| Separate window (EDICT.EXE, 765×480) | n/a | a page here | — |

## Search

| Feature | Status | Evidence | Gap |
|---|---|---|---|
| Full-text search with category tabs | done | tantivy, 586,702 items | — |
| Autocomplete (word wheel, aliases) | done | | — |
| Did you mean / spelling from the disc lexicons | done | | — |
| Deep link to the best paragraph | done | | — |
| Kids search | done | | — |
| Encarta's own ranking | partial | ours (BM25 + priors); compared on 8 VM queries (vm-findings › Search ranking): top-1 7/8, top-6 overlap 20/48 after two fixes (no Kids-only items without `edition`, "Rome (Georgia)" counts as an exact "rome") | Encarta lists index-term aliases ("Ass, related to zebra") and matches whole words only; re-run `encarta-convert search` for the fixes |

## Timeline

| Feature | Status | Evidence | Gap |
|---|---|---|---|
| Band with points, periods, essays; sizes by importance | done | VM `fixtures/timeline/*` | — |
| Eras, zoom, pan, category and region filters | done | | — |
| Event pop-up with date line "4? BC - AD 29?" | done | VM `jesus-popup.png`; full dates and the "years ago" wording before 8000 BC ("120,000 years ago", "2.50 million years ago", "4.60 billion years ago") from 23 VM pop-ups (timeline.md §3.2, 2026-10-14) | re-run `encarta-convert content` for the new `date_text`; the exact cut between 9000 and 8000 BC is not observable |
| Topic timelines (28 named + 13,033 search targets) | done | `/timeline?term=` over TIMELINE.DAT terms (this audit; `timeline_term` was converted but unused, topics opened a thumbnail) | 1 in 50 sampled targets has neither a term nor a matching event title |
| Timeline items open in the timeline (`msencarta://id=`) | done | an event item redirects to `/timeline?event=` (window fitted, pop-up open) | — |
| Event pop-ups with media (Flash in 2009) | done | pop-up shows the first media child's picture (links to its page) and lists related links (VM `jesus-popup.png`) | the 2009 Flash pop-up chrome is not reproduced |

## World Atlas

| Feature | Status | Evidence | Gap |
|---|---|---|---|
| Globe, Miller flat view, measured P/m | done | atlas-client.md, VM V1/V2/V12; flat-view tiles made in Web Workers | — |
| 20 menu styles + statistical, moon | done | VM V4 | 4, 18, 21–23, 26 have no MapStyle |
| Labels, symbols, road shields | done | road shields are the labels of shield road classes, numbered by name (mad-styling.md §8.1, matches VM V8); alternative names (U+0004) on a `w6` line (§7.3); pen draw-over masks on lines and fills (§5.2) | the alternative line is confirmed by V15 (Mumbai/Bombay, Bolzano/Bozen) with a rule between the lines, drawn by "rule" glyph faces (atlas-client.md; about 2 px lower than the VM, names with an alternative do not wrap); point labels try MOBB's candidates in its order (above right, below right, above left, below left; centred first without a symbol; §7.4, from the code), but MOBB picks the cheapest candidate by what lies under it and its neighbours where MapLibre takes the first that fits; Encarta puts one shield per run near its middle, ours repeat at a spacing (§8.1) |
| Click menu with Map / Read Article | done | VM V11; pen group 134 (time zones) left out, as ENCARTA.EXE does (mad-styling.md §16), confirmed on the Time Zones map itself (V16: Western Europe, West Central Europe, France, Europe); no land area is listed over the sea (VM: only "Bay of Biscay" 1-3 km off Brittany; lakes still list their country): area classes are split at the coast and only land parts are hit-tested (atlas-output.md); menu order from MOBB's sort (§4.5, VM pass 3): label hits, points, routable roads, lines, areas, then class scale ascending and GeoObj descending, matching every captured menu; the time-zone exclusion now goes by row (it went by class id, which listed time zones and dropped the Mexico/US shield roads) | needs `ENCARTA_ATLAS_ONLY=style` and `=vector` re-runs |
| Place links (P2MTBL) | done | 206/206 sampled links open (this audit: 2,656 off-disc targets are left out instead of "Item not found"; 1,229 MSWORLD.ITR photos and sounds now open) | 910 of those photos show only their 64-px pushpin picture; 152 sounds have no audio on the disc (msworld.md §3) |
| Place search | done | gazetteer FTS | — |
| Map Legend, Map Customizer | done | | legend captions are class names |
| Statistics: Choose Statistic, table, details, hover | done | VM V14 | binning unknown |
| Locator globe | done | | — |
| Map Trek (171 treks, 680 stops, from the Geography pages) | done | `MapTrekView`: full-height atlas plus the VM's modeless Map Trek panel (`maptrek/MapTrekPanel.tsx`: banner, stop text and thumbnail, stop list, Stop n of N, New Trek / Back / Next, close button; sizes and colours as in the VM, UI pass 2) | — |
| "About this map" notes (1,516) | done | render as articles | — |
| Live weather / news links (147 `livemedia`) | n/a | MSN services | shown as link pages with the 2008 address |

## 3D virtual tours

| Feature | Status | Evidence | Gap |
|---|---|---|---|
| 7 tours: walking, collision, ground following (gravity mode), lights, statues, captions | done | 47 views match the VM read-back (tour-runtime.md); gravity fall and the drifted pitch limit as traced with the debugger; the water ripple steps once per drawn frame below 80 fps (cdb: 631 steps in 641 frames at 64 fps) and `CLEARBACKGROUND=0` keeps the colour buffer, which is what `3D.TOGGLEHIDDEN` shows (2026-10-15) | hardware cloud step and MaxView (no 3D device) |
| Tour page layout, site map, Tour Stops menu | done | VM `701504257/stops` | rotated maps unverified |
| Labels and Hot Spots toggle | done | | — |
| Hotspot → media page, Back restores pose | done | VM | — |
| Start Over | done | default stop (this audit) | — |
| Appears in | done | menu of the articles linking the tour (this audit) | — |
| Music | done | heard in the VM (2026-10-14): on at start once the world loads, plays and loops the tour's "Ambient Audio" record, off stops, on restarts from the beginning (`TourView.tsx` `MusicToggle`) | — |
| Credits | done | | — |
| Tour nodes as items (search hits on stops, hotspots, labels) | done | open the tour at the stop (this audit; showed "Virtual tour stop.") | — |

## Panoramic virtual tours (2D)

| Feature | Status | Evidence | Gap |
|---|---|---|---|
| 16 tours (Westminster Abbey, Alcatraz, USS Missouri …): 65 scenes, labels, hotspots, site maps, scene pictures | done | `PanoramaTour` (this audit; before it no page showed them): wrapping panorama strip, labels at their `x`,`y`, hotspots (`shed`) to scenes or pictures, site map with scene points, scene text | the tour viewer has no zoom and no keyboard control in the VM (wheel, +/−, arrows, Page Up/Down tried, 2026-10-14); ours keeps them as extras *(ours)*; single panoramas open `initangle` degrees right of the picture's middle and auto-pan at about 26 °/s, as Flash 10.3 does in the VM (2026-10-15) |

## Games and quizzes

| Feature | Status | Evidence | Gap |
|---|---|---|---|
| Geography Quiz: 4 levels, 20 questions, 5 buttons, points | done | VM `fixtures/geoquiz` | — |
| Geography Quiz locator globe, zoom slider, hand tool; after-answer and end screens | done | `games/geoquiz/QuizMap.tsx` on `<Atlas quizMode>`; Scores dialog after question 20 with the all-level table; Try Again reopens the start dialog | |
| Quizzes (1,811 SWQUIZ) | done | native quiz player per question group; inside the `BGAControl` boards under Ruffle (questions with media, rewards); all quiz media resolve; all ten boards played to an ending; picture answers show (games.md) | — |
| Match games (64) | done | native | — |
| MindMaze, City Climate, Population Chart | n/a | extras (not in 2009) | — |

## Encarta Kids

| Feature | Status | Evidence | Gap |
|---|---|---|---|
| Kids home with ten panels, mouse-over pictures | done | | — |
| Kids category screens | done | | — |
| Kids articles (orange title, tile grid, no numbers) | done | VM `kids-*` | — |
| Kids search, Kids dictionary | done | | — |
| Kids interactives and games (264, Flash) | done | Ruffle (18 Kids copies in the test pass); match games native | — |
| Kids' own menu bar, Favorites, Help | done | `kids/KidsShell.tsx` (2026-10-14): lime bar with Home/Back/Forward/Copy/Print orbs and File / Edit / View / Favorites / Tools / Help with the VM's submenus (Go to, Closed Captioning, Read Aloud, Menu and Button Sounds, Startup Sound, Troubleshooting; `fixtures/ui/kids/sub-*.png`) | Read Aloud uses the browser's speech synthesis (no highlighting while reading, as in the VM); yellow-arrow tooltips (`ArrowTip.tsx`, also on the adult orbs; Back says "Back to <page>"; position checked through the DOM only); Kids Copy copies the selection or the whole article without a dialog (UI pass 2) |

## Interactives

| Feature | Status | Evidence | Gap |
|---|---|---|---|
| Slide shows, narrated slide shows (57), Flash maps (25) | done | IAF player under Ruffle; labels, captions, narration buttons tried | — |
| Drag-and-drop, cross-fade, ZFL, RFL, BGA controls | done | Ruffle test pass: drag, slider, zoom/pan/locator, element pick, boards (web-client.md); all ten BGA games played to an ending (games.md) | BGA picture answers blank |

## Favorites, print, copy, export

| Feature | Status | Evidence | Gap |
|---|---|---|---|
| Add to Favorites (any page, with its deep link) | done | `app/favorites.ts`, localStorage; Add to Favorites… opens a Name dialog ("Article - <title>"), as in the VM (UI pass 2) | per browser *(ours)*: 2009 kept them per Windows user; folders are one level deep (the menu cascades one level); not run in a browser |
| Favorites list in the menu (20) and Organize Favorites (folders, rename, reorder, move, remove) | done | `/favorites`; folders (New Folder, rename, delete with contents) as submenus of the menu; Add to Favorites has "Create in:" and New Folder… (VM favorites captures) | one level only |
| Print with a print style sheet | done | `styles/print.css` (this audit): no chrome, strip, tools or viewers; right column after the text | — |
| Copy with source line | done | see Edit › Copy | — |
| Export / Save As, Send to Word (Works task launcher) | n/a | `EE/WORKS` omitted (media.md §4) | — |

## Settings

| Feature | Status | Evidence | Gap |
|---|---|---|---|
| Options dialog (5 pages: General, Encarta Updates, Closed Captioning, World Atlas, Quality) | done | `app/OptionsDialog.tsx`, `settings/settings.tsx` (VM 2026-10-14): text size, captions (audio and video), atlas crosshairs, lat/long format, units, linear stats are wired | startup sound, rollover tick, press/release click, Kids rollover and timeline sounds play as measured in the VM (`settings/uiSounds.ts`; media.md §9); menu and button sounds default to off, as the original's Default button sets; 578/579 play on tour site-map stops (3D tour canvas and Virtual Tour points), dialog buttons tick on rollover; the other unheard ids are never played by the original (media.md §9) |
| Kids / Premium switch | done | `/kids` | — |

## Accessibility

| Feature | Status | Evidence | Gap |
|---|---|---|---|
| Keyboard menus (Features, menu bar, tour Appears in): arrows, Home/End, Esc, ←/→ across the bar | done | `components/Menu.tsx`, tests (this audit); the Features menu could not be used from the keyboard before | — |
| Skip to content link, `main` landmark focus target | done | (this audit) | — |
| Focus outline (`:focus-visible`) | done | `base.css` | — |
| ARIA: menus, dialog, tabs, live status for menu actions, labelled hotspots | partial | media viewer traps Tab and restores focus; Timeline band is a labelled group with a skip link to the date-ordered list, events carry aria-labels; axe-core (jsdom, no colour contrast) clean on Timeline, Options and the viewer | Ruffle content is opaque to screen readers; axe not run in a real browser (contrast) |
| Alt text on pictures | partial | titles where known | many decorative images have empty alt (correct); thumbnails in strips use titles |
| Panorama and Map Trek keyboard use | done | ←/→ in the panorama, buttons for stops (this audit) | — |

## Content not lost

See the next section for the per-family table; summary rows:

| Check | Status | Evidence | Gap |
|---|---|---|---|
| Every catalog id converted or explained | done | content-schema §10: 634,247 ids, 0 unexplained | — |
| Every item kind opens a non-empty page | partial | 50 per kind through `/api/item` (table below) | 1,083 empty audio stubs (index terms are search results) |
| Every converted table reachable | partial | | `index_tree` unreachable; `atlas_media` and `timeline_term` made reachable in this audit |

---

## No content lost: data families

Each family of the inventory, whether it is converted, and the UI path that reaches it.

| Family (inventory.md) | Converted | Reached from the UI by | Status |
|---|---|---|---|
| Media containers (80 EITs, CATALOG.DLX) | yes (437,049 files, 127,255 unique media) | article media, viewer, gallery, search | done |
| Images, audio, video, Flash, MIDI | yes | media viewer, item pages | done |
| Article bodies (CONT*.AKC) | yes (102,223 articles) | search, links, browse pages, related trees | done |
| Article metadata, related articles (DATA, REL) | yes | article panels, item pages | done |
| Index terms (DATAF, 180,545 items, 212,631 trees) | yes (as search results; 2009 has no index browser) | search results that expand to their entries (`/api/index-term/{id}`); terms with ≤ 30 targets are search aliases | done |
| Sidebars (MD*.EIT) | yes (2,696) | teasers, search | done |
| Dictionary, thesaurus, translation (EDICT) | yes (292,642 entries) | dictionary pages, search | done |
| Interactives (`.iax` + SWF) | yes (721 + 860 items) | media viewer (Ruffle / native) | done |
| Catalog | yes | (internal) | done |
| Titles, aliases, section heads (FINDDLX) | yes | search, autocomplete | done |
| Search exports (`search_entry` 3,084,729, `search_word` 1,175,993) | yes | not used: search is rebuilt (inventory.md "Not needed") | n/a by design |
| Timeline (TIMELINE.DAT + EIT) | yes (16,465 events, 22,749 terms) | Timeline; terms through topic timelines (this audit) | done |
| Atlas rasters, vectors, places | yes | World Atlas | done |
| World statistics, flags (MSWORLD.SDB, ITR) | yes | statistical style; ITR photos and sounds through place links (this audit) | partial (ITR files missing on disc) |
| 3D tours | yes (7) | `/tours`, articles, hotspots | done |
| Panoramic tours (`virttour` 2DVT, `.svj`) | yes (16 tours, 65 panoramas) | `/item/{tour}` from articles, browse pages and search (this audit) | partial |
| Director/Shockwave | yes (2 datasets) | Extras | done (n/a in 2009) |
| Games (MindMaze, Geo Quiz D*.TXT) | yes | `/games` | done |
| Place → media links (P2MTBL) | yes (16,122) | atlas click menu | done (2,656 targets are on no edition of this disc) |
| Spelling lexicons (LEX) | yes (783,245 words) | did-you-mean, `/api/spell` | done |
| Quizzes (SWQUIZ) | yes (3,622) | `/games/quizzes`; the boards under Ruffle | done |
| Contributors (`list/expert.xml`) | yes (2,034) | article "Contributed By", contributor pages (this audit) | done |
| Disc fonts | yes (8) | every page | done |
| CHM help, Works integration, executables | omitted by design (media.md §4, CLEAN_ROOM rule 6) | — | n/a |

### Spot check: 50 random ids of each item kind through `/api/item`

Script: `target/agent-parity/scratch/spot.py` (local, not committed): samples 50 ids per `item.kind` with a fixed seed, fetches
`/api/item/{id}`, applies the client's view rules (`ItemPage` → redirect, kind view, article, media class) and checks that
the page has content (article body, playable variant, picture answering 200, data table, links, scenes …).

| kind | items | before this audit | after |
|---|---|---|---|
| article, yearbook-article, quotation, sidebar, further-reading, timeline-event, map-about, about, quickstart, yearbook-intro | 141,236 | 50/50 each | 50/50 each |
| map, media, composite-media, video, table, facts, audio-set, home-page | 141,623 | 50/50 each | 50/50 each |
| weblink | 29,095 | 0/50 ("Nothing to display") | 50/50 |
| live-link | 147 | 0/50 | 50/50 (address + "service closed") |
| timeline-topic | 13,061 | thumbnail only | 49/50 open a topic timeline |
| quotation-author | 6,196 | 0/50 | 50/50 |
| contributor | 2,034 | 0/50 | 50/50 render; 47/50 list articles |
| virtual-tour-node | 1,685 | 9/50 (a picture) | 41/50 (tour, scene or picture); 9 are orphan site hotspots ("TBD", no `destmedia`) |
| virtual-tour (site maps) | 79 | picture only | 44/50 open their panoramic tour |
| panorama | 151 | 0/50 (icon only) | 50/50 open their Flash viewer |
| interactive | 860 | 48/50 (2 opened as audio) | 50/50 |
| maptrek-tour, maptrek-stop | 851 | thumbnail / text | 50/50 Map Trek view |
| feature | 27 | 1 routed | 27/27 (25 pages, 2 online-only notes) |
| audio | 3,542 | 35/50 | 33–35/50: the rest are the 1,083 empty stubs below |
| unclassified (`TBD`) | 617 | 0/50 | 0/50: placeholders whose content is inline in the article body (below) |
| home-theme, home-background | 157 | — | decoration of home and browse screens (drawn there) |
| index-term | 180,545 | 0/50 | 0/50 (below) |

### Converted but unreachable (or empty)

| What | Count | Why | Proposal |
|---|---|---|---|
| Index terms (DATAF) and `index_tree` | 180,545 items, 212,631 trees | search results that expand in place; 170,159 terms are aliases of their targets (867,302 aliases) | done |
| Empty `audio` records | 1,100 (1,083 untitled) | 17 recovered from interactives (`audiorefid`, content-schema §10.1); the 1,083 others are empty on the disc | nothing to show; listed as empty stubs |
| `unclassified` (`group="TBD"`) | 617 | inline equation pictures' records (e.g. "phys2" in Common Physics Equations); the picture is drawn inline from the body's `img` | none needed (content is shown); give them a "see article" page if search ever indexes them |
| Orphan tour nodes | 151 | site-map hotspots with empty `destmedia` and no parent | none (no content) |
| MSWORLD.ITR sounds and pictures | 152 sounds, 1,121 pictures linked | `media/atlas.rs` resolves by name, id, stem, credit, thumbnail and crop (media.md §7): 146 silent and 910 pushpin-only left, not on the disc | done (≈15 more need a full-disc image search, not worth it) |
| MSWORLD.ITR photos without a main file | 969 linked | not in `baggage.ecs`; only the 64-px pushpin exists | look for the pictures under other names (S) |
| P2MTBL targets on no edition of this disc | 2,656 | the table is shared with other editions | none (left out of the menu) |

---

## Fixes made in this audit

Server (`crates/encarta-server`, tests on the synthetic fixture):
1. `/api/item` adds `referenced_by` (forward associations that name the item) and `parents` (`item_child`).
2. `/api/item` serves the 2,354 MSWORLD.ITR media without a DATA record (`atlas_media`), with the pushpin picture as
   fallback; files that do not resolve are left out.
3. `/api/place/{gid}` titles atlas photos and sounds and leaves out 2,656 links to ids on no edition of this disc
   (they opened "Item not found").
4. `/api/timeline?term=`: a TIMELINE.DAT term's events (topic timelines), falling back to an event of that exact title.
5. Compression for `application/wasm`, `font/ttf`/`otf`, `model/gltf-binary`, `application/octet-stream`.
6. Ruffle's hashed files (`core.ruffle.<hash>.js`, `<hash>.wasm`) are `immutable`.

Client (`web/`, tests in `src/parity.test.ts{,x}`):
7. Menu bar File / Edit / Favorites / Help; File › Print, Edit › Copy (with source), Edit › Find in Article.
8. Favorites in localStorage: add, menu list, Organize Favorites page (rename, reorder, remove); works without storage.
9. Help page with keyboard shortcuts.
10. Accessible menu button (`components/Menu.tsx`) for the Features menu, the menu bar and the tour's Appears in.
11. Skip-to-content link; `main` as focus target.
12. Print style sheet.
13. Web Sites panel in the article's right column.
14. Pages for web links and live links (address, Internet Archive copy, "listed in").
15. Pages for quotation authors and contributors.
16. All 27 feature records route to their pages (translation pairs, quick searches, online-only notice).
17. Topic timelines open the timeline on their term, fitted to its events.
18. Panoramas open their viewer interactive; map items open the atlas at their place.
19. `mediaClass`: the 82 narrated and map interactives get the interactive player.
20. Map Trek view.
21. Panoramic virtual tours (`PanoramaTour`): 16 tours that no page showed.
22. 3D and 2D tour nodes open their tour at the stop or scene; tour pictures show as pictures.
23. Tour menu: Start Over, Appears in.
24. Dictionary Copy and Print.
25. Item pages list `referenced_by` when DATA gives no "appears in".

## Remaining gaps

The open work, in priority order, is the "Next" list in [PLAN.md](PLAN.md).

## Performance (production build served by `encarta-server`)

| Check | Result |
|---|---|
| Initial load | `index.html` (0.8 kB) + entry JS 291 kB (93 kB gzip) + CSS 47 kB (11 kB gzip); no other chunk on first paint |
| Lazy chunks | atlas (MapLibre 1.07 MB / 292 kB gzip, its CSS 70 kB), tour runtime with three.js (634 kB / 170 kB gzip), games (5–21 kB each), Ruffle only when an interactive opens |
| Cache headers | `/assets/*` and `/media/*` `immutable` for a year; `index.html`, `/ruffle/ruffle.js` `no-cache` with ETag; atlas, tours, fonts `max-age=3600` + ETag; Ruffle's hashed wasm and chunks now `immutable` (were revalidated on every open) |
| Compression | JSON, HTML, JS, CSS brotli/gzip; **new**: Ruffle wasm 14.3 MB → 4.7 MB (br), tour `scene.glb` 15.6 MB → 7.3 MB, `pvs.bin` 2.4 MB → 0.6 MB, disc fonts 251 kB → ~159 kB; ranges and media never |
| Images by `Accept` | `/media/file/*` answers AVIF or WebP only when smaller than the original and the same width (`Vary: Accept`); e.g. 38 kB JPEG → 25 kB AVIF; WebP that would be larger is not sent |
| Done | API responses carry `Cache-Control: public, max-age=3600` (not the randomised ones); the MS Reference fonts also ship as WOFF2 (api.md) |

## Verification

- `cargo test -p encarta-server`, `vitest` (244 tests), `tsc --noEmit`, `npm run build`: clean.
- Chrome against the production build on port 8097 (and Vite on 5197): menu bar and Favorites (stored, listed,
  status line), Features menu by keyboard (↓ to Thesaurus, Enter), Canada's Web Sites panel (8 of 24, "Show all"), a web
  link page, author and contributor pages, feature routes (Quick Search: Maps → `/search?kind=map`, Encarta on the Web
  notice), the Westminster Abbey panoramic tour (site map, scene switch by hotspot, picture hotspot → viewer "Choir"),
  Map Trek "Arctic and Antarctic Deserts" (stop 2 at 14,195 km in Precipitation (Annual)), the Aztec Empire topic timeline
  (23 events, window 484 BC – 1724), a 3D tour hotspot node → `/tours/04257?stop=…`, a 2D label → its scene, the
  Acropolis tour with Start Over and Appears in (loaded with the compressed glb and PVS, ticked by hand because the
  automation tab is hidden), Ruffle initialising from the brotli wasm (Vasco da Gama), the print style sheet present.
