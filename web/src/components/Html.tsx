import { useEffect, useRef, type MouseEvent } from "react";
import { useNavigate } from "react-router-dom";
import { rewriteHref } from "../content/links";

/**
 * A pre-rendered HTML fragment from the server (`*_html` fields, content-schema §8). Clicks on
 * internal links are routed client-side; `/item/` links stay in Kids mode when `kids` is set.
 */
export function Html({
  html,
  className,
  kids = false,
  as = "div",
  onItemLink,
}: {
  html: string;
  className?: string;
  kids?: boolean;
  as?: "div" | "span";
  /** Optional interception of `/item/{id}` links (e.g. dictionary cross-references). */
  onItemLink?: (ref: number, para: number | null, a: HTMLAnchorElement) => boolean;
}) {
  const navigate = useNavigate();
  const ref = useRef<HTMLElement>(null);

  useEffect(() => {
    // Make external links open outside the app.
    ref.current?.querySelectorAll<HTMLAnchorElement>("a[href^='http']").forEach((a) => {
      a.target = "_blank";
      a.rel = "noopener noreferrer";
    });
  }, [html]);

  const onClick = (e: MouseEvent) => {
    if (e.defaultPrevented || e.button !== 0 || e.metaKey || e.ctrlKey || e.shiftKey || e.altKey) return;
    const a = (e.target as HTMLElement).closest("a");
    if (!a) return;
    const href = a.getAttribute("href");
    if (!href || !href.startsWith("/")) return;
    if (href.startsWith("/media/")) return;
    e.preventDefault();
    const m = /^\/item\/(\d+)(?:#p(\d+))?/.exec(href);
    if (m && onItemLink && onItemLink(Number(m[1]), m[2] ? Number(m[2]) : null, a)) return;
    navigate(rewriteHref(href, kids));
  };

  const Tag = as;
  return <Tag ref={ref as never} className={className} onClick={onClick} dangerouslySetInnerHTML={{ __html: html }} />;
}
