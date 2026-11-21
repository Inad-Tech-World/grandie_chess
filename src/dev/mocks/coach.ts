// Mock coach, voice and "learn from your mistakes" commands. Scripts are
// built with real chess rules (chessops) so the board shows legal positions.

import { parseSan } from "chessops/san";
import { makeUci } from "chessops/util";
import type { CoachScript, CoachStep, CoachVerdict, ExplainMoveRequest, MistakeCard, ReviewResult, Side, VoiceIntent } from "../../bindings";
import type { MockHelpers, Register } from "./index";

let scriptCounter = 0;

function sanToUci(h: MockHelpers, fen: string, san: string): string | null {
  const pos = h.load(fen);
  const m = parseSan(pos, san);
  return m ? makeUci(m) : null;
}

/** A small script builder over the mock's chess helpers. */
function builder(h: MockHelpers, fen: string, lastMove: string | null) {
  const steps: CoachStep[] = [];
  const history: { fen: string; lastMove: string | null }[] = [];
  let cur = fen;
  let last = lastMove;
  let says = 0;
  return {
    say(text: string, role: "intro" | "refutation" | "better" | "summary" | "idea" | "moment" = "intro") {
      steps.push({ type: "say", id: `s${++says}`, role, text });
    },
    move(uci: string) {
      const applied = h.apply(cur, uci);
      history.push({ fen: cur, lastMove: last });
      cur = applied.position.fen;
      last = applied.uci;
      steps.push({ type: "move", uci: applied.uci, san: applied.san, fen: cur });
      steps.push({ type: "pause", ms: 300 });
      return applied.san;
    },
    san(san: string) {
      const uci = sanToUci(h, cur, san);
      return uci ? this.move(uci) : null;
    },
    backAll() {
      while (history.length) {
        const prev = history.pop()!;
        cur = prev.fen;
        last = prev.lastMove;
        steps.push({ type: "back", fen: cur, lastMove: last });
      }
    },
    arrow(uci: string, brush: string) {
      steps.push({ type: "shapes", shapes: [{ orig: uci.slice(0, 2), dest: uci.slice(2, 4), brush }] });
    },
    reset(fen: string, lastMove: string | null) {
      history.length = 0;
      cur = fen;
      last = lastMove;
      steps.push({ type: "reset", fen, lastMove });
    },
    finish(title: string, returnFen: string, returnLastMove: string | null): CoachScript {
      return {
        id: `mock-script-${++scriptCounter}`,
        coachId: "stockfish",
        title,
        startFen: fen,
        startLastMove: lastMove,
        returnFen,
        returnLastMove,
        steps,
        aiNarrated: false,
        notice: null,
      };
    },
  };
}

/** Any legal move other than `not`, for demonstrations. */
function someMove(h: MockHelpers, fen: string, not?: string): string | null {
  const info = h.info(h.load(fen));
  for (const [from, tos] of Object.entries(info.dests)) {
    for (const to of tos ?? []) {
      const uci = `${from}${to}`;
      if (uci !== not) return uci;
    }
  }
  return null;
}

/** Moves to the a- or h-file count as mistakes in the mock. */
function mockClass(uci: string): CoachVerdict["class"] {
  return /^..[ah]/.test(uci) ? "mistake" : "best";
}

function explainScript(h: MockHelpers, fenBefore: string, lastMoveBefore: string | null, played: string, returnFen: string, returnLastMove: string | null): CoachScript {
  const b = builder(h, fenBefore, lastMoveBefore);
  const cls = mockClass(played);
  b.arrow(played, cls === "mistake" ? "red" : "green");
  const san = h.apply(fenBefore, played).san;
  b.say(cls === "mistake" ? `${san} is a mistake. Let's see why.` : `${san} is the best move.`);
  b.move(played);
  const reply = someMove(h, h.apply(fenBefore, played).position.fen);
  if (reply) {
    b.say("The opponent answers.", "refutation");
    b.move(reply);
  }
  b.backAll();
  const better = someMove(h, fenBefore, played);
  if (cls === "mistake" && better) {
    b.arrow(better, "green");
    b.say("Instead, this was stronger.", "better");
    b.move(better);
    b.backAll();
  }
  b.say("That's the idea.", "summary");
  return b.finish(`${san}: ${cls}`, returnFen, returnLastMove);
}

// ---- learn from your mistakes ----
let cards: MistakeCard[] = [];
let nextCardId = 1;

function interpret(h: MockHelpers, text: string, fen: string | null): VoiceIntent {
  const t = text.trim();
  if (!t) return { type: "empty" };
  if (/^take ?back$/i.test(t)) return { type: "command", command: "takeBack" };
  if (/^(stop|quiet)$/i.test(t)) return { type: "command", command: "stop" };
  if (/^why\??$/i.test(t)) return { type: "command", command: "explain" };
  if (fen) {
    const san = t.replace(/^knight\s*/i, "N").replace(/^bishop\s*/i, "B").replace(/\s+/g, "");
    const uci = sanToUci(h, fen, san);
    if (uci) return { type: "move", uci, san: h.apply(fen, uci).san };
  }
  return { type: "question", text: t };
}

