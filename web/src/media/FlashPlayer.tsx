import { useEffect, useRef, useState } from "react";
import { iafPlayer, loadRuffle } from "./ruffle";
import { DEFAULT_FONTS, ruffleFontSources } from "./ruffleFonts";

/**
 * An `.iax` interactive run by the disc's IAF Flash player under Ruffle, at its own size
 * (`width`×`height` from the DATA `item` file), scaled down to fit the viewer. The disc's fonts
 * stand in for the device fonts the players name (`ruffleFonts.ts`).
 */
export function FlashInteractive({ iax, width, height, kids = false }: { iax: string; width: number; height: number; kids?: boolean }) {
  const box = useRef<HTMLDivElement>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const el = box.current;
    if (!el) return;
    let cancelled = false;
    let player: HTMLElement | null = null;
    setError(null);
    Promise.all([loadRuffle(), ruffleFontSources()])
      .then(([r, fontSources]) => {
        if (cancelled) return;
        const p = r.newest().createPlayer();
        p.className = "enc-flash-player";
        el.appendChild(p);
        player = p;
        const { url, filePath } = iafPlayer(iax, kids ? "kids" : "deluxe");
        return p.ruffle().load({ url, parameters: { filePath }, fontSources, defaultFonts: DEFAULT_FONTS });
      })
      .catch((e: unknown) => !cancelled && setError(e instanceof Error ? e.message : String(e)));
    return () => {
      cancelled = true;
      // Removing the element shuts the Ruffle instance down.
      player?.remove();
    };
  }, [iax, kids]);

  return (
    <div className="enc-flash" style={{ aspectRatio: `${width} / ${height}`, maxWidth: width }}>
      <div className="enc-flash-box" ref={box} />
      {error && <p className="enc-error">The interactive could not start ({error}).</p>}
    </div>
  );
}
