// While the coach owns a screen's board, the board shows the coach's
// position, arrows and circles, and can't be moved.

import type { Arrow, Brush, Circle } from "../board/Board";
import type { CoachShape } from "../bindings";
import { useCoach } from "../stores/coach";

const BRUSHES: Brush[] = ["green", "red", "blue", "yellow", "paleBlue", "paleGreen", "paleGrey"];

function brush(b: string): Brush {
  return (BRUSHES as string[]).includes(b) ? (b as Brush) : "green";
}

export function shapesToBoard(shapes: CoachShape[]): { arrows: Arrow[]; circles: Circle[] } {
  const arrows: Arrow[] = [];
  const circles: Circle[] = [];
  for (const s of shapes) {
    if (s.dest) arrows.push({ uci: `${s.orig}${s.dest}`, brush: brush(s.brush) });
    else circles.push({ square: s.orig, brush: brush(s.brush) });
  }
  return { arrows, circles };
}

export interface CoachBoard {
  /** The coach is showing its own position. */
  active: boolean;
  fen: string | null;
  lastMove: string | null;
  arrows: Arrow[];
  circles: Circle[];
}

const IDLE: CoachBoard = { active: false, fen: null, lastMove: null, arrows: [], circles: [] };

export function useCoachBoard(owner: string): CoachBoard {
  const mine = useCoach((s) => s.owner === owner && s.fen !== null);
  const fen = useCoach((s) => s.fen);
  const lastMove = useCoach((s) => s.lastMove);
  const shapes = useCoach((s) => s.shapes);
  if (!mine) return IDLE;
  return { active: true, fen, lastMove, ...shapesToBoard(shapes) };
}
