/**
 * Encarta 3D virtual tours runtime (plan Step 6). Framework-agnostic: mount a tour into any
 * element and drive it like the original ACTX3DX3 control (`do(command)`, `Event3DX` events).
 */
export { mountTour, type MountOptions, type TourEvent, type TourHandle } from "./runtime";
export { TourControl, type ControlOptions } from "./control";
export type { ViewOptions } from "./render/view";
export { parseWorldLoaded, siteMapMarker, type SiteMapConfig } from "./sitemap";
export type { TourScene } from "./types";
