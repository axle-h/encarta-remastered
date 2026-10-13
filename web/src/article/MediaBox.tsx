import { useState } from "react";
import { api } from "../api/client";
import type { AssocItem } from "../api/types";
import { useApi } from "../api/useApi";
import { Html } from "../components/Html";
import { Icon, mediaIcon } from "../components/Icons";
import { useMediaViewer } from "../media/MediaViewer";
import { AudioPlayer } from "../media/Players";
import { fileUrl, galleryImage, mediaClass } from "../media/pick";
import { useArticleSequence } from "./sequence";

/**
 * A box in the left media column: one picture at gallery size with its title; when several
 * items were placed together, a row of small thumbnails switches between them.
 */
export function MediaBox({ items }: { items: AssocItem[] }) {
  const [sel, setSel] = useState(0);
  const cur = items[Math.min(sel, items.length - 1)]!;
  const item = useApi((s) => api.item(cur.ref_id, s), [cur.ref_id]);
  const viewer = useMediaViewer();
  const sequence = useArticleSequence();
  const cls = mediaClass(item.data ?? cur);
  const pic = cur.picon ?? galleryImage(item.data);
  const title = cur.title_html ?? cur.title ?? item.data?.title ?? "";
  const open = () => viewer.open(cur.ref_id, sequence);

  return (
    <figure className={`enc-mbox enc-mbox-${cls}`} data-ref={cur.ref_id} data-group={cur.group}>
      <button type="button" className="enc-mbox-pic" onClick={open} aria-label={`Open ${cur.title ?? "media"}`}>
        {pic ? (
          <img
            src={fileUrl(pic)}
            alt=""
            width={pic.width}
            height={pic.height}
            loading="lazy"
            style={pic.width && pic.height ? { aspectRatio: `${pic.width} / ${pic.height}` } : undefined}
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
      {cls === "audio" && item.data && <AudioPlayer item={item.data} compact />}
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
              {m.thumb ? <img src={fileUrl(m.thumb)} alt={m.title ?? ""} loading="lazy" /> : <Icon name="image" />}
            </button>
          ))}
        </div>
      )}
    </figure>
  );
}
