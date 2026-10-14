/**
 * Match games: a native player for the IAF `MatchGame` interactives (pictures on top, names
 * underneath; pick one of each to match them). Lists every match game on the disc.
 */
import { useEffect, useMemo, useRef, useState } from "react";
import { Link, useParams } from "react-router-dom";
import { useApi } from "../../api/useApi";
import { Html } from "../../components/Html";
import { useDocumentTitle } from "../../components/useDocumentTitle";
import type { MediaInfo } from "../../api/types";
import { getInteractive, getMatchGames } from "../api";
import { rng, shuffle } from "../mindmaze/castle";
import { MATCH_POINTS, mediaUrl, parseMatch, rewardIndex, type Bucket, type MatchDefinition, type MatchItem } from "./match";

export function MatchList() {
  useDocumentTitle("Match Games");
  const games = useApi((s) => getMatchGames(s), []);
  const groups = useMemo(() => {
    const g = games.data?.games ?? [];
    return [
      { label: "Encarta Premium", list: g.filter((x) => x.audience === "adult") },
      { label: "Encarta Kids", list: g.filter((x) => x.audience === "kids") },
    ];
  }, [games.data]);
  return (
    <div className="games-page">
      <div className="games-crumbs">
        <Link to="/games">Games &amp; Fun Stuff</Link> › Match Games
      </div>
      <h1 className="games-h1">Match Games</h1>
      {games.error && <p className="games-error">{games.error.message}</p>}
      {games.loading && <div className="enc-loading"><div className="enc-spinner" /></div>}
      {groups.map(
        (g) =>
          g.list.length > 0 && (
            <section key={g.label}>
              <h2 className="games-h2">{g.label}</h2>
              <ul className="match-list">
                {g.list.map((m) => (
                  <li key={m.file_name}>
                    <Link to={`/games/match/${m.file_name}${m.edition === "kids" ? "?edition=kids" : ""}`}>
                      {m.items[0]?.thumb ? <img src={m.items[0].thumb} alt="" width={64} height={64} /> : <span className="match-noimg" />}
                      <span>{m.items[0]?.title ?? m.title}</span>
                    </Link>
                  </li>
                ))}
              </ul>
            </section>
          ),
      )}
    </div>
  );
}

export function MatchRoute() {
  const { file = "" } = useParams();
  const edition = new URLSearchParams(location.search).get("edition") ?? undefined;
  const ia = useApi((s) => getInteractive(file, edition, s), [file, edition]);
  const def = useMemo(() => (ia.data ? parseMatch(ia.data.definition) : null), [ia.data]);
  const title = ia.data?.items[0]?.title ?? ia.data?.title ?? "Match Game";
  useDocumentTitle(title);
  return (
    <div className="games-page">
      <div className="games-crumbs">
        <Link to="/games">Games &amp; Fun Stuff</Link> › <Link to="/games/match">Match Games</Link> › {title}
      </div>
      {ia.error && <p className="games-error">{ia.error.message}</p>}
      {ia.loading && <div className="enc-loading"><div className="enc-spinner" /></div>}
      {def && ia.data && (ia.data.control_type === "MatchGame" ? <MatchBoard key={file} title={title} def={def} media={ia.data.media} /> : <p>This interactive is not a match game.</p>)}
    </div>
  );
}

function Card({ b, media, kind, show }: { b: Bucket; media: Record<string, MediaInfo>; kind: "image" | "text"; show: "top" | "bottom" | "solved" }) {
  const audio = useRef<HTMLAudioElement>(null);
  const img = kind === "image" || show === "solved" ? mediaUrl(media, b.image, "image") : undefined;
  const snd = mediaUrl(media, b.audio, "audio");
  return (
    <span className="match-card-inner">
      {img && <img src={img} alt="" />}
      {(kind === "text" || show === "solved") && (b.title || b.body) && (
        <span className="match-text">
          {b.title && <Html as="span" html={b.title} />}
          {show !== "solved" && b.body && <Html as="span" className="match-body" html={b.body} />}
        </span>
      )}
      {kind === "image" && !img && b.body && <Html as="span" className="match-text" html={b.body} />}
      {snd && (
        <>
          <audio ref={audio} src={snd} preload="none" />
          <span
            role="button"
            tabIndex={0}
            className="match-play"
            onClick={(e) => {
              e.stopPropagation();
              void audio.current?.play();
            }}
            onKeyDown={(e) => e.key === "Enter" && void audio.current?.play()}
          >
            ▶
          </span>
        </>
      )}
    </span>
  );
}

