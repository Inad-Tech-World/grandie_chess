import { describe, expect, it } from "vitest";
import { fullFen, inferCastling } from "./BoardEditor";

describe("inferCastling", () => {
  it("keeps rights only for kings and rooks on home squares", () => {
    expect(inferCastling("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR")).toBe("KQkq");
    expect(inferCastling("r3k3/8/8/8/8/8/8/4K2R")).toBe("Kq");
    expect(inferCastling("8/8/8/8/8/8/8/R3K1R1")).toBe("Q");
    expect(inferCastling("8/8/4k3/8/8/8/8/4K3")).toBe("-");
  });
  it("builds a full FEN", () => {
    expect(fullFen("8/8/4k3/8/8/8/8/4K3", "black")).toBe("8/8/4k3/8/8/8/8/4K3 b - - 0 1");
  });
});
