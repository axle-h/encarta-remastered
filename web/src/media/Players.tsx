import { useEffect, useRef, useState } from "react";
import { api } from "../api/client";
import type { Item } from "../api/types";
import { useApi } from "../api/useApi";
import { audioSources, captionTracks, fileByRole, posterUrl, videoSources } from "./pick";

/** An audio player; captions from the item's `cc` track are shown under it as they play. */
export function AudioPlayer({ item, autoPlay = false, compact = false }: { item: Item; autoPlay?: boolean; compact?: boolean }) {
  const audio = fileByRole(item, "audio");
  const sources = audioSources(audio);
  const tracks = captionTracks(item);
  const ref = useRef<HTMLAudioElement>(null);
  const [cue, setCue] = useState("");

  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const onAdd = () => {
      for (const t of Array.from(el.textTracks)) {
        t.mode = "hidden";
        t.oncuechange = () => {
          const active = t.activeCues ? Array.from(t.activeCues) : [];
          setCue(active.map((c) => (c as VTTCue).text.replace(/<[^>]+>/g, "")).join("\n"));
        };
      }
    };
    onAdd();
    el.textTracks.addEventListener("addtrack", onAdd);
    return () => el.textTracks.removeEventListener("addtrack", onAdd);
  }, [item.ref_id]);

  if (sources.length === 0) {
    return <AudioSet item={item} autoPlay={autoPlay} compact={compact} />;
  }
  return (
    <div className={`enc-audio${compact ? " compact" : ""}`}>
      <audio ref={ref} controls autoPlay={autoPlay} preload="none" crossOrigin="anonymous">
        {sources.map((s) => (
          <source key={s.src} src={s.src} type={s.type || undefined} />
        ))}
        {tracks.map((t) => (
          <track key={t.src} kind="captions" src={t.src} label={t.label} srcLang={t.lang} default />
        ))}
      </audio>
      {!compact && tracks.length > 0 && <div className="enc-audio-cue" aria-live="polite">{cue}</div>}
    </div>
  );
}

/** An audio set (`audionested`): its children are the audio items; play the first, list the rest. */
function AudioSet({ item, autoPlay, compact }: { item: Item; autoPlay: boolean; compact: boolean }) {
  const kids = (item.children ?? []).filter((c) => c.kind === "media" || c.item_kind === "audio");
  const [idx, setIdx] = useState(0);
  const child = kids[idx];
  const loaded = useApi(child ? (s) => api.item(child.ref_id, s) : null, [child?.ref_id]);
  if (!child) return <p className="enc-muted">No playable audio.</p>;
  if (!loaded.data) return <div className="enc-audio loading" />;
  const hasAudio = audioSources(fileByRole(loaded.data, "audio")).length > 0;
  return (
    <div className="enc-audio-set">
      {hasAudio ? <AudioPlayer item={loaded.data} autoPlay={autoPlay} compact={compact} /> : <p className="enc-muted">Audio not available.</p>}
      {kids.length > 1 && (
        <ol className="enc-audio-tracks">
          {kids.map((k, i) => (
            <li key={k.ref_id}>
              <button type="button" className={i === idx ? "current" : ""} onClick={() => setIdx(i)}>
                {k.title || `Part ${i + 1}`}
              </button>
            </li>
          ))}
        </ol>
      )}
    </div>
  );
}

export function VideoPlayer({ item, autoPlay = false }: { item: Item; autoPlay?: boolean }) {
  const video = fileByRole(item, "video");
  const sources = videoSources(video);
  const tracks = captionTracks(item);
  if (sources.length === 0) return <p className="enc-muted">This video has not been converted yet.</p>;
  const w = video?.width ?? 320;
  const h = video?.height ?? 240;
  return (
    <video
      className="enc-video"
      controls
      autoPlay={autoPlay}
      preload="metadata"
      poster={posterUrl(item)}
      width={w * 2}
      height={h * 2}
      crossOrigin="anonymous"
    >
      {sources.map((s) => (
        <source key={s.src} src={s.src} type={s.type || undefined} />
      ))}
      {tracks.map((t, i) => (
        <track key={t.src} kind="captions" src={t.src} label={t.label} srcLang={t.lang} default={i === 0} />
      ))}
    </video>
  );
}
