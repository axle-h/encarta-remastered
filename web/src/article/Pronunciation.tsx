import { useRef } from "react";
import { api } from "../api/client";
import { useApi } from "../api/useApi";
import { Icon } from "../components/Icons";
import type { DictEntry } from "../api/types";

/** The first playable audio source of a dictionary entry's pronunciation(s). */
export function pronunciationSrc(entry: DictEntry | undefined): string | undefined {
  for (const a of entry?.audio ?? []) {
    const v = a.media?.variants;
    const src = v?.opus?.url ?? v?.aac?.url ?? v?.mp3?.url;
    if (src) return src;
  }
  return undefined;
}

/**
 * A speaker button next to an article title when the English dictionary has a spoken
 * pronunciation of exactly that headword.
 */
export function Pronunciation({ title }: { title: string }) {
  const word = title.trim();
  const lookup = useApi(
    word && word.length < 40 ? (s) => api.dictLookup({ q: word, dict: "DICTA", limit: 5 }, s) : null,
    [word],
  );
  const hit = lookup.data?.entries.find((e) => e.headword.toLowerCase() === word.toLowerCase() && e.form_kind === "headword");
  const entry = useApi(hit ? (s) => api.dictEntry(hit.dictionary, hit.ref_id, s) : null, [hit?.ref_id]);
  const src = pronunciationSrc(entry.data);
  const audio = useRef<HTMLAudioElement>(null);
  if (!src) return null;
  return <SpeakerButton src={src} label={`Pronounce ${word}`} audioRef={audio} />;
}

export function SpeakerButton({ src, label, audioRef }: { src: string; label: string; audioRef?: React.RefObject<HTMLAudioElement> }) {
  const own = useRef<HTMLAudioElement>(null);
  const ref = audioRef ?? own;
  return (
    <button
      type="button"
      className="enc-speaker"
      aria-label={label}
      title={label}
      onClick={() => {
        const el = ref.current;
        if (!el) return;
        el.currentTime = 0;
        void el.play();
      }}
    >
      <Icon name="speaker" size={20} />
      <audio ref={ref} src={src} preload="none" />
    </button>
  );
}
