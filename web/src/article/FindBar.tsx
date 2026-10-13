import { useEffect, useRef, useState, type RefObject } from "react";
import { Icon } from "../components/Icons";

/** Ranges of every case-insensitive occurrence of `needle` in the text nodes under `root`. */
export function findRanges(root: Node, needle: string): Range[] {
  const out: Range[] = [];
  if (!needle) return out;
  const lower = needle.toLocaleLowerCase();
  const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
  for (let n = walker.nextNode(); n; n = walker.nextNode()) {
    const text = (n.nodeValue ?? "").toLocaleLowerCase();
    let i = text.indexOf(lower);
    while (i >= 0) {
      const r = document.createRange();
      r.setStart(n, i);
      r.setEnd(n, i + needle.length);
      out.push(r);
      i = text.indexOf(lower, i + needle.length);
    }
  }
  return out;
}

type Registry = { set(name: string, h: unknown): void; delete(name: string): void };
function highlights(): Registry | null {
  const c = (globalThis as { CSS?: { highlights?: Registry } }).CSS;
  const H = (globalThis as { Highlight?: new (...r: Range[]) => unknown }).Highlight;
  return c?.highlights && H ? c.highlights : null;
}

/** "Find in Article": highlights matches with the CSS Highlight API and steps through them. */
export function FindBar({ root, onClose }: { root: RefObject<HTMLElement>; onClose: () => void }) {
  const [q, setQ] = useState("");
  const [ranges, setRanges] = useState<Range[]>([]);
  const [cur, setCur] = useState(0);
  const input = useRef<HTMLInputElement>(null);

  useEffect(() => input.current?.focus(), []);

  useEffect(() => {
    const r = root.current && q.length > 1 ? findRanges(root.current, q) : [];
    setRanges(r);
    setCur(0);
  }, [q, root]);

  useEffect(() => {
    const reg = highlights();
    const H = (globalThis as { Highlight?: new (...r: Range[]) => unknown }).Highlight;
    if (!reg || !H) return;
    reg.set("enc-find", new H(...ranges));
    const c = ranges[cur];
    if (c) reg.set("enc-find-current", new H(c));
    else reg.delete("enc-find-current");
    c?.startContainer.parentElement?.scrollIntoView({ block: "center", behavior: "smooth" });
    return () => {
      reg.delete("enc-find");
      reg.delete("enc-find-current");
    };
  }, [ranges, cur]);

  const step = (d: number) => ranges.length && setCur((cur + d + ranges.length) % ranges.length);
  return (
    <form
      className="enc-findbar"
      role="search"
      onSubmit={(e) => {
        e.preventDefault();
        step(1);
      }}
    >
      <Icon name="find" size={14} />
      <input
        ref={input}
        value={q}
        onChange={(e) => setQ(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Escape") onClose();
          if (e.key === "Enter" && e.shiftKey) {
            e.preventDefault();
            step(-1);
          }
        }}
        placeholder="Find in article"
        aria-label="Find in article"
      />
      <span className="enc-findbar-count">{q.length > 1 ? (ranges.length ? `${cur + 1} of ${ranges.length}` : "No matches") : ""}</span>
      <button type="button" onClick={() => step(-1)} aria-label="Previous match">
        <Icon name="chevron" size={14} className="flip" />
      </button>
      <button type="button" onClick={() => step(1)} aria-label="Next match">
        <Icon name="chevron" size={14} />
      </button>
      <button type="button" onClick={onClose} aria-label="Close find">
        <Icon name="close" size={14} />
      </button>
    </form>
  );
}
