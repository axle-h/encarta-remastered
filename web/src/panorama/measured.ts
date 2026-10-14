/**
 * Constants measured on the original program's panoramic tours (docs/vm-findings.md, section
 * "Panoramas (2D virtual tours and singles)", items (a) to (e)).
 */
export const TOUR = {
  /** (a)/(b) The view (picture area) in screen pixels. */
  viewWidth: 624,
  viewHeight: 289,
  /** (a) Opening yaw: the picture's left edge (x = 0) is at the view centre, whatever `panangle` is. */
  openYaw: 0,
  /** (c) Auto-pan: one full turn in this many seconds (angular, so independent of the picture width). */
  secondsPerTurn: 18.3,
  /** (c) A click on the view pauses auto-pan; it resumed by itself 9 to 10 s later. */
  autoResumeMs: 9500,
  /** (d) Hotspot box colour (181,203,148), black fill. */
  hotspotColour: "#b5cb94",
  /** (e) Site-map view wedge tint (247,227,214). */
  wedgeColour: "rgb(247, 227, 214)",
} as const;

/** (c) Auto-pan speed in degrees per second. */
export const TOUR_AUTO_PAN = 360 / TOUR.secondsPerTurn;
