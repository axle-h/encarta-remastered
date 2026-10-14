# Content schema: the source XML dialect and our normalised database

Status: **implemented** in `crates/encarta-convert/src/content*` (`encarta-convert content`, about 35 s,
writes `out/content.sqlite`, 3.5 GB). The survey that this document is written from is
`encarta-convert content-survey` → `out/report/content-survey.{json,txt}` (every element, attribute,
parent/child pair and small value set of every XML source). The completeness report is
`out/report/content.json` plus `out/report/content-summary.txt`.

Sources of meaning: the decoded data itself, the original XSL templates in `ENCXSL.EIT` (read for meaning
only, CLEAN_ROOM rule 7; nothing is copied), the fonts in `AREF/FONTS` (glyphs rendered and identified by
eye) and the reference VM captures in `fixtures/articles/` (private).

Contents: [1 Sources](#1-sources) · [2 Article dialect (CONT)](#2-article-dialect-cont) ·
[3 Metadata dialect (DATA)](#3-metadata-dialect-data) · [4 Trees (REL, DATAF)](#4-trees-rel-dataf) ·
[5 Sidebar files (MD*.EIT)](#5-sidebar-xml-mdeit-baggage) · [6 Dictionaries (EDICT)](#6-dictionaries-edict) ·
[7 Characters](#7-characters) · [8 Our model](#8-our-model-json-and-html) · [9 Database](#9-database-outcontentsqlite) ·
[10 Completeness](#10-completeness) · [11 Open questions](#11-open-questions-for-the-vm)

---

## 1. Sources

| source | records | root | prolog | what |
|---|---|---|---|---|
| `ContStd`, `ContDlx`, `ContKdc` | 44,055 / 69,949 / 637 | `content` | none | article bodies (Std = core encyclopedia, Dlx = Deluxe-only items: yearbook, essays, quotations' bodies …, Kdc = Kids) |
| `DataStd`, `DataDlx`, `DataKdc` | 121,055 / 175,793 / 5,874 | `data` | none | one metadata record per item (every content id has one) |
| `DataFStd`, `DataFDlx`, `DataFKdc` | 152,433 / 28,112 / 100 | `data group="indx"` | BOM + XML decl | index terms (210… ids): where a term occurs |
| `RelStd`, `RelDlx`, `RelKdc` | 34,606 / 970 / 663 | `relarts` | BOM + XML decl | "Related Articles" trees |
| `DataESK` | 1 (`list/expert.xml`) | `list Classname="expert"` | none | contributor list |
| stubs `ContESK`, `ContEDK`, `DataEDK`, `ContECK`, `DataECK` | 0 | | | empty packages |
| `MDSTD.EIT`, `MDDLX.EIT` `baggage/*.xml` | 3,132 / 495 | `text` | `windows-1252` decl | sidebar and further-reading bodies of items whose `<content>` is empty |
| `EDICT/*.EIT` (one file per entry) | 292,642 entries | `DICTIONARY`, `THES_ARTICLE`, `ARTICLE`, `Signature` | UTF-8 decl | dictionary, thesaurus, 8 bilingual dictionaries |
| `SW*.EIT` `*.iax` | 3,622 quizzes + 721 interactives | `quiz`, `iai` | BOM + decl | quiz questions and interactive definitions (texts are escaped HTML) |

All AKC text is UTF-8 with CRLF line breaks; `xml:space="preserve"` on `<text>` (layout white space between
blocks is not content). No comments, PIs, CDATA or DOCTYPEs occur anywhere. The five predefined entities and
numeric references are the only XML entity references; editorial characters are `<entity tag="…"/>`
elements (§7). A few texts still contain literal `&eacute;` after XML unescaping (double-escaped
`&amp;eacute;` in the source); we resolve them through the same table.

## 2. Article dialect (CONT)

`<content refid revision="100">` → `<text refid xml:space="preserve">` → blocks. An empty `<content/>`
(2,810 Std, 13,235 Dlx) means the body lives elsewhere (a sidebar XML file named by the DATA record) or the
item has no body (the 13,061 timeline topics).

| element | meaning | our model |
|---|---|---|
| `section id type` | section; `id` is the target of section jumps and of DATA `sec` | `section` block, `level` = nesting depth |
| `section@type` | **heading style, not depth**: 4 introduction, 5 major (upper case title, roman numeral), 6 minor (A, B …), 7 sub-minor (1, 2 …). ContDlx has type 6 at the top level and 5 nested | `kind`: `introduction`, `major`, `minor`, `subminor`; raw `type` kept |
| `sectiontitle` | heading text | `title` |
| `sec`, `seca`, `secb`, `secc` | number labels at depth 0–3 (`III`, `A`, `1`, `a`); each section repeats the outer ones | `number` (path) |
| `pkey id` | paragraph; `id` is the paragraph link target (`xref@paraID`, search `para`) | `p` block |
| `example` | treated as a paragraph (XSL handles it like `pkey`; not on this disc) | `p` |
| `headline id type` | ContDlx essay heading: 32/33/34 levels 1–3, 35/36/37 the same centred | `heading` (`level`, `align`) |
| `intro id` | lead paragraph of an essay or sidebar | `intro` |
| `author id` | by-line | `byline` |
| `rule` | horizontal rule | `rule` |
| `i`, `b`, `sub`, `sup`, `smallcaps` | inline styles | `em`, `strong`, `sub`, `sup`, `smallcaps` |
| `br` | line break (mostly in verse quotes) | `br` |
| `xref type RefID [paraID] [URL]` | cross-reference, see the table below | `link` |
| `inlinetitle` | always empty; the reader shows the item's title (bold) — **confirmed in the VM** (Zebra 761575094 starts with "**Zebra**,"). With an alternative title both are bold, joined by a plain " or ": East Timor 461510929 shows "**East Timor** or **Timor-Leste**, independent republic …" (VM) | `title-ref` with separate `title` and `alt` parts (§8) |
| `inlinebmp id type` | inline picture `‹id›.gif` from baggage; 27 centred block, 28 in the line, 29 right, 30 left | `img` |
| `fs type` | text typed in a private font (§7) | converted to Unicode / `frac` / `stack` … |
| `quote type` | verse or block quotation; 27 centre, 29 right, 30 left, 38 justify | `quote` |
| `list type` + `listitem` | 1 plain lines, 18 square bullets, 19 bullets, 20 numbered | `list` |
| `fl` | displayed formula | `formula` |
| `notation type="fraction"` + `item pos` | built-up fraction | `frac` |

Cross-reference types (`xref@type`; prefixes are what the reader prints, from the XSL strings):

| type | meaning | our `kind` | printed |
|---|---|---|---|
| 8 | quick-verify link in running text | `qv` | — |
| 9 | web link (`URL`, ContDlx bibliographies) | `web` | — |
| 10 | first item of a "See also" list | `see-also` | "See also " |
| 11 | further item of that list | `see-also-next` | — |
| 12 | parenthetical see-also | `see-also-paren` | "(" … ")" |
| 13 | embedded see-also | `see-also-embed` | "See also " |
| 14 | first item of a "See" list | `see` | "See " |
| 15 | further item of that list | `see-next` | — |
| 16 | parenthetical reference | `paren` | "(" … ")" |
| 17 | embedded "see" | `see-embed` | "see " |

`RefID="0"` is a dead link (text only).

**Media placement.** The body does not contain media; the reader inserts the item's DATA `forward`
associations with `sec` = section id. Placed groups: `media`, `audio`, `audionested`, `video`, `flash`,
`table`, `tableinline`, `sidebar`, `composite`, `virttour`, `factsandfigures`, `timeevent`, `timetopic`
(quotation, archive, weblink, furtherread, expert, maps, livemedia are not); items with `hideappears="1"`
are not. The original computes the layout in native code before its templates run (the templates receive
`assoc` and `cluster` elements already interleaved with the paragraphs), so the rules below are inferred
from the 33 article captures (1,660 placed items). Per section, with *m* = the section's own paragraphs
(`pkey`/`intro`, not those of subsections) and the items in `sequence` order:

1. Four streams: **sidebars**, the **timeline** stream (`timeevent`, `timetopic` and `virttour`), **Facts and
   Figures** (`factsandfigures`), and the **primary** stream (everything else). Sequence numbers are only
   compared within a stream.
2. Sidebars are spread from paragraph 1 (paragraph 0 when there is no primary item) a whole number of
   paragraphs apart: item *k* of *n* goes before paragraph `start + k·⌊span/n⌋`, `span` = m − 1 (m when
   starting at 0, at least 1); with more sidebars than `span` they take consecutive paragraphs.
3. The timeline stream starts at the same paragraph as the sidebars and skips their paragraphs: of the
   paragraphs from there on that hold no sidebar, item *k* of *n* takes number `k·⌊avail/n⌋`. When fewer
   than *n* are left, the items take consecutive paragraphs from the start, sharing the sidebars'. A
   timeline item has a paragraph of its own or none: those that would go after the last paragraph move
   on to the **next section in reading order** (subsections included), where they take consecutive
   paragraphs from its start, after its sidebar, without keeping those paragraphs from its other items;
   when that section has timeline items of its own, the carried ones go first in its stream. Items carried
   past the last section close it.
4. Facts and Figures goes after the last paragraph of its section and takes no paragraph from the others
   (the five country introductions captured).
5. The primary items share the remaining paragraphs ("free slots"; at least slot 0). With no more items than
   free slots they go `step = ⌊free/items⌋` slots apart from the first free slot. With more, the items that
   cannot share a box (video, inline tables, anything not *clusterable*) take their own free slots first,
   free slot `k·free/count` for item *k* (when they are at least as many as the free slots, they share them
   front-loaded and the clusterable items form one box in the last); the *clusterable* items (`media`,
   `audio`, `audionested`, `flash`, `table`, `composite`) fill the other free slots in sequence order,
   front-loaded (each slot takes ceil(remaining / remaining slots)), and several in one slot form a
   **cluster** — one box in the margin showing one item at a time with a strip of thumbnails. In one slot:
   primary first, then the sidebar, then the timeline item. Slot *m* (after the last paragraph) goes before
   the section's subsections.
6. **Column.** Sidebar teasers and video span the **text** column; a `media` picture spans the text column
   when its `files/image` is at least 360 px wide and it is not `NoSquish="1"`, otherwise it is drawn in the
   left **margin** column (192 px picon); an audio set (`audionested`) follows its picture member (by that
   rule, or text when the member has no `image` file and a 216-px picon); everything else, and every
   cluster, is in the margin.

Against the captures this gives the VM's paragraph for 99.4 % of the items, the VM's order for all of them
and the VM's cluster boxes for all of them (1,660 items in the 25 encyclopedia articles captured; the test
`content::disc_tests::placement_matches_vm_captures` measures it when `extracted/` and the captures are
present). The previous rules (one timeline stream without virtual tours, Facts and Figures among the primary
items, no carrying) gave 95.7 %, 98.4 % and 98.0 %; the old "spread evenly" rule 66 %, 89 % and 89 %. The rest,
ten items in four sections, are primary items the VM puts later in the section than `⌊free/items⌋` spacing
does (Mathematics and United Kingdom introductions, Moon §2, East Timor §13); no rule found over paragraph
counts or the other items' positions explains all four (§11).

**Sidebar teasers** (`sidebar` placed in an article) are boxes in the text column: header "Sidebars", the
category by the sidebar's DATA `type` (VM: `SBD` HISTORIC DOCUMENTS, `SBG` GREAT MOMENTS IN SPORTS, `SBH`
HISTORIC HEADLINES, `SBI` INTERVIEWS, `SBL` GREAT WORKS OF LITERATURE, `SBO` HISTORIC SPEECHES, `SBS` WORLD OF
SCIENCE, `SBZ` POINTS OF THE COMPASS, `SHE` HISTORICAL ESSAYS, `SPC` POINT/COUNTERPOINT; `SBX`, `SBC`, `CUS`,
`ZPA`, lower-case `sbl` and codes not seen show SIDEBAR; stored as `item.meta_json.sidebar_category`), the
title, the preview text and "open sidebar".

**Sidebar view** (items of group `sidebar`, VM: two frames): a header frame (72 pt; "Appears In", the
banner, the title) and a body frame: intro (italic), rule, headline, by-line, then the paragraphs and
headings in source order. All the sidebar's own placed media are in **one cluster** floated at the top
left of the body (Customs of Russia 631522251: two pictures), not beside their sections.

**Kids articles** (layout `kids`, VM): orange title (MS Reference Sans Serif 20 pt) top left and a "Look at
Pictures"/"Read Article" button; text on the right in large type; the introduction has no heading; the
other sections have orange upper-case headings and **no section numbers**; the media are a two-column grid
of tiles on the left (rounded orange border, blue caption, badge: speaker = audio, "+" = picture, hand =
interactive) in `sequence` order (all 3 captures), scrolling with the text; an inline table (`tableinline`,
"Facts About France", green header) sits in the text at the end of its section. Below the text: "Also in
Encarta Kids" (the related-articles tree) and "On the Web" (web links).

**Checked against the VM captures** (`fixtures/articles/`, all 60 folders): paragraph for paragraph (each
`P id=pN` of the capture, frames included, against our `paragraph` row), 4,795 of the 4,796 captured
paragraphs of the 29 encyclopedia articles, 3 sidebars and 3 Kids articles are identical, after mapping
the raw private-font characters of the capture's DOM through our tables (§7). The one difference is
deliberate: the original prints `<notation type="fraction">` 7 over 2 as plain "72" (no template), we keep a
fraction. Tables, Quick Facts and Facts and Figures: every text run of our table HTML occurs in the capture;
media items: titles and captions present; dictionary entries: every text run of the displayed homograph.
Not comparable: the yearbook article and the quotation (their captures have no body text), the two timeline
views (native), the outline views (only the outline is in the DOM).

## 3. Metadata dialect (DATA)

`<data refid group …>`; root attributes (all kept in `meta_json.source_attrs`):

| attribute | meaning |
|---|---|
| `group` | item type; mapped to our `item.kind` (table below) |
| `ticon` | media-type icon: `image`, `audio`, `video`, `map`, `chart`, `table`, `ia` (interactive), `pan` (panorama), `vtour`, `quo`, `archive`, `timeline`, `sidebar`, `weblink`, `narr`, `game`, `quickstart`, `none` → `item.media_type` |
| `subgroup` | presentation subtype (`Slideshow`, `360`, `grid`, `list`, `large`, `MAG` …) |
| `type` | sidebar category (`SBA` sport, `SBD` historic documents, `SBG` sports moments, `SBH` headlines, `SBI` interviews, `SBL` literature, `SBO` speeches, `SBP` popular culture, `SBS` science, `SBX`/other generic sidebar, `SBZ` compass, `SHE` eyewitness accounts, `SPC` point/counterpoint — meanings from the Dutch XSL strings), `Archive`, live-link kinds (`City Weather`, `Radio` …) |
| `style` | map style (0 for all 73,187 maps; others on home pages) |
| `geocode` | atlas place id |
| `wordcount`, `revision`, `origin` (`EE`), `InVB`, `minimob`, `NoSquish`, `hideappears`, `partner`, `subcategory`, `destmedia`, `alt`/`lat`/`long`/`x`/`y` (map trek stops), `year` (yearbook), `colorvalue` | kept verbatim |

Children:

| element | meaning | where it goes |
|---|---|---|
| `title`, `fulltitle`, `alttitle`, `jtitle` | display titles (inline markup `it`, `scp`, `fs`, `tbd` placeholder) | `item.title*`, `meta_json` |
| `preview` (`pkey`/`intro`) | first paragraph, for lists | `item.preview_html` |
| `credit`, `caption` (`it`, `scp`, `break`, `fs`, …) | media credit and caption | `item.credit*`, `item.caption_html` |
| `files/*` (`ticon picon thumb image audio video cc xml item surround default mouseover background`, `x y srcid disc`) | baggage files and pixel sizes (`msencdata::baggage/<file>`) | `item_file` (role, lower-case baggage key, width, height) |
| `forward/assoc`, `reverse/assoc` (`refid group sec sequence appears par reviewed`) | links to other items: what this item shows (forward, with section) and where it appears (reverse) | `assoc` |
| `outline` (`pkey`, `sec type id title`) | the article outline | `article.outline_json` |
| `children/{article,media,hotspot,stop,label,site,assoc}` (`Sequence`) | members of tours, panoramas, slide shows, audio sets | `item_child` |
| `source/{work,title,date,ref,author}`, `theme`, `context`, `attributee`, `date` | quotation fields | `meta_json` |
| `name`, `born`, `died`, `flourished`, `nationality`, `designation`, `id` | quotation author | `meta_json` |
| `name/{first,middle,last,sir}`, `sort_name`, `type`, `credentials`, `info`, `adjective`, `area` | contributor | `meta_json` (title built from the name) |
| `timespan/{start,end}` (`year month day approx bc year_billion`) | timeline event dates | `meta_json.timespan` |
| `table` (`head/columns/column`, `colrow/colgroup`, `body/row/cell`, `foot/footnotes/footnote`, `source`), `quickfacts` (`head/title`, `body/entry/name/value`), `faf` (`section/title`, `entry name value links/xr`) | statistical tables, Quick Facts, Facts and Figures | `data_table` (our table/facts JSON + HTML) |
| `shed`, `quadrant` (`pt x y`) | panorama hot regions | `meta_json` |
| `default` (`width panangle lightangle coord3d refid x y`) | panorama/3D defaults | `meta_json.panorama` |
| `providers/provider` (`name code zip`), `url`, `urlparams` | live news/weather links | `meta_json` |
| `caption/{MouseOverColor,AtRestColor,DecorationOffset}` | home-screen button colours (misused caption) | `meta_json.button_style` |
| `ENC` (in `context`) | editor's note | inline `note` |

`group` → `item.kind`: article/kids → `article`, sidebar, furtherread → `further-reading`, table/tableinline →
`table`, factsandfigures → `facts`, quotation, author → `quotation-author`, expert → `contributor`,
timeevent/timetopic → `timeline-event`/`timeline-topic`, archive → `yearbook-article`, maps → `map`,
aboutmap → `map-about`, media, audio, audionested → `audio-set`, video, flash → `interactive`, composite →
`composite-media`, surround → `panorama`, virttour/vtour → `virtual-tour-node`/`virtual-tour`,
maptrektour/maptrekstop, weblink, livemedia → `live-link`, home/hsbg/hthm → `home-page`/`home-background`/
`home-theme`, feat → `feature`, indx → `index-term`, TBD → `unclassified`.

### 3.1 Associations (`forward`/`reverse` `assoc`, table `assoc`)

`forward` rows list what an item shows: `sec` is the section the reader places it in, `sequence` its order there
(see §2 "Media placement"). `reverse` rows list where an item appears, and come in two kinds:

- with `sec` set (and `appears` empty): the mirror of the target's forward row, i.e. "placed in section `sec` of
  article `refid`" (272,081 rows);
- with `sec` empty and **`appears` set** (99,781 rows): an entry of the item's "Appears in" list. `appears` is the
  **content id of the item that the target article actually shows for this one**: the item itself (52,830 rows,
  `appears` = own id) or **another copy of the same media** (46,951 rows). Copies exist because the Premium and Kids
  editions, and several articles, have their own records for one picture or sound: Grévy's Zebra 461550745 appears in
  the Kids article Zebras 701639707 as the Kids item 701667164, and that one appears in Premium Zebra 761575094 as
  461550745; the audio set 461517535 (Mountain Zebra) appears in Mammal 761561349 as the set 461556091, which has the
  same children. Checked on the whole database: of 33,873 distinct (item, `appears`) pairs with a different id,
  33,756 (99.7 %) share a DATA file (`image`, `picon`, `audio`, `video` or `thumb`) with the item; the rest point
  from a composite or audio set to the composite that holds it, or to an interactive that uses the picture.
  In 97 % of the rows the target article has a forward association to `appears`.

So a client that opens an "Appears in" entry should open article `target_ref_id` and highlight (or open in the
viewer) `appears`, not the item's own id. Forward rows never carry `appears`, and no row has `appears` = 0 (items
kept out of the body are marked by the target's DATA `hideappears="1"`). `par` occurs on 12 rows only, timeline events
without a `sec` in six articles (values 1 and 2; meaning unconfirmed).

## 4. Trees (REL, DATAF)

Each record holds one `<tree ed>` per product edition: `STD` Standard, `STE`/`STC` (student editions,
presumably English/Canada), `DLX` Deluxe/Premium, `KDC` Kids. Identical trees are stored once with the list
of editions; Premium should use the tree whose `editions` contains `DLX` (Kids: `KDC`).

- REL: nested `relart [nodeid] [refid group pkey]` + `title` (inline `it`, `scp`, `place gid rid`, `entity`).
  A node with `refid` links to an item, `pkey` to a paragraph; a node with only `nodeid` is a heading.
- DATAF: nested `entry [refid group pn paragraphid IsList]` + `title`. `paragraphid` is the paragraph,
  `pn` a small ordinal (1–27; meaning unconfirmed), `IsList` marks list entries.
- `<entity tag="sortattop"/>` at the start of a title is a sort marker (keep this node first) → `sort_top`.
- Titles are pretty-printed; we collapse white space.

Our tree JSON: `[{"title": [inline], "ref": id, "group": "...", "para": n, "pn": n, "node": nodeid,
"list": true, "sort_top": true, "children": [...]}]`.

## 5. Sidebar XML (`MD*.EIT/baggage`)

Windows-1252 files `<text refid xml:space>`, the body of an item whose DATA `files/xml` names the file
(3,627 files: 2,984 for items with an empty `<content/>`, 643 for items that have a DATA record but no CONT
record; a file that no DATA record names would be converted under its root `refid`). Dialect: `pkey id`, `bib` → `au` author, `ti` title, `ser` series, `pub` → `pubn`
publisher, `fdt` date, `edt` edition, `voln` volumes, `ann` annotation; `xr type syntax` + `x id para`
links (`qv`, `sea` = see-also line, `syntax="sep"` own line; `para="0"` = whole article); `list type="bullet"`
+ `li`; `it`, `i`, `I`, `b`, `scp`, `smallcap`, `span style="font-variant:small-caps"`, `quote`, `br`;
`<entity tag="smallcaps">bc</entity>` wraps text (small caps); `entity tag="startcomma"` occurs only in two
"new article placeholder" files (dropped). Bibliography fields outside `<bib>` are grouped into entries.

## 6. Dictionaries (EDICT)

File name = entry ref id. `$_EDictSignature2007_$` gives the dictionary's names (per UI language
`A` English …), `GUID`, `ParentGUID`, `UniqTag`, `LexFile` → `dictionary`.

- **DICTA** (English, 105,032): `DICTIONARY/HOMOGRAPH(ID Variety Type Group EncType Target)/HEAD/HEADWORD/
  {ENTRY (syllabified), DisplayHeadword, PRONS/ONE-PRON/PRON, INFLECTIONS/INFLECTION/{ENTRY-INF-TYPE,
  ONE-INFLECTION Type/ENTRY}}`, `BODY/POS/{PARTS-OF-SPEECH/ONE-POS/PART-OF-SPEECH, SENSES/SENSE(ID
  IsSensitive Variety)/{SENSE-NUMBER, SUBJECT-AREA, STYLE-LEVEL, QUICK-DEFINITION, DEFINITION/DEF,
  EXAMPLES/EX}}`, `TAIL/ETYMOLOGY/ANC/{LANG,REF}`, `RUNONS/ONE-RUNON`, `PHRASES/ONE-PHRASE`, essays,
  usage notes, encyclopedic data (`POP`, `CAPITAL`, `AREA` …), `AUDIO_PRON RefID/FG_AUDIO_WAV` (baggage
  `<id>.wav` in `PRON.EIT`). Cross-references `HXREF`, `XREF`, `REF`, `HWREF`, `IXREF`, `ETYREF`,
  `SYNONYM`, `ITEM-REF`, `XREF-ENTRY`, `XREFHW` carry `TargetRefID` and/or a `Target` GUID (resolved through
  the GUIDs of all entries; 11 stay unresolved). 158 element names in all; `ITAL`, `BOLD`, `SUP`, `SUB`,
  `BR`, `SC` are formatting.
- **THESA** (35,127): `THES_ARTICLE/THESAURUS/{HEAD(RefID REGISTER DIALECT POS TYPE), BODY/SENSE/
  {SYNONYMGROUP/SYNONYM, ANTONYMGROUP/ANTONYM}(TargetRefID REGISTER DIALECT)}`, `BUILD_REFIDS/REFID`.
- **Bilingual** (`ENG_FRA` … `SPA_ENG`, 16k–22k each): `ARTICLE` mixed content: `Num` sense number, `Pos`,
  `SCP` context/label in small caps, `INF` phrase or idiom, `Label`, `B`, `BR`/`DBLBR`, `Media RefID
  Type="BLP"` link to the target-language entry, `XREF-ENTRY`, `PRON`, `Homograph`, and `Headword` (last in
  the file; also carries " (2)" homograph numbers, stripped in `headword`).
- Dropped: `WAV_AUDIO_PATH`, `MP3_AUDIO_PATH` (build-machine paths `\\TKXTICTFS\…`).

## 7. Characters

- **`<entity tag>`**: 153 tags on the disc. ISO 8879/HTML names (`eacute`, `amacr`, `hellip` …), numeric
  `#233`, and Encarta's own transliteration names (`hlowdot` ḥ, `Hcedil` Ḩ, `tlowdot` ṭ, `zlowmacr` ẕ,
  `acircacute` ấ, `numl` n̈, `frac1.2` ½ …); the search exports, MindMaze and timeline strings add ~60
  more, some misspelt (`ecute`, `eumul`, `uml` in "D&uml;rer") and chemistry/music ones (`z.1horbar` —,
  `z.2horbar` =, `z.3horbar` ≡, `z.notation3.4`). Table: `content/entities.rs`. Markers: `sortattop`,
  `startcomma`. Unknown tags are reported (none remain).
- **`<fs type>` font runs** (text typed in a private font, mapped per character):
  - 1 Symbol: Adobe Symbol encoding (`p` π, `S` Σ, 0xC5 ⊕ …).
  - 2 MS Reference Specialty (`REFSPCL.TTF`): the text holds Latin-1 code points that the font draws as
    whole notations, so they must be mapped through the font's glyph table, never shown as the Latin-1
    character. Built-up fractions with a bar (`y` 1/2, 0x82 1/4, 0x88 1/8, `w` 1/16, 0x80 1/3 …) → `frac`;
    stacked numbers without bar (time signatures: U+00BD `½` is a stacked **6 over 8**, U+00AC `¬` **2 over
    4**, `¶` 3/8, `º` 4/8, `»` 5/8 — confirmed in the VM, Musical Rhythm 761553014; nuclides ²³⁵₉₂) → `stack`; radicals (√2) → `radical`;
    vinculum (s‑a) → `overline`; (0xC1 √−1, 0xC2 √16, 0xC3 √2 in Algebra/Mathematics read correctly in
    context: "x = 1/2", "a cube root of 1/8", "the imaginary number √−1") vectors (AB⃗) → `vector`; chemical bonds — = ≡; voice-leading arrow pairs;
    a Phoenician and an Egyptian letter.
  - 3 MS PGothic, 8 MS Reference Serif: already Unicode (font hint only).
  - 6 MS Reference 1: ♯ ♭ ⓪①②③ ↗; 7 MS Reference 2: `i` = 2/8 stack.
- **Private-use code points** (U+F6FE–F7DA) of the MS Reference text fonts: letters with diacritics that
  had no code point (ḫ, z̧, s̆, œ̆, ų …). Identified by rendering `GRGAREF.TTF` and reading the words
  (`Petaḫ Tiqwa`, `Muḩāfaz̧at`, `Lågen`, `a ∉ S`). Confirmed in the VM (`fixtures/articles/glyphs/`): U+F73D
  and U+F73E are two archaic digamma forms in the article F → Ϝ (U+03DC) and Ͷ (U+0376, Pamphylian digamma),
  the nearest Unicode letters; U+F71B is ć ("Milošević"); U+F700 is å ("Lågen").
- **`<entity>` tags do not occur in article bodies**; they are in REL/DATAF tree titles, DATA titles,
  sidebar files and the search exports. Of the 49 tags in the related-article titles of the captured
  articles, 25 could be matched letter for letter in the capture's DOM (the others' titles are not
  rendered there); none disagrees with the table. A line break between two `<entity/>` elements in a
  pretty-printed title is layout ("Ł⏎ódź" is "Łódź").
- MindMaze text is code page 437 with SGML entities (decoded by `encarta_formats::mindmaze`).

## 8. Our model (JSON and HTML)

**Inline nodes** (`content/model.rs`): text is a JSON string; other nodes are objects with `t`:

| `t` | fields | HTML |
|---|---|---|
| `em` `strong` `sub` `sup` `u` | `c` | `<em>` … |
| `smallcaps` | `c` | `<span class="enc-sc">` |
| `br` | | `<br>` |
| `link` | `kind` (§2), `ref`, `para`, `url`, `c` | `<a class="enc-link enc-‹kind›" href="/item/‹ref›#p‹para›" data-ref data-para>`; web: `enc-web`; `msencarta://` app URLs: `enc-app data-app-url`; prefixes in `<span class="enc-see">` |
| `title-ref` | `title` (inline array), `alt` (inline array, `[]` when none); `c` repeats `title` (schema 1 readers) | `<span class="enc-title-ref">East Timor</span> or <span class="enc-title-ref enc-alt-title">Timor-Leste</span>` — both bold, " or " plain |
| `img` | `id`, `placement` | `<img class="enc-inline-img enc-place-‹p›" src="/media/file/‹id›.gif">` |
| `frac` | `num`, `den` | `<span class="enc-frac"><sup>…</sup>⁄<sub>…</sub></span>` |
| `stack` | `top`, `bottom` | `<span class="enc-stack">` |
| `radical`, `overline`, `vector` | `c` (string) | `enc-radical`, `enc-overline`, `enc-vector` |
| `place` | `gid`, `rid`, `c` | `<a class="enc-place" href="/atlas/place/‹gid›">` |
| `quote` | `align`, `c` | `<blockquote class="enc-quote enc-align-‹a›">` |
| `list` | `style` (`plain`, `bullet`, `square`, `numbered`), `items` | `<ul|ol class="enc-list enc-list-‹s›">` |
| `formula` | `c` | `<div class="enc-formula">` |
| `unknown` | `name` (`note` for editor's notes), `c` | `<span class="enc-note">` |

**Blocks** (`article.body_json` is an array of blocks): `{"t":"p","id","c"}`, `intro`, `byline`,
`heading` (`level`, `align`), `rule`, `bib` (`author title series publisher edition volumes date
annotation`), `media` (`ref`, `group`, `seq`, `col`: `text` | `margin`), `cluster` (`col` = `margin`,
`items`: `[{ref, group, seq}]`, shown as one box), `media-grid` (Kids tiles, `items` as for clusters,
at the end of the body), `section` (`id kind type level number title blocks`). `article.layout`
(`encyclopedia`, `kids`, `sidebar`) says which page layout (§2) to use.

**Article HTML**: `<article class="enc-article" data-ref>` with `<section class="enc-section enc-‹kind›
enc-level-‹n›" id="s‹id›">`, `<h2..6 class="enc-section-title"><span class="enc-secnum">II</span> …`,
paragraphs `<p id="p‹id›" class="enc-p">` (a `<div>` when they contain a quote/list/formula), intro
`id="i‹id›"`, by-line `id="b‹id›"`, headings `id="h‹id›"`, media
`<figure class="enc-media enc-media-‹group› enc-col-‹col›" data-ref data-group><a href="/item/‹id›"><img
class="enc-thumb" src="/media/file/‹file›"></a><figcaption>…` (text column: the `image` file, else the
picon; margin and grid: the picon, else the thumb); clusters `<div class="enc-media-cluster enc-col-margin"
data-refs="‹id› ‹id›">` around the members' figures; Kids `<div class="enc-media-grid">`; sidebar teasers
`<aside class="enc-media enc-sidebar-teaser enc-col-text" data-ref data-group="sidebar"><a href>` with
`enc-sb-hdr`, `enc-sb-cat`, `enc-sb-title`, `enc-sb-body`, `enc-sb-open` spans. Links go to `/item/‹refId›` and `/media/file/‹baggage key›`
(server routes to define; the client may rewrite).

## 9. Database (`out/content.sqlite`)

Built into `content.sqlite.partial` and renamed over the old file (idempotent; a server that has the old
file open keeps reading it until it reopens). Rows from the current disc in brackets.

| table | key | contents |
|---|---|---|
| `item` [483,267] | `ref_id` | every DATA id: `kind`, `source_group`, `title`, `title_html`, `sort_title`/`display_title_html` (PFX entry matching the title), full/alt titles, `in_deluxe`/`in_kids`, AKC package names, `media_type` (ticon), `subgroup`, `subtype`, `style`, `geocode`, `origin`, `revision`, `word_count`, `caption_html`, `credit`(`_html`), `preview_html`, `meta_json` (everything else, §3) |
| `article` [102,223] | `ref_id` | `source` (`content` or `sidebar-xml`), `source_file`, `body_json`, `html`, `outline_json`, counts, `layout` (`encyclopedia` 98,890, `kids` 637, `sidebar` 2,696) |
| `section` [198,678] | `(ref_id, section_id)` | parent, level, number label, plain title, type code |
| `paragraph` [663,219] | `(ref_id, kind, para_id)` | plain text per paragraph (`kind` p/heading/intro/byline: separate id spaces) — the search-index source |
| `link` [225,225] | | every cross-reference: from paragraph, kind, target ref/para, url, text |
| `assoc` [703,854] | | DATA forward/reverse associations |
| `item_file` [514,686] | | DATA files: role, `file_name` (baggage key — join with `baggage` and the media manifest), width, height |
| `item_child` [19,619] | | DATA `children` |
| `data_table` [2,691] | `ref_id` | table/facts JSON + HTML |
| `related_tree` [49,769], `index_tree` [212,631] | `(ref_id, editions)` | tree JSON (§4) |
| `contributor` [2,034] | `ref_id` | `list/expert.xml` entries |
| `baggage` [88,876] | `(key, edition)` | baggage/generic catalog keys → EIT package |
| `alias` [501,827] | | PFX titles and partial keys (`collection` 0/1, `class_id`, sort/display title) |
| `search_entry` [3,084,729] | `(edition, entry_id)` | IdXE records: ref, content type, section or paragraph range, display text/HTML, parents |
| `search_word` [1,175,993] | | IdxW vocabulary (word groups = synonyms) |
| `dictionary` [13], `dict_entry` [292,642], `dict_form` [409,010], `dict_audio` [54,865] | | §6: headword, variety, summary, body JSON, HTML; lookup forms (headword, inflection, run-on, phrase, variant); pronunciation audio files |
| `timeline_category`/`_region`/`_event` [16,465]/`_term` [22,749] | | `TIMELINE.DAT` (dates decoded, masks expanded) + term texts from `TIMELINE.EIT`. Display columns (VM, `timeline.md` §3.2): `display` (`point` 3,259, `period` 110 = orange bar, `essay` 35 = striped bar, NULL for item-table rows), `label` (boxed upper-case bar label, "HISTORICAL ESSAY: …" for essays), `date_text` (pop-up date line, "4? BC - AD 29?"), `start_text`, `end_text`, `prominence` (title size rank, 5 = importance 1 = largest; 0 unranked) |
| `mindmaze_question` [8,020], `mindmaze_answer` [32,080] | | answers with `article_exists` (67 point at ids not on this disc) |
| `stat_*` | | `MSWORLD.SDB` countries, categories, qualifiers, units, 363 indicators, 57,347 values (with rank); `stat_value.col_c` is the year span: `year_end` = year + C, `period` = "1990-2005" (C > 0) or "1995" — what the atlas prints in parentheses |
| `geoquiz_level` [4], `geoquiz_place` [1,372] | | `D1–4.TXT`, `D1–4PLACES.TXT` |
| `place_link` [16,122] | | `P2MTBL.DAT` place → content links with kind labels (`p2mtbl.md`; 13 `maptrek-stop`, 14 `flag`, 15 `anthem`, 16 `about-map`) |
| `atlas_media` [5,413] | `ref_id` | `MSWORLD.ITR` media catalog: place, files, paired item, legacy id, **credit** |
| `lexicon` [12], `lexicon_word` [783,245] | | every `*.LEX` word list |
| `quiz` [3,622] | `(quiz_id, edition)` | SWQUIZ `*.iax` as JSON (`@attr` keys, HTML text fragments) |
| `interactive` [721] | `(file_name, edition)` | other `*.iax` definitions as JSON |
| `meta` | `key` | schema version (**2**: `title-ref` parts, `cluster`/`media-grid` blocks, `media.col`, `article.layout`, timeline and statistics display columns), converter, SDB revision |

The same step copies the disc's MS Reference Serif and Sans text fonts (`AREF/FONTS/REFSER*.TTF`,
`REFSAN*.TTF`) and the Flash interactives' device fonts (`TAHOMA*.TTF`, `TIMES*.TTF`, `ARIAL.TTF`,
`ARIALBD.TTF`, `ARIALI.TTF`), 17 files, to `out/fonts/` (each written to `‹name›.partial` and renamed); the
server serves them as `/fonts/{file}` and lists them in `/api/meta`.

