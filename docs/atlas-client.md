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
  dataUrl="/out/atlas" apiUrl="/api"        // defaults
/>
```

- The component fills its parent (`height: 100%`); give the route container a height.
- `onOpenArticle(refId, link)` is called for every P2MTBL link the user opens (click menu, "Read Article").
  `link.kindLabel` is `article`, `flag-media`, `photo:people` …; all of them are items that `/item/{refId}` shows.
  Without the callback the component navigates to `/item/{refId}`.
- `initialPlace` is read on mount. To move an open atlas, remount it (the app keys it by route).
- Dev page: `web/atlas.html` (`?style=&lat=&lon=&alt=`, `?place=<entity>`, `?url=msencarta://…`, `?view=flat`),
  served by `npm run dev`; it is not part of the production build.

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
| Globe and flat views | MapLibre `globe` and `mercator` projections over the same tiles. The flat map keeps the altitude when switching: Encarta's flat view draws half the globe's pixels per km (mad-styling.md §2.1), so it is one zoom level further out (`FLAT_ZOOM_OFFSET`). |
| Altitude ↔ zoom | The converter's mapping (`zoom = log2(40075·(P/m)/(1280·a))`, P/m from the style's `encarta:view_p_over_m`), at latitude 0 like the styles, so the rules MapLibre applies are the rules Encarta applies at the displayed altitude. Zoom limits from `encarta:minzoom/maxzoom`, raised inside MinRect boxes (`encarta:min_rects`, checked on every move). |
| Style carousel | The 20 styles of Encarta's carousel in its order, with its captions (V4 panel shots), thumbnails cut from the Miller level-0 images; the other 13 MapStyles under "More styles". `msencarta://…/style=n` numbers map to MapStyle ids (`MENU_STYLE`, from the V4 shots). |
| Terrain look / recolouring | The Customizer's "Terrain look" draws any LOOK of the style's raster. Baked LOOKs use their archive; others (HYPSO `LBU_BORDERS`) are drawn from the `index` archive through `palettes.json` by the `encarta-look://` protocol (own PNG decoder, so CMT values stay exact). |
| Labels and symbols | The converted styles as they are; the Moon's alternative names go on a second line ("Mare Frigoris / Sea of Cold", V4 Moon shot). Label size Large (×1.25) / Normal / None. |
| Hover | The place under the cursor gets feature-state `hover`, which the styles' highlight layers (b15/b16/w17) draw: lines and water first, then the country, then other areas (V11: France in Political, Lake Geneva). A tooltip shows the name. |
| Click | A menu of every place under the cursor (points, lines, areas, the country last, as in V11), each with its symbol and a submenu of its P2MTBL links (articles first) and "Go to". Hovering a menu entry highlights it. Areas are hit-tested through an invisible fill layer over the named `areas` features. |
| Title | The country under the centre and its continent ("Germany \| Europe"); below 2,000 km the first-level division and its country ("Wien \| Austria", V9); "World" from 14,000 km; "Moon". "Read Article" opens the title place's article. |
| Search | `/api/atlas/search` as the user types; picking a hit flies to its "go to" altitude (from `/api/atlas/place`) or extent, and selects it. |
| Map Legend | Every class the style draws at this altitude with its symbol, pens or fill, taken from the style layers; by default only classes with features in the loaded tiles. The raster LOOK's RCT colours as a strip. |
| Map Customizer | Feature groups with per-class checkboxes, applied as MOBB's per-class overrides (mad-styling.md §4.3: off = never, on = also where the rule makes it optional). Flat/Globe, labels, terrain look, scale units. |
| Statistics | In the Statistical style, "Choose Statistic" colours the countries from an MSWORLD.SDB indicator with the style's `Z - (STAT) Positive/Negative n` colours (feature-state on Sovereign/CountryRegion areas, ids = SDB country ids), with the sorted-values chart and source line of V14. |
| Locator globe | Orthographic rendering of the LOCATOR (or Moon) equirectangular texture with the view outline. |

## Known differences

- Away from the equator the view is magnified by 1/cos φ against Encarta at the same altitude (styles are compiled
  at φ = 0). V4 at 50° N suggests P/m ≈ 3,200 rather than 2,400 once that is accounted for; a VM measurement
  (V1/V2) should settle it in the converter.
- The flat view is Web Mercator, not Encarta's Miller projection; the Miller images are used for thumbnails only.
- Hit-testing cannot see Encarta's per-style "interactive" flag (b4): it lists named areas and drawn lines and
  points, and leaves out organisation-membership areas, plain graticule lines and world-wide thematic areas the
  style does not draw. Highlights of cell-clipped areas outline the GeoGem cell seams (lakes at 1,000 km).
- Highlight fills use the converter's 50 % placeholder colour; Encarta's tint ramp gives lighter results (V11).
- Statistics binning is linear from 0 to the extreme; Encarta's binning is not known.
- Legend captions are PDCAttribute class names; Encarta's own captions are in ENCARTA.EXE resources.
- Large styles (Atlas Default, 1,024 layers) take a few seconds to load at world zooms.