export function install(register: Register, h: MockHelpers) {
  register("coach_explain_move", (a) => {
    const r = a.request as ExplainMoveRequest;
    const after = h.apply(r.fenBefore, r.played).position.fen;
    return explainScript(h, r.fenBefore, r.lastMoveBefore, r.played, r.returnFen ?? after, r.returnLastMove ?? r.played);
  });

  register("coach_check_move", (a) => {
    const fenBefore = a.fenBefore as string;
    const played = a.played as string;
    const cls = mockClass(played);
    const after = h.apply(fenBefore, played).position.fen;
    const verdict: CoachVerdict = {
      class: cls,
      winBefore: 55,
      winAfter: cls === "mistake" ? 38 : 55,
      bestSan: cls === "mistake" ? h.apply(fenBefore, someMove(h, fenBefore, played)!).san : null,
      script: cls === "mistake" || a.alwaysExplain ? explainScript(h, fenBefore, (a.lastMoveBefore as string | null) ?? null, played, after, played) : null,
    };
    return verdict;
  });

  register("coach_explain_line", (a) => {
    const fen = a.fen as string;
    const b = builder(h, fen, (a.lastMove as string | null) ?? null);
    const line = (a.line as string[]) ?? [];
    b.say(`The engine's idea here is ${line[0] ?? "this"}.`);
    for (const san of line.slice(0, 4)) if (!b.san(san)) break;
    b.backAll();
    b.say("That's where it leads.", "summary");
    return b.finish("The engine's idea", fen, (a.lastMove as string | null) ?? null);
  });

  register("coach_walk_game", (a) => {
    const review = a.review as ReviewResult;
    const moves = a.moves as string[];
    const plies = ((a.plies as number[] | null) ?? review.keyMoments).slice(0, 3);
    const first = review.evals[0]?.fen ?? review.startFen;
    const b = builder(h, first, null);
    b.say(`Let's go through the ${plies.length || 1} key moments of this game.`, "moment");
    for (const ply of plies) {
      const before = review.evals[ply - 1]?.fen;
      const uci = moves[ply - 1];
      if (!before || !uci) continue;
      b.reset(before, ply > 1 ? moves[ply - 2] : null);
      b.say(`Move ${Math.ceil(ply / 2)}: here is what happened.`, "moment");
      b.move(uci);
      b.backAll();
    }
    const last = review.evals[review.evals.length - 1]?.fen ?? first;
    return b.finish("Key moments of the game", last, moves[moves.length - 1] ?? null);
  });

  register("coach_ask", async (a) => {
    const requestId = a.requestId as string;
    const fen = (a.context as { fen: string }).fen;
    const question = a.question as string;
    const say = (text: string) => h.emit("coach-event", { requestId, kind: { type: "delta", text } });
    await h.emit("coach-event", { requestId, kind: { type: "tool", name: "analyse_position" } });
    const answer = `Good question about "${question}". The engine likes developing your pieces here.`;
    for (const word of answer.split(" ")) await say(word + " ");
    const demo = someMove(h, fen);
    if (demo) {
      const b = builder(h, fen, null);
      b.say("Watch this idea.", "idea");
      b.move(demo);
      b.backAll();
      await h.emit("coach-event", { requestId, kind: { type: "script", script: b.finish("A line", fen, null) } });
    }
    return answer;
  });

  register("coach_cancel", () => null);

  // ---- voice ----
  register("voice_start", () => null);
  register("voice_stop", (a) => {
    const text = (window as unknown as { __GRANDIE_MOCK_VOICE__?: string }).__GRANDIE_MOCK_VOICE__ ?? "";
    return { transcript: text, intent: interpret(h, text, (a.fen as string | null) ?? null) };
  });
  register("voice_cancel", () => null);
  register("voice_interpret", (a) => interpret(h, a.text as string, (a.fen as string | null) ?? null));

  // ---- learn from your mistakes ----
  register("mistakes_add", (a) => {
    const review = a.review as ReviewResult;
    const user = a.user as Side;
    let added = 0;
    review.moves.forEach((m, i) => {
      if (m.side !== user || (m.class !== "mistake" && m.class !== "blunder") || !m.best || !m.bestSan) return;
      const fen = review.evals[i]?.fen;
      if (!fen || cards.some((c) => c.fen === fen && c.played === m.uci)) return;
      cards.push({
        id: nextCardId++,
        gameId: (a.gameId as number | null) ?? null,
        fen,
        side: m.side,
        played: m.uci,
        playedSan: m.san,
        best: m.best,
        bestSan: m.bestSan,
        bestLine: m.bestLine,
        refutation: review.evals[i + 1]?.line ?? [],
        winBefore: m.winBefore,
        winAfter: m.winAfter,
        due: 0,
        reps: 0,
      });
      added++;
    });
    return added;
  });
  register("mistakes_due", () => cards.filter((c) => (c.due ?? 0) <= Date.now() / 1000));
  register("mistakes_stats", () => ({ total: cards.length, due: cards.filter((c) => (c.due ?? 0) <= Date.now() / 1000).length }));
  register("mistakes_grade", (a) => {
    const card = cards.find((c) => c.id === a.id);
    if (card) {
      card.reps += 1;
      card.due = a.grade === "again" ? Date.now() / 1000 + 600 : Date.now() / 1000 + 86_400;
    }
    return { due: card?.due ?? 0, stability: 1, difficulty: 5, elapsedDays: 0, scheduledDays: 1, reps: card?.reps ?? 0, lapses: 0, state: 2, lastReview: Date.now() / 1000 };
  });
  register("mistakes_remove", (a) => {
    cards = cards.filter((c) => c.id !== a.id);
    return null;
  });
}
