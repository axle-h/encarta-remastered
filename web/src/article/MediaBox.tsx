import { useState } from "react";
import { api } from "../api/client";
import type { AssocItem, FileRef } from "../api/types";
import { useApi } from "../api/useApi";
import { Html } from "../components/Html";
import { Icon, mediaIcon } from "../components/Icons";
import { useMediaViewer } from "../media/MediaViewer";
import { AudioPlayer } from "../media/Players";
import { fileUrl, mediaClass } from "../media/pick";
import { WIDE_MAX_WIDTH } from "./layout";
import { useArticleSequence } from "./sequence";

/** Display size of a picture scaled down to at most `max` px wide. */
export function fitWidth(f: FileRef, max: number): { width?: number; height?: number } {
  if (!f.width || !f.height || f.width <= max) return { width: f.width, height: f.height };
  return { width: max, height: Math.round((f.height * max) / f.width) };
}

/**
 * A media box: in the left margin column, one picture at gallery size (`picon`) with its title,
 * and for a stored `cluster` a row of small thumbnails switches between the members; `wide`, an
 * item the converter put in the text column, at up to 432 px (flag-and-anthem boxes, photos, video).
 * Everything comes from the article's association rows; only audio boxes fetch their item (for
 * the playable files).
 */
export function MediaBox({ items, wide = false }: { items: AssocItem[]; wide?: boolean }) {
  const [sel, setSel] = useState(0);
  const cur = items[Math.min(sel, items.length - 1)]!;
  const viewer = useMediaViewer();
  const sequence = useArticleSequence();
  const cls = mediaClass(cur);
  const audio = useApi(cls === "audio" ? (s) => api.item(cur.ref_id, s) : null, [cur.ref_id, cls]);
  const pic = wide ? (cur.image ?? cur.picon) : (cur.picon ?? cur.image);
  const size = pic ? fitWidth(pic, wide ? WIDE_MAX_WIDTH : 216) : {};
  const title = cur.title_html ?? cur.title ?? "";
  const open = () => viewer.open(cur.ref_id, sequence);

  return (
    <figure
      className={`enc-mbox enc-mbox-${cls}${wide ? " enc-mbox-wide" : ""}`}
      data-ref={cur.ref_id}
      data-group={cur.group}
      style={wide && size.width ? { width: size.width + 12 } : undefined}
    >
      <button type="button" className="enc-mbox-pic" onClick={open} aria-label={`Open ${cur.title ?? "media"}`}>
        {pic ? (
          <img
            src={fileUrl(pic)}
            alt=""
            width={size.width}
            height={size.height}
            loading="lazy"
            style={size.width && size.height ? { aspectRatio: `${size.width} / ${size.height}` } : undefined}
          />
        ) : cur.thumb ? (
          <img src={fileUrl(cur.thumb)} alt="" loading="lazy" className="small" />
        ) : (
          <span className="enc-mbox-noimg">
            <Icon name={mediaIcon(cls)} size={36} />
          </span>
        )}
        {cls !== "image" && (
          <span className={`enc-badge enc-badge-${cls}`}>
            <Icon name={mediaIcon(cls)} size={14} />
          </span>
        )}
      </button>
      {cls === "audio" && audio.data && <AudioPlayer item={audio.data} compact />}
      <figcaption>
        <button type="button" className="enc-mbox-title" onClick={open}>
          <Html as="span" html={title} />
        </button>
      </figcaption>
      {items.length > 1 && (
        <div className="enc-mbox-thumbs" role="tablist">
          {items.map((m, i) => (
            <button
              key={m.ref_id}
              type="button"
              role="tab"
              aria-selected={i === sel}
              className={i === sel ? "current" : ""}
              onClick={() => setSel(i)}
              title={m.title}
            >
              {m.thumb ? <img src={fileUrl(m.thumb)} alt={m.title ?? ""} loading="lazy" /> : <Icon name={mediaIcon(mediaClass(m))} />}
            </button>
          ))}
        </div>
      )}
    </figure>
  );
}
