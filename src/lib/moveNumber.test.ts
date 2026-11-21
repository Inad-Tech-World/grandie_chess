import { describe, expect, it } from "vitest";
import { moveLabel, plyNumber } from "./moveNumber";

const START = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

describe("plyNumber", () => {
  it("numbers a normal game from move 1", () => {
    expect(plyNumber(START, 1)).toEqual({ number: 1, black: false });
    expect(plyNumber(START, 2)).toEqual({ number: 1, black: true });
    expect(plyNumber(START, 3)).toEqual({ number: 2, black: false });
    expect(moveLabel(START, 4, "Nf6")).toBe("2… Nf6");
  });

  it("starts at Black's move in a set-up position", () => {
    const fen = "4k3/8/8/8/8/8/8/4K2R b K - 3 17";
    expect(plyNumber(fen, 1)).toEqual({ number: 17, black: true });
    expect(plyNumber(fen, 2)).toEqual({ number: 18, black: false });
    expect(plyNumber(fen, 3)).toEqual({ number: 18, black: true });
    expect(moveLabel(fen, 1, "Kd7")).toBe("17… Kd7");
  });

  it("uses the fullmove number of a White-to-move position", () => {
    const fen = "4k3/8/8/8/8/8/8/4K2R w K - 0 30";
    expect(moveLabel(fen, 1, "O-O")).toBe("30. O-O");
    expect(moveLabel(fen, 2, "Ke7")).toBe("30… Ke7");
  });

  it("falls back to move 1 for a FEN without counters", () => {
    expect(plyNumber("4k3/8/8/8/8/8/8/4K3 b - -", 1)).toEqual({ number: 1, black: true });
  });
});
