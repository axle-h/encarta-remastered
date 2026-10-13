/**
 * Client routes for the server's link targets (api.md "Conventions"). The pre-rendered HTML and
 * the body JSON link to `/item/{id}[#p{para}]`, `/media/file/{name}` and `/atlas/place/{gid}`.
 * In Kids mode item links stay inside `/kids`.
 */

export function itemHref(ref: number, para?: number | null, kids = false): string {
  const base = kids ? `/kids/item/${ref}` : `/item/${ref}`;
  return para ? `${base}#p${para}` : base;
}

/**
 * Maps an `msencarta://` application URL (stat tables, map views; content-schema §8) to a client
 * route, or null when it has no equivalent yet.
 */
export function appUrlHref(url: string): string | null {
  const m = /^msencarta:\/\/(.*)$/i.exec(url);
  if (!m) return null;
  const parts = new Map<string, string>();
  for (const seg of (m[1] ?? "").split("/")) {
    const eq = seg.indexOf("=");
    if (eq > 0) parts.set(seg.slice(0, eq).toLowerCase(), decodeURIComponent(seg.slice(eq + 1)));
  }
  const id = parts.get("id") ?? parts.get("refid");
  const view = (parts.get("view") ?? "").toLowerCase();
  if (view.includes("map") || view.includes("atlas")) return "/atlas";
  if (id && /^\d+$/.test(id)) return `/item/${id}`;
  return null;
}

/** Rewrites an `/item/...` href from pre-rendered HTML for Kids mode. */
export function rewriteHref(href: string, kids: boolean): string {
  if (kids && href.startsWith("/item/")) return `/kids${href}`;
  return href;
}
