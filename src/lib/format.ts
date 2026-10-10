import type { EndReason, GameOutcome, Side, TimeControl } from "../bindings";

/** Clock display: h:mm:ss, m:ss, or s.t under twenty seconds. */
export function formatClock(ms: number | null | undefined): string {
  const t = Math.max(0, ms ?? 0);
  if (t < 20_000) {
    return (Math.floor(t / 100) / 10).toFixed(1);
  }
  const total = Math.floor(t / 1000);
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = total % 60;
  const ss = s.toString().padStart(2, "0");
  return h > 0 ? `${h}:${m.toString().padStart(2, "0")}:${ss}` : `${m}:${ss}`;
}

export function formatTimeControl(tc: TimeControl | null | undefined): string {
  if (!tc) return "Untimed";
  const minutes = tc.initialMs / 60_000;
  const base = Number.isInteger(minutes) ? `${minutes}` : minutes.toFixed(1);
  return `${base}+${Math.round(tc.incrementMs / 1000)}`;
}

const REASONS: Record<EndReason, string> = {
  checkmate: "checkmate",
  stalemate: "stalemate",
  insufficientMaterial: "insufficient material",
  threefoldRepetition: "threefold repetition",
  fiftyMoveRule: "the fifty-move rule",
  timeout: "time",
  timeoutVsInsufficientMaterial: "time vs insufficient material",
  resignation: "resignation",
  agreement: "agreement",
  aborted: "abandoned",
};

export function describeOutcome(outcome: GameOutcome, userSide?: Side): string {
  if (outcome.reason === "aborted") return "Game abandoned";
  const how = REASONS[outcome.reason];
  if (!outcome.winner) return `Draw by ${how}`;
  if (userSide) {
    return outcome.winner === userSide ? `You won by ${how}` : `You lost by ${how}`;
  }
  return `${outcome.winner === "white" ? "White" : "Black"} won by ${how}`;
}

export function formatBytes(bytes: number | null | undefined): string {
  const b = bytes ?? 0;
  if (b >= 1e9) return `${(b / 1e9).toFixed(1)} GB`;
  if (b >= 1e6) return `${(b / 1e6).toFixed(1)} MB`;
  if (b >= 1e3) return `${Math.round(b / 1e3)} KB`;
  return `${b} B`;
}

/** Move number prefix for a ply counted from a FEN's turn and fullmove fields. */
export function moveNumber(fen: string): { number: number; white: boolean } {
  const parts = fen.split(" ");
  return { white: parts[1] !== "b", number: Number(parts[5] ?? 1) || 1 };
}

const NAG_SYMBOLS: Record<number, string> = { 1: "!", 2: "?", 3: "!!", 4: "??", 5: "!?", 6: "?!" };

export function nagSymbol(nag: number): string {
  return NAG_SYMBOLS[nag] ?? `$${nag}`;
}

/** Expected score for White from a WDL triple seen by the side to move. */
export function whiteExpectedScore(wdl: { w: number; d: number; l: number }, turn: Side): number {
  const total = wdl.w + wdl.d + wdl.l || 1;
  const forMover = (wdl.w + wdl.d / 2) / total;
  return turn === "white" ? forMover : 1 - forMover;
}
