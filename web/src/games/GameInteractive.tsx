/**
 * For the media viewer: an `.iax` interactive that the games module plays natively. MatchGame
 * interactives (whose scripts run away under Ruffle 0.7.1 until its wasm memory is exhausted) are
 * drawn by the games' match player; anything else renders `fallback` (the Ruffle player).
 */
import { lazy, Suspense, type ReactNode } from "react";
import { useApi } from "../api/useApi";
import { getInteractive } from "./api";

const NativeMatch = lazy(() => import("./match/NativeMatch"));

/** Control types the games module plays itself. */
export const NATIVE_CONTROLS = new Set(["MatchGame"]);

export function GameInteractive({ iax, kids = false, title, fallback }: { iax: string; kids?: boolean; title?: string; fallback: ReactNode }) {
  const ia = useApi((s) => getInteractive(iax, kids ? "kids" : undefined, s), [iax, kids]);
  if (ia.loading) return <div className="enc-loading"><div className="enc-spinner" /></div>;
  if (!ia.data || !NATIVE_CONTROLS.has(ia.data.control_type)) return <>{fallback}</>;
  return (
    <Suspense fallback={<div className="enc-loading"><div className="enc-spinner" /></div>}>
      <NativeMatch title={title ?? ia.data.items[0]?.title ?? ia.data.title} definition={ia.data.definition} media={ia.data.media} />
    </Suspense>
  );
}
