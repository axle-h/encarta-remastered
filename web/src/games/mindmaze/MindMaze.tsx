/**
 * MindMaze: the castle question game (docs/games.md "MindMaze"). Screens follow the game's own
 * artwork: title, logon (name list, click the castle door), the two story pages, the castle rooms
 * with doors and denizens, the stairway between floors and the six throne-room endings.
 */
import { useCallback, useEffect, useMemo, useRef, useState, type CSSProperties, type ReactNode } from "react";
import { Link } from "react-router-dom";
import { useDocumentTitle } from "../../components/useDocumentTitle";
import { getMazeQuestions, midiUrl, type MazeQuestion } from "../api";
import { playEffect, playMusic, setMusic, setMuted, soundState, stopMusic, onSoundChange } from "../sound";
import { artSize, artUrl, loadAssets, type Assets } from "./art";
import {
  DENIZENS,
  REWARDS,
  SCORING,
  answerOrder,
  applyAnswer,
  buildFloor,
  door,
  roomLook,
  sideDir,
  turn,
  unbarIfTrapped,
  walk,
  type Dir,
  type DoorInfo,
  type Floor,
  type Position,
  type PropSlot,
} from "./castle";
import { findPlayer, listPlayers, newPlayer, removePlayer, savePlayer, type Player } from "./players";

export const CATEGORY_LABELS = [
  "Physical Science & Technology",
  "Life Science",
  "Geography",
  "History",
  "Social Science",
  "Religion, Mythology & Philosophy",
  "Art, Language & Literature",
  "Performing Arts",
  "Sports, Games & Hobbies",
];

export const FLOOR_NAMES = ["The Dungeon", "The Lower Hall", "The Upper Hall", "The Tower"];

const W = 636;
const H = 437;
const ROOM_H = 301;
const DIR_NAMES = ["north", "east", "south", "west"];

// --- art helpers --------------------------------------------------------------------------------

function useArt(assets: Assets | null, name: string | null | undefined): string | undefined {
  const [url, setUrl] = useState<string>();
  useEffect(() => {
    if (!assets || !name) return setUrl(undefined);
    let live = true;
    void artUrl(assets, name).then((u) => live && setUrl(u));
    return () => {
      live = false;
    };
  }, [assets, name]);
  return url;
}

function Sprite({
  assets,
  name,
  x,
  y,
  anchor = "top-left",
  className,
  style,
  onClick,
  title,
}: {
  assets: Assets;
  name: string;
  x: number;
  y: number;
  anchor?: "top-left" | "bottom-center" | "bottom-left" | "bottom-right" | "top-center";
  className?: string;
  style?: CSSProperties;
  onClick?: () => void;
  title?: string;
}) {
  const url = useArt(assets, name);
  const { w, h } = artSize(assets, name);
  if (!url || !w) return null;
  let left = x;
  let top = y;
  if (anchor.startsWith("bottom")) top = y - h;
  if (anchor.endsWith("center")) left = x - w / 2;
  if (anchor === "bottom-right") left = x - w;
  const img = <img src={url} alt="" draggable={false} width={w} height={h} />;
  const s: CSSProperties = { left, top, width: w, height: h, ...style };
  return onClick ? (
    <button type="button" className={`mm-sprite mm-hot ${className ?? ""}`} style={s} onClick={onClick} title={title} aria-label={title}>
      {img}
    </button>
  ) : (
    <div className={`mm-sprite ${className ?? ""}`} style={s}>
      {img}
    </div>
  );
}

function Picture({ assets, name, className }: { assets: Assets; name: string; className?: string }) {
  const url = useArt(assets, name);
  const { w, h } = artSize(assets, name);
  return url ? <img className={className} src={url} alt="" width={w} height={h} draggable={false} /> : null;
}

