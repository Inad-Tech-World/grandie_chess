import { useEffect, useState } from "react";
import { commands, type PositionInfo } from "./api";

const cache = new Map<string, PositionInfo>();

/** Seed the cache with a position Rust already returned (e.g. after a move),
 *  so the board can accept the next move without waiting. */
export function primePositionInfo(info: PositionInfo) {
  if (cache.size > 2000) cache.clear();
  cache.set(info.fen, info);
}

/** Legal moves and status for a FEN, computed by the Rust rules engine and cached. */
export function usePositionInfo(fen: string | undefined): PositionInfo | null {
  const [state, setState] = useState<{ fen: string; info: PositionInfo } | null>(null);
  useEffect(() => {
    if (!fen) return;
    const hit = cache.get(fen);
    if (hit) {
      setState({ fen, info: hit });
      return;
    }
    let live = true;
    void commands.chessPositionInfo(fen).then((r) => {
      if (r.status !== "ok") return;
      if (cache.size > 2000) cache.clear();
      cache.set(fen, r.data);
      if (live) setState({ fen, info: r.data });
    });
    return () => {
      live = false;
    };
  }, [fen]);
  if (!fen) return null;
  if (state?.fen === fen) return state.info;
  return cache.get(fen) ?? null;
}
