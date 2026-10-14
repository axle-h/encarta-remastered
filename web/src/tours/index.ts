/**
 * Encarta 3D virtual tours runtime (plan Step 6). Framework-agnostic: mount a tour into any
 * element and drive it like the original ACTX3DX3 control (`do(command)`, `Event3DX` events).
 */
export { mountTour, type MountOptions, type TourEvent, type TourHandle } from "./runtime";
export { TourControl, type ControlOptions } from "./control";
export type { ViewOptions } from "./render/view";
export { currentPose, decodePose, defaultStop, encodePose, hotspotDestination, poseCommand, type Pose } from "./host";
export { SITE_MAP_WINDOW, drawSiteMap, markerMoved, parseWorldLoaded, siteMapMarker, wedgeAngles, type SiteMapConfig, type SiteMapMarker } from "./sitemap";
export type { TourScene } from "./types";
