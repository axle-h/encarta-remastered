# Web client (`web/`)

The browser front end of plan Step 4: a React 18 single-page app that renders the converted content
served by `encarta-server` ([api.md](api.md)). It embeds the 3D tour runtime ([tour-runtime.md](tour-runtime.md))
and lazy-loads the World Atlas from `web/src/atlas/` (plan Step 5, built separately). Everything on
screen comes from the user's own disc through the server; the repository holds only our code and our own
SVG icons.

## Running

```sh
cargo build --release -p encarta-server
target/release/encarta-server --out out --listen 127.0.0.1:8080      # API, media, tours, atlas data

cd web
npm install                  # once; every version is pinned
npm run dev                  # http://localhost:5173/  (proxies /api, /media, /out, /healthz)
npm test                     # vitest: synthetic unit and component tests (+ tour/atlas suites)
npm run build                # tsc --noEmit + vite build → web/dist
```

`ENCARTA_SERVER=http://host:port npm run dev` points the proxy at another server (`/api`, `/media`, `/fonts`,
`/healthz`, `/out`). Ruffle (`@ruffle-rs/ruffle` 0.7.1, pinned) is self-hosted: a small Vite plugin serves the
package's files at `/ruffle/` in dev and copies them (wasm cores and licences included, no source maps) to
`dist/ruffle/` in the build; nothing loads it until an interactive is opened. `ENCARTA_DEV_OUT=1`
makes Vite serve `../out` under `/out/` itself, so the tours dev page works without the Rust server.
For production, the server serves the build: `encarta-server --static web/dist`. Client routes are real
paths, so the server's `index.html` fallback is required. `/tours.html` (the tour dev page) is also
built; `/atlas.html` (the atlas dev page) is served by the dev server only. In dev, a middleware sends
`/tours` and `/atlas` to the SPA, because Vite would otherwise answer them with those dev pages.

## Routes

