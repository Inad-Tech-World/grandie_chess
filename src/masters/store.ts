// What the Masters screen shows, kept outside the screen so switching to
// another section and back doesn't lose your place.

import { create } from "zustand";
import type { Side } from "../bindings";

export const LEGENDS_ID = "legends";

export type StudyMode = "replay" | "guess";

export interface Study {
  mode: StudyMode;
  gameId: number;
  /** Catalogue id of the master being followed (null for legends). */
  subject: string | null;
  /** The player whose moves are guessed or followed. */
  masterName: string;
  /** Their colour in this game, when known. */
  masterColor: Side | null;
  /** e.g. "Tal vs Botvinnik, 1960". */
  title: string;
}

interface MastersStore {
  /** A catalogue id, or LEGENDS_ID. */
  selected: string;
  study: Study | null;
  select: (id: string) => void;
  openStudy: (study: Study) => void;
  closeStudy: () => void;
}

export const useMasters = create<MastersStore>()((set) => ({
  selected: LEGENDS_ID,
  study: null,
  select: (id) => set({ selected: id, study: null }),
  openStudy: (study) => set({ study }),
  closeStudy: () => set({ study: null }),
}));

/** Name tokens, lower-case, without accents: "José Raúl Capablanca" -> ["jose","raul","capablanca"]. */
function tokens(name: string): string[] {
  return name
    .normalize("NFD")
    .replace(/[̀-ͯ]/g, "")
    .toLowerCase()
    .split(/[^a-z]+/)
    .filter(Boolean);
}

/** The surname of a catalogue name ("Mikhail Tal" -> "tal"). */
export function surname(name: string): string {
  const t = tokens(name);
  return t[t.length - 1] ?? "";
}

/** Which colour `masterName` played, by matching the surname to the PGN names. */
export function masterColorIn(masterName: string, white: string | null, black: string | null): Side | null {
  const s = surname(masterName);
  if (!s) return null;
  const w = tokens(white ?? "").includes(s);
  const b = tokens(black ?? "").includes(s);
  if (w && !b) return "white";
  if (b && !w) return "black";
  return null;
}

/** "Tal, Mikhail" -> "Tal"; "Magnus Carlsen" -> "Carlsen". */
export function shortName(name: string | null): string {
  if (!name) return "?";
  if (name.includes(",")) return name.split(",")[0].trim();
  const parts = name.trim().split(/\s+/);
  return parts[parts.length - 1];
}

export function yearOf(date: string | null): string {
  const y = date?.slice(0, 4) ?? "";
  return /^\d{4}$/.test(y) ? y : "";
}

export function gameTitle(white: string | null, black: string | null, date: string | null): string {
  const y = yearOf(date);
  return `${shortName(white)} vs ${shortName(black)}${y ? `, ${y}` : ""}`;
}