/** Scales the 636×437 stage to the space available (up to 1.6×). */
function Stage({ children }: { children: ReactNode }) {
  const ref = useRef<HTMLDivElement>(null);
  const [scale, setScale] = useState(1);
  useEffect(() => {
    const el = ref.current;
    if (!el || typeof ResizeObserver === "undefined") return;
    const ro = new ResizeObserver(() => {
      const avail = el.clientWidth;
      const availH = window.innerHeight - el.getBoundingClientRect().top - 24;
      setScale(Math.max(0.5, Math.min(1.6, avail / W, availH > 200 ? availH / H : 1.6)));
    });
    ro.observe(el);
    return () => ro.disconnect();
  }, []);
  return (
    <div className="mm-stage-wrap" ref={ref} style={{ height: H * scale }}>
      <div className="mm-stage" style={{ width: W, height: H, transform: `scale(${scale})` }}>
        {children}
      </div>
    </div>
  );
}

// --- sound helpers --------------------------------------------------------------------------------

function useSoundState() {
  const [s, setS] = useState(soundState());
  useEffect(() => onSoundChange(() => setS(soundState())), []);
  return s;
}

function sfx(assets: Assets | null, name: string, volume?: number) {
  return playEffect(assets?.[name], volume);
}

function music(assets: Assets | null, name: string) {
  playMusic(midiUrl(assets?.[name]));
}

// --- the game ------------------------------------------------------------------------------------

type Screen = "title" | "logon" | "intro1" | "intro2" | "room" | "stairs" | "ending";

interface Asking {
  dir: Dir;
  phase: "knock" | "question" | "result";
  question?: MazeQuestion;
  chosen?: number;
  right?: boolean;
  pose: number;
}

