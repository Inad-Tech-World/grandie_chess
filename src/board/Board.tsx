import { useEffect, useMemo, useRef, useState } from "react";
import { Chessground } from "@lichess-org/chessground";
import type { Api } from "@lichess-org/chessground/api";
import type { Config } from "@lichess-org/chessground/config";
import type { DrawShape } from "@lichess-org/chessground/draw";
import type { Key } from "@lichess-org/chessground/types";
import type { Side } from "../bindings";
import { PromotionPicker, type PromotionRole } from "./PromotionPicker";

export interface Arrow {
  uci: string;
  brush?: "green" | "red" | "blue" | "yellow" | "paleBlue" | "paleGreen" | "paleGrey";
}

export interface BoardProps {
  fen: string;
  orientation: Side;
  turn: Side;
  /** Legal moves; omit or leave empty to make the board view-only. */
  dests?: Partial<Record<string, string[]>>;
  /** Which side the user may move (and premove). */
  movableColor?: Side;
  lastMove?: string | null;
  check?: boolean;
  arrows?: Arrow[];
  premove?: boolean;
  coordinates?: boolean;
  animationMs?: number;
  size: number;
  /** Called with a UCI move; resolve false to snap the piece back. */
  onMove?: (uci: string) => Promise<boolean> | boolean | void;
}

function toDests(dests: BoardProps["dests"]): Map<Key, Key[]> {
  const map = new Map<Key, Key[]>();
  for (const [from, tos] of Object.entries(dests ?? {})) {
    if (tos && tos.length) map.set(from as Key, tos as Key[]);
  }
  return map;
}

function lastMoveKeys(uci: string | null | undefined): Key[] | undefined {
  if (!uci || uci.length < 4) return undefined;
  return [uci.slice(0, 2) as Key, uci.slice(2, 4) as Key];
}

export function Board(props: BoardProps) {
  const el = useRef<HTMLDivElement>(null);
  const api = useRef<Api | null>(null);
  const onMoveRef = useRef(props.onMove);
  onMoveRef.current = props.onMove;
  const [promotion, setPromotion] = useState<{ orig: string; dest: string; color: Side } | null>(null);

  const dests = useMemo(() => toDests(props.dests), [props.dests]);
  const canMove = dests.size > 0 && !!props.movableColor;

  const config: Config = useMemo(
    () => ({
      fen: props.fen,
      orientation: props.orientation,
      turnColor: props.turn,
      check: props.check ? props.turn : false,
      lastMove: lastMoveKeys(props.lastMove),
      coordinates: props.coordinates ?? true,
      animation: { enabled: (props.animationMs ?? 200) > 0, duration: props.animationMs ?? 200 },
      highlight: { lastMove: true, check: true },
      movable: {
        free: false,
        color: canMove || props.premove ? props.movableColor : undefined,
        dests,
        showDests: true,
        rookCastle: true,
      },
      premovable: { enabled: !!props.premove, showDests: true, castle: true },
      draggable: { enabled: true, showGhost: true },
      drawable: { enabled: true, visible: true },
    }),
    [props.fen, props.orientation, props.turn, props.check, props.lastMove, props.coordinates, props.animationMs, props.movableColor, props.premove, dests, canMove],
  );

  const configRef = useRef(config);
  configRef.current = config;

  // Create once, then push config changes.
  useEffect(() => {
    if (!el.current) return;
    const cg = Chessground(el.current, {
      ...config,
      movable: {
        ...config.movable,
        events: {
          after: (orig, dest) => {
            const piece = cg.state.pieces.get(dest);
            const lastRank = dest[1] === "8" || dest[1] === "1";
            if (piece?.role === "pawn" && lastRank) {
              setPromotion({ orig, dest, color: piece.color });
              return;
            }
            void submit(`${orig}${dest}`);
          },
        },
      },
    });
    api.current = cg;
    return () => {
      cg.destroy();
      api.current = null;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    const cg = api.current;
    if (!cg) return;
    cg.set(config);
    // Play a queued premove once it becomes the user's turn.
    if (props.premove && canMove && props.turn === props.movableColor) {
      cg.playPremove();
    }
  }, [config, props.premove, canMove, props.turn, props.movableColor]);

  useEffect(() => {
    const shapes: DrawShape[] = (props.arrows ?? [])
      .filter((a) => a.uci.length >= 4)
      .map((a) => ({ orig: a.uci.slice(0, 2) as Key, dest: a.uci.slice(2, 4) as Key, brush: a.brush ?? "green" }));
    api.current?.setAutoShapes(shapes);
  }, [props.arrows]);

  useEffect(() => {
    api.current?.redrawAll();
  }, [props.size]);

  async function submit(uci: string) {
    const ok = await onMoveRef.current?.(uci);
    if (ok === false) {
      // Rejected: restore the whole board state (pieces, turn, legal moves);
      // chessground already moved the piece and flipped the turn locally.
      api.current?.set(configRef.current);
    }
  }

  function choosePromotion(role: PromotionRole | null) {
    const p = promotion;
    setPromotion(null);
    if (!p) return;
    if (!role) {
      api.current?.set(configRef.current);
      return;
    }
    const letter = { queen: "q", rook: "r", bishop: "b", knight: "n" }[role];
    void submit(`${p.orig}${p.dest}${letter}`);
  }

  return (
    <div className="board-box" style={{ width: props.size, height: props.size }}>
      <div ref={el} style={{ width: "100%", height: "100%" }} />
      {promotion && <PromotionPicker color={promotion.color} onChoose={choosePromotion} />}
    </div>
  );
}

/** Measures the largest square board that fits a container. The returned
 *  `ref` is a callback ref, so it works for containers that appear later. */
export function useBoardSize(reserveHeight = 0, reserveWidth = 0) {
  const [node, ref] = useState<HTMLDivElement | null>(null);
  const [size, setSize] = useState(480);
  useEffect(() => {
    if (!node) return;
    const measure = () => {
      const r = node.getBoundingClientRect();
      const s = Math.floor(Math.min(r.width - reserveWidth, r.height - reserveHeight) / 8) * 8;
      setSize(Math.max(240, s));
    };
    measure();
    const ro = new ResizeObserver(measure);
    ro.observe(node);
    return () => ro.disconnect();
  }, [node, reserveHeight, reserveWidth]);
  return { ref, size };
}
