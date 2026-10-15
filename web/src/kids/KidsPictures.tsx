import { useCallback, useEffect, useRef, useState } from "react";
import { Link, useParams, useSearchParams } from "react-router-dom";
import { api } from "../api/client";
import type { Article, AssocItem, Item } from "../api/types";
import { useApi } from "../api/useApi";
import { useArticleModel } from "../article/useArticleModel";
import { Html } from "../components/Html";
import { useDocumentTitle } from "../components/useDocumentTitle";
import { MediaStage } from "../media/MediaViewer";
import { AudioPlayer } from "../media/Players";
import { fileUrl, mediaClass } from "../media/pick";
import "../styles/kids-pictures.css";

/** Corner badges that exist as files, by `media_type`. */
const TICONS = new Set(["anim", "audio", "game", "ia", "image", "map", "narr", "pan", "video"]);

/** Pixels per frame (about 60 a second) while the pointer rests on an arrow, and while it is held down. */
const HOVER_SPEED = 1.5;
const PRESS_SPEED = 6;

/**
 * Encarta Kids "Look at Pictures" (`/kids/item/{id}/media`): the article's pictures as a vertical strip
 * of tiles on the left and the selected one large on the right, with its title, caption and credit.
 * Layout and metrics follow the VM captures in fixtures/ui/kids2 (docs/vm-findings.md).
 */
export function KidsPicturesPage() {
  const id = Number(useParams().id);
  const res = useApi((s) => api.article(id, "kids", s), [id]);
  useDocumentTitle(res.data ? `${res.data.title}: Look at Pictures` : null);
  if (res.error) return <div className="enc-page-error">Could not load the article ({res.error.message}).</div>;
  if (!res.data) return <div className="enc-loading"><div className="enc-spinner" /></div>;
  return <KidsPictures article={res.data} />;
}

export function KidsPictures({ article }: { article: Article }) {
  const m = useArticleModel(article);
  const [params, setParams] = useSearchParams();
  const list = (m.tiles.length > 0 ? m.tiles : m.sequence).filter((x) => x.group !== "weblink");
  const wanted = Number(params.get("max"));
  const current = list.find((x) => x.ref_id === wanted) ?? list[0];
  const select = (ref: number) => setParams({ max: String(ref) }, { replace: true });
  return (
    <div className="kp">
      <div className="kp-head">
        <h1 className="kp-title">
          <Html as="span" html={article.title_html ?? article.title} />
        </h1>
        <Link to={`/kids/item/${article.ref_id}`} className="kp-read" aria-label="Read Article" />
      </div>
      <div className="kp-body">
        <Strip list={list} current={current?.ref_id} onSelect={select} />
        {current && <Selected key={current.ref_id} entry={current} />}
      </div>
    </div>
  );
}

/** The scrolling strip: an up arrow, a clipped column of tiles, a down arrow. */
function Strip({ list, current, onSelect }: { list: AssocItem[]; current?: number; onSelect: (ref: number) => void }) {
  const clip = useRef<HTMLDivElement>(null);
  const timer = useRef<number>(0);
  const [ends, setEnds] = useState({ top: true, bottom: true });
  const measure = useCallback(() => {
    const el = clip.current;
    if (!el) return;
    setEnds({ top: el.scrollTop <= 0, bottom: el.scrollTop + el.clientHeight >= el.scrollHeight - 1 });
  }, []);
  useEffect(() => {
    measure();
    window.addEventListener("resize", measure);
    return () => window.removeEventListener("resize", measure);
  }, [measure, list.length]);
  // Keep a newly selected tile in view.
  useEffect(() => {
    const el = clip.current?.querySelector<HTMLElement>('[aria-current="true"]');
    el?.scrollIntoView?.({ block: "nearest" });
    measure();
  }, [current, measure]);
  const stop = () => window.clearInterval(timer.current);
  useEffect(() => stop, []);
  const run = (dir: number, speed: number) => {
    stop();
    timer.current = window.setInterval(() => {
      const el = clip.current;
      if (!el) return;
      el.scrollTop += dir * speed;
      measure();
    }, 16);
  };
  return (
    <div className="kp-strip">
      <Arrow dir="up" disabled={ends.top} run={run} stop={stop} />
      <div className="kp-clip" ref={clip} onScroll={measure}>
        {list.map((x) => (
          <Tile key={x.ref_id} m={x} selected={x.ref_id === current} onSelect={onSelect} />
        ))}
      </div>
      <Arrow dir="down" disabled={ends.bottom} run={run} stop={stop} />
    </div>
  );
}