export function MindMaze() {
  useDocumentTitle("MindMaze");
  const [assets, setAssets] = useState<Assets | null>(null);
  const [error, setError] = useState<string>();
  const [screen, setScreen] = useState<Screen>("title");
  const [player, setPlayer] = useState<Player | null>(null);
  const [asking, setAsking] = useState<Asking | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [fading, setFading] = useState(false);
  const [ending, setEnding] = useState(1);
  const pool = useRef<MazeQuestion[]>([]);
  const loadingPool = useRef<Promise<void> | null>(null);

  useEffect(() => {
    loadAssets().then(setAssets, (e: Error) => setError(e.message));
    return () => stopMusic();
  }, []);

  const update = useCallback((p: Player) => {
    savePlayer(p);
    setPlayer({ ...p });
  }, []);

  const refill = useCallback(async (p: Player) => {
    if (loadingPool.current) return loadingPool.current;
    const task = getMazeQuestions({ level: p.level, categories: p.categories, count: 60, exclude: p.seen })
      .then((g) => {
        const have = new Set(pool.current.map((q) => q.id));
        pool.current.push(...g.questions.filter((q) => !have.has(q.id)));
      })
      .finally(() => (loadingPool.current = null));
    loadingPool.current = task;
    return task;
  }, []);

  const startFloor = useCallback(
    (p: Player) => {
      const f = buildFloor(p.level, Math.floor(Math.random() * 2 ** 31), p.categories);
      p.floor = f;
      p.pos = { x: f.start.x, y: f.start.y, facing: 0 };
      pool.current = [];
      void refill(p);
      update(p);
    },
    [refill, update],
  );

  const enter = useCallback(
    (p: Player) => {
      setPlayer(p);
      if (!p.floor || !p.pos) startFloor(p);
      else {
        pool.current = [];
        void refill(p);
      }
      if (!p.introSeen) setScreen("intro1");
      else setScreen("room");
    },
    [refill, startFloor],
  );

  // Music per screen.
  useEffect(() => {
    if (!assets) return;
    if (screen === "title") return; // no sound before the first click
    if (screen === "logon" || screen.startsWith("intro")) music(assets, "intro.mid");
    else if (screen === "room") music(assets, asking?.phase === "question" ? "question.mid" : `bgloop${1 + (((player?.level ?? 1) - 1) % 3)}.mid`);
    else if (screen === "ending") music(assets, "intro.mid");
  }, [assets, screen, asking?.phase, player?.level]);

  const floor = player?.floor;
  const pos = player?.pos;

  const moveThrough = useCallback(
    async (d: Dir) => {
      if (!player || !floor || !pos) return;
      const next = walk(floor, pos, d);
      if (!next) return;
      setFading(true);
      void sfx(assets, `foot${1 + Math.floor(Math.random() * 5)}.wav`, 0.6);
      await new Promise((r) => setTimeout(r, 350));
      player.pos = next;
      setNotice(null);
      update(player);
      setFading(false);
      void sfx(assets, roomLook(floor, next.x, next.y).ambience, 0.25);
      if (next.x === floor.exit.x && next.y === floor.exit.y) {
        player.score += SCORING.floor(player.level);
        player.rewards = Math.max(player.rewards, player.level);
        update(player);
        if (player.level >= 4) {
          setEnding(1 + Math.floor(Math.random() * 6));
          player.best = Math.max(player.best ?? 0, player.score);
          player.games = (player.games ?? 0) + 1;
          update(player);
          setScreen("ending");
        } else setScreen("stairs");
      }
    },
    [player, floor, pos, assets, update],
  );

  const knock = useCallback(
    async (d: Dir) => {
      if (!player || !floor || !pos || asking) return;
      const info = door(floor, pos.x, pos.y, d);
      if (!info) return;
      if (info.state === "open") return moveThrough(d);
      if (info.state === "barred") {
        setNotice("This door has been barred. Try another way.");
        return;
      }
      setAsking({ dir: d, phase: "knock", pose: 0 });
      await sfx(assets, "knock.wav");
      if (!pool.current.some((q) => q.category === info.category) && pool.current.length < 8) await refill(player);
      let i = pool.current.findIndex((q) => q.category === info.category);
      if (i < 0) i = 0;
      const q = pool.current.splice(i, 1)[0];
      if (pool.current.length < 10) void refill(player);
      if (!q) {
        setAsking(null);
        setNotice("No one answers. (No questions could be loaded.)");
        return;
      }
      sfx(assets, "dooropen.wav", 0.5);
      setAsking({ dir: d, phase: "question", question: q, pose: q.id % 4 });
    },
    [player, floor, pos, asking, assets, refill, moveThrough],
  );

  const answer = useCallback(
    (ordinal: number) => {
      if (!player || !floor || !pos || !asking?.question || asking.phase !== "question") return;
      const q = asking.question;
      const right = q.answers.find((a) => a.ordinal === ordinal)?.correct ?? false;
      const info = door(floor, pos.x, pos.y, asking.dir)!;
      info.state = right ? "open" : "barred";
      player.score = applyAnswer(player.score, player.level, right);
      player.seen = [...player.seen.filter((x) => x !== q.id), q.id];
      void sfx(assets, right ? "right.wav" : "wrong.wav");
      update(player);
      setAsking({ ...asking, phase: "result", chosen: ordinal, right });
    },
    [player, floor, pos, asking, assets, update],
  );

  const closeResult = useCallback(() => {
    if (!player || !floor || !pos || !asking) return;
    const d = asking.dir;
    const right = asking.right;
    setAsking(null);
    if (right) void moveThrough(d);
    else if (unbarIfTrapped(floor, pos.x, pos.y)) {
      update(player);
      setNotice("You hear bolts sliding back: the castle servants have unbarred the doors so you can find your way.");
    }
  }, [player, floor, pos, asking, moveThrough, update]);

  const climb = useCallback(() => {
    if (!player) return;
    player.level = Math.min(4, player.level + 1);
    startFloor(player);
    setScreen("room");
  }, [player, startFloor]);

  const playAgain = useCallback(() => {
    if (!player) return;
    const p: Player = { ...player, level: 1, score: 0, rewards: 0, floor: undefined, pos: undefined };
    startFloor(p);
    setPlayer(p);
    setScreen("room");
  }, [player, startFloor]);

  if (error) return <div className="games-page"><p className="games-error">MindMaze could not load its artwork: {error}</p></div>;
  if (!assets) return <div className="enc-loading"><div className="enc-spinner" /></div>;

  return (
    <div className="games-page mm-page">
      <div className="games-crumbs">
        <Link to="/games">Games &amp; Fun Stuff</Link> › MindMaze
      </div>
      <Stage>
        {screen === "title" && <TitleScreen assets={assets} onNext={() => setScreen("logon")} />}
        {screen === "logon" && <Logon assets={assets} onEnter={enter} />}
        {screen === "intro1" && <StoryPage assets={assets} name="open1.dib" onNext={() => setScreen("intro2")} />}
        {screen === "intro2" && (
          <StoryPage
            assets={assets}
            name="open2.dib"
            onNext={() => {
              if (player) update({ ...player, introSeen: true });
              setScreen("room");
            }}
          />
        )}
        {screen === "room" && player && floor && pos && (
          <>
            <RoomView
              assets={assets}
              floor={floor}
              pos={pos}
              fading={fading}
              asking={asking}
              onDoor={(d) => void knock(d)}
              onAnswer={answer}
              onCloseResult={closeResult}
            />
            <Hud
              assets={assets}
              player={player}
              pos={pos}
              notice={notice}
              busy={!!asking}
              onTurn={(by) => {
                setNotice(null);
                update({ ...player, pos: turn(pos, by) });
              }}
              onForward={() => {
                const d = pos.facing;
                const info = door(floor, pos.x, pos.y, d);
                if (info?.state === "open") void moveThrough(d);
                else if (info) void knock(d);
                else setNotice("There is no door that way.");
              }}
              onQuit={() => setScreen("logon")}
            />
          </>
        )}
        {screen === "stairs" && player && <Stairs assets={assets} player={player} onClimb={climb} />}
        {screen === "ending" && player && <Ending assets={assets} player={player} ending={ending} onAgain={playAgain} onLogon={() => setScreen("logon")} />}
      </Stage>
      <SoundToggles />
    </div>
  );
}

