import { useEffect, useRef } from "react";
import { moveNumber } from "../lib/format";

export interface ListMove {
  san: string;
  /** Position before the move, used for numbering. */
  fenBefore: string;
  annotation?: { symbol: string; color: string } | null;
}

interface MoveListProps {
  moves: ListMove[];
  /** Index of the highlighted move (0-based), or -1 for the start position. */
  current: number;
  onSelect?: (index: number) => void;
}

/** Two-column move list (number, White, Black) for a single line. */
export function MoveList({ moves, current, onSelect }: MoveListProps) {
  const currentRef = useRef<HTMLSpanElement>(null);
  useEffect(() => {
    currentRef.current?.scrollIntoView({ block: "nearest" });
  }, [current, moves.length]);

  const rows: { number: number; white?: number; black?: number }[] = [];
  moves.forEach((m, i) => {
    const { number, white } = moveNumber(m.fenBefore);
    if (white || rows.length === 0 || rows[rows.length - 1].number !== number) {
      if (white) rows.push({ number, white: i });
      else rows.push({ number, black: i });
    } else {
      rows[rows.length - 1].black = i;
    }
  });

  const cell = (i: number | undefined) => {
    if (i === undefined) return <span />;
    const m = moves[i];
    return (
      <span
        className="move"
        data-current={i === current}
        ref={i === current ? currentRef : undefined}
        onClick={() => onSelect?.(i)}
      >
        {m.san}
        {m.annotation && (
          <span className="nag" style={{ color: m.annotation.color }}>
            {m.annotation.symbol}
          </span>
        )}
      </span>
    );
  };

  return (
    <div className="move-list" role="list" aria-label="Moves">
      {rows.map((r) => (
        <div className="move-row" key={`${r.number}-${r.white ?? r.black}`} role="listitem">
          <span className="move-num">{r.number}.</span>
          {r.white !== undefined ? cell(r.white) : <span className="move muted">…</span>}
          {cell(r.black)}
        </div>
      ))}
    </div>
  );
}
