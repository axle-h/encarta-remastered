/**
 * A single panorama (the 167 `vrslide` interactives behind the 151 `surround` items, e.g. "View
 * from the Great Wall") drawn by the native player instead of the IAF Flash player: the `.iax`
 * gives the FlashVR scene parameters, the image SWF the picture and its horizon.
 */
import { useEffect, useMemo, useState, type ReactNode } from "react";
import { api } from "../api/client";
import type { Interactive, Item } from "../api/types";
import { useApi } from "../api/useApi";
import { PanoramaView } from "./PanoramaView";
import { singleParams, slideImageUrl, vrSlide, type VrSlide } from "./params";
import { readPanoramaSwf, type SwfPanorama } from "./swf";

/** DATA marks the panorama interactives with `subgroup="360"` (194 items; exactly those whose `.iax` has a `vrslide`). */
export function isPanoramaInteractive(item: Pick<Item, "meta">): boolean {
  return (item.meta?.source_attrs as { subgroup?: string } | undefined)?.subgroup === "360";
}

interface Loaded {
  ia: Interactive;
  vr: VrSlide;
  swf: SwfPanorama;
}

async function load(iax: string, kids: boolean, signal: AbortSignal): Promise<Loaded | null> {
  const ia = await api.interactive(iax, kids ? "kids" : undefined, signal);
  const vr = vrSlide(ia.definition);
  if (!vr) return null;
  const res = await fetch(slideImageUrl(ia, vr.image), { signal });
  if (!res.ok) throw new Error(`${vr.image}: HTTP ${res.status}`);
  const swf = await readPanoramaSwf(new Uint8Array(await res.arrayBuffer()));
  return { ia, vr, swf };
}

export function SinglePanorama({ iax, kids = false, title, fallback }: { iax: string; kids?: boolean; title?: string; fallback: ReactNode }) {
  const data = useApi((s) => load(iax, kids, s), [iax, kids]);
  const [url, setUrl] = useState<string | null>(null);
  const swf = data.data?.swf;
  useEffect(() => {
    if (!swf) return;
    const u = URL.createObjectURL(new Blob([swf.jpeg as BlobPart], { type: "image/jpeg" }));
    setUrl(u);
    return () => {
      URL.revokeObjectURL(u);
      setUrl(null);
    };
  }, [swf]);
  const params = useMemo(() => (data.data ? singleParams(data.data.vr) : null), [data.data]);

  if (data.loading) return <div className="enc-loading"><div className="enc-spinner" /></div>;
  // Not a panorama after all, or its picture could not be read: the Flash player is still there.
  if (data.data === null || (data.error && !data.data)) {
    return (
      <>
        {data.error && <p className="enc-muted">The native panorama viewer could not read this panorama ({data.error.message}); using the Flash player.</p>}
        {fallback}
      </>
    );
  }
  if (!data.data || !params || !url || !swf) return <div className="enc-loading"><div className="enc-spinner" /></div>;
  return (
    <div className="pano-single">
      <PanoramaView src={url} width={swf.width} height={swf.height} horizon={swf.horizon} params={params} title={title ?? data.data.ia.title ?? "Panorama"} maxScale={1.5} />
    </div>
  );
}