// --- screens ---------------------------------------------------------------------------------------

function TitleScreen({ assets, onNext }: { assets: Assets; onNext: () => void }) {
  return (
    <button type="button" className="mm-full mm-title" onClick={onNext} aria-label="Begin MindMaze">
      <Picture assets={assets} name="titlescr.dib" />
      <span className="mm-title-hint">Click to begin</span>
    </button>
  );
}

function Logon({ assets, onEnter }: { assets: Assets; onEnter: (p: Player) => void }) {
  const initial = useMemo(() => listPlayers(), []);
  const [players, setPlayers] = useState(initial.players);
  const [name, setName] = useState(initial.last ?? "");
  const [cats, setCats] = useState<number[]>(() => findPlayer(initial.last ?? "")?.categories ?? []);
  const [subjects, setSubjects] = useState(false);
  const existing = players.find((p) => p.name.toLowerCase() === name.trim().toLowerCase());

  const go = () => {
    const n = name.trim();
    if (!n) return;
    const p = existing ? { ...existing, categories: cats } : newPlayer(n, cats);
    if (existing && JSON.stringify(existing.categories) !== JSON.stringify(cats)) {
      // New subjects take effect on a fresh floor.
      p.floor = undefined;
      p.pos = undefined;
    }
    savePlayer(p);
    onEnter(p);
  };

  return (
    <div className="mm-full">
      <Picture assets={assets} name="logonscr.dib" />
      <button type="button" className="mm-hotspot mm-castle-door" onClick={go} disabled={!name.trim()} title="Enter the castle" aria-label="Enter the castle" />
      <form
        className="mm-logon"
        onSubmit={(e) => {
          e.preventDefault();
          go();
        }}
      >
        <label>
          Your name:
          <input value={name} maxLength={24} autoFocus onChange={(e) => setName(e.target.value)} />
        </label>
        <ul className="mm-names" aria-label="Players">
          {players.map((p) => (
            <li key={p.name}>
              <button
                type="button"
                className={p.name === existing?.name ? "is-current" : ""}
                onClick={() => {
                  setName(p.name);
                  setCats(p.categories);
                }}
              >
                {p.name}
                <small>
                  {" "}
                  floor {p.level}, {p.score} pts
                </small>
              </button>
              <button
                type="button"
                className="mm-remove"
                title={`Forget ${p.name}`}
                aria-label={`Forget ${p.name}`}
                onClick={() => {
                  removePlayer(p.name);
                  setPlayers(listPlayers().players);
                }}
              >
                ×
              </button>
            </li>
          ))}
        </ul>
        <div className="mm-logon-buttons">
          <button type="button" onClick={() => setSubjects(true)}>
            Subjects…
          </button>
          <button type="submit" disabled={!name.trim()}>
            Enter
          </button>
        </div>
      </form>
      {subjects && <SubjectsDialog assets={assets} value={cats} onClose={(v) => (setCats(v), setSubjects(false))} />}
    </div>
  );
}

