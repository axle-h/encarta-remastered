/**
 * MindMaze castle logic, free of React and the DOM (docs/games.md "MindMaze").
 *
 * The castle has four floors, one per question level. A floor is a grid of rooms joined by doors.
 * Every door starts closed: knocking brings a castle denizen who asks a question from the bank.
 * A right answer opens the door for good; a wrong one bars it. The stairway to the next floor (or,
 * on the top floor, the throne room) is in the room farthest from where the floor starts.
 */

export type Dir = 0 | 1 | 2 | 3; // north, east, south, west
export type DoorState = "closed" | "open" | "barred";

export const DX = [0, 1, 0, -1] as const;
export const DY = [-1, 0, 1, 0] as const;

/** Small seeded generator (mulberry32), so a saved seed rebuilds the same floor. */
export function rng(seed: number): () => number {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

export function shuffle<T>(xs: T[], rand: () => number): T[] {
  const a = xs.slice();
  for (let i = a.length - 1; i > 0; i--) {
    const j = Math.floor(rand() * (i + 1));
    [a[i], a[j]] = [a[j]!, a[i]!];
  }
  return a;
}

/** The castle denizens who answer a knock (art names in MINDMAZE.EIT; first is the default pose). */
export const DENIZENS: { id: string; art: string[]; name: string }[] = [
  { id: "jester", art: ["jester2.dib", "jester3.dib", "jester1.dib", "jester4.dib"], name: "the court jester" },
  { id: "king", art: ["king1.dib"], name: "the king" },
  { id: "queen", art: ["queen.dib"], name: "the queen" },
  { id: "duke", art: ["duke1.dib", "duke2.dib", "duke3.dib"], name: "the duke" },
  { id: "lady", art: ["lady1.dib"], name: "a lady of the court" },
  { id: "secondlady", art: ["secnldy1.dib", "secnldy2.dib"], name: "a lady-in-waiting" },
  { id: "servant", art: ["servant2.dib", "servant1.dib"], name: "a servant" },
  { id: "sorceress", art: ["sorceres.dib"], name: "the sorceress" },
  { id: "knight", art: ["suitarm1.dib"], name: "a suit of armor" },
  { id: "alchemist", art: ["alchem.dib"], name: "the alchemist" },
  { id: "asiantraveler", art: ["asiantra.dib"], name: "the Asian traveler" },
  { id: "traveler", art: ["metravel.dib"], name: "the traveler from the Middle East" },
];

/** Floor sizes (columns × rows), one per level. */
export const FLOOR_SIZES: [number, number][] = [
  [3, 3],
  [4, 3],
  [4, 4],
  [5, 4],
];

export const REWARDS = ["ribbon.dib", "medal.dib", "trophy.dib", "ring.dib"];

export interface RoomLook {
  /** Wall art: plain plaster or the stone dungeon. */
  walls: "plnwalls.dib" | "dunrm.dib";
  ceiling: string | null;
  floor: string | null;
  /** Props by slot (see `PROP_SLOTS` in Room.tsx). */
  props: Partial<Record<PropSlot, string>>;
  ambience: string;
}

export type PropSlot = "backLeft" | "backRight" | "wallLeft" | "wallRight" | "nearLeft" | "nearRight";

export interface DoorInfo {
  state: DoorState;
  category: number;
  denizen: number;
}

export interface Floor {
  level: number;
  seed: number;
  w: number;
  h: number;
  /** Doors by canonical key (`x,y,E` or `x,y,S`). */
  doors: Record<string, DoorInfo>;
  start: { x: number; y: number };
  exit: { x: number; y: number };
  rooms: RoomLook[];
}

export interface Position {
  x: number;
  y: number;
  facing: Dir;
}

export function doorKey(x: number, y: number, d: Dir): string {
  // Each door belongs to the room on its west or north side.
  if (d === 0) return `${x},${y - 1},S`;
  if (d === 3) return `${x - 1},${y},E`;
  return `${x},${y},${d === 1 ? "E" : "S"}`;
}

export function door(f: Floor, x: number, y: number, d: Dir): DoorInfo | undefined {
  return f.doors[doorKey(x, y, d)];
}

export function inside(f: Floor, x: number, y: number): boolean {
  return x >= 0 && y >= 0 && x < f.w && y < f.h;
}

const CEILINGS = ["ceiling1.dib", "ceiling3.dib", null, "ceiling2.dib"];
const FLOORS = ["carpet1.dib", "carpet2.dib", "flrcovr1.dib", null, null];
const BACK_PROPS = ["bookshlf.dib", "furnitur.dib", "table1.dib", "dtable1.dib", "dtable2.dib", "trunk.dib", "vase4.dib", "table2.dib", "parrot.dib"];
const WALL_PROPS = ["walltre1.dib", "walltre2.dib", "candle1.dib", "shelf.dib", "vase1.dib"];
const NEAR_PROPS = ["vase2.dib", "candle2.dib", "suitarm2.dib", "broom.dib"];

/**
 * Builds a floor: a random spanning tree over the grid (every room reachable), plus extra doors so
 * that a barred door rarely leaves the exit unreachable. Doors get a category (cycling through the
 * player's subjects) and a denizen.
 */
export function buildFloor(level: number, seed: number, categories: number[]): Floor {
  const rand = rng(seed);
  const [w, h] = FLOOR_SIZES[Math.min(Math.max(level, 1), 4) - 1]!;
  const cats = categories.length ? categories : [0, 1, 2, 3, 4, 5, 6, 7, 8];
  const doors: Record<string, DoorInfo> = {};
  const seen = new Set<string>();
  const start = { x: 0, y: h - 1 };
  let catIndex = Math.floor(rand() * cats.length);
  const addDoor = (x: number, y: number, d: Dir) => {
    const k = doorKey(x, y, d);
    if (doors[k]) return;
    doors[k] = { state: "closed", category: cats[catIndex++ % cats.length]!, denizen: Math.floor(rand() * DENIZENS.length) };
  };
  // Randomised depth-first search.
  const stack: [number, number][] = [[start.x, start.y]];
  seen.add(`${start.x},${start.y}`);
  while (stack.length) {
    const [x, y] = stack[stack.length - 1]!;
    const options = shuffle([0, 1, 2, 3] as Dir[], rand).filter((d) => {
      const nx = x + DX[d];
      const ny = y + DY[d];
      return nx >= 0 && ny >= 0 && nx < w && ny < h && !seen.has(`${nx},${ny}`);
    });
    if (!options.length) {
      stack.pop();
      continue;
    }
    const d = options[0]!;
    addDoor(x, y, d);
    const nx = x + DX[d];
    const ny = y + DY[d];
    seen.add(`${nx},${ny}`);
    stack.push([nx, ny]);
  }
  // Extra doors: about a third of the remaining inner walls.
  for (let y = 0; y < h; y++) {
    for (let x = 0; x < w; x++) {
      for (const d of [1, 2] as Dir[]) {
        if (x + DX[d] >= w || y + DY[d] >= h) continue;
        if (!doors[doorKey(x, y, d)] && rand() < 0.34) addDoor(x, y, d);
      }
    }
  }
  const floor: Floor = { level, seed, w, h, doors, start, exit: start, rooms: [] };
  // The exit is the room farthest from the start.
  const dist = distances(floor, start.x, start.y, () => true);
  let best = start;
  let bestD = -1;
  for (let y = 0; y < h; y++)
    for (let x = 0; x < w; x++) {
      const dd = dist.get(`${x},${y}`) ?? -1;
      if (dd > bestD) {
        bestD = dd;
        best = { x, y };
      }
    }
  floor.exit = best;
  for (let i = 0; i < w * h; i++) {
    const pick = <T,>(xs: T[]) => xs[Math.floor(rand() * xs.length)]!;
    const props: RoomLook["props"] = {};
    if (rand() < 0.75) props.backLeft = pick(BACK_PROPS);
    if (rand() < 0.6) props.backRight = pick(BACK_PROPS.filter((p) => p !== props.backLeft));
    if (rand() < 0.5) props.wallLeft = pick(WALL_PROPS);
    if (rand() < 0.5) props.wallRight = pick(WALL_PROPS.filter((p) => p !== props.wallLeft));
    if (rand() < 0.35) props.nearLeft = pick(NEAR_PROPS);
    if (rand() < 0.35) props.nearRight = pick(NEAR_PROPS.filter((p) => p !== props.nearLeft));
    floor.rooms.push({
      walls: level === 1 ? "dunrm.dib" : "plnwalls.dib",
      ceiling: pick(CEILINGS),
      floor: level === 1 ? null : pick(FLOORS),
      props,
      ambience: `amb${1 + Math.floor(rand() * 23)}.wav`,
    });
  }
  return floor;
}

export function roomLook(f: Floor, x: number, y: number): RoomLook {
  return f.rooms[y * f.w + x]!;
}

/** Breadth-first distances from (x, y) through doors that `pass` allows. */
export function distances(f: Floor, x: number, y: number, pass: (d: DoorInfo) => boolean): Map<string, number> {
  const out = new Map<string, number>([[`${x},${y}`, 0]]);
  const queue: [number, number][] = [[x, y]];
  while (queue.length) {
    const [cx, cy] = queue.shift()!;
    const cd = out.get(`${cx},${cy}`)!;
    for (const d of [0, 1, 2, 3] as Dir[]) {
      const info = door(f, cx, cy, d);
      if (!info || !pass(info)) continue;
      const nx = cx + DX[d];
      const ny = cy + DY[d];
      if (!inside(f, nx, ny) || out.has(`${nx},${ny}`)) continue;
      out.set(`${nx},${ny}`, cd + 1);
      queue.push([nx, ny]);
    }
  }
  return out;
}

/** True when barred doors cut the player off from the exit. */
export function trapped(f: Floor, x: number, y: number): boolean {
  return !distances(f, x, y, (d) => d.state !== "barred").has(`${f.exit.x},${f.exit.y}`);
}

/** Reopens barred doors (as closed) when the player is trapped; returns how many. */
export function unbarIfTrapped(f: Floor, x: number, y: number): number {
  if (!trapped(f, x, y)) return 0;
  let n = 0;
  for (const d of Object.values(f.doors)) {
    if (d.state === "barred") {
      d.state = "closed";
      n++;
    }
  }
  return n;
}

export function turn(p: Position, by: -1 | 1 | 2): Position {
  return { ...p, facing: (((p.facing + by) % 4) + 4) % 4 as Dir };
}

/** The wall a view side shows: front, left or right of where the player faces. */
export function sideDir(facing: Dir, side: "front" | "left" | "right" | "back"): Dir {
  const off = { front: 0, right: 1, back: 2, left: 3 }[side];
  return ((facing + off) % 4) as Dir;
}

/** Walks through the door on `d` (which must be open): the new position faces the way walked. */
export function walk(f: Floor, p: Position, d: Dir): Position | null {
  const info = door(f, p.x, p.y, d);
  if (!info || info.state !== "open") return null;
  const nx = p.x + DX[d];
  const ny = p.y + DY[d];
  if (!inside(f, nx, ny)) return null;
  return { x: nx, y: ny, facing: d };
}

export const SCORING = {
  /** Points for a right answer, per level. */
  right: (level: number) => 10 * level,
  /** Points lost for a wrong answer, per level (the score never drops below zero). */
  wrong: (level: number) => 5 * level,
  /** Bonus for climbing to the next floor (or reaching the throne room). */
  floor: (level: number) => 50 * level,
};

export function applyAnswer(score: number, level: number, right: boolean): number {
  return right ? score + SCORING.right(level) : Math.max(0, score - SCORING.wrong(level));
}

/** Answers in display order: the bank lists the right one first, so they are shuffled per question. */
export function answerOrder<T>(answers: T[], questionId: number, salt: number): T[] {
  return shuffle(answers, rng(questionId * 7919 + salt));
}

/** Plain-text lower-case comparison key of a name ("Ålesund " → "alesund"). */
export function nameKey(s: string): string {
  return s.normalize("NFKD").replace(/[̀-ͯ]/g, "").trim().toLowerCase();
}
