import { describe, expect, it } from "vitest";
import { playSound, soundForSan } from "./sounds";

describe("sounds", () => {
  it("picks the sound from the SAN", () => {
    expect(soundForSan("e4")).toBe("move");
    expect(soundForSan("Nxe5")).toBe("capture");
    expect(soundForSan("Qh5+")).toBe("check");
    expect(soundForSan("Qxf7#")).toBe("check");
    expect(soundForSan(null)).toBe("move");
  });

  it("does nothing without Web Audio (e.g. in tests)", () => {
    expect(() => playSound("move")).not.toThrow();
  });
});
