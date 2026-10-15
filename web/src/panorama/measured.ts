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
  /** (c) Auto-pan moves the view in jumps, 7 to 8 a second. */
  autoStepsPerSecond: 7.5,
  /**
   * (c) Dragging turns the view at a speed proportional to the pointer's distance from the press
   * point: view px/s per px (least squares through 10 → 38, 25 → 98, 50 → 176, 100 → 380).
   */
  dragRate: 3.75,
  /** (d) The bold white line across the top of the view, removed after the first drag of the tour. */
  instruction: "To rotate the image, hold down your mouse button and drag in any direction.",
  /** (d) Hotspot box colour (181,203,148), black fill. */
  hotspotColour: "#b5cb94",
  /** (e) Site-map view wedge tint (247,227,214). */
  wedgeColour: "rgb(247, 227, 214)",
} as const;

/** (c) Auto-pan speed in degrees per second. */
export const TOUR_AUTO_PAN = 360 / TOUR.secondsPerTurn;

/**
 * Single panoramas in Flash Player 10.3 (docs/vm-findings.md, "Flash pass"): the FlashVR player opens with
 * the view centre `initangle` degrees right of the picture's middle (180 = the seam at x = 0), and its auto-pan
 * moves `auto` view pixels per timer tick of a 5 ms interval that Flash runs 42 to 58 times a second in the VM
 * (five scenes: 22 to 31 °/s on the 544-px view at fov 80).
 */
export const SINGLE = {
  /** Mean auto-pan timer rate measured over five scenes (ticks per second). */
  autoTicksPerSecond: 52,
} as const;