export function MatchBoard({ title, def, media, seed }: { title: string; def: MatchDefinition; media: Record<string, MediaInfo>; seed?: number }) {
  const [round, setRound] = useState(0);
  const [levelIx, setLevelIx] = useState(0);
  const level = def.levels[levelIx];
  const rand = useMemo(() => rng((seed ?? Math.floor(Math.random() * 2 ** 31)) + round * 101 + levelIx), [seed, round, levelIx]);
  const board = useMemo<MatchItem[]>(() => (level ? shuffle(level.items, rand).slice(0, level.perBoard) : []), [level, rand]);
  const bottomOrder = useMemo(() => shuffle(board, rand), [board, rand]);
  const [top, setTop] = useState<string | null>(null);
  const [bottom, setBottom] = useState<string | null>(null);
  const [solved, setSolved] = useState<Set<string>>(new Set());
  const [tries, setTries] = useState(0);
  const [score, setScore] = useState(0);
  const [miss, setMiss] = useState<string | null>(null);

  useEffect(() => {
    setSolved(new Set());
    setTop(null);
    setBottom(null);
    setTries(0);
  }, [board]);

  useEffect(() => {
    if (!top || !bottom) return;
    setTries((t) => t + 1);
    if (top === bottom) {
      setSolved((s) => new Set([...s, top]));
      setScore((s) => s + MATCH_POINTS);
      setTop(null);
      setBottom(null);
    } else {
      setMiss(bottom);
      const t = setTimeout(() => {
        setMiss(null);
        setTop(null);
        setBottom(null);
      }, 700);
      return () => clearTimeout(t);
    }
  }, [top, bottom]);

  if (!level) return <p>This game has no items.</p>;
  const finished = solved.size === board.length && board.length > 0;
  const lastLevel = levelIx + 1 >= def.levels.length;

  return (
    <div className="match-window" role="region" aria-label={title}>
      <div className="quiz-titlebar">
        <span>{title}</span>
        <span>
          {def.levels.length > 1 && (
            <>
              {def.strings.level} {levelIx + 1}{" "}
            </>
          )}
          <span className="quiz-score">
            {def.strings.score} {score}
          </span>
        </span>
      </div>
      {level.caption && <Html className="match-caption" html={level.caption} />}
      <ul className={`match-row match-top is-${level.topType}`}>
        {board.map((it) => (
          <li key={it.id}>
            <button type="button" className={`match-card${top === it.id ? " is-picked" : ""}${solved.has(it.id) ? " is-solved" : ""}`} disabled={solved.has(it.id) || !!miss} onClick={() => setTop(it.id)}>
              {solved.has(it.id) ? <Card b={it.solved} media={media} kind="image" show="solved" /> : <Card b={it.top} media={media} kind={level.topType} show="top" />}
            </button>
          </li>
        ))}
      </ul>
      <ul className={`match-row match-bottom is-${level.bottomType}`}>
        {bottomOrder.map((it) =>
          solved.has(it.id) ? (
            <li key={it.id} className="match-gone" />
          ) : (
            <li key={it.id}>
              <button type="button" className={`match-card${bottom === it.id ? " is-picked" : ""}${miss === it.id ? " is-miss" : ""}`} disabled={!top || !!miss} onClick={() => setBottom(it.id)}>
                <Card b={it.bottom} media={media} kind={level.bottomType} show="bottom" />
              </button>
            </li>
          ),
        )}
      </ul>
      <div className="match-status">
        {solved.size} {def.strings.matchesIn} {tries} {def.strings.tries}
        {!top && !finished && <span className="match-hint"> — pick a {level.topType === "image" ? "picture" : "card"} on top, then its match below.</span>}
        {top && level.topType === "image" && board.find((b) => b.id === top)?.top.body && (
          <span className="match-hint">
            {" "}
            — <Html as="span" html={board.find((b) => b.id === top)!.top.body} />
          </span>
        )}
      </div>
      {finished && (
        <div className="match-done">
          <p>{level.rewards[rewardIndex(solved.size, tries)] ?? "Well done!"}</p>
          {!lastLevel ? (
            <button type="button" className="games-btn games-btn-primary" onClick={() => setLevelIx(levelIx + 1)}>
              {def.strings.level} {levelIx + 2}
            </button>
          ) : (
            <button
              type="button"
              className="games-btn games-btn-primary"
              onClick={() => {
                setLevelIx(0);
                setScore(0);
                setRound((r) => r + 1);
              }}
            >
              {def.strings.reset}
            </button>
          )}
        </div>
      )}
    </div>
  );
}