function SubjectsDialog({ assets, value, onClose }: { assets: Assets; value: number[]; onClose: (v: number[]) => void }) {
  const [sel, setSel] = useState<number[]>(value.length ? value : CATEGORY_LABELS.map((_, i) => i));
  const toggle = (i: number) => setSel((s) => (s.includes(i) ? s.filter((x) => x !== i) : [...s, i].sort((a, b) => a - b)));
  return (
    <div className="mm-modal" role="dialog" aria-label="Subjects">
      <h3>Choose the subjects for your questions</h3>
      <ul className="mm-subjects">
        {CATEGORY_LABELS.map((label, i) => (
          <li key={i}>
            <label>
              <input type="checkbox" checked={sel.includes(i)} onChange={() => toggle(i)} />
              <Picture assets={assets} name={`pic2-${i + 1}.dib`} className="mm-subject-pic" />
              {label}
            </label>
          </li>
        ))}
      </ul>
      <div className="mm-logon-buttons">
        <button type="button" onClick={() => setSel(CATEGORY_LABELS.map((_, i) => i))}>
          All
        </button>
        <button type="button" disabled={!sel.length} onClick={() => onClose(sel.length === CATEGORY_LABELS.length ? [] : sel)}>
          OK
        </button>
      </div>
    </div>
  );
}

function StoryPage({ assets, name, onNext }: { assets: Assets; name: string; onNext: () => void }) {
  return (
    <div className="mm-full mm-story">
      <Picture assets={assets} name={name} />
      <button type="button" className="mm-btn mm-story-next" onClick={onNext} autoFocus>
        Continue
      </button>
    </div>
  );
}

function Stairs({ assets, player, onClimb }: { assets: Assets; player: Player; onClimb: () => void }) {
  return (
    <div className="mm-full mm-stairs">
      <Sprite assets={assets} name="doortow.dib" x={W / 2} y={18} anchor="top-center" />
      <div className="mm-stairs-text">
        <p>
          You have found the stairway out of <b>{FLOOR_NAMES[player.level - 1]}</b>!
        </p>
        <p className="mm-reward-line">
          <Picture assets={assets} name={REWARDS[player.level - 1]!} /> You earn a reward and {SCORING.floor(player.level)} bonus points. Score: {player.score}
        </p>
        <button type="button" className="mm-btn" onClick={onClimb} autoFocus>
          Climb to {FLOOR_NAMES[player.level]}
        </button>
      </div>
    </div>
  );
}