function Arrow({ dir, disabled, run, stop }: { dir: "up" | "down"; disabled: boolean; run: (d: number, s: number) => void; stop: () => void }) {
  const [state, setState] = useState<"rest" | "over" | "down">("rest");
  const sign = dir === "up" ? -1 : 1;
  useEffect(() => {
    if (disabled) stop();
  }, [disabled, stop]);
  const shown = disabled ? "disabled" : state;
  return (
    <div className="kp-arrow-row">
      <button
        type="button"
        className="kp-arrow"
        aria-label={dir === "up" ? "Scroll up" : "Scroll down"}
        disabled={disabled}
        style={{ backgroundImage: `url(/ui/kids-strip-${dir}-${shown}.gif)` }}
        onMouseEnter={() => {
          setState("over");
          run(sign, HOVER_SPEED);
        }}
        onMouseLeave={() => {
          setState("rest");
          stop();
        }}
        onMouseDown={() => {
          setState("down");
          run(sign, PRESS_SPEED);
        }}
        onMouseUp={() => {
          setState("over");
          run(sign, HOVER_SPEED);
        }}
      />
    </div>
  );
}

function Tile({ m, selected, onSelect }: { m: AssocItem; selected: boolean; onSelect: (ref: number) => void }) {
  const pic = m.picon ?? m.image ?? m.thumb ?? undefined;
  const ticon = m.media_type && TICONS.has(m.media_type) ? m.media_type : undefined;
  return (
    <Link
      to={`?max=${m.ref_id}`}
      replace
      className={`kp-tile${selected ? " selected" : ""}`}
      aria-current={selected ? "true" : undefined}
      onClick={(e) => {
        e.preventDefault();
        onSelect(m.ref_id);
      }}
    >
      <span className="kp-tile-pic">
        {pic && <img src={fileUrl(pic)} alt="" className="kp-tile-img" />}
        {ticon && <img className="kp-ticon" src={`/ui/kids-ticon-${ticon}.gif`} alt="" width={28} height={28} />}
      </span>
      <span className="kp-tile-title">
        <Html as="span" html={m.title_html ?? m.title ?? ""} />
      </span>
    </Link>
  );
}

/** The selected item: its picture or player, then its title, caption and credit. */
function Selected({ entry }: { entry: AssocItem }) {
  const res = useApi((s) => api.item(entry.ref_id, s), [entry.ref_id]);
  const item = res.data;
  const cls = mediaClass(entry);
  return (
    <div className="kp-main" id="kp-main">
      <div className="kp-stage">{item ? <Stage item={item} entry={entry} cls={cls} /> : <div className="enc-spinner" />}</div>
      <div className="kp-info">
        <div className="kp-item-title">
          <Html as="span" html={item?.title_html ?? entry.title_html ?? entry.title ?? ""} />
        </div>
        {item?.caption_html && <Html className="kp-caption" html={item.caption_html} />}
        <div className="kp-credit">
          <b>Encarta Encyclopedia</b>
        </div>
        {item?.credit && <div className="kp-credit">{item.credit_html ? <Html as="span" html={item.credit_html} /> : item.credit}</div>}
      </div>
    </div>
  );
}

function Stage({ item, entry, cls }: { item: Item; entry: AssocItem; cls: string }) {
  if (cls === "audio") {
    // An audio set is its picture over a player bar; the picture comes from the article's own row.
    const pic = entry.image ?? entry.picon ?? undefined;
    return (
      <div className="kp-audio">
        {pic && <img src={fileUrl(pic)} alt="" width={pic.width} height={pic.height} />}
        <AudioPlayer item={item} autoPlay compact />
      </div>
    );
  }
  const video = item.files?.find((f) => f.role === "video");
  const vw = Math.round((video?.width ?? 320) * 1.8);
  return (
    <div className="kp-media" style={{ "--vw": `${vw}px` } as React.CSSProperties}>
      <MediaStage item={item} kids />
    </div>
  );
}
