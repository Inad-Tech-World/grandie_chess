import type { MoveClass, Score } from "../bindings";

/** "+0.35", "-1.20", "M3", "-M2" from White's point of view. */
export function formatScore(score: Score | null | undefined): string {
  if (!score) return "";
  if (score.kind === "mate") {
    if (score.value === 0) return "#";
    return score.value > 0 ? `M${score.value}` : `-M${-score.value}`;
  }
  const pawns = score.value / 100;
  return `${pawns > 0 ? "+" : ""}${pawns.toFixed(2)}`;
}

export function formatWin(whiteWin: number | null | undefined): string {
  if (whiteWin === null || whiteWin === undefined) return "";
  return `${Math.round(whiteWin)}%`;
}

export const CLASS_STYLE: Record<MoveClass, { label: string; symbol: string; color: string }> = {
  brilliant: { label: "Brilliant", symbol: "!!", color: "var(--annot-brilliant)" },
  great: { label: "Great move", symbol: "!", color: "var(--annot-great)" },
  best: { label: "Best", symbol: "★", color: "var(--annot-best)" },
  good: { label: "Good", symbol: "", color: "var(--text-dim)" },
  inaccuracy: { label: "Inaccuracy", symbol: "?!", color: "var(--annot-inaccuracy)" },
  mistake: { label: "Mistake", symbol: "?", color: "var(--annot-mistake)" },
  blunder: { label: "Blunder", symbol: "??", color: "var(--annot-blunder)" },
};