## 10. Completeness

`out/report/content.json`: every catalog id (content/data/relart, both editions: 634,247) has a status
(`article`, `article (sidebar xml)`, `empty (metadata-only item)` = the 13,061 timeline topics, `item`,
`index_tree`, `related_tree`); every AKC/EIT record is accounted for; every element and attribute of the
survey is either consumed by the converter or listed in `report::DROPPED` with a reason (only the two
EDICT build paths); character problems (unknown entity tags, unmapped glyphs) are listed with an example
record. Current result: **0 unexplained ids, 0 unaccounted records, 0 unexplained elements/attributes,
0 character problems.** String catalog keys are classified (sidebar XML → article, quiz/interactive →
JSON tables, everything else → media converter via `baggage`/`item_file.file_name`).

### 10.1 Records without content of their own

Checked against everything the disc has (every DATA/CONT/REL record, the catalogs, the media manifest's file names
and stems, `.iax` definitions, quizzes, the search exports and P2MTBL):

| records | outcome |
|---|---|
| `audio` items without a sound file: 1,100 | **17 recovered**: their sound is a slide or clip inside an interactive, whose definition names it by the item's id in hex (`audiorefid="29D02BC1"` = 701508545, next to `audiofile`). The converter adds that file to `item_file` (role `audio`, raw path `iax:<definition>`; report count `audio.from_interactive`): the ten "Sept. 11th ASD" slides (`000a3efc.iax`) and seven Kids sound clips ("Did You Hear That?", `00175545.iax`). The other **1,083 are empty on the disc**: untitled, only the generic `audio.gif/.jtn/.jsm` icons, no children, no associations (three "Photography" records are listed in an interactive with no sounds), and no file, catalog key or `.iax` reference carries their id |
| `unclassified` (`group="TBD"`): 617 | **Empty on the disc**: no `files` element, no children; 339 have no title and the rest a working caption ("Names and formulas of Selected Ions"). Articles associate them (641 forward rows, group `TBD`) section by section with the inline pictures (`inlinebmp` → `<id>.gif`, THUMBSTD) of that section, which the body draws; the counts agree in 80 of 252 sections, so the record ↔ picture pairing is not recoverable beyond that and the records stay empty |
| tour hotspots titled `TBD`: 158 (151 of them in the parity count) | **Empty on the disc**: `virttour` nodes with an `x`/`y` and an empty `destmedia`, no files, no children, no parent; nothing else names them |
| index terms (DATAF): 180,545 | used by search (api.md "Search"): each term is a result that expands to its entries (`/api/index-term/{id}`), and an alias of the items it points to when it has at most 30 of them (170,159 terms, 867,302 aliases). Encarta 2009 has no index browser (VM) |

## 11. Open questions for the VM

Answered by the VM captures (`docs/vm-findings.md`): the alternative title (§2, §8), the Specialty and
private-use glyphs (§7), sidebar layout (§2), Kids layout (§2), and the media placement rules (§2).
Still open:

1. Media placement residue (§2): the spacing of the primary items in four sections (Mathematics and United
   Kingdom introductions, Moon §2, East Timor §13), where the VM leaves more paragraphs between them than
   `⌊free/items⌋`. Answered: the country introductions (Facts and Figures closes the section) and the items
   carried into the next section (timeline items and virtual tours).
2. ~~Sidebar categories `SBA`, `SBP`~~: answered (VM 2026-10-14, 761593235 and 761594207): both show the plain
   purple "sidebar" banner like the other categories; the category changes nothing on screen.
3. ~~Timeline pop-up wording of full dates and geologic dates~~: answered, see timeline.md §3.2 (VM 2026-10-14).
4. DATAF `pn`, REL/DATAF editions `STE`/`STC` meaning; `section@type` vs heading look (type 6 at top level).
5. The right-column modules of an article (World Atlas map, Quick Facts, Related Articles, Web Sites):
   which DATA records feed them and in which order.