| path | view |
|---|---|
| `/` | Premium home screen (home item 701712539: its links, logo, decoration and background) |
| `/browse/{id}` | a category screen (`home-page` item, `grid`/`list` subgroups, separators with captions) |
| `/item/{id}[#p{para}]` | any item: article reader, sidebar/media page, category screen, or redirect (Kids-only items go to `/kids/item`, tour items to `/tours/{tour}`) |
| `/item/{tour}?scene=&yaw=` | a panoramic tour at a scene, looking at `yaw` degrees ([Panoramas](#panoramas)) |
| `/item/{id}/media` | Show All Media for an article |
| `/search?q=&kind=&offset=` | full search with category tabs (facets), did-you-mean and spelling fallback |
| `/media?q=&type=` | Media Gallery: search restricted to multimedia, by item kind |
| `/dictionary`, `/thesaurus`, `/translate?pair=`, `/dictionary/{dict}/{id}`, `/dictionary/entry/{id}` | Encarta Dictionaries (`entry` = any dictionary, by id) |
| `/timeline?from=&to=&category=&region=&q=` | Timeline |
| `/tours`, `/tours/{tourId}` | 3D Virtual Tours list and a tour (`mountTour`) |
| `/atlas`, `/atlas/place/{gid}`, `/atlas?url=msencarta://…`, `/atlas?lat=&lon=&alt=[&style=][&view=flat]` | World Atlas (`src/atlas/`), or a placeholder listing the place's links when that module is missing. The map writes its view back into the URL (`onViewChange`, debounced, replace); only URL changes it did not write remount it |
| `/kids`, `/kids/browse/{id}`, `/kids/item/{id}`, `/kids/search`, `/kids/dictionary` | Encarta Kids |
| `/favorites` | Organize Favorites (rename, reorder, remove; kept in localStorage) |
| `/help` | About and keyboard shortcuts |
| any route `?media={id}` | the media viewer overlay on top of that page (Back closes it) |

## Layout

| path | role |
|---|---|
| `src/main.tsx`, `src/app/` | router, chrome (`Shell`: menu bar File/Edit/Favorites/Help, Back/Forward/Home orbs, Features menu, search, skip link), history button state, disc fonts (`fonts.ts`), Favorites store (`favorites.ts`), menu commands (`commands.ts`: print and copy by scope, copyright line), Features flyout (`FeaturesMenu.tsx`, data in `featuresData.ts`: the VM's 3 groups, More flyouts and Browse all by content id), keyboard shortcuts (`shortcuts.ts`), print/copy scope dialog and Options dialog, lazy atlas (`atlasModule.tsx`); `src/settings/`: Options store (localStorage) |
| `src/api/` | typed client for api.md (`client.ts`, in-memory cache, AbortSignal), `useApi` hook, response types |
| `src/content/` | the content model of content-schema §8 (`model.ts`), React inline renderer (`Inlines.tsx`), link mapping |
| `src/article/` | article reader: body renderer, layout rules (`layout.ts`), media column boxes, side panels, find bar, pronunciation |
| `src/media/` | media viewer overlay, players (audio with captions, audio sets, video with WebVTT, MIDI), variant selection (`pick.ts`), MIDI reader and synth, Ruffle loader (`ruffle.ts`), its device fonts (`ruffleFonts.ts`) and Flash interactives (`FlashPlayer.tsx`) |
| `src/pages/` | home, browse, item dispatcher (+ `itemRoute.ts`: feature records, topic timelines, panoramas, Map Trek stops), views of the other item kinds (`ItemKinds.tsx`: web links, contributors, quotation authors, tour nodes, Map Trek, online features), panoramic tours (`PanoramaTour.tsx`, `panorama.ts`), search, media gallery, dictionary (+ `wordWheel.ts`), timeline (+ pure layout/date helpers), tours, favorites, help, atlas placeholder |
| `src/panorama/` | the native panorama player ([Panoramas](#panoramas)): cylinder geometry, WebGL and Canvas 2D renderers, `PanoramaView`, parameters from the data, the image-SWF reader, single panoramas in the media viewer |
| `src/components/Menu.tsx` | the accessible menu button (arrows, Home/End, Esc, ←/→ along a `menubar`) used by every menu |
| `src/kids/` | Kids shell, home screen and article view |
| `src/styles/` | CSS by area; tokens in `base.css` |
| `src/tours/` | the tour runtime (Step 6), unchanged |
| `src/atlas/` | the atlas (Step 5), owned separately; loaded with `import.meta.glob` so the client builds without it |

## Rendering decisions

- **Articles are rendered from `body` JSON**, not the pre-rendered `html`, so sections can collapse and
  media can be laid out. Every inline node of content-schema §8 has a React view; an unknown node shows
  its text. Cross-reference prefixes ("See also ", "(" … ")") follow content-schema §2. Other `*_html`
  fragments (captions, tables, dictionary entries, search snippets) are inserted as HTML; clicks on their
  internal links are routed client-side (`components/Html.tsx`).
- **Page layout** comes from `article.layout` (api.md, content-schema §2): `ArticlePage` → `ArticleByLayout` picks the
  encyclopedia view, the Kids view (`kids/KidsArticle.tsx`) or the sidebar view, on `/item/…` and `/kids/item/…` alike.
- **Placement is the converter's** (schema 2, content-schema §2 rules): the client draws a `media` block in the
  **text column** (`col: "text"`: the `image`, else the picon, scaled to at most 432 px) or as a **margin box**
  (`col: "margin"`, picon) floated into the 248 px gutter beside the paragraph it precedes, and a `cluster` as one
  margin box showing one member at a time with a row of thumbnails (VM, Zebra: Grévy's Zebra + Mountain Zebra; Plains
  Zebra + Zebra Quick Facts). The client does no grouping or width guessing of its own. By group (`article/layout.ts`
  `placementOf`): a lone facts table (`table`/`factsandfigures` whose data table is `facts`) goes only to the right
  column as **Quick Facts** (a facts table inside a cluster is also a member of the box, as in the VM); `tableinline`
  tables are drawn in the text, reaching 90 px into the margin as in the VM (East Timor); `sidebar` items are
  **sidebar teasers** in the text column (`enc-sidebar-teaser`: blue "Sidebars" bar, the category from the
  association's `sidebar_category`, the title, its `preview_html`, "open sidebar").
- **Kids layout**: orange title and "Look at Pictures", no section numbers and no (−)/(+) toggles, and the stored
  `media-grid` as the two-column tile grid on the left (in its `sequence` order; the viewer steps through the tiles
  first). Inline tables stay in the text.
- **Sidebar layout** (VM, Customs of Russia): a purple header band ("Appears In", the "sidebar" banner, the title),
  then the body with the sidebar's media cluster first (thumbnails above the picture) and the text beside it; no
  media strip, outline tools or right column. "Appears In" lists the item's `appears_in` (one `/api/item` request,
  on demand).
- **Media boxes need no item requests**: picture (`picon`, or `image` for wide boxes), sizes, kind and title come
  from the article's association rows (api.md `AssocItem`); only audio boxes fetch their item, for the playable
  files. Kids media tiles do the same.
- **Media strip and viewer sequence**: placed body media in reading order (`mediaInOrder` walks cluster and grid
  members in place), then other shown associations not placed in the body (not maps, weblinks, further reading,
  contributors). The viewer steps through it.
- **Right column**: World Atlas (first `maps` association; links to `/atlas/place/{geocode}`), Quick
  Facts (a facts table's first five entries, or a Facts and Figures table's entries marked `quick`;
  "More" opens the whole table), Related Articles (the REL tree; headings expand), Further Reading.
  Contributors (`expert` group) print under the text.
- **"X or Y" titles**: a `title-ref` is drawn from its `title` and `alt` parts as two bold spans joined by a plain
  " or " (VM: "**East Timor** or **Timor-Leste**, …"); with an empty `alt` it is the title alone.
- **Outline view** collapses every section to its heading and child headings; the introduction shows a
  three-line preview with "more…". Each section's (−)/(+) toggles it. A deep link `#p{n}` expands the
  sections on its path, scrolls and flashes the paragraph.
- **Find in Article** (button or Ctrl+F on an article) highlights matches with the CSS Custom Highlight API.
- **Pronunciation**: when the English dictionary has a headword equal to the article title with spoken
  audio, a speaker button follows the title. Dictionary entries show the same button.
- **Home screens** are data: the Premium home is item 701712539 (its `hthm` theme pieces give the logo and
  butterfly decoration; `hsbg` the background). 701712540 and 701750687 are the Standard edition's
  variants. Separators (`hthm` with `Separator` subgroups) split the links into columns/groups.
- **Kids home**: the 1024×600 background has ten panels; each category page's `hthm` "(top category)"
  picture is drawn on its panel (slot positions measured from the background), with the mouse-over
  picture on hover.
- **Audio**: Opus, then AAC, from the item's `audio` file; captions (`cc` → WebVTT) are shown as text
  under the player. Audio sets (`audionested`) are children: a picture plus audio items. **MIDI** (236
  music files, e.g. national anthems) plays through our own SMF reader and a small WebAudio synth
  (oscillator voice per GM family, noise drums); the plan's soundfont synth would replace the voices.
- **Video**: WebM (VP9) then MP4, the converter's poster, WebVTT captions as a `<track>`.
- **Dictionary**: the word wheel is `lookup?around=8`: the eight headwords before the typed text, then the following
  ones, in the reader's order (hyphens ignored), homographs numbered "Paris (1)", "Paris (2)" (`wordWheel.ts`), as in
  the VM's `dict-paris` capture; the entry is the first exact match or an explicit id. An entry whose guessed
  dictionary is wrong is fetched again by id alone (`/api/dictionary/entry/{id}`). Grammar codes (`vi`, `n` …) are spelled out as the VM shows them. Links inside an entry open the
  same dictionary, or for bilingual translations the reverse pair (`ENG_FRA` → `FRA_ENG`); a link whose id
  is not on the disc falls back to looking up its text.
- **Timeline**: the band shows the 400 most prominent events of the window (one `order=importance` request), in
  lanes so labels do not overlap; the list below pages through every event in date order. Both ask for `text=1`, so
  selecting an event shows its text without fetching its article. The band is black like the VM's view and draws
  each event by its `display` (timeline.md §3.2): a point is a category-coloured square and the title, sized by
  `prominence` (importance 1 largest); a period an orange bar from start to end with its boxed upper-case `label`; an
  essay a two-tone striped bar with "HISTORICAL ESSAY: …". Dates (band tooltips, list, pop-up) are the converter's
  `date_text` ("4? BC - AD 29?"). Eras jump between windows of very
  different spans (4.7 billion years ago to 2006).
- **Tours**: `TourView` mounts `mountTour` with `baseUrl: "/out/tours"`, maps the labels checkbox to
  `2D.ON`/`2D.OFF`, and opens `HOTSPOTCLICKED` ids in a side panel (`/api/item/{id}`).
- **Search results and the Media Gallery** draw the hits' `thumb`s (api.md), no per-hit requests.
- **Interactives** (`.iax`, 860 media items, all `group="flash"`): the viewer runs the disc's own IAF player under
  Ruffle: `base.swf` from `/media/disc/{edition}/dswMedia/shared/players/IAFPlayer/` with the FlashVar
  `filePath=/media/disc/{edition}/dswMedia/IAF/a/{id}/{id}.iax`. It reads the `.iax`, loads its control
  (`SlideShowControl.swf`, `MatchGame.swf` …), skins, `flashvr.swf` and the slide media from that tree by relative URL
  (found by black-box observation of its requests). The player is created on open and removed on close. The 194
  panorama interactives (DATA `subgroup="360"`, exactly the `.iax` files with a `vrslide`) are drawn by the native
  panorama player instead ([Panoramas](#panoramas)); Ruffle remains their fallback. **Device fonts**
  (`media/ruffleFonts.ts`): the players draw text in device fonts (`.iax` text styles: Trebuchet MS 4,229, Tahoma
  2,757, Georgia 1,228, Arial 1,179, Verdana 351, Times New Roman 5), which Ruffle replaces with its wider Noto
  Sans, clipping single-line fields. The disc's own fonts from `/api/meta` `fonts` are handed to Ruffle
  (`fontSources`, fetched once per page): Tahoma (also for Trebuchet MS, which the disc lacks; it sets about as wide),
  Arial (its italic also for Tahoma/Trebuchet italic), Times New Roman, MS Reference Sans Serif as Verdana and MS
  Reference Serif as Georgia (same metrics). Ruffle names a TrueType file by its full name ("Tahoma Bold"), so each
  face is wrapped in a one-tag SWF (`DefineFont4`: family name, bold/italic flags, the TTF) as a Blob URL.
  `defaultFonts` maps `_sans`/`_serif` to Arial/Times New Roman, as Windows Flash did. Lucida Console (15) and
  Impact (4) are not on the disc and fall back.
- Fonts: MS Reference Serif / Sans Serif are named first in the font stacks; when the server lists font files
  (`/api/meta` `fonts`, served from `out/fonts/`, which the content converter fills) `app/fonts.ts` declares the
  `REFSER*`/`REFSAN*` faces; without them the fallbacks (Lucida Bright/Georgia and Verdana/Tahoma/DejaVu Sans) are used.

## Panoramas

`src/panorama/` is our own cylindrical panorama player. It is used by the 16 panoramic tours (`/item/{tour}`,
`pages/PanoramaTour.tsx`, 65 scenes) and by the single panoramas: the 151 `surround` items redirect to their
interactive (`itemRoute.ts`), and the media viewer and the item page draw the 194 `subgroup="360"` interactives
(167 `.iax` files, 152 Deluxe and 15 Kids) with `SinglePanorama` instead of the IAF Flash player.

**Projection** (`geometry.ts`). The picture (W × H) lines the inside of a vertical cylinder of radius
r = W / part (in radians). The viewer stands on the axis at the horizon row. A view is (yaw, f, shift): the picture
angle at the view centre, in degrees from the picture's left edge; the focal length in view pixels; and a vertical
image-plane shift, so verticals stay vertical and the horizon stays straight. View point (x, y), measured from the
centre, sees angle = yaw + atan(x/f) and row = horizon + (y + shift)·r/√(x² + f²). The WebGL renderer evaluates this
per pixel. The Canvas 2D fallback draws 2-pixel columns, each one picture column scaled vertically. It is used when
WebGL or a texture of the picture's size is missing (the widest picture is 7645 px). Hotspots and labels are DOM
elements placed with the inverse mapping on every frame.

**Limits.** The view never shows past the picture's top or bottom: f ≥ H_view·r/H (the "fill" focal length), and
the shift range follows from it. Zoom runs from the widest allowed view (the fill limit, and never wider than 120°)
to `maxzoom` % of the opening view. A full turn (`part` = 360, every disc panorama) wraps; a partial one would clamp
the yaw.

**Where each parameter comes from**

| parameter | single panoramas (`.iax` `vrdata/scene`, image SWF) | tour scenes (DATA scene record) |
|---|---|---|
| picture | the slide's `<image>` SWF (`/api/interactive` `refs` URL, else `dswMedia/IAF/a/{id}/`): its `DefineBitsJPEG2` JPEG. The size comes from the JPEG's SOF marker. All 167 are uncompressed SWF 5 files, 1200–7645 × 183–992 | the `surround` file (`.svj` JPEG). Its size comes from the media manifest's `original` variant, else `default/@width` |
| horizon (eye level) | the image SWF's `DefineShape3` bitmap fill: the shape draws the JPEG at 1/10 scale (matrix scale 2 twips/px) with its origin on one row, horizon = −ty/sy. The SWF's DoAction also sets `scale = 0.1` and `offset` = (H/2 − horizon)·0.1; the two agree on 166 of the 167 (one has no `offset`). The row falls between 11 % and 91 % of the height and follows the subject: lavender field 11 %, Masada 19 %, Québec basilica ceiling 91 % | no field: the middle row *(ours)* |
| cylinder height | derived: the picture height on a cylinder of radius W/(2π), so the picture covers atan(horizon/r) above and atan((H − horizon)/r) below eye level | same derivation |
| angular width | `part` (360 on all 167) | 360 *(ours; the records have no field, and the pictures wrap seamlessly)* |
| field of view | `fov`: the horizontal field of view of the `displaywidth` view, in degrees (80 on 150 scenes, 85/90/100 on 6). `AUTO` (11 scenes) = the fill limit. A value that would show past the picture opens at the fill limit | from the picture width: f ≈ W/2π px on a 624-px view (about 94° for W = 1824) *(VM: vm-findings.md › UI parity pass, Panoramas)* |
| view size | `displaywidth` × `displayheight` (544 × 320–380; equal to the slide's `imagewidth`/`imageheight`), scaled to fit the page | 624 × 289, horizon on the middle row *(VM: vm-findings.md › UI parity pass, Panoramas)* |
| opening yaw | `initangle`: degrees from the picture's left edge to the view centre (180 = the picture's middle, 103 of 167) | x = 0 at the view centre for every scene; `default/@panangle` only orients the site-map view wedge *(VM: vm-findings.md › UI parity pass, Panoramas)*. `lightangle` (always `panangle`/10) is unused |
| zoom-in limit | `maxzoom` % of the opening view (200; one scene has 50, read as "no zoom-in") | 200 % *(ours)* |
| vertical look | `tilt` (`on` everywhere) | on *(ours)* |
| auto-pan | `auto` (3 everywhere), read as degrees per second *(ours: constant on the disc, so its unit cannot be derived)*. It runs when the panorama opens and stops at the first drag, key or button | on at open, 18.3 s per turn in 7.5 jumps a second; a click pauses it and it resumes after 9.5 s; Tour Settings toggles it and the labels and hot spots *(VM: vm-findings.md › UI parity pass, Panoramas)* |
| keyboard pan speed | `maxspeed` (50 everywhere), read as degrees per second at the opening zoom *(ours)* | 50 *(ours)* |
| unused | `quality`, `interlace` (45; FlashVR's own rendering), `hsvisibility` (0; vrslides have no hotspots), `src`, `name`, and `flashvr.xml` (player chrome only: preloader, menu, pan/stop quality; no geometry) | `coord3d`, `refid` |

**Tours.** Hotspots (`shed`) are boxes with an icon and title, always visible while labels and hot spots are on: a
four-way arrow for a scene, a speaker for a picture with a sound (a node with an `audio` media child, 12 of them), a
square for other pictures. Until the first drag of the tour, the VM's instruction ("To rotate the image, hold down your
mouse button and drag in any direction.") runs across the top of the view. A hotspot whose `destmedia` is
a scene of the tour opens that scene (`?scene=`); any other opens the destination's media page (`/item/{id}`),
as the 3D tours do in the VM. Before leaving, the page writes the current yaw into its own history entry
(`?scene=&yaw=`, replace), so Back returns to the same scene and direction. The yaw is also written, debounced,
when a drag or key ends. `quadrant` (four points; 287 of 318 make a 10 × 24 box beside or inside the `shed`, and
19 are cut to 0 width at the picture's right edge) is drawn as a fixed-size marker: blue for scenes, yellow for
media *(ours: the marker's look was never captured)*. Labels are centred on their `x`, `y`. The "Labels and hot
spots" check box hides both. The site map shows the left half of its picture. Its points lead to scenes (large) and
pictures (small) or pictures with a sound (a speaker), and the current scene's point is the "you are here" marker,
with the view wedge (yaw − `panangle`) drawn from it.

**Controls** (both kinds): drag (single panoramas grab; tours turn toward the pointer at 3.75 view px/s per px of
distance from the press point, as measured, vertically too *(ours)*), ←/→ pan, ↑/↓ look up and down, +/− (main row or keypad) and the wheel zoom,
Home returns to the opening view, A toggles auto-pan. There are also buttons for left/right, zoom, Auto and Reset,
and a status line with the hotspot title and the field of view. Keys stop at the view, so the media viewer's ←/→
do not change the item while a panorama has focus.

Checked in Chrome (ports 8098/5198): Westminster Abbey tour (labels on their objects, e.g. "Reredos screen", "Oaken
pulpit"; scene hotspot Nave → Sanctuary; media hotspot "Coronation of Queen Elizabeth II" → its page, Back → the
same scene at yaw 173; site map marker follows the scene; drag, keys, zoom), Alcatraz (tilt to the picture's top,
zoom 80° → 56°), "View from the Great Wall" in the media viewer and on its item page (WebGL, and Canvas 2D forced
once). In the browser, 30 image SWFs (including the extreme sizes and horizons and 3 Kids copies) gave JPEGs whose
decoded size and horizon match an independent survey of all 167.

## Verification

- `npm test`: `src/client.test.ts` (placement by stored column and group, cluster members, Kids grid, sidebar media
  first, links, media selection, home sections, quick facts, timeline bars/lanes/prominence and dates, dictionary
  helpers and word-wheel labels, title-ref parts, font faces, atlas URL, IAF player URLs, WebVTT cues, MIDI reader on
  a synthetic SMF) and `src/components.test.tsx` (jsdom: inline renderer with "X or Y" titles, article body with a
  cluster box, text-column and margin items, sidebar teasers, the Kids layout, no per-box requests, outline and
  inline tables, search box autocomplete, media viewer URL/keyboard behaviour), all on synthetic data.
- In Chrome against the real server: home, History category, Zebra (outline, strip, viewer, audio set,
  Quick Facts, Related Articles, pronunciation), China and East Timor (atlas panel, Facts and Figures
  Quick Facts, inline tables, MIDI anthem), Jim Carrey's sidebar, Zulu Sangoma video, search and
  autocomplete, dictionary/thesaurus/translations, timeline, tours list and the Acropolis tour, Kids home
  and Volcanoes, the atlas route. Compared with `fixtures/articles/{761575094,761575094-outline,
  761573055,461510929,kids-701639680,dict-run,dict-trans-house}` and `fixtures/vm/home.png`.

- Follow-ups checked in Chrome against the real server (ports 8091/5191): East Timor (flag-and-anthem box and photos in
  the text column, "**East Timor** or **Timor-Leste**"), Zebra, Voyage of Vasco da Gama interactive in the viewer,
  the dictionary wheel for "Paris" (same eight words before as `dict-paris`), `/dictionary/entry/{id}`, the timeline
  band and event text, search "french rev", the atlas route's URL sync.

- Schema 2 checked in Chrome against the real server (ports 8095/5195, 1256 px wide): Zebra (both clusters and
  Burchell's Zebra in the text column, as in `761575094`), China (flag-and-anthem box, teasers, clusters), East Timor
  ("**East Timor** or **Timor-Leste**", inline tables into the margin), Kids Volcanoes (the 8 tiles in the capture's
  order, no numbers), Customs of Russia (sidebar view, cluster first, "Appears In"), France (media order of the
  introduction as in `761568934`), the timeline band (periods, essays, "4? BC - AD 29?").

- Ruffle test pass (headless Chromium over the DevTools protocol against the real server, SwiftShader; harness
  kept out of the repo): 46 interactives, every control type in both editions (DragDrop 10, CrossFadeControl 7,
  ZFLControl 5, RFLControl 3, BGAControl 6, MatchGame 2, SlideShowControl 13 incl. narrated and panoramas; 18 Kids
  copies), opened standalone and as `/item/{id}` / `/kids/item/{id}` pages. No 404s; all drew except `MatchGame`.
  Interactions that worked: jigsaw drag (piece follows, a wrong drop returns and counts a try), CrossFade slider and
  arrow ("Propylaea Now" → "Then", WWI before → after), ZFL zoom, pan and locator map (Venus), RFL element pick
  (Periodic Table: Iron), slide-show labels (Castle: Keep, Turret; Vasco da Gama: 1), narrated audio buttons
  (Counterpoint: the MP3 loads and the progress bar runs), BGA boards and questions (games.md). SWQUIZ media: every
  picture and sound of the 1,811 quizzes in each edition (1,274 + 626 references, 968 files) resolves and loads.
  Timeline pop-ups have no Flash here (the timeline is native; their media are the PLAN's Timeline item). Ruffle logs only stubs
  (`System.security.allowDomain`, `Sound.getBytesLoaded/Total`), "Cannot enumerate Undefined" and the device fonts.
  Ruffle's "hardware acceleration is disabled" box appears on first mouse-over only with a CPU (software) adapter.

## Known gaps

- Placement residue is the converter's (content-schema §11.1): country introductions and items the VM carries into
  the next section.
- Essay bars have no thumbnail (the VM shows the essay's picture at the bar's start; the event rows carry no file).
- Inline tables lack the VM's blue box border; the Quick Facts and atlas panels are unchanged.
- Interactives under Ruffle 0.7.1: see the test pass below. `MatchGame` exhausts Ruffle's memory (games.md) and is
  native. In the `BGAControl` boards, picture answers drew blank until the server patched the skin (games.md). Trebuchet MS, the players' main font,
  is approximated by the disc's Tahoma; Lucida Console and Impact by Arial. No VM
  ground truth: Flash is not installed in the guest (vm-findings.md).
- Vasco da Gama's missing ocean fill was seen once (the first headless run against a cold server: the full-frame
  ocean rectangle and the graticule absent, land drawn) and not again in 30 runs (wgpu-webgl, webgl, canvas,
  WebGPU; sizes 460–700 px; throttled network; in the app). Unexplained; watch for it on real GPUs.
- MIDI uses simple synthesized voices, not a General MIDI soundfont.
- Parental Controls of the dictionary window are dropped by choice (PLAN.md).
- Item kinds, menus, Favorites, print and the panoramic tours added by the parity audit: see [parity.md](parity.md).
- Panoramas: none of the player's behaviour has been compared with the VM yet. The open questions are the tour
  scenes' yaw origin, the `quadrant` marker's look, `auto`/`maxspeed` units, the scenes' horizon and field of view,
  and whether 2009 showed hot regions before hover.
- Games live under `/games/*` (`web/src/games/`, see [games.md](games.md)); the Geography Quiz feature item
  701712336 redirects to `/games/geoquiz`. `MatchGame` interactives play natively in the media viewer.
- Light theme only, like the original.