function Ending({ assets, player, ending, onAgain, onLogon }: { assets: Assets; player: Player; ending: number; onAgain: () => void; onLogon: () => void }) {
  return (
    <div className="mm-full mm-ending">
      <Sprite assets={assets} name={`end${ending}.dib`} x={W / 2} y={8} anchor="top-center" />
      <div className="mm-ending-bar">
        <span className="mm-rewards">
          {REWARDS.map((r) => (
            <Picture key={r} assets={assets} name={r} />
          ))}
        </span>
        <span>
          Final score: <b>{player.score}</b>
          {player.best ? ` (best ${player.best})` : ""}
        </span>
        <button type="button" className="mm-btn" onClick={onAgain}>
          Play again
        </button>
        <button type="button" className="mm-btn" onClick={onLogon}>
          Change player
        </button>
      </div>
    </div>
  );
}

// --- the room ---------------------------------------------------------------------------------------

/** Where furniture stands (bottom-centre anchors on the 636×301 room view). */
const PROP_SLOTS: Record<PropSlot, { x: number; y: number; anchor: "bottom-center" | "top-center" }> = {
  backLeft: { x: 196, y: 204, anchor: "bottom-center" },
  backRight: { x: 438, y: 204, anchor: "bottom-center" },
  wallLeft: { x: 205, y: 34, anchor: "top-center" },
  wallRight: { x: 430, y: 34, anchor: "top-center" },
  nearLeft: { x: 150, y: 296, anchor: "bottom-center" },
  nearRight: { x: 486, y: 296, anchor: "bottom-center" },
};

const DOOR_SPOTS = {
  front: { art: "ndoor.dib", x: 274, y: 64, w: 87, h: 131, pic: (c: number) => `pic2-${c + 1}.dib`, px: 287, py: 6 },
  left: { art: "doorlt.dib", x: 26, y: 84, w: 87, h: 171, pic: (c: number) => `pic1-${c + 1}.dib`, px: 44, py: 10 },
  right: { art: "doorrt.dib", x: 523, y: 84, w: 87, h: 171, pic: (c: number) => `pic1-${c + 1}.dib`, px: 540, py: 10 },
} as const;

function RoomView({
  assets,
  floor,
  pos,
  fading,
  asking,
  onDoor,
  onAnswer,
  onCloseResult,
}: {
  assets: Assets;
  floor: Floor;
  pos: Position;
  fading: boolean;
  asking: Asking | null;
  onDoor: (d: Dir) => void;
  onAnswer: (ordinal: number) => void;
  onCloseResult: () => void;
}) {
  const look = roomLook(floor, pos.x, pos.y);
  const sides = (["left", "front", "right"] as const).map((side) => {
    const d = sideDir(pos.facing, side);
    return { side, d, info: door(floor, pos.x, pos.y, d) };
  });
  const hasSideDoor = (side: "left" | "right") => !!sides.find((s) => s.side === side)?.info;
  return (
    <div className={`mm-room${fading ? " is-fading" : ""}`} style={{ height: ROOM_H }}>
      <Sprite assets={assets} name={look.walls} x={0} y={0} />
      {look.ceiling && <Sprite assets={assets} name={look.ceiling} x={W / 2} y={0} anchor="top-center" />}
      {look.floor && <Sprite assets={assets} name={look.floor} x={W / 2} y={ROOM_H} anchor="bottom-center" />}
      {(Object.keys(PROP_SLOTS) as PropSlot[]).map((slot) => {
        const name = look.props[slot];
        if (!name) return null;
        if ((slot === "nearLeft" && hasSideDoor("left")) || (slot === "nearRight" && hasSideDoor("right"))) return null;
        const s = PROP_SLOTS[slot];
        // Nothing may stand in front of the back-wall door.
        const half = artSize(assets, name).w / 2;
        const front = DOOR_SPOTS.front;
        if (slot !== "nearLeft" && slot !== "nearRight" && sides.find((x) => x.side === "front")?.info && s.x + half > front.x - 4 && s.x - half < front.x + front.w + 4) return null;
        return <Sprite key={slot} assets={assets} name={name} x={s.x} y={s.y} anchor={s.anchor} />;
      })}
      {sides.map(({ side, d, info }) => info && <Door key={side} assets={assets} side={side} info={info} disabled={!!asking} onClick={() => onDoor(d)} dirName={DIR_NAMES[d]!} />)}
      {asking && <Denizen assets={assets} asking={asking} info={door(floor, pos.x, pos.y, asking.dir)!} onAnswer={onAnswer} onClose={onCloseResult} />}
    </div>
  );
}

