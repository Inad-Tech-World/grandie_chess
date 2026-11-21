import { commands, unwrap, type AppliedMove } from "./api";
import { primePositionInfo } from "./usePositionInfo";
import { playSound, soundForSan } from "./sounds";

/** Play a move with the Rust rules engine, seed the position cache with the
 *  result (so the board can take the next move at once) and play its sound.
 *  Throws an `AppError` for an illegal move. */
export async function applyMove(fen: string, uci: string, opts: { sound?: boolean } = {}): Promise<AppliedMove> {
  const applied = await unwrap(commands.chessApplyMove(fen, uci));
  primePositionInfo(applied.position);
  if (opts.sound !== false) playSound(soundForSan(applied.san));
  return applied;
}
