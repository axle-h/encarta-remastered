import { useEffect, useState } from "react";

export interface Loadable<T> {
  data: T | undefined;
  error: Error | undefined;
  loading: boolean;
}

/**
 * Runs `load` whenever `deps` change and tracks its result. Stale responses are dropped: the
 * previous request's AbortSignal fires when the dependencies change or the component unmounts.
 */
export function useApi<T>(load: ((signal: AbortSignal) => Promise<T>) | null, deps: unknown[]): Loadable<T> {
  const [state, setState] = useState<Loadable<T>>({ data: undefined, error: undefined, loading: load !== null });
  useEffect(() => {
    if (!load) {
      setState({ data: undefined, error: undefined, loading: false });
      return;
    }
    const ctl = new AbortController();
    setState((s) => ({ data: s.data, error: undefined, loading: true }));
    load(ctl.signal).then(
      (data) => {
        if (!ctl.signal.aborted) setState({ data, error: undefined, loading: false });
      },
      (error: unknown) => {
        if (ctl.signal.aborted) return;
        setState({ data: undefined, error: error instanceof Error ? error : new Error(String(error)), loading: false });
      },
    );
    return () => ctl.abort();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, deps);
  return state;
}