function Door({ assets, side, info, disabled, onClick, dirName }: { assets: Assets; side: "left" | "front" | "right"; info: DoorInfo; disabled: boolean; onClick: () => void; dirName: string }) {
  const spot = DOOR_SPOTS[side];
  const label =
    info.state === "open" ? `Go through the ${dirName} door` : info.state === "barred" ? `The ${dirName} door is barred` : `Knock on the ${dirName} door (${CATEGORY_LABELS[info.category]})`;
  return (
    <>
      <Sprite assets={assets} name={spot.pic(info.category)} x={spot.px} y={spot.py} />
      <button
        type="button"
        className={`mm-door mm-door-${side} is-${info.state}`}
        style={{ left: spot.x, top: spot.y, width: spot.w, height: spot.h }}
        onClick={onClick}
        disabled={disabled}
        title={label}
        aria-label={label}
      >
        {info.state !== "open" && <Picture assets={assets} name={spot.art} />}
      </button>
    </>
  );
}

function Denizen({ assets, asking, info, onAnswer, onClose }: { assets: Assets; asking: Asking; info: DoorInfo; onAnswer: (o: number) => void; onClose: () => void }) {
  const who = DENIZENS[info.denizen % DENIZENS.length]!;
  const art = who.art[asking.pose % who.art.length]!;
  const q = asking.question;
  const order = useMemo(() => (q ? answerOrder(q.answers, q.id, 17) : []), [q]);
  if (asking.phase === "knock") return <div className="mm-knock">Knock, knock…</div>;
  return (
    <>
      <Sprite assets={assets} name={art} x={0} y={ROOM_H} anchor="bottom-left" className="mm-denizen" />
      {q && asking.phase === "question" && (
        <div className="mm-banner" role="dialog" aria-label={`Question from ${who.name}`}>
          <Picture assets={assets} name="banner.dib" />
          <div className="mm-banner-text">
            <p className="mm-clue" dangerouslySetInnerHTML={{ __html: q.clue_html }} />
            <ol className="mm-answers">
              {order.map((a) => (
                <li key={a.ordinal}>
                  <button type="button" onClick={() => onAnswer(a.ordinal)} dangerouslySetInnerHTML={{ __html: a.text_html }} />
                </li>
              ))}
            </ol>
          </div>
        </div>
      )}
      {q && asking.phase === "result" && <ResultDialog assets={assets} question={q} right={!!asking.right} who={who.name} onClose={onClose} />}
    </>
  );
}

function ResultDialog({ assets, question, right, who, onClose }: { assets: Assets; question: MazeQuestion; right: boolean; who: string; onClose: () => void }) {
  const correct = question.answers.find((a) => a.correct)!;
  const href = correct.article_exists ? `/item/${correct.article_ref_id}${correct.para && correct.para > 1 ? `#p${correct.para}` : ""}` : null;
  return (
    <div className="mm-dialog" role="alertdialog" aria-label={right ? "Correct" : "Wrong"}>
      <Picture assets={assets} name="dialog.dib" />
      <div className="mm-dialog-text">
        <h3>{right ? "Correct!" : "Sorry, that is not right."}</h3>
        <p>
          {right ? (
            <>
              {capital(who)} steps aside and the door swings open. (+{SCORING.right(question.level)})
            </>
          ) : (
            <>
              The answer is <b dangerouslySetInnerHTML={{ __html: correct.text_html }} />. The door is barred. (−{SCORING.wrong(question.level)})
            </>
          )}
        </p>
        {href && (
          <p className="mm-read">
            Read about it in Encarta:{" "}
            <a href={href} target="_blank" rel="noopener">
              {correct.article_title ?? correct.text}
            </a>
          </p>
        )}
        <button type="button" className="mm-btn" onClick={onClose} autoFocus>
          OK
        </button>
      </div>
    </div>
  );
}

