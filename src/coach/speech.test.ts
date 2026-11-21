import { describe, expect, it } from "vitest";
import { readingMs, sanToSpeech } from "./speech";

describe("sanToSpeech", () => {
  it("reads moves the way a coach says them", () => {
    expect(sanToSpeech("e4")).toBe("e4");
    expect(sanToSpeech("Nf3")).toBe("Knight to f3");
    expect(sanToSpeech("Nxe5+")).toBe("Knight takes e5, check");
    expect(sanToSpeech("exd5")).toBe("e takes d5");
    expect(sanToSpeech("Rad1")).toBe("Rook a to d1");
    expect(sanToSpeech("O-O")).toBe("Castles kingside");
    expect(sanToSpeech("O-O-O+")).toBe("Castles queenside, check");
    expect(sanToSpeech("e8=Q#")).toBe("e8, promotes to queen, checkmate");
    expect(sanToSpeech("Qxf7#")).toBe("Queen takes f7, checkmate");
  });

  it("waits longer for longer lines, within limits", () => {
    expect(readingMs("Nf3")).toBe(1200);
    expect(readingMs("one two three four five six seven eight nine ten")).toBe(3600);
    expect(readingMs("word ".repeat(200))).toBe(12000);
    expect(readingMs("one two three four five six seven eight nine ten", 2)).toBe(1800);
  });
});
