import { describe, expect, it } from "vitest";
import { themeLabel } from "./PuzzlesScreen";

describe("themeLabel", () => {
  it("turns Lichess theme keys into readable labels", () => {
    expect(themeLabel("mateIn2")).toBe("Mate in 2");
    expect(themeLabel("hangingPiece")).toBe("Hanging piece");
    expect(themeLabel("fork")).toBe("Fork");
    expect(themeLabel("backRankMate")).toBe("Back rank mate");
  });
});
