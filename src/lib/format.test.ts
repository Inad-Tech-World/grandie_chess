import { describe, expect, it } from "vitest";
import { describeOutcome, formatBytes, formatClock, formatTimeControl, moveNumber, whiteExpectedScore } from "./format";

describe("formatClock", () => {
  it("shows tenths under twenty seconds", () => {
    expect(formatClock(19_950)).toBe("19.9");
    expect(formatClock(0)).toBe("0.0");
    expect(formatClock(-50)).toBe("0.0");
  });
  it("shows minutes and hours", () => {
    expect(formatClock(300_000)).toBe("5:00");
    expect(formatClock(65_400)).toBe("1:05");
    expect(formatClock(3_723_000)).toBe("1:02:03");
  });
});

describe("formatting helpers", () => {
  it("formats time controls", () => {
    expect(formatTimeControl(null)).toBe("Untimed");
    expect(formatTimeControl({ initialMs: 180_000, incrementMs: 2_000 })).toBe("3+2");
    expect(formatTimeControl({ initialMs: 30_000, incrementMs: 0 })).toBe("0.5+0");
  });
  it("describes outcomes from the user's side", () => {
    const win = { result: "1-0", winner: "white" as const, reason: "checkmate" as const };
    expect(describeOutcome(win, "white")).toBe("You won by checkmate");
    expect(describeOutcome(win, "black")).toBe("You lost by checkmate");
    expect(describeOutcome(win)).toBe("White won by checkmate");
    expect(describeOutcome({ result: "1/2-1/2", winner: null, reason: "stalemate" })).toBe("Draw by stalemate");
  });
  it("formats sizes and move numbers", () => {
    expect(formatBytes(18_648_209)).toBe("18.6 MB");
    expect(moveNumber("8/8/8/8/8/8/8/8 b - - 0 40")).toEqual({ white: false, number: 40 });
  });
  it("converts WDL to White's expected score", () => {
    expect(whiteExpectedScore({ w: 500, d: 500, l: 0 }, "white")).toBeCloseTo(0.75);
    expect(whiteExpectedScore({ w: 500, d: 500, l: 0 }, "black")).toBeCloseTo(0.25);
  });
});
