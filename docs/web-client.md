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

`ENCARTA_SERVER=http://host:port npm run dev` points the proxy at another server. `ENCARTA_DEV_OUT=1`
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
| `/item/{id}/media` | Show All Media for an article |
| `/search?q=&kind=&offset=` | full search with category tabs (facets), did-you-mean and spelling fallback |
| `/media?q=&type=` | Media Gallery: search restricted to multimedia, by item kind |
| `/dictionary`, `/thesaurus`, `/translate?pair=`, `/dictionary/{dict}/{id}` | Encarta Dictionaries |
| `/timeline?from=&to=&category=&region=&q=` | Timeline |
| `/tours`, `/tours/{tourId}` | 3D Virtual Tours list and a tour (`mountTour`) |
| `/atlas`, `/atlas/place/{gid}`, `/atlas?url=msencarta://…`, `/atlas?lat=&lon=&alt=` | World Atlas (`src/atlas/`), or a placeholder listing the place's links when that module is missing |
| `/kids`, `/kids/browse/{id}`, `/kids/item/{id}`, `/kids/search`, `/kids/dictionary` | Encarta Kids |
| any route `?media={id}` | the media viewer overlay on top of that page (Back closes it) |

## Layout

| path | role |
|---|---|
| `src/main.tsx`, `src/app/` | router, chrome (`Shell`: Back/Forward/Home orbs, Features menu, search), history button state |
| `src/api/` | typed client for api.md (`client.ts`, in-memory cache, AbortSignal), `useApi` hook, response types |
| `src/content/` | the content model of content-schema §8 (`model.ts`), React inline renderer (`Inlines.tsx`), link mapping |
| `src/article/` | article reader: body renderer, layout rules (`layout.ts`), media column boxes, side panels, find bar, pronunciation |
| `src/media/` | media viewer overlay, players (audio with captions, audio sets, video with WebVTT, MIDI), variant selection (`pick.ts`), MIDI reader and synth |
| `src/pages/` | home, browse, item dispatcher, search, media gallery, dictionary, timeline (+ pure layout/date helpers), tours, atlas placeholder |
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
- **Placement of `media` blocks** (`article/layout.ts`): facts tables (`table`/`factsandfigures` groups
  whose data table is `facts`) go to the right column as **Quick Facts**; `tableinline` tables are drawn
  in the text; everything else goes to the **left media column**. Consecutive media blocks become one box
  with a row of small thumbnails, as in the VM's Zebra capture (Grévy's zebra + mountain zebra sound).
  Boxes float into a 248 px gutter beside the paragraph they precede.
- **Media strip and viewer sequence**: placed body media in reading order, then other shown associations
  not placed in the body (not maps, weblinks, further reading, contributors). The viewer steps through it.
- **Right column**: World Atlas (first `maps` association; links to `/atlas/place/{geocode}`), Quick
  Facts (a facts table's first five entries, or a Facts and Figures table's entries marked `quick`;
  "More" opens the whole table), Related Articles (the REL tree; headings expand), Further Reading.
  Contributors (`expert` group) print under the text.
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
- **Dictionary**: the word wheel is a prefix lookup; the entry is the first exact match or an explicit
  id. Grammar codes (`vi`, `n` …) are spelled out as the VM shows them. Links inside an entry open the
  same dictionary, or for bilingual translations the reverse pair (`ENG_FRA` → `FRA_ENG`); a link whose id
  is not on the disc falls back to looking up its text.
- **Timeline**: the band shows the most important events of the window (the importance floor rises until
  at most 400 remain), in lanes so labels do not overlap; the list below pages through every event in date
  order. Selecting one shows its text (the event item's article body). Eras jump between windows of very
  different spans (4.7 billion years ago to 2006).
- **Tours**: `TourView` mounts `mountTour` with `baseUrl: "/out/tours"`, maps the labels checkbox to
  `2D.ON`/`2D.OFF`, and opens `HOTSPOTCLICKED` ids in a side panel (`/api/item/{id}`).
- Fonts: MS Reference Serif / Sans Serif are named first in the font stacks (they are on the disc but not
  served); otherwise Lucida Bright/Georgia and Verdana/Tahoma/DejaVu Sans.

## Verification

- `npm test`: `src/client.test.ts` (layout, links, media selection, home sections, quick facts, timeline
  layout and dates, dictionary helpers, WebVTT cues, MIDI reader on a synthetic SMF) and
  `src/components.test.tsx` (jsdom: inline renderer, article body with grouping, outline and inline
  tables, search box autocomplete, media viewer URL/keyboard behaviour), all on synthetic data.
- In Chrome against the real server: home, History category, Zebra (outline, strip, viewer, audio set,
  Quick Facts, Related Articles, pronunciation), China and East Timor (atlas panel, Facts and Figures
  Quick Facts, inline tables, MIDI anthem), Jim Carrey's sidebar, Zulu Sangoma video, search and
  autocomplete, dictionary/thesaurus/translations, timeline, tours list and the Acropolis tour, Kids home
  and Volcanoes, the atlas route. Compared with `fixtures/articles/{761575094,761575094-outline,
  761573055,461510929,kids-701639680,dict-run,dict-trans-house}` and `fixtures/vm/home.png`.

## Known gaps

- Wide pictures that the VM shows in the text column (China's and East Timor's flag-and-anthem boxes)
  stay in the left column.
- The VM's media order within a section differs by a position or two in some articles (content-schema
  §11.2): the client keeps the converter's order.
- Interactives (`.iax`, Flash) show their picture and a note; Ruffle is not integrated yet.
- MIDI uses simple synthesized voices, not a General MIDI soundfont.
- The dictionary word wheel lists only words after the typed text (the VM also shows the words before).
- The title-ref "X or Y" prints both titles in bold (the VM bolds only the titles, not "or").
- Search results have no thumbnails (the API does not return them); a media box needs one
  `/api/item` request per item to get its gallery picture.
- Copy, Print and Parental Controls of the dictionary window, Favorites, and the Features menu's
  games (MindMaze, quizzes) are not implemented (Step 7).
- Light theme only, like the original.
