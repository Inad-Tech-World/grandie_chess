import type { Score, Side } from "../bindings";
import { formatScore } from "../lib/score";

interface EvalBarProps {
  /** White's win percentage, 0..100; null hides the fill. */
  whiteWin: number | null | undefined;
  score?: Score | null;
  orientation: Side;
  height: number;
}

/** Vertical bar beside the board: white's share is White's winning chances. */
export function EvalBar({ whiteWin, score, orientation, height }: EvalBarProps) {
  const pct = whiteWin === null || whiteWin === undefined ? 50 : Math.max(2, Math.min(98, whiteWin));
  const whiteBottom = orientation === "white";
  const label = formatScore(score);
  const whiteAhead = (whiteWin ?? 50) >= 50;
  return (
    <div
      className="eval-bar"
      style={{ height, flexDirection: whiteBottom ? "column" : "column-reverse" }}
      role="meter"
      aria-label="Evaluation"
      aria-valuenow={Math.round(pct)}
      aria-valuemin={0}
      aria-valuemax={100}
    >
      <div style={{ flex: 1 }} />
      <div className="eval-bar-white" style={{ height: `${pct}%` }} />
      {label && (
        <span
          className="eval-bar-label"
          style={{
            [whiteAhead === whiteBottom ? "bottom" : "top"]: 4,
            color: whiteAhead ? "#333" : "#eee",
          }}
        >
          {label.replace("+", "")}
        </span>
      )}
    </div>
  );
}
