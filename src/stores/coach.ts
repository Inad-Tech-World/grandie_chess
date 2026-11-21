// The coach runner: plays a CoachScript step by step on whichever board owns
// it. While a script runs, that screen's board shows the coach's position
// (see useCoachBoard) and the user's own position comes back when it ends or
// is stopped. One runner for the whole app, so only one coach talks at once.

import { create } from "zustand";
import type { CoachScript, CoachShape } from "../bindings";
import { currentSpeaker, type Speaker } from "../coach/speech";
import { playSound, soundForSan } from "../lib/sounds";
import { useSettings } from "./settings";

export type CoachStatus = "idle" | "loading" | "playing" | "paused" | "done";

export interface TranscriptEntry {
  id: number;
  from: "coach" | "user";
  text: string;
  /** A script this entry belongs to, so it can be replayed. */
  scriptId?: string;
  pending?: boolean;
}

interface CoachState {
  /** Screen that owns the board, e.g. "analysis:<tab>" or "play:<session>". */
  owner: string | null;
  script: CoachScript | null;
  index: number;
  status: CoachStatus;
  loadingLabel: string | null;
  /** Position the coach is showing; null shows the screen's own position. */
  fen: string | null;
  lastMove: string | null;
  shapes: CoachShape[];
  caption: string | null;
  notice: string | null;
  /** Conversation and narration log, per screen. */
  transcripts: Record<string, TranscriptEntry[]>;
  /** Scripts by id, for replaying from the transcript. */
  scripts: Record<string, CoachScript>;
  play: (script: CoachScript, owner: string) => void;
  /** Play after whatever is running now (or straight away if nothing is). */
  enqueue: (script: CoachScript, owner: string) => void;
  /** Speak a line without taking over the board (queued like a script). */
  speak: (owner: string, text: string) => void;
  setLoading: (owner: string, label: string | null) => void;
  pause: () => void;
  resume: () => void;
  skip: () => void;
  stop: () => void;
  replay: (scriptId?: string) => void;
  say: (owner: string, text: string, from?: "coach" | "user") => number;
  updateEntry: (id: number, patch: Partial<TranscriptEntry>) => void;
  clearTranscript: (owner: string) => void;
  /** Stop and forget everything that belongs to a screen (it closed). */
  release: (owner: string) => void;
}

let generation = 0;
let abort: AbortController | null = null;
let speaker: Speaker | null = null;
let resumeWaiter: (() => void) | null = null;
let nextEntryId = 1;
let queue: { script: CoachScript; owner: string }[] = [];

/** A script that only talks: an empty start position means "leave the board alone". */
export function speechScript(text: string): CoachScript {
  return {
    id: `say-${Date.now()}-${Math.random().toString(36).slice(2, 7)}`,
    coachId: "",
    title: "",
    startFen: "",
    startLastMove: null,
    returnFen: "",
    returnLastMove: null,
    steps: [{ type: "say", id: "s1", role: "summary", text }],
    aiNarrated: false,
    notice: null,
  };
}

function sleep(ms: number, signal: AbortSignal): Promise<void> {
  return new Promise((resolve) => {
    if (signal.aborted) return resolve();
    const fast = (window as unknown as { __GRANDIE_COACH_FAST__?: boolean }).__GRANDIE_COACH_FAST__;
    const t = setTimeout(resolve, fast ? Math.min(ms, 20) : ms);
    signal.addEventListener(
      "abort",
      () => {
        clearTimeout(t);
        resolve();
      },
      { once: true },
    );
  });
}

