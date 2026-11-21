// Move numbers for plies counted from a game's start position. The start may
// be a set-up position with Black to move or a fullmove number other than 1,
// so "ply 1 is White's first move" isn't safe to assume.

export interface PlyNumber {
  /** The fullmove number shown before the move. */
  number: number;
  /** True when Black makes this move. */
  black: boolean;
}

function startOf(startFen: string): { fullmove: number; blackFirst: boolean } {
  const parts = startFen.trim().split(/\s+/);
  const fullmove = Number(parts[5]);
  return { blackFirst: parts[1] === "b", fullmove: Number.isFinite(fullmove) && fullmove > 0 ? fullmove : 1 };
}

/** Move number and mover of the `ply`-th move (1-based) after `startFen`. */
export function plyNumber(startFen: string, ply: number): PlyNumber {
  const { fullmove, blackFirst } = startOf(startFen);
  if (blackFirst) {
    return { number: fullmove + Math.floor(ply / 2), black: ply % 2 === 1 };
  }
  return { number: fullmove + Math.floor((ply - 1) / 2), black: ply % 2 === 0 };
}

/** "12." before a White move, "12…" before a Black move. */
export function moveNumberPrefix(startFen: string, ply: number): string {
  const { number, black } = plyNumber(startFen, ply);
  return `${number}${black ? "…" : "."}`;
}

/** "12. Nf3" or "12… Nf6". */
export function moveLabel(startFen: string, ply: number, san: string): string {
  return `${moveNumberPrefix(startFen, ply)} ${san}`;
}
