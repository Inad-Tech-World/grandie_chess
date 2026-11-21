import { useCallback, useRef, useState } from "react";
import { commands, notifyError, unwrap, type MoveReview } from "../lib/api";
import { playSound } from "../lib/sounds";

export interface RetryState {
  move: MoveReview;
  /** Node whose position the user must find a better move in. */
  nodeId: number;
  status: "waiting" | "checking" | "correct" | "wrong";
  tried?: { san: string; winAfter: number };
}

/** "Retry this position": the user tries to find a better move than the one
 *  played. Results that come back after retry was left (or restarted) are
 *  ignored, so the board never shows an outdated verdict. */
export function useRetry() {
  const [retry, setRetryState] = useState<RetryState | null>(null);
  const ref = useRef<RetryState | null>(null);

  // The ref is the source of truth so async checks can read it right away.
  const set = useCallback((next: RetryState | null | ((cur: RetryState | null) => RetryState | null)) => {
    const value = typeof next === "function" ? next(ref.current) : next;
    ref.current = value;
    setRetryState(value);
  }, []);

  const start = useCallback((move: MoveReview, nodeId: number) => set({ move, nodeId, status: "waiting" }), [set]);
  const stop = useCallback(() => set(null), [set]);
  const again = useCallback(() => set((cur) => (cur ? { ...cur, status: "waiting", tried: undefined } : cur)), [set]);

  /** Check a candidate move in the retry position `fen`. */
  const check = useCallback(
    async (fen: string, uci: string) => {
      const attempt = ref.current;
      if (!attempt) return;
      const same = (cur: RetryState | null) => !!cur && cur.nodeId === attempt.nodeId && cur.move.ply === attempt.move.ply;
      set((cur) => (same(cur) ? { ...cur!, status: "checking" } : cur));
      try {
        const result = await unwrap(commands.reviewCheckMove(fen, uci));
        if (!same(ref.current)) return;
        const ok = (result.winAfter ?? 0) >= (attempt.move.winBefore ?? 0) - 5 || uci === attempt.move.best;
        set((cur) => (same(cur) ? { ...cur!, status: ok ? "correct" : "wrong", tried: { san: result.san, winAfter: result.winAfter ?? 0 } } : cur));
        playSound(ok ? "correct" : "wrong");
      } catch (e) {
        if (!same(ref.current)) return;
        notifyError("Could not check the move", e);
        set((cur) => (same(cur) ? { ...cur!, status: "waiting" } : cur));
      }
    },
    [set],
  );

  return { retry, active: () => ref.current, start, stop, again, check };
}
