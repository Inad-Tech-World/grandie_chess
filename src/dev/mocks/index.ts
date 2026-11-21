// Mock commands for the coach, AI, voice and masters features, kept out of
// mockBackend.ts so each feature owns its own file. Each module exports an
// `install(register, helpers)` function.

import type { emit } from "@tauri-apps/api/event";
import type { AppliedMove, PositionInfo } from "../../bindings";
import type { Chess } from "chessops/chess";
import { install as installAi } from "./ai";
import { install as installCoach } from "./coach";
import { install as installMasters } from "./masters";

export type Handler = (args: Record<string, unknown>) => unknown;
export type Register = (name: string, handler: Handler) => void;

/** Chess helpers from the main mock (chessops-based). */
export interface MockHelpers {
  emit: typeof emit;
  apply: (fen: string, uci: string) => AppliedMove;
  info: (pos: Chess) => PositionInfo;
  load: (fen: string) => Chess;
}

export function installFeatureMocks(register: Register, helpers: MockHelpers) {
  // Coach scripts don't wait for real speech in tests and previews.
  (window as unknown as { __GRANDIE_COACH_FAST__: boolean }).__GRANDIE_COACH_FAST__ = true;
  installAi(register, helpers);
  installCoach(register, helpers);
  installMasters(register, helpers);
}
