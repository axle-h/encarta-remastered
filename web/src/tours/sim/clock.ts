/**
 * The fixed simulation tick (tours-3d.md §2.3, §6.9). Display frames of any rate feed elapsed
 * time in; whole ticks of `1/tickHz` come out, so walking, turning, alarms and effects follow
 * real time and not the display refresh rate.
 */
export class FixedTicker {
  private acc = 0;

  constructor(
    readonly tickHz: number,
    /** Longest frame gap honoured; a longer stall (background tab) is not caught up. */
    readonly maxFrameSeconds = 0.25,
    /** Most ticks run for one frame. */
    readonly maxTicksPerFrame = 16,
  ) {}

  /** Ticks to run for a frame `seconds` after the previous one. */
  frame(seconds: number): number {
    const dt = 1 / this.tickHz;
    this.acc += Math.min(this.maxFrameSeconds, Math.max(0, seconds));
    let n = Math.floor(this.acc / dt + 1e-9);
    if (n > this.maxTicksPerFrame) {
      n = this.maxTicksPerFrame;
      this.acc = 0;
    } else {
      this.acc = Math.max(0, this.acc - n * dt);
    }
    return n;
  }
}