export const useCoach = create<CoachState>()((set, get) => {
  async function run(script: CoachScript, myGen: number, signal: AbortSignal) {
    const anim = useSettings.getState().settings.animationMs;
    for (let i = get().index; i < script.steps.length; i++) {
      if (myGen !== generation || signal.aborted) return;
      if (get().status === "paused") {
        await new Promise<void>((resolve) => (resumeWaiter = resolve));
        if (myGen !== generation) return;
      }
      set({ index: i });
      const step = script.steps[i];
      switch (step.type) {
        case "say": {
          set({ caption: step.text });
          const lineAbort = new AbortController();
          const onAbort = () => lineAbort.abort();
          signal.addEventListener("abort", onAbort, { once: true });
          currentLine = lineAbort;
          await speaker!.speak(step.text, lineAbort.signal);
          signal.removeEventListener("abort", onAbort);
          currentLine = null;
          break;
        }
        case "move":
          set({ fen: step.fen, lastMove: step.uci });
          playSound(soundForSan(step.san));
          await sleep(anim + 200, signal);
          break;
        case "back":
          set({ fen: step.fen, lastMove: step.lastMove });
          playSound("move");
          await sleep(anim + 120, signal);
          break;
        case "reset":
          set({ fen: step.fen, lastMove: step.lastMove });
          await sleep(250, signal);
          break;
        case "shapes":
          set({ shapes: step.shapes });
          break;
        case "pause":
          await sleep(step.ms, signal);
          break;
      }
    }
    if (myGen !== generation) return;
    set({ status: "done", shapes: [] });
    const next = queue.shift();
    if (next) {
      start(next.script, next.owner);
      return;
    }
    // Let the last position sink in, then give the board back.
    if (script.startFen) await sleep(900, signal);
    if (myGen !== generation) return;
    set({ fen: null, lastMove: null, index: 0 });
  }

  let currentLine: AbortController | null = null;

  function start(script: CoachScript, owner: string, fromIndex = 0) {
    generation++;
    abort?.abort();
    speaker?.cancel();
    resumeWaiter?.();
    resumeWaiter = null;
    const ctrl = new AbortController();
    abort = ctrl;
    speaker = currentSpeaker((message) => set({ notice: message }));
    set({
      owner,
      script,
      index: fromIndex,
      status: "playing",
      loadingLabel: null,
      fen: script.startFen || (get().owner === owner ? get().fen : null),
      lastMove: script.startFen ? script.startLastMove : get().owner === owner ? get().lastMove : null,
      shapes: [],
      caption: null,
      notice: script.notice,
      scripts: { ...get().scripts, [script.id]: script },
    });
    void run(script, generation, ctrl.signal);
  }

  const setTranscript = (owner: string, list: TranscriptEntry[]) => set({ transcripts: { ...get().transcripts, [owner]: list } });
  const transcriptOf = (owner: string) => get().transcripts[owner] ?? [];

  return {
    owner: null,
    script: null,
    index: 0,
    status: "idle",
    loadingLabel: null,
    fen: null,
    lastMove: null,
    shapes: [],
    caption: null,
    notice: null,
    transcripts: {},
    scripts: {},

    play: (script, owner) => {
      const lines = script.steps.flatMap((s) => (s.type === "say" ? [s.text] : []));
      if (lines.length) {
        setTranscript(owner, [...transcriptOf(owner), { id: nextEntryId++, from: "coach", text: lines.join(" "), scriptId: script.id }]);
      }
      start(script, owner);
    },

    enqueue: (script, owner) => {
      const busy = get().status === "playing" || get().status === "paused";
      if (busy && get().owner === owner) {
        queue.push({ script, owner });
        const lines = script.steps.flatMap((s) => (s.type === "say" ? [s.text] : []));
        if (lines.length && script.startFen) {
          setTranscript(owner, [...transcriptOf(owner), { id: nextEntryId++, from: "coach", text: lines.join(" "), scriptId: script.id }]);
        }
      } else if (script.startFen) {
        get().play(script, owner);
      } else {
        start(script, owner);
      }
    },

    speak: (owner, text) => {
      if (text.trim()) get().enqueue(speechScript(text), owner);
    },

    setLoading: (owner, label) => {
      if (label) {
        if (get().status === "playing" || get().status === "paused") get().stop();
        set({ owner, status: "loading", loadingLabel: label });
      } else if (get().status === "loading" && get().owner === owner) {
        set({ status: "idle", loadingLabel: null });
      }
    },

    pause: () => {
      if (get().status !== "playing") return;
      speaker?.cancel();
      currentLine?.abort();
      set({ status: "paused" });
    },

    resume: () => {
      if (get().status !== "paused") return;
      set({ status: "playing" });
      const waiter = resumeWaiter;
      resumeWaiter = null;
      waiter?.();
    },

    skip: () => {
      // Finish the current line now; the script moves on to the next step.
      currentLine?.abort();
      speaker?.cancel();
    },

    stop: () => {
      generation++;
      queue = [];
      abort?.abort();
      speaker?.cancel();
      resumeWaiter?.();
      resumeWaiter = null;
      set({ status: "idle", fen: null, lastMove: null, shapes: [], caption: null, index: 0 });
    },

    replay: (scriptId) => {
      const script = scriptId ? get().scripts[scriptId] : get().script;
      if (script && get().owner) start(script, get().owner!);
    },

    say: (owner, text, from = "coach") => {
      const id = nextEntryId++;
      setTranscript(owner, [...transcriptOf(owner), { id, from, text }]);
      return id;
    },

    updateEntry: (id, patch) => {
      for (const [owner, list] of Object.entries(get().transcripts)) {
        if (list.some((e) => e.id === id)) {
          setTranscript(
            owner,
            list.map((e) => (e.id === id ? { ...e, ...patch } : e)),
          );
        }
      }
    },

    clearTranscript: (owner) => setTranscript(owner, []),

    release: (owner) => {
      queue = queue.filter((q) => q.owner !== owner);
      const { [owner]: _gone, ...rest } = get().transcripts;
      set({ transcripts: rest });
      if (get().owner === owner) {
        get().stop();
        set({ owner: null, script: null, notice: null });
      }
    },
  };
});

/** True while the coach is showing something on this screen's board. */
export function useCoachActive(owner: string) {
  return useCoach((s) => s.owner === owner && s.fen !== null);
}

const EMPTY: TranscriptEntry[] = [];
export function useTranscript(owner: string) {
  return useCoach((s) => s.transcripts[owner] ?? EMPTY);
}
