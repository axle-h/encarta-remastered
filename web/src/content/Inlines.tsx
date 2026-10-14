import { Fragment, type ReactNode } from "react";
import { Link } from "react-router-dom";
import { appUrlHref, itemHref } from "./links";
import { linkPrefix, linkSuffix, titleRefParts, type Inline, type InlineNode } from "./model";

export interface RenderOptions {
  kids?: boolean;
  /** Called for an inline picture id (`000f3bca`) to get its URL. */
  imageUrl?: (id: string) => string;
}

const defaultImage = (id: string) => `/media/file/${id.toLowerCase()}.gif`;

/** External URL for a web link, or null when it is not a usable URL. */
export function externalUrl(url: string): string | null {
  if (/^(https?|ftp):\/\//i.test(url)) return url;
  if (/^www\./i.test(url)) return `http://${url}`;
  return null;
}

function textOf(c: string | Inline[]): Inline[] {
  return typeof c === "string" ? [c] : c;
}

export function Inlines({ c, opts = {} }: { c: Inline[] | undefined; opts?: RenderOptions }) {
  if (!c) return null;
  return (
    <>
      {c.map((n, i) => (
        <InlineView key={i} n={n} opts={opts} />
      ))}
    </>
  );
}

function LinkView({ n, opts }: { n: Extract<InlineNode, { t: "link" }>; opts: RenderOptions }) {
  const pre = linkPrefix(n.kind);
  const suf = linkSuffix(n.kind);
  const inner = <Inlines c={n.c} opts={opts} />;
  let a: ReactNode;
  if (n.kind === "web" || (n.ref === undefined && n.url)) {
    const ext = n.url ? externalUrl(n.url) : null;
    const app = n.url ? appUrlHref(n.url) : null;
    if (ext) {
      a = (
        <a className="enc-link enc-web" href={ext} target="_blank" rel="noopener noreferrer">
          {inner}
        </a>
      );
    } else if (app) {
      a = (
        <Link className="enc-link enc-app" to={app}>
          {inner}
        </Link>
      );
    } else {
      a = <span className="enc-link enc-dead">{inner}</span>;
    }
  } else if (n.ref) {
    a = (
      <Link className={`enc-link enc-${n.kind}`} to={itemHref(n.ref, n.para, opts.kids)} data-ref={n.ref}>
        {inner}
      </Link>
    );
  } else {
    a = <span className="enc-link enc-dead">{inner}</span>;
  }
  return (
    <>
      {pre === "(" ? "(" : pre ? <span className="enc-see">{pre}</span> : null}
      {a}
      {suf}
    </>
  );
}

export function InlineView({ n, opts = {} }: { n: Inline; opts?: RenderOptions }): ReactNode {
  if (typeof n === "string") return n;
  switch (n.t) {
    case "em":
      return (
        <em>
          <Inlines c={n.c} opts={opts} />
        </em>
      );
    case "strong":
      return (
        <strong>
          <Inlines c={n.c} opts={opts} />
        </strong>
      );
    case "sub":
      return (
        <sub>
          <Inlines c={n.c} opts={opts} />
        </sub>
      );
    case "sup":
      return (
        <sup>
          <Inlines c={n.c} opts={opts} />
        </sup>
      );
    case "u":
      return (
        <u>
          <Inlines c={n.c} opts={opts} />
        </u>
      );
    case "smallcaps":
      return (
        <span className="enc-sc">
          <Inlines c={n.c} opts={opts} />
        </span>
      );
    case "title-ref": {
      // Both titles are bold, the joining " or " is plain (VM: "East Timor or Timor-Leste").
      const { title, alt } = titleRefParts(n);
      return (
        <>
          <span className="enc-title-ref">
            <Inlines c={title} opts={opts} />
          </span>
          {alt.length > 0 && (
            <>
              {" or "}
              <span className="enc-title-ref enc-alt-title">
                <Inlines c={alt} opts={opts} />
              </span>
            </>
          )}
        </>
      );
    }
    case "br":
      return <br />;
    case "link":
      return <LinkView n={n} opts={opts} />;
    case "img":
      return (
        <img
          className={`enc-inline-img enc-place-${n.placement ?? "inline"}`}
          src={(opts.imageUrl ?? defaultImage)(n.id)}
          alt=""
          loading="lazy"
        />
      );
    case "frac":
      return (
        <span className="enc-frac" aria-label={`${n.num}/${n.den}`}>
          <sup>{n.num}</sup>
          {"⁄"}
          <sub>{n.den}</sub>
        </span>
      );
    case "stack":
      return (
        <span className="enc-stack">
          <span className="enc-stack-top">{n.top}</span>
          <span className="enc-stack-bottom">{n.bottom}</span>
        </span>
      );
    case "radical":
      return (
        <span className="enc-radical">
          {"√"}
          <span className="enc-radicand">
            <Inlines c={textOf(n.c)} opts={opts} />
          </span>
        </span>
      );
    case "overline":
    case "vector":
      return (
        <span className={`enc-${n.t}`}>
          <Inlines c={textOf(n.c)} opts={opts} />
        </span>
      );
    case "place":
      return n.gid ? (
        <Link className="enc-place" to={`/atlas/place/${n.gid}`}>
          <Inlines c={n.c} opts={opts} />
        </Link>
      ) : (
        <span className="enc-place">
          <Inlines c={n.c} opts={opts} />
        </span>
      );
    case "quote":
      return (
        <blockquote className={`enc-quote enc-align-${n.align ?? "left"}`}>
          <Inlines c={n.c} opts={opts} />
        </blockquote>
      );
    case "list": {
      const items = n.items.map((it, i) => (
        <li key={i}>
          <Inlines c={it} opts={opts} />
        </li>
      ));
      const cls = `enc-list enc-list-${n.style ?? "plain"}`;
      return n.style === "numbered" ? <ol className={cls}>{items}</ol> : <ul className={cls}>{items}</ul>;
    }
    case "formula":
      return (
        <div className="enc-formula">
          <Inlines c={n.c} opts={opts} />
        </div>
      );
    case "unknown":
      return (
        <span className={n.name === "note" ? "enc-note" : "enc-unknown"}>
          <Inlines c={n.c} opts={opts} />
        </span>
      );
    default: {
      // A node type this client does not know yet: show its text rather than dropping it.
      const any = n as { c?: Inline[] | string };
      return any.c ? <Fragment>{<Inlines c={textOf(any.c)} opts={opts} />}</Fragment> : null;
    }
  }
}

/** True when inline content holds block-level nodes (then the paragraph must be a div). */
export function hasBlockContent(c: Inline[]): boolean {
  return c.some((n) => typeof n !== "string" && (n.t === "quote" || n.t === "list" || n.t === "formula"));
}
