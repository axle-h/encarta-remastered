import { useEffect, useRef, useState } from "react";
import { api } from "../api/client";
import type { Item } from "../api/types";
import { useApi } from "../api/useApi";
import { MidiPlayer } from "./MidiPlayer";
import { audioSources, captionTracks, fileByRole, hasAudio, isMidi, originalUrl, posterUrl, videoSources } from "./pick";

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

  if (audio && isMidi(audio)) {
    return (
      <div className={`enc-audio${compact ? " compact" : ""}`}>
        <MidiPlayer src={originalUrl(audio)} captions={compact ? undefined : tracks[0]?.src} />
      </div>
    );
  }
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

/**
 * An audio set (`audionested`): its children are a picture and one or more audio items. Plays the
 * first audio child and lists the others.
 */
function AudioSet({ item, autoPlay, compact }: { item: Item; autoPlay: boolean; compact: boolean }) {
  const ids = (item.children ?? []).map((c) => c.ref_id);
  const kids = useApi(ids.length ? (s) => Promise.all(ids.map((id) => api.item(id, s))) : null, [ids.join(",")]);
  const [idx, setIdx] = useState(0);
  if (ids.length === 0) return <p className="enc-muted">No playable audio.</p>;
  if (!kids.data) return <div className="enc-audio loading" />;
  const tracks = kids.data.filter((k) => hasAudio(k));
  const cur = tracks[Math.min(idx, tracks.length - 1)];
  if (!cur) return <p className="enc-muted">Audio not available.</p>;
  const picItem = kids.data.find((k) => !tracks.includes(k) && fileByRole(k, "image", "picon"));
  const pic = picItem ? fileByRole(picItem, "image", "picon") : undefined;
  return (
    <div className="enc-audio-set">
      {!compact && pic && <img className="enc-audio-pic" src={pic.url} alt="" width={pic.width} height={pic.height} />}
      <AudioPlayer key={cur.ref_id} item={cur} autoPlay={autoPlay} compact={compact} />
      {tracks.length > 1 && (
        <ol className="enc-audio-tracks">
          {tracks.map((k, i) => (
            <li key={k.ref_id}>
              <button type="button" className={k === cur ? "current" : ""} onClick={() => setIdx(i)}>
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
