# Atlas client (`web/src/atlas/`)

The World Atlas (PLAN.md Step 5) on MapLibre GL JS 5 (`maplibre-gl` 5.24.0, `pmtiles` 4.5.0). It draws the
converter's output (`out/atlas/`, [atlas-output.md](atlas-output.md)) through the content server
([api.md](api.md)) and follows Encarta's behaviour as written up in [mad-styling.md](formats/mad-styling.md) and
seen in the reference-VM shots (`fixtures/atlas/V1…V14`, private).

## Component

```tsx
import { Atlas } from "./atlas";

<Atlas
  initialPlace={{ entity: 25262 }}          // or { lat, lon, altitudeKm } or "msencarta://view=MapView/cmd=map/…"
  initialStyle="atlas-political"            // MapStyle id, slug, MapStyle name or carousel caption ("Political")
  initialProjection="globe"                 // or "flat"
  onOpenArticle={(refId, link) => navigate(`/item/${refId}`)}
  onViewChange={(v) => …}                   // { lat, lon, altitudeKm, zoom, style, styleId, projection, title, entity? }
  highlightEntity={84} highlightColor="#ffd400"  // fill and outline one entity (e.g. a quiz question's country)
  quizMode                                  // no place names (labels, title, tooltips, search), no click menu
  focus={…} markerAt={[lon, lat]} dragPan   // fit a box or fly to a point; target ring; pan tool
  onPlaceClick={(hits, { lat, lon }) => …}  // every click, with the places under the point in menu order
  dataUrl="/out/atlas" apiUrl="/api"        // defaults
/>
```

- The component fills its parent (`height: 100%`); give the route container a height.
- `onOpenArticle(refId, link)` is called for every P2MTBL link the user opens (click menu, "Read Article").
  `link.kindLabel` is `article`, `flag-media`, `photo:people` …; all of them are items that `/item/{refId}` shows.
  Without the callback the component navigates to `/item/{refId}`.
- `initialPlace` is read on mount. To move an open atlas, remount it (the app keys it by route).
- Options (`useSettings()`): `atlasCrosshairs` draws a centre crosshair, `latLongFormat` formats the About this Map
  readout (`atlas/format.ts`), `units` seeds the scale bar and altitude units, `atlasLinearStats` blends stat colours
  linearly between the ramp ends instead of the default equal-width classes.
- `highlightEntity`, `quizMode`, `onPlaceClick`, `focus`, `markerAt` and `dragPan` are what the Geography Quiz uses: the highlight is drawn by extra layers (`encarta-highlight-*`, filter `["==", ["id"], entity]`)
  under the symbols; in quiz mode the symbols and the styling stay, every name goes.
- Dev page: `web/atlas.html` (`?style=&lat=&lon=&alt=`, `?place=<entity>`, `?url=msencarta://…`, `?view=flat`,
  `?quiz=1`, `?highlight=<entity>`), served by `npm run dev`; it is not part of the production build.

## Data and URLs

Styles are fetched from `<dataUrl>/styles/index.json` and `<dataUrl>/styles/<slug>.json`. The converter writes
root-relative URLs (`pmtiles:///atlas/…`, `/atlas/sprites/sprite`, `/atlas/glyphs/…`); `data.ts` rewrites
`/atlas/…` to `<dataUrl>/…` and makes it absolute. URLs the server already made absolute are kept, except
same-origin `…/atlas/…`: behind the Vite dev proxy the server sees Vite's host, where `/atlas/…` is a client
route, so those go through `dataUrl` too. `transformRequest` repeats the rewrite for anything root-relative.

Server endpoints used: `/api/atlas/search`, `/api/atlas/place/{entity}`, `/api/place/{gid}` (links),
`/api/stats`, `/api/stats/indicator/{id}`, and the files under `/out/atlas/`.

## What follows Encarta, and how