function capital(s: string) {
  return s.charAt(0).toUpperCase() + s.slice(1);
}

// --- HUD ------------------------------------------------------------------------------------------

function Hud({
  assets,
  player,
  pos,
  notice,
  busy,
  onTurn,
  onForward,
  onQuit,
}: {
  assets: Assets;
  player: Player;
  pos: Position;
  notice: string | null;
  busy: boolean;
  onTurn: (by: -1 | 1 | 2) => void;
  onForward: () => void;
  onQuit: () => void;
}) {
  useEffect(() => {
    if (busy) return;
    const key = (e: KeyboardEvent) => {
      if ((e.target as HTMLElement)?.closest?.("input, textarea")) return;
      if (e.key === "ArrowLeft") onTurn(-1);
      else if (e.key === "ArrowRight") onTurn(1);
      else if (e.key === "ArrowDown") onTurn(2);
      else if (e.key === "ArrowUp") onForward();
      else return;
      e.preventDefault();
    };
    window.addEventListener("keydown", key);
    return () => window.removeEventListener("keydown", key);
  }, [busy, onTurn, onForward]);
  return (
    <div className="mm-hud" style={{ top: ROOM_H, height: H - ROOM_H }}>
      <div className="mm-hud-player">
        <div className="mm-hud-name">{player.name}</div>
        <div>
          {FLOOR_NAMES[player.level - 1]} <small>(floor {player.level} of 4)</small>
        </div>
        <div>
          Score: <b>{player.score}</b>
        </div>
        <div className="mm-hud-facing">Facing {DIR_NAMES[pos.facing]}</div>
      </div>
      <div className="mm-hud-nav" aria-label="Move">
        <button type="button" onClick={() => onTurn(-1)} disabled={busy} title="Turn left (←)" aria-label="Turn left">
          ↶
        </button>
        <button type="button" onClick={onForward} disabled={busy} title="Go forward / knock (↑)" aria-label="Go forward">
          ↑
        </button>
        <button type="button" onClick={() => onTurn(1)} disabled={busy} title="Turn right (→)" aria-label="Turn right">
          ↷
        </button>
        <button type="button" className="mm-hud-back" onClick={() => onTurn(2)} disabled={busy} title="Turn around (↓)" aria-label="Turn around">
          <Picture assets={assets} name="sdoor.dib" />
        </button>
      </div>
      <div className="mm-hud-rewards" aria-label="Rewards">
        {REWARDS.map((r, i) => (
          <Picture key={r} assets={assets} name={i < player.rewards ? r : "award.dib"} />
        ))}
      </div>
      <button type="button" className="mm-hud-quit" onClick={onQuit} disabled={busy}>
        Leave
      </button>
      {notice && <div className="mm-notice">{notice}</div>}
    </div>
  );
}

function SoundToggles() {
  const s = useSoundState();
  return (
    <div className="games-sound">
      <label>
        <input type="checkbox" checked={!s.muted} onChange={(e) => setMuted(!e.target.checked)} /> Sound
      </label>
      <label>
        <input type="checkbox" checked={s.musicOn} disabled={s.muted} onChange={(e) => setMusic(e.target.checked)} /> Music
      </label>
    </div>
  );
}

export { SoundToggles, Stage };
