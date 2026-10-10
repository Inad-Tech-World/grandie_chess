import { useEffect, useRef, useState } from "react";
import { ActionIcon, Group, SegmentedControl, Tooltip } from "@mantine/core";
import { IconEraser } from "@tabler/icons-react";
import { Chessground } from "@lichess-org/chessground";
import type { Api } from "@lichess-org/chessground/api";
import type { Key, Piece, Role } from "@lichess-org/chessground/types";
import type { Side } from "../bindings";

const ROLES: Role[] = ["king", "queen", "rook", "bishop", "knight", "pawn"];

/** Castling rights that are still possible with kings and rooks on their
 *  home squares (the editor can't know the game's history). */
export function inferCastling(boardFen: string): string {
  const rows = boardFen.split("/");
  const expand = (row: string) => row.replace(/\d/g, (d) => ".".repeat(Number(d)));
  const rank1 = expand(rows[7] ?? "");
  const rank8 = expand(rows[0] ?? "");
  let rights = "";
  if (rank1[4] === "K") {
    if (rank1[7] === "R") rights += "K";
    if (rank1[0] === "R") rights += "Q";
  }
  if (rank8[4] === "k") {
    if (rank8[7] === "r") rights += "k";
    if (rank8[0] === "r") rights += "q";
  }
  return rights || "-";
}

export function fullFen(boardFen: string, turn: Side): string {
  return `${boardFen} ${turn === "white" ? "w" : "b"} ${inferCastling(boardFen)} - 0 1`;
}

interface BoardEditorProps {
  /** Initial piece placement (the first field of a FEN). */
  initialBoard: string;
  size: number;
  turn: Side;
  onTurnChange: (turn: Side) => void;
  /** Called with the board part of the FEN whenever it changes. */
  onChange: (boardFen: string) => void;
}

/** Free-placement board: drag pieces, drop them off to delete, or pick a
 *  piece from the palette and click squares to place it. */
export function BoardEditor({ initialBoard, size, turn, onTurnChange, onChange }: BoardEditorProps) {
  const el = useRef<HTMLDivElement>(null);
  const api = useRef<Api | null>(null);
  const [brush, setBrush] = useState<Piece | "erase" | null>(null);
  const brushRef = useRef(brush);
  brushRef.current = brush;
  const onChangeRef = useRef(onChange);
  onChangeRef.current = onChange;

  useEffect(() => {
    if (!el.current) return;
    const cg = Chessground(el.current, {
      fen: initialBoard,
      movable: { free: true, color: "both", showDests: false },
      draggable: { deleteOnDropOff: true },
      premovable: { enabled: false },
      animation: { enabled: false },
      highlight: { lastMove: false, check: false },
      events: {
        change: () => onChangeRef.current(cg.getFen()),
        select: (key: Key) => {
          const b = brushRef.current;
          if (!b) return;
          cg.setPieces(new Map([[key, b === "erase" ? undefined : b]]));
          cg.selectSquare(null);
          onChangeRef.current(cg.getFen());
        },
      },
    });
    api.current = cg;
    return () => cg.destroy();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    api.current?.set({ fen: initialBoard });
    api.current && onChangeRef.current(api.current.getFen());
  }, [initialBoard]);

  useEffect(() => {
    api.current?.redrawAll();
  }, [size]);

  const paletteButton = (piece: Piece) => {
    const active = brush !== null && brush !== "erase" && brush.role === piece.role && brush.color === piece.color;
    return (
      <button
        key={`${piece.color}-${piece.role}`}
        type="button"
        className="palette-piece cg-wrap"
        data-active={active}
        aria-label={`Place ${piece.color} ${piece.role}`}
        onClick={() => setBrush(active ? null : piece)}
      >
        <piece className={`${piece.role} ${piece.color}`} />
      </button>
    );
  };

  return (
    <div>
      <Group gap={4} mb={6} wrap="nowrap">
        {ROLES.map((role) => paletteButton({ role, color: "black" }))}
      </Group>
      <div className="board-box" style={{ width: size, height: size }}>
        <div ref={el} style={{ width: "100%", height: "100%" }} />
      </div>
      <Group gap={4} mt={6} wrap="nowrap" justify="space-between">
        <Group gap={4} wrap="nowrap">
          {ROLES.map((role) => paletteButton({ role, color: "white" }))}
          <Tooltip label="Eraser: click squares to clear them">
            <ActionIcon
              variant={brush === "erase" ? "filled" : "default"}
              size={34}
              aria-label="Eraser"
              onClick={() => setBrush(brush === "erase" ? null : "erase")}
            >
              <IconEraser size={18} />
            </ActionIcon>
          </Tooltip>
        </Group>
        <SegmentedControl
          size="xs"
          value={turn}
          onChange={(v) => onTurnChange(v as Side)}
          data={[
            { value: "white", label: "White to move" },
            { value: "black", label: "Black to move" },
          ]}
        />
      </Group>
    </div>
  );
}