| Feature | Implementation |
|---|---|
| Globe view | MapLibre's `globe` projection. |
| Flat view (Miller) | Encarta's flat map is Miller cylindrical (§12.3, VM V12). MapLibre has no custom projections, so `miller.ts` draws Miller with the Mercator renderer in substitute "view latitudes" φ′ (Mercator y(φ′) = Miller y(φ); the sphere fits in ±78.6°). Two protocols re-make the tiles: `encarta-miller-raster://` crops Encarta's own flat images (`miller/<raster>/<look>-<level>.webp`, an affine image in view coordinates) below z4 and remaps the rows of the Mercator raster tiles above (the flat image fills the polar rows beyond ±85.05°); `encarta-miller-mvt://` reads the real vector tiles that a view tile covers (a coarser real zoom where more than 4 rows would be needed), clips them to their own tile, moves every vertex to its view latitude, clips to the view tile and re-encodes (`mvt.ts`, our own MVT codec). Ids and properties are unchanged, so styles, feature-state, hit-testing and the legend work as on the globe; the client converts real ↔ view latitudes wherever it talks to the map. The flat view opens on (and goes no further out than) the whole map fitted to the pane height, as V12 shows (1.97 px/deg in the 518-px pane). Switching keeps place and altitude; the flat view draws half the globe's pixels per km (§2.1, from the code, not measured), one zoom level further out (`FLAT_ZOOM_OFFSET`). |
| Altitude ↔ zoom | Encarta's scale is the true ground scale at the view centre, `P/(2.5·m·a)` px/km at any latitude, with `P/m ≈ 6.05 × pane height` (VM V1/V2: 3135 for its 812 × 518 pane). The client takes P/m from its own map container's height and uses `zoom = log2(40075·cos φ·(P/m)/(1280·a))` (no cos φ in the flat view). The styles are compiled at φ = 0 for P/m 3135, so every layer's zoom range is shifted by `log2(cos φ · P/m / 3135)` (`zoomShift`, rounded to 0.05; recomputed at the end of each move), which makes MapLibre apply Encarta's rule for the displayed altitude. Zoom limits from `encarta:minzoom/maxzoom`, raised inside MinRect boxes (`encarta:min_rects`, checked on every move), shifted alike. |
| Style carousel | The 20 styles of Encarta's carousel in its order, with its captions (V4 panel shots), thumbnails cut from the Miller level-0 images; the other 13 MapStyles under "More styles". `msencarta://…/style=n` numbers map to MapStyle ids (`MENU_STYLE`, from the V4 table in docs/vm-findings.md; 27–33 are style 0 in Encarta; 4, 18, 21–23 and 26, which Encarta uses outside its menu, have no MapStyle identified and open the default style). |
| Terrain look / recolouring | The Customizer's "Terrain look" draws any LOOK of the style's raster. Baked LOOKs use their archive; others (HYPSO `LBU_BORDERS`) are drawn from the `index` archive through `palettes.json` by the `encarta-look://` protocol (own PNG decoder, so CMT values stay exact). |
| Labels and symbols | The converted styles as they are; the Moon's alternative names go on a second line ("Mare Frigoris / Sea of Cold", V4 Moon shot). Label size Large (×1.25) / Normal / None. |
| Hover | Nothing, as in Encarta (V11: only the cursor changes; hovering menu entries highlights nothing either). The Customizer's "Highlight places under the pointer" turns on the old behaviour: feature-state `hover` drawn by the styles' highlight layers (b15/b16/w17; strokes on the seam-free `outlines`), lines and water first, then the country, then other areas, with a name tooltip. The selected place (search, "Map") keeps its highlight. In the Statistical style a coloured country shows its value (V14). |
| Click | A menu of every place under the cursor (points, lines, areas, the country last, as in V11), each with its symbol and a submenu "Map" (go there) and its P2MTBL links (articles first). The hit list is the style's own: classes whose rule at this altitude is interactive (`encarta:interactive`, PDCAltitudeGrid `b4`; tri-state 2 only when the user turns the class on, none when off), named and not anonymous. Areas are hit-tested through invisible fill layers (one per zoom range of the hit list), lines and points on their drawn layers. |
| Title | The country under the centre and its continent ("Germany \| Europe"); below 2,000 km the first-level division and its country ("Wien \| Austria", V9); "World" from 14,000 km; "Moon". "Read Article" opens the title place's article. |
| Search | `/api/atlas/search` as the user types; picking a hit flies to its "go to" altitude (from `/api/atlas/place`) or extent, and selects it. |
| Map Legend | Every class the style draws at this altitude with its symbol, pens or fill, taken from the style layers; by default only classes with features in the loaded tiles. The raster LOOK's RCT colours as a strip. |
| Map Customizer | Feature groups with per-class checkboxes, applied as MOBB's per-class overrides (mad-styling.md §4.3: off = never, on = also where the rule makes it optional). Flat/Globe, labels, terrain look, scale units. |
| Statistics | In the Statistical style, "Choose Statistic" colours the countries from an MSWORLD.SDB indicator with the style's `Z - (STAT) Positive/Negative n` colours (feature-state on Sovereign/CountryRegion areas, ids = SDB country ids), with the sorted-values chart and source line of V14. |
| Locator globe | Orthographic rendering of the LOCATOR (or Moon) equirectangular texture with the view outline. |

## Known differences

- The time zones: Political marks them interactive exactly like the statistical regions (b2 16, b3 0, b4 1), and
  V11's menu lists "Western Europe" but no time zone; this client lists both. Nothing in the rules explains it yet.
- The vector tiles' band is chosen per zoom for the reference pane at the equator; at high latitudes or on a
  taller pane the shifted rules may ask for detail from a band that the tile at that zoom does not carry yet
  (MapLibre overzooms the coarser band).
- The flat view re-tiles in the main thread (MapLibre runs custom protocols there): its first tiles take a
  moment, longest over the large z0–z2 vector tiles. MapLibre's scale bar assumes Mercator, so it is off in the flat
  view away from the equator. The flat view's altitude relation (half the globe's scale) is from the code; V12
  only shows the opening view. Client-side LOOKs that are not baked (HYPSO `LBU_BORDERS`) fall back to the
  style's LOOK in the flat view.
- Highlight fills with a tint ramp use one ramp entry at 60 % (mad-styling.md §6: per-pixel needs baking).
- Statistics binning is linear from 0 to the extreme; Encarta's binning is not known.
- Legend captions are PDCAttribute class names; Encarta's own captions are in ENCARTA.EXE resources. Pens that
  draw only over water are left out of the key.
- Large styles (Atlas Default, 1,024 layers) take a few seconds to load at world zooms.
