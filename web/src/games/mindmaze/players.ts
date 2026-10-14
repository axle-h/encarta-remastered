/**
 * MindMaze players, kept in this browser (the logon screen's name list). Storage can be missing or
 * throw (private windows); the game then simply forgets players between visits.
 */
import type { Floor, Position } from "./castle";

export interface Player {
  name: string;
  created: number;
  categories: number[];
  /** Current floor (1–4). */
  level: number;
  score: number;
  /** Floors cleared in this game (rewards earned). */
  rewards: number;
  /** Question ids already asked (newest last). */
  seen: number[];
  /** The floor being played and where the player stands. */
  floor?: Floor;
  pos?: Position;
  introSeen?: boolean;
  best?: number;
  games?: number;
}

const KEY = "encarta.mindmaze.v1";

interface Store {
  players: Player[];
  last?: string;
}

function read(): Store {
  try {
    const raw = localStorage.getItem(KEY);
    if (raw) {
      const s = JSON.parse(raw) as Store;
      if (Array.isArray(s.players)) return s;
    }
  } catch {
    /* unavailable */
  }
  return { players: [] };
}

function write(s: Store) {
  try {
    localStorage.setItem(KEY, JSON.stringify(s));
  } catch {
    /* unavailable or full */
  }
}

export function listPlayers(): { players: Player[]; last?: string } {
  const s = read();
  return { players: s.players.slice().sort((a, b) => a.name.localeCompare(b.name)), last: s.last };
}

export function newPlayer(name: string, categories: number[] = []): Player {
  return { name: name.trim(), created: Date.now(), categories, level: 1, score: 0, rewards: 0, seen: [] };
}

export function savePlayer(p: Player) {
  const s = read();
  const i = s.players.findIndex((x) => x.name.toLowerCase() === p.name.toLowerCase());
  const copy: Player = { ...p, seen: p.seen.slice(-1500) };
  if (i >= 0) s.players[i] = copy;
  else s.players.push(copy);
  s.last = p.name;
  write(s);
}

export function removePlayer(name: string) {
  const s = read();
  s.players = s.players.filter((x) => x.name.toLowerCase() !== name.toLowerCase());
  if (s.last?.toLowerCase() === name.toLowerCase()) s.last = undefined;
  write(s);
}

export function findPlayer(name: string): Player | undefined {
  return read().players.find((x) => x.name.toLowerCase() === name.trim().toLowerCase());
}
