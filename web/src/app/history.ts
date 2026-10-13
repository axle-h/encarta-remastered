import { useEffect, useRef, useState } from "react";
import { useLocation, useNavigationType } from "react-router-dom";

/**
 * Whether Back and Forward have somewhere to go. React Router keeps an `idx` in
 * `history.state`; we remember the highest index reachable by Forward (a push truncates it).
 */
export function useHistoryButtons(): { canBack: boolean; canForward: boolean } {
  const location = useLocation();
  const type = useNavigationType();
  const max = useRef(0);
  const [state, setState] = useState({ canBack: false, canForward: false });
  useEffect(() => {
    const idx = (window.history.state as { idx?: number } | null)?.idx ?? 0;
    if (type === "PUSH" || idx > max.current) max.current = idx;
    setState({ canBack: idx > 0, canForward: idx < max.current });
  }, [location, type]);
  return state;
}
