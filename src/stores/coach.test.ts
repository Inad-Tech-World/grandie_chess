import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { CoachScript } from "../bindings";
import { speechScript, useCoach } from "./coach";
import { useSettings } from "./settings";

vi.mock("../lib/sounds", () => ({ playSound: () => {}, soundForSan: () => "move" }));

const START = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";
const AFTER_E4 = "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq - 0 1";

function script(): CoachScript {
  return {
    id: "t1",
    coachId: "stockfish",
    title: "Test",
    startFen: START,
    startLastMove: null,
    returnFen: START,
    returnLastMove: null,
    steps: [
      { type: "say", id: "s1", role: "intro", text: "Watch the centre." },
      { type: "move", uci: "e2e4", san: "e4", fen: AFTER_E4 },
      { type: "say", id: "s2", role: "summary", text: "That's the idea." },
      { type: "back", fen: START, lastMove: null },
    ],
    aiNarrated: false,
    notice: null,
  };
}

describe("coach runner", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    useSettings.setState((s) => ({ settings: { ...s.settings, animationMs: 0, coach: { ...s.settings.coach, voiceOutput: "off", speechRate: 1 } } }));
    useCoach.getState().release("a");
    useCoach.getState().release("b");
  });
  afterEach(() => {
    useCoach.getState().stop();
    vi.useRealTimers();
  });

  it("plays steps in order and gives the board back at the end", async () => {
    const coach = useCoach.getState();
    coach.play(script(), "a");
    expect(useCoach.getState().fen).toBe(START);
    await vi.advanceTimersByTimeAsync(0);
    expect(useCoach.getState().caption).toBe("Watch the centre.");
    await vi.advanceTimersByTimeAsync(1300);
    expect(useCoach.getState().fen).toBe(AFTER_E4);
    expect(useCoach.getState().lastMove).toBe("e2e4");
    await vi.advanceTimersByTimeAsync(5000);
    expect(useCoach.getState().status).toBe("done");
    expect(useCoach.getState().fen).toBeNull();
    expect(useCoach.getState().transcripts["a"][0].text).toBe("Watch the centre. That's the idea.");
  });

  it("pauses, resumes and stops with the position restored", async () => {
    const coach = useCoach.getState();
    coach.play(script(), "a");
    await vi.advanceTimersByTimeAsync(10);
    coach.pause();
    expect(useCoach.getState().status).toBe("paused");
    await vi.advanceTimersByTimeAsync(10_000);
    expect(useCoach.getState().fen).toBe(START);
    coach.resume();
    await vi.advanceTimersByTimeAsync(300);
    expect(useCoach.getState().fen).toBe(AFTER_E4);
    coach.stop();
    expect(useCoach.getState().fen).toBeNull();
    expect(useCoach.getState().status).toBe("idle");
  });

  it("queues speech behind a running demonstration without moving the board", async () => {
    const coach = useCoach.getState();
    coach.play(script(), "a");
    coach.speak("a", "And one more thing.");
    await vi.advanceTimersByTimeAsync(8000);
    expect(useCoach.getState().caption).toBe("And one more thing.");
    await vi.advanceTimersByTimeAsync(3000);
    expect(useCoach.getState().fen).toBeNull();
    expect(speechScript("x").startFen).toBe("");
  });

  it("only the owning screen sees the coach", () => {
    useCoach.getState().play(script(), "a");
    expect(useCoach.getState().owner).toBe("a");
    useCoach.getState().release("b");
    expect(useCoach.getState().owner).toBe("a");
    useCoach.getState().release("a");
    expect(useCoach.getState().owner).toBeNull();
  });
});
