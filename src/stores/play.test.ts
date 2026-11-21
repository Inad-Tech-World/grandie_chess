import { beforeEach, describe, expect, it } from "vitest";
import type { PlaySnapshot } from "../bindings";
import { usePlay } from "./play";

const START = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

function snap(seq: number, moves: string[]): PlaySnapshot {
  return {
    sessionId: 1,
    seq,
    setup: { opponent: "maia-1100", userSide: "white", timeControl: null, startFen: null, allowTakebacks: true },
    level: { id: "maia-1100", name: "Maia 1100", kind: "maia", network: "maia-1100", nodes: 1, temperature: null, tempDecayMoves: 0, elo: 1100 },
    network: "maia-1100",
    userSide: "white",
    whiteName: "You",
    blackName: "Maia 1100",
    startFen: START,
    moves: moves.map((uci) => ({ uci, san: uci, fen: START, clockMs: null, thinkMs: 0 })),
    position: { fen: START, turn: "white", dests: {}, check: false, fullmoves: 1, halfmoves: 0, outcome: null },
    lastMove: moves[moves.length - 1] ?? null,
    clock: null,
    outcome: null,
    engineThinking: false,
    hint: null,
    pgnPath: null,
    canTakeback: false,
    canOfferDraw: true,
    error: null,
  };
}

describe("play store", () => {
  beforeEach(() => usePlay.getState().remove(1));

  it("ignores a snapshot older than the one it has", () => {
    // The engine's reply (seq 3) arrives before the answer to the user's move (seq 2).
    usePlay.getState().setSnapshot(snap(3, ["e2e4", "a7a5"]));
    usePlay.getState().setSnapshot(snap(2, ["e2e4"]));
    expect(usePlay.getState().snapshots[1].moves).toHaveLength(2);
    usePlay.getState().setSnapshot(snap(4, ["e2e4", "a7a5", "d2d4"]));
    expect(usePlay.getState().snapshots[1].seq).toBe(4);
  });

  it("keeps one-time flags per game until the game is removed", () => {
    usePlay.getState().setFlag(1, "reviewOpened");
    expect(usePlay.getState().flags[1]?.reviewOpened).toBe(true);
    expect(usePlay.getState().flags[1]?.endSoundPlayed).toBeUndefined();
    usePlay.getState().remove(1);
    expect(usePlay.getState().flags[1]).toBeUndefined();
  });
});
