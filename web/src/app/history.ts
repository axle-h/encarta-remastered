import { useEffect, useRef, useState } from "react";
import { useLocation, useNavigationType } from "react-router-dom";
import { pageTitle } from "./favorites";

/** Page titles by history index, for the Back/Forward tooltips ("Back to Animals"). */
const titles = new Map<number, string>();

const currentIdx = () => (window.history.state as { idx?: number } | null)?.idx ?? 0;

export interface HistoryButtons {
  canBack: boolean;
  canForward: boolean;
  /** Title of the page Back / Forward would show, when known. */
  backTitle?: string;
  forwardTitle?: string;
}

/**
 * Whether Back and Forward have somewhere to go. React Router keeps an `idx` in
 * `history.state`; we remember the highest index reachable by Forward (a push truncates it)
 * and the title each index had.
 */
export function useHistoryButtons(): HistoryButtons {
  const location = useLocation();
  const type = useNavigationType();
  const max = useRef(0);
  const [state, setState] = useState<HistoryButtons>({ canBack: false, canForward: false });
  useEffect(() => {
    const idx = currentIdx();
    if (type === "PUSH" || idx > max.current) max.current = idx;
    const update = () => {
      titles.set(currentIdx(), pageTitle(document.title));
      const i = currentIdx();
      const next: HistoryButtons = { canBack: i > 0, canForward: i < max.current, backTitle: titles.get(i - 1), forwardTitle: titles.get(i + 1) };
      // Same values: keep the old object so a title set on every render cannot loop.
      setState((old) =>
        old.canBack === next.canBack && old.canForward === next.canForward && old.backTitle === next.backTitle && old.forwardTitle === next.forwardTitle ? old : next,
      );
    };
    update();
    // Pages set their title after rendering; follow it.
    const head = document.querySelector("title") ?? document.head;
    if (typeof MutationObserver === "undefined" || !head) return;
    const obs = new MutationObserver(update);
    obs.observe(head, { childList: true, characterData: true, subtree: true });
    return () => obs.disconnect();
  }, [location, type]);
  return state;
}
