/**
 * Encarta's tool-button tooltips (docs/vm-findings.md, "Arrow tooltips"): a pale orange box with
 * dark blue text and a darker orange strip holding an arrow that points back at the button.
 * Centred under the button with the strip on top (↑); when that would leave the window it moves
 * to the button's lower right (strip at the left, ↖) or lower left (strip at the right, ↗).
 */
import { cloneElement, useLayoutEffect, useRef, useState, type FocusEvent, type MouseEvent, type ReactElement } from "react";

export type TipPlacement = "below" | "below-right" | "below-left";

/** Where a tip of width `w` goes under an anchor spanning `left`..`right` in a window `vw` wide. */
export function tipPlacement(left: number, right: number, w: number, vw: number, margin = 2): { placement: TipPlacement; x: number } {
  const centre = (left + right) / 2;
  const x = centre - w / 2;
  if (x < margin) return { placement: "below-right", x: right };
  if (x + w > vw - margin) return { placement: "below-left", x: left - w - STRIP };
  return { placement: "below", x };
}

/** Width of the arrow strip when it stands at the side (matches `.arrow-tip-strip`). */
export const STRIP = 14;

const ARROW: Record<TipPlacement, string> = { below: "↑", "below-right": "↖", "below-left": "↗" };
const DELAY_MS = 400;

type Triggerable = ReactElement<{
  onMouseEnter?: (e: MouseEvent) => void;
  onMouseLeave?: (e: MouseEvent) => void;
  onFocus?: (e: FocusEvent) => void;
  onBlur?: (e: FocusEvent) => void;
  onClick?: (e: MouseEvent) => void;
}>;

export function ArrowTip({ text, children }: { text: string; children: Triggerable }) {
  const [anchor, setAnchor] = useState<DOMRect | null>(null);
  const [pos, setPos] = useState<{ placement: TipPlacement; x: number; y: number } | null>(null);
  const timer = useRef<number>();
  const tip = useRef<HTMLDivElement>(null);
  const show = (el: Element) => {
    window.clearTimeout(timer.current);
    timer.current = window.setTimeout(() => setAnchor(el.getBoundingClientRect()), DELAY_MS);
  };
  const hide = () => {
    window.clearTimeout(timer.current);
    setAnchor(null);
    setPos(null);
  };
  useLayoutEffect(() => {
    if (!anchor || !tip.current) return;
    const w = tip.current.offsetWidth;
    const { placement, x } = tipPlacement(anchor.left, anchor.right, w, window.innerWidth);
    setPos({ placement, x, y: anchor.bottom + 2 });
  }, [anchor]);
  useLayoutEffect(() => () => window.clearTimeout(timer.current), []);
  const p = children.props;
  const trigger = cloneElement(children, {
    onMouseEnter: (e: MouseEvent) => {
      p.onMouseEnter?.(e);
      show(e.currentTarget);
    },
    onMouseLeave: (e: MouseEvent) => {
      p.onMouseLeave?.(e);
      hide();
    },
    onFocus: (e: FocusEvent) => {
      p.onFocus?.(e);
      if ((e.currentTarget as HTMLElement).matches(":focus-visible")) show(e.currentTarget);
    },
    onBlur: (e: FocusEvent) => {
      p.onBlur?.(e);
      hide();
    },
    onClick: (e: MouseEvent) => {
      p.onClick?.(e);
      hide();
    },
  });
  const placement = pos?.placement ?? "below";
  return (
    <>
      {trigger}
      {anchor && (
        <div
          ref={tip}
          className={`arrow-tip arrow-tip-${placement}`}
          role="tooltip"
          style={pos ? { left: pos.x, top: pos.y } : { left: -9999, top: 0, visibility: "hidden" }}
        >
          <span className="arrow-tip-strip" aria-hidden="true">
            {ARROW[placement]}
          </span>
          <span className="arrow-tip-text">{text}</span>
        </div>
      )}
    </>
  );
}
