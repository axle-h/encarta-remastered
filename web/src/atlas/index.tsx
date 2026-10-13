/**
 * The World Atlas (PLAN.md Step 5). The app routes `/atlas` and `/atlas/place/{gid}` here:
 *
 *   <Atlas initialPlace={{ entity: gid }} onOpenArticle={(id) => navigate(`/item/${id}`)} />
 *
 * See docs/atlas-client.md for the props and the design.
 */
export { Atlas } from "./Atlas";
export type { AtlasLink, AtlasPlace, AtlasProps, AtlasViewState } from "./Atlas";
export type { Projection } from "./style";
export { MENU_STYLES, MENU_STYLE } from "./catalog";
export { altitudeForZoom, zoomForAltitude, parseMsEncartaUrl } from "./zoom";
