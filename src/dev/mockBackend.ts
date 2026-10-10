// Browser-only stand-in for the Rust backend, used by Playwright end-to-end
// tests and UI previews (`VITE_MOCK_TAURI=1 pnpm dev`). macOS WKWebView has
// no WebDriver, so the real app can't be driven by Playwright; this mock lets
// the UI flows run in Chromium. Chess rules here come from chessops; the real
// app uses the Rust implementation.

import { mockIPC, mockWindows } from "@tauri-apps/api/mocks";
import { emit } from "@tauri-apps/api/event";
import { Chess, normalizeMove } from "chessops/chess";
import { chessgroundDests } from "chessops/compat";
import { makeFen, parseFen } from "chessops/fen";
import { makeSan } from "chessops/san";
import { makeUci, parseUci, squareFile } from "chessops/util";
import levelsJson from "../../src-tauri/resources/levels.json";
import networksJson from "../../src-tauri/resources/networks.json";
import type {
  AnalysisRequest,
  AppliedMove,
  EngineConfig,
  MoveReview,
  PositionEval,
  ReviewResult,
  ClockState,
  GameOutcome,
  GameTree,
  OpponentLevel,
  PlaySetup,
  PlaySnapshot,
  PositionInfo,
  Settings,
  Side,
} from "../bindings";

const START = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

function load(fen: string): Chess {
  return Chess.fromSetup(parseFen(fen).unwrap()).unwrap();
}

function outcomeOf(pos: Chess): GameOutcome | null {
  const o = pos.outcome();
  if (!o) return null;
  if (!o.winner) {
    const reason = pos.isStalemate() ? "stalemate" : "insufficientMaterial";
    return { result: "1/2-1/2", winner: null, reason };
  }
  return { result: o.winner === "white" ? "1-0" : "0-1", winner: o.winner, reason: "checkmate" };
}

function info(pos: Chess): PositionInfo {
  const dests: Record<string, string[]> = {};
  for (const [from, tos] of chessgroundDests(pos)) dests[from] = tos;
  return {
    fen: makeFen(pos.toSetup()),
    turn: pos.turn,
    dests,
    check: pos.isCheck(),
    fullmoves: pos.fullmoves,
    halfmoves: pos.halfmoves,
    outcome: outcomeOf(pos),
  };
}

function apply(fen: string, uci: string): AppliedMove {
  const pos = load(fen);
  const parsed = parseUci(uci);
  if (!parsed) throw `bad move: ${uci}`;
  const move = normalizeMove(pos, parsed);
  if (!pos.isLegal(move)) throw `illegal move: ${uci}`;
  const san = makeSan(pos, move);
  let normalized = makeUci(move);
  // chessops writes castling as king-takes-rook; use e1g1 style like Rust.
  if ("from" in move && pos.board.get(move.from)?.role === "king" && Math.abs(squareFile(move.from) - squareFile(move.to)) > 1) {
    const rank = normalized[1];
    normalized = `${normalized.slice(0, 2)}${squareFile(move.to) > squareFile(move.from) ? "g" : "c"}${rank}`;
  }
  pos.play(move);
  return { uci: normalized, san, position: info(pos) };
}

function firstMove(fen: string): string {
  const pos = load(fen);
  const moves: string[] = [];
  for (const [from, tos] of chessgroundDests(pos)) {
    for (const to of tos) moves.push(`${from}${to}`);
  }
  return moves.sort()[0];
}

// ---- static data ----

const levels: OpponentLevel[] = [
  ...levelsJson.maia.map((l) => ({ id: l.id, name: l.name, kind: "maia" as const, network: l.network, nodes: 1, temperature: 1, tempDecayMoves: 10, elo: l.elo })),
  ...levelsJson.leela.map((l) => ({
    id: l.id,
    name: l.name,
    kind: "leela" as const,
    network: l.network,
    nodes: l.nodes,
    temperature: l.temperature,
    tempDecayMoves: l.tempDecayMoves,
    elo: l.elo,
  })),
  { id: "full", name: "Leela full strength", kind: "full", network: null, nodes: null, temperature: 0, tempDecayMoves: 0, elo: null },
];

const installed = new Set(["791556", "703810", "maia-1100", "maia-1300", "maia-1500", "maia-1700", "maia-1900"]);
let settings: Settings = {
  playerName: "Tester",
  defaultNetwork: "791556",
  backend: "metal",
  backendOpts: "gpu=1",
  threads: null,
  humanThinkTime: false,
  opponentVariety: true,
  colorScheme: "dark",
  boardTheme: "brown",
  pieceSet: "cburnett",
  showCoordinates: true,
  animationMs: 0,
  showEvalBar: true,
  lichessUsername: null,
  chesscomUsername: null,
  onboardingDone: true,
};

// ---- play sessions ----

interface MockSession {
  snap: PlaySnapshot;
  clock: { white: number; black: number; running: Side | null; since: number } | null;
}

const sessions = new Map<number, MockSession>();
let nextSession = 1;

function clockState(s: MockSession): ClockState | null {
  if (!s.clock) return null;
  const now = Date.now();
  const spent = (side: Side) => (s.clock!.running === side ? now - s.clock!.since : 0);
  return { whiteMs: Math.max(0, s.clock.white - spent("white")), blackMs: Math.max(0, s.clock.black - spent("black")), running: s.clock.running };
}

function refresh(s: MockSession) {
  const pos = load(s.snap.position.fen);
  const pi = info(pos);
  const userTurn = !s.snap.outcome && pi.turn === s.snap.userSide && !s.snap.engineThinking;
  s.snap = {
    ...s.snap,
    position: { ...pi, dests: userTurn ? pi.dests : {} },
    clock: clockState(s),
    canTakeback: s.snap.setup.allowTakebacks && !s.snap.outcome && s.snap.moves.length >= (s.snap.userSide === "white" ? 1 : 2),
    canOfferDraw: !s.snap.outcome,
  };
  if (!s.snap.outcome && pi.outcome) finish(s, pi.outcome);
}

function finish(s: MockSession, outcome: GameOutcome) {
  if (s.clock) {
    const c = clockState(s)!;
    s.clock = { white: c.whiteMs ?? 0, black: c.blackMs ?? 0, running: null, since: Date.now() };
  }
  s.snap = { ...s.snap, outcome, engineThinking: false, pgnPath: "/mock/pgn/game.pgn", clock: clockState(s) };
  s.snap.position = { ...s.snap.position, dests: {} };
}

function pushMove(s: MockSession, uci: string) {
  const applied = apply(s.snap.position.fen, uci);
  const mover: Side = s.snap.position.turn;
  if (s.clock) {
    const now = Date.now();
    if (s.clock.running) {
      s.clock[s.clock.running] -= now - s.clock.since;
      s.clock[mover] += s.snap.setup.timeControl?.incrementMs ?? 0;
    }
    s.clock.running = mover === "white" ? "black" : "white";
    s.clock.since = now;
  }
  s.snap = {
    ...s.snap,
    moves: [...s.snap.moves, { uci: applied.uci, san: applied.san, fen: applied.position.fen, clockMs: null, thinkMs: 0 }],
    position: applied.position,
    lastMove: applied.uci,
    hint: null,
  };
  refresh(s);
}

function engineTurn(s: MockSession) {
  if (s.snap.outcome || s.snap.position.turn === s.snap.userSide) return;
  s.snap = { ...s.snap, engineThinking: true };
  refresh(s);
  void emit("play-update", s.snap);
  setTimeout(() => {
    if (s.snap.outcome) return;
    s.snap = { ...s.snap, engineThinking: false };
    pushMove(s, firstMove(s.snap.position.fen));
    void emit("play-update", s.snap);
  }, 150);
}

function startGame(setup: PlaySetup): PlaySnapshot {
  const level = levels.find((l) => l.id === setup.opponent)!;
  const userSide: Side = setup.userSide === "random" ? "white" : setup.userSide;
  const fen = setup.startFen ?? START;
  const name = level.kind === "full" ? "Leela (791556)" : level.name;
  const s: MockSession = {
    snap: {
      sessionId: nextSession++,
      setup,
      level,
      network: level.network ?? "791556",
      userSide,
      whiteName: userSide === "white" ? settings.playerName! : name,
      blackName: userSide === "white" ? name : settings.playerName!,
      startFen: fen,
      moves: [],
      position: info(load(fen)),
      lastMove: null,
      clock: null,
      outcome: null,
      engineThinking: false,
      hint: null,
      pgnPath: null,
      canTakeback: false,
      canOfferDraw: true,
      error: null,
    },
    clock: setup.timeControl ? { white: setup.timeControl.initialMs, black: setup.timeControl.initialMs, running: null, since: Date.now() } : null,
  };
  sessions.set(s.snap.sessionId, s);
  refresh(s);
  engineTurn(s);
  return s.snap;
}

function session(id: number): MockSession {
  const s = sessions.get(id);
  if (!s) throw `no game with id ${id}`;
  return s;
}

function gamePgn(s: MockSession): string {
  const moves = s.snap.moves
    .map((m, i) => (i % 2 === 0 ? `${i / 2 + 1}. ${m.san}` : m.san))
    .join(" ");
  const result = s.snap.outcome?.result ?? "*";
  return `[Event "Grandie Chess training game"]\n[White "${s.snap.whiteName}"]\n[Black "${s.snap.blackName}"]\n[Result "${result}"]\n\n${moves} ${result}\n`;
}

/** Very small PGN reader for the mock: mainline SAN moves only. */
function parsePgn(text: string): { games: GameTree[]; errors: string[] } {
  const headers = [...text.matchAll(/\[(\w+) "([^"]*)"\]/g)].map((m) => ({ name: m[1], value: m[2] }));
  const body = text.replace(/\[[^\]]*\]/g, "").replace(/\{[^}]*\}/g, "");
  const tokens = body.split(/\s+/).filter((t) => t && !/^\d+\.+$/.test(t) && !/^(1-0|0-1|1\/2-1\/2|\*)$/.test(t));
  const tree: GameTree = {
    headers,
    nodes: [{ id: 0, parent: null, children: [], ply: 0, fen: START, uci: null, san: null, comment: null, nags: [], clockMs: null, eval: null }],
  };
  let parent = 0;
  for (const raw of tokens) {
    const san = raw.replace(/^\d+\.+/, "");
    const pos = load(tree.nodes[parent].fen);
    let found: AppliedMove | null = null;
    for (const [from, tos] of chessgroundDests(pos)) {
      for (const to of tos) {
        const a = apply(tree.nodes[parent].fen, `${from}${to}`);
        if (a.san === san || a.san.replace(/[+#]/, "") === san.replace(/[+#]/, "")) found = a;
      }
    }
    if (!found) return { games: [tree], errors: [`illegal move ${san}`] };
    const id = tree.nodes.length;
    tree.nodes.push({ id, parent, children: [], ply: tree.nodes[parent].ply + 1, fen: found.position.fen, uci: found.uci, san: found.san, comment: null, nags: [], clockMs: null, eval: null });
    tree.nodes[parent].children.push(id);
    parent = id;
  }
  return { games: [tree], errors: [] };
}

type Handler = (args: Record<string, unknown>) => unknown;

const handlers: Record<string, Handler> = {
  app_info: () => ({ version: "0.1.0", arch: "x86_64", lc0Path: "/mock/lc0", dataDir: "/mock", networksDir: "/mock/networks", pgnDir: "/mock/pgn" }),
  chess_position_info: (a) => info(load(a.fen as string)),
  chess_apply_move: (a) => apply(a.fen as string, a.uci as string),
  settings_get: () => settings,
  settings_set: (a) => {
    settings = a.settings as Settings;
    return null;
  },
  networks_list: () =>
    networksJson.networks.map((n) => ({ ...n, kind: n.kind as "leela" | "maia", installed: installed.has(n.id), path: `/mock/networks/${n.id}.pb.gz` })),
  network_download: async (a) => {
    const id = a.id as string;
    const size = networksJson.networks.find((n) => n.id === id)?.size ?? 1;
    for (let i = 1; i <= 4; i++) {
      await new Promise((r) => setTimeout(r, 40));
      await emit("download-progress", { id, received: (size * i) / 4, total: size, done: i === 4, error: null });
    }
    installed.add(id);
    return null;
  },
  network_delete: (a) => {
    installed.delete(a.id as string);
    return null;
  },
  engine_detect_backend: () => [
    { backend: "metal", backendOpts: "gpu=0", device: "Intel(R) UHD Graphics 630", nps: 13, error: null },
    { backend: "metal", backendOpts: "gpu=1", device: "AMD Radeon Pro 560X", nps: 505, error: null },
    { backend: "blas", backendOpts: null, device: null, nps: 106, error: null },
  ],
  levels_list: () => levels,
  play_missing_networks: (a) => {
    const level = levels.find((l) => l.id === a.opponent);
    return level?.network && !installed.has(level.network) ? [level.network] : [];
  },
  play_start: (a) => startGame(a.setup as PlaySetup),
  play_snapshot: (a) => session(a.sessionId as number).snap,
  play_move: (a) => {
    const s = session(a.sessionId as number);
    if (s.snap.outcome) throw "the game is over";
    if (s.snap.position.turn !== s.snap.userSide || s.snap.engineThinking) throw "it is not your turn";
    pushMove(s, a.uci as string);
    void emit("play-update", s.snap);
    engineTurn(s);
    return s.snap;
  },
  play_takeback: (a) => {
    const s = session(a.sessionId as number);
    const moves = s.snap.moves.slice(0, s.snap.moves.length - (s.snap.position.turn === s.snap.userSide ? 2 : 1));
    s.snap = { ...s.snap, moves, position: info(load(moves.length ? moves[moves.length - 1].fen : s.snap.startFen)), lastMove: moves[moves.length - 1]?.uci ?? null };
    refresh(s);
    return s.snap;
  },
  play_resign: (a) => {
    const s = session(a.sessionId as number);
    const winner: Side = s.snap.userSide === "white" ? "black" : "white";
    finish(s, { result: winner === "white" ? "1-0" : "0-1", winner, reason: "resignation" });
    void emit("play-update", s.snap);
    return s.snap;
  },
  play_offer_draw: () => false,
  play_hint: (a) => {
    const s = session(a.sessionId as number);
    const best = firstMove(s.snap.position.fen);
    s.snap = { ...s.snap, hint: best };
    void emit("play-update", s.snap);
    return best;
  },
  play_pgn: (a) => gamePgn(session(a.sessionId as number)),
  play_close: (a) => {
    sessions.delete(a.sessionId as number);
    return null;
  },
  pgn_parse: (a) => parsePgn(a.text as string),
  pgn_read_file: () => parsePgn('[White "A"]\n[Black "B"]\n\n1. e4 e5 2. Nf3 Nc6 *'),
  pgn_write: () => "[Event \"?\"]\n\n*\n",
  file_write_text: () => null,
};

// ---- engines, analysis and review (phase 2) ----

const engines: EngineConfig[] = [{ id: "lc0", name: "Leela Chess Zero", kind: "lc0", path: null, options: [], availableOptions: [] }];
const analysisTimers = new Map<string, ReturnType<typeof setInterval>>();

function fakeLines(fen: string, multipv: number) {
  const pos = load(fen);
  const moves: string[] = [];
  for (const [from, tos] of chessgroundDests(pos)) for (const to of tos) moves.push(`${from}${to}`);
  moves.sort();
  return moves.slice(0, multipv).map((uci, i) => {
    const applied = apply(fen, uci);
    const whiteWin = 55 - i * 4;
    return {
      multipv: i + 1,
      depth: 12,
      seldepth: 20,
      nodes: 12000,
      nps: 400,
      score: { kind: "cp" as const, value: 25 - i * 20 },
      wdl: { w: 300, d: 500, l: 200 },
      whiteWin,
      pv: [uci],
      san: [applied.san],
    };
  });
}

/** The mock engine's preferred move: first legal move for White, last for Black. */
function mockBest(fen: string): string {
  const pos = load(fen);
  const moves: string[] = [];
  for (const [from, tos] of chessgroundDests(pos)) for (const to of tos) moves.push(`${from}${to}`);
  moves.sort();
  return pos.turn === "white" ? moves[0] : moves[moves.length - 1];
}

function mockReview(startFen: string, moves: string[]): ReviewResult {
  const evals: PositionEval[] = [];
  let fen = startFen;
  const fens = [fen];
  for (const uci of moves) {
    fen = apply(fen, uci).position.fen;
    fens.push(fen);
  }
  fens.forEach((f, ply) => {
    // Ply 4 (after Black's second move) is a scripted blunder by Black.
    const whiteWin = ply >= 4 ? 80 : 50 + (ply % 2);
    const pos = load(f);
    const best = pos.isEnd() ? null : mockBest(f);
    evals.push({ ply, fen: f, whiteWin, score: { kind: "cp", value: Math.round((whiteWin - 50) * 10) }, best, bestSan: best ? apply(f, best).san : null, line: best ? [apply(f, best).san] : [], secondWhiteWin: null });
  });
  const reviews: MoveReview[] = moves.map((uci, i) => {
    const side = load(fens[i]).turn;
    const pov = (w: number) => (side === "white" ? w : 100 - w);
    const before = pov(evals[i].whiteWin ?? 50);
    const after = pov(evals[i + 1].whiteWin ?? 50);
    const drop = before - after;
    const cls = drop >= 15 ? "blunder" : drop >= 10 ? "mistake" : drop >= 5 ? "inaccuracy" : evals[i].best === uci ? "best" : "good";
    return {
      ply: i + 1,
      side,
      uci,
      san: apply(fens[i], uci).san,
      class: cls,
      winBefore: before,
      winAfter: after,
      accuracy: Math.min(100, Math.max(0, 100 - drop * 3)),
      best: evals[i].best,
      bestSan: evals[i].bestSan,
      bestLine: evals[i].line,
    };
  });
  const summary = (side: Side) => {
    const mine = reviews.filter((r) => r.side === side);
    const count = (c: string) => mine.filter((r) => r.class === c).length;
    return {
      accuracy: mine.length ? mine.reduce((a, r) => a + (r.accuracy ?? 0), 0) / mine.length : null,
      brilliant: 0,
      great: 0,
      best: count("best"),
      good: count("good"),
      inaccuracies: count("inaccuracy"),
      mistakes: count("mistake"),
      blunders: count("blunder"),
    };
  };
  return {
    startFen,
    engine: "Mock Leela",
    depth: "quick",
    evals,
    moves: reviews,
    white: summary("white"),
    black: summary("black"),
    keyMoments: reviews.filter((r) => r.class === "blunder" || r.class === "mistake").map((r) => r.ply),
  };
}

Object.assign(handlers, {
  chess_apply_san_line: (a: Record<string, unknown>) => {
    let fen = a.fen as string;
    const out: AppliedMove[] = [];
    for (const san of a.sans as string[]) {
      const pos = load(fen);
      let found: AppliedMove | null = null;
      for (const [from, tos] of chessgroundDests(pos)) for (const to of tos) {
        const m = apply(fen, `${from}${to}`);
        if (m.san === san) found = m;
      }
      if (!found) break;
      out.push(found);
      fen = found.position.fen;
    }
    return out;
  },
  engines_list: () => engines,
  engine_add: () => {
    throw "mock: no file system";
  },
  engine_update: () => null,
  engine_remove: () => null,
  engine_install_stockfish: () => {
    const sf: EngineConfig = { id: "stockfish", name: "Stockfish 19", kind: "uci", path: "/mock/stockfish", options: [{ name: "Threads", value: "4" }], availableOptions: [{ name: "Threads", kind: { type: "spin", default: 1, min: 1, max: 64 } }] };
    engines.push(sf);
    return sf;
  },
  analysis_start: (a: Record<string, unknown>) => {
    const req = a.request as AnalysisRequest;
    const key = req.engineId;
    clearInterval(analysisTimers.get(key));
    let fen = req.rootFen;
    for (const m of req.moves) fen = apply(fen, m).position.fen;
    const send = () => void emit("analysis-event", { tabId: req.tabId, engineId: req.engineId, fen, lines: fakeLines(fen, req.multipv), done: false });
    send();
    analysisTimers.set(key, setInterval(send, 300));
    return null;
  },
  analysis_stop: (a: Record<string, unknown>) => {
    clearInterval(analysisTimers.get(a.engineId as string));
    return null;
  },
  analysis_backend: () => "metal (Mock GPU)",
  review_start: async (a: Record<string, unknown>) => {
    const moves = a.moves as string[];
    for (let i = 0; i <= moves.length; i++) {
      await emit("review-progress", { reviewId: a.reviewId, done: i + 1, total: moves.length + 1 });
      await new Promise((r) => setTimeout(r, 15));
    }
    return mockReview(a.startFen as string, moves);
  },
  review_cancel: () => null,
  review_check_move: (a: Record<string, unknown>) => {
    const applied = apply(a.fen as string, a.uci as string);
    const good = (a.uci as string) === mockBest(a.fen as string);
    return { winAfter: good ? 60 : 20, san: applied.san, line: [] };
  },
});

// ---- database (phase 3) ----

interface MockGame {
  row: import("../bindings").GameRow;
  pgn: string;
  fens: string[];
  review: ReviewResult | null;
}

const SAMPLE_GAMES: { white: string; black: string; result: string; eco: string; opening: string; date: string; moves: string; user: Side | null; source: string }[] = [
  { white: "Tester", black: "Rival", result: "1-0", eco: "C50", opening: "Italian Game", date: "2026.09.01", moves: "e4 e5 Nf3 Nc6 Bc4 Bc5 c3 Nf6", user: "white", source: "lichess" },
  { white: "Rival", black: "Tester", result: "0-1", eco: "B20", opening: "Sicilian Defense", date: "2026.09.12", moves: "e4 c5 Nf3 d6 d4 cxd4", user: "black", source: "lichess" },
  { white: "Tester", black: "Someone", result: "1/2-1/2", eco: "D30", opening: "Queen's Gambit Declined", date: "2026.09.20", moves: "d4 d5 c4 e6 Nf3 Nf6", user: "white", source: "chesscom" },
  { white: "Carlsen", black: "Nepo", result: "1-0", eco: "C65", opening: "Ruy Lopez", date: "2026.08.02", moves: "e4 e5 Nf3 Nc6 Bb5 Nf6", user: null, source: "pgn" },
];
const dbGames: MockGame[] = [];
let nextGameId = 1;

function addSampleGames(source?: string) {
  let imported = 0;
  for (const g of SAMPLE_GAMES) {
    if (source && g.source !== source) continue;
    if (dbGames.some((d) => d.row.white === g.white && d.row.date === g.date)) continue;
    const pgn = `[White "${g.white}"]\n[Black "${g.black}"]\n[Result "${g.result}"]\n[Date "${g.date}"]\n[ECO "${g.eco}"]\n[Opening "${g.opening}"]\n\n${g.moves} ${g.result}\n`;
    const tree = parsePgn(pgn).games[0];
    dbGames.push({
      row: {
        id: nextGameId++,
        source: g.source,
        white: g.white,
        black: g.black,
        whiteElo: 1600,
        blackElo: 1580,
        result: g.result,
        date: g.date,
        eco: g.eco,
        opening: g.opening,
        timeControl: "300+3",
        speed: "blitz",
        userColor: g.user,
        opponentLevel: null,
        plyCount: g.moves.split(" ").length,
        reviewed: false,
        userAccuracy: null,
      },
      pgn,
      fens: tree.nodes.map((n) => n.fen),
      review: null,
    });
    imported++;
  }
  return imported;
}

Object.assign(handlers, {
  db_stats: () => ({
    games: dbGames.length,
    mine: dbGames.filter((g) => g.row.userColor).length,
    reviewed: dbGames.filter((g) => g.review).length,
    sources: [],
  }),
  db_query: (a: Record<string, unknown>) => {
    const q = a.query as import("../bindings").GameQuery;
    const text = q.text?.toLowerCase();
    const sameBoard = (f: string, g: string) => f.split(" ").slice(0, 2).join(" ") === g.split(" ").slice(0, 2).join(" ");
    let rows = dbGames.filter((g) => {
      const r = g.row;
      if (text && ![r.white, r.black, r.opening, r.eco].some((v) => v?.toLowerCase().includes(text))) return false;
      if ((q.onlyMine || q.color || q.outcome) && !r.userColor) return false;
      if (q.color && r.userColor !== q.color) return false;
      if (q.outcome) {
        const won = (r.userColor === "white" && r.result === "1-0") || (r.userColor === "black" && r.result === "0-1");
        const outcome = r.result === "1/2-1/2" ? "draw" : won ? "win" : "loss";
        if (outcome !== q.outcome) return false;
      }
      if (q.source && r.source !== q.source) return false;
      if (q.position?.exact && !g.fens.some((f) => sameBoard(f, q.position!.fen))) return false;
      return true;
    }).map((g) => g.row);
    rows = rows.sort((x, y) => (y.date ?? "").localeCompare(x.date ?? ""));
    return { total: rows.length, rows: rows.slice(q.offset ?? 0, (q.offset ?? 0) + (q.limit || 200)), elapsedMs: 3 };
  },
  db_game: (a: Record<string, unknown>) => {
    const g = dbGames.find((d) => d.row.id === a.id);
    if (!g) throw "game not found";
    return { row: g.row, tree: parsePgn(g.pgn).games[0], review: g.review };
  },
  db_delete: (a: Record<string, unknown>) => {
    const ids = a.ids as number[];
    const before = dbGames.length;
    for (const id of ids) {
      const i = dbGames.findIndex((d) => d.row.id === id);
      if (i >= 0) dbGames.splice(i, 1);
    }
    return before - dbGames.length;
  },
  db_find: () => null,
  db_save_review: (a: Record<string, unknown>) => {
    const g = dbGames.find((d) => d.row.id === a.gameId);
    if (g) {
      g.review = a.review as ReviewResult;
      g.row = { ...g.row, reviewed: true };
    }
    return null;
  },
  db_import_grandie: () => ({ seen: 0, imported: 0, duplicates: 0, errors: [], newest: null }),
  db_import_files: async (a: Record<string, unknown>) => {
    await emit("import-progress", { jobId: a.jobId, phase: "Importing", seen: 1, imported: 1, bytes: 0 });
    const n = addSampleGames("pgn");
    return { seen: n, imported: n, duplicates: 0, errors: [], newest: null };
  },
  db_import_lichess: async (a: Record<string, unknown>) => {
    await emit("import-progress", { jobId: a.jobId, phase: "Downloading from Lichess", seen: 2, imported: 0, bytes: 2048 });
    await new Promise((r) => setTimeout(r, 100));
    const n = addSampleGames("lichess");
    return { seen: 2, imported: n, duplicates: 2 - n, errors: [], newest: null };
  },
  db_import_chesscom: async () => {
    const n = addSampleGames("chesscom");
    return { seen: 1, imported: n, duplicates: 1 - n, errors: [], newest: null };
  },
});

// ---- openings and repertoire (phase 4) ----

interface MockRepMove { id: number; color: Side; parentFen: string; uci: string; san: string; childFen: string; comment: string | null }
const repMoves: MockRepMove[] = [];
const repGrades = new Map<string, number>(); // key -> due timestamp (ms)
let nextRepId = 1;
const spot = (f: string) => f.split(" ").slice(0, 4).join(" ");

function repCards(color: Side) {
  const turn = color === "white" ? " w " : " b ";
  const seen = new Map<string, string>();
  for (const m of repMoves) if (m.color === color && m.parentFen.includes(turn)) seen.set(spot(m.parentFen), m.parentFen);
  return [...seen.entries()].map(([key, fen]) => ({ key, fen }));
}

function lineTo(color: Side, target: string): string[] | null {
  const queue: { fen: string; line: string[] }[] = [{ fen: START, line: [] }];
  const seen = new Set<string>();
  while (queue.length) {
    const { fen, line } = queue.shift()!;
    if (spot(fen) === target) return line;
    if (seen.has(spot(fen))) continue;
    seen.add(spot(fen));
    for (const m of repMoves.filter((r) => r.color === color && spot(r.parentFen) === spot(fen))) queue.push({ fen: m.childFen, line: [...line, m.uci] });
  }
  return null;
}

Object.assign(handlers, {
  opening_name: (a: Record<string, unknown>) => {
    const moves = a.moves as string[];
    if (moves[0] === "e2e4" && moves[1] === "c7c5") return { eco: "B20", name: "Sicilian Defense" };
    if (moves[0] === "e2e4") return { eco: "B00", name: "King's Pawn Game" };
    return null;
  },
  explorer_db: (a: Record<string, unknown>) => {
    const fen = a.fen as string;
    const counts = new Map<string, { san: string; games: number; white: number; draws: number; black: number }>();
    for (const g of dbGames) {
      const tree = parsePgn(g.pgn).games[0];
      for (const n of tree.nodes) {
        if (spot(n.fen) !== spot(fen) || !n.children.length) continue;
        const next = tree.nodes[n.children[0]];
        const c = counts.get(next.uci!) ?? { san: next.san!, games: 0, white: 0, draws: 0, black: 0 };
        c.games++;
        if (g.row.result === "1-0") c.white++;
        else if (g.row.result === "0-1") c.black++;
        else c.draws++;
        counts.set(next.uci!, c);
      }
    }
    const moves = [...counts.entries()].map(([uci, c]) => ({ uci, ...c, avgRating: 1600 })).sort((x, y) => y.games - x.games);
    const total = moves.reduce((t, m) => t + m.games, 0);
    return { source: "database", fen, total, white: 0, draws: 0, black: 0, moves, opening: null };
  },
  explorer_lichess: () => {
    throw "Add a Lichess API token in Settings to use the Lichess explorer.";
  },
  lichess_token_status: () => false,
  lichess_token_set: () => null,
  file_read_text: () => "1. e4 e5 2. Nf3 *",
  repertoire_moves: (a: Record<string, unknown>) => repMoves.filter((m) => m.color === a.color).map(({ color: _c, ...m }) => m),
  repertoire_add_line: (a: Record<string, unknown>) => {
    let fen = START;
    let added = 0;
    for (const uci of a.moves as string[]) {
      const applied = apply(fen, uci);
      if (!repMoves.some((m) => m.color === a.color && spot(m.parentFen) === spot(fen) && m.uci === applied.uci)) {
        repMoves.push({ id: nextRepId++, color: a.color as Side, parentFen: fen, uci: applied.uci, san: applied.san, childFen: applied.position.fen, comment: null });
        added++;
      }
      fen = applied.position.fen;
    }
    return added;
  },
  repertoire_remove_move: (a: Record<string, unknown>) => {
    const i = repMoves.findIndex((m) => m.id === a.id);
    if (i >= 0) repMoves.splice(i, 1);
    return null;
  },
  repertoire_set_comment: () => null,
  repertoire_import_pgn: () => 0,
  repertoire_export_pgn: () => "*",
  repertoire_stats: (a: Record<string, unknown>) => {
    const cards = repCards(a.color as Side);
    const now = Date.now();
    return {
      moves: repMoves.filter((m) => m.color === a.color).length,
      cards: cards.length,
      dueNow: cards.filter((c) => (repGrades.get(c.key) ?? Infinity) <= now).length,
      newCards: cards.filter((c) => !repGrades.has(c.key)).length,
      nextDue: null,
    };
  },
  repertoire_due: (a: Record<string, unknown>) => {
    const colors: Side[] = a.color ? [a.color as Side] : ["white", "black"];
    const now = Date.now();
    const items = [];
    for (const color of colors) {
      for (const c of repCards(color)) {
        const due = repGrades.get(c.key);
        if (due !== undefined && due > now) continue;
        const line = lineTo(color, c.key);
        if (!line) continue;
        const expected = repMoves.filter((m) => m.color === color && spot(m.parentFen) === c.key).map((m) => ({ uci: m.uci, san: m.san }));
        items.push({ color, key: c.key, fen: c.fen, line, expected, isNew: due === undefined });
      }
    }
    return items.sort((x, y) => x.line.length - y.line.length);
  },
  repertoire_grade: (a: Record<string, unknown>) => {
    const later = a.grade === "again" ? 60_000 : 86_400_000;
    repGrades.set(a.key as string, Date.now() + later);
    return { due: (Date.now() + later) / 1000, stability: 1, difficulty: 5, elapsedDays: 0, scheduledDays: 1, reps: 1, lapses: 0, state: 2, lastReview: Date.now() / 1000 };
  },
});

// ---- puzzles and progress (phase 5) ----

let puzzleCount = 0;
let puzzleRating = { rating: 1500, deviation: 350, volatility: 0.06 };
const MOCK_PUZZLE = {
  id: "0000D",
  fen: "5rk1/1p3ppp/pq3b2/8/8/1P1Q1N2/P4PPP/3R2K1 w - - 2 27",
  moves: ["d3d6", "f8d8", "d6d8", "f6d8"],
  rating: 1529,
  deviation: 74,
  popularity: 96,
  plays: 37506,
  themes: ["advantage", "endgame", "short"],
  gameUrl: "https://lichess.org/F8M8OS71#53",
  opening: null,
};
const puzzleAttempts: { solved: boolean; rating: number }[] = [];

Object.assign(handlers, {
  puzzles_info: () => ({ count: puzzleCount, rating: puzzleRating }),
  puzzles_download: async () => {
    for (let i = 1; i <= 3; i++) {
      await emit("download-progress", { id: "puzzles", received: i * 1e8, total: 3e8, done: false, error: null });
      await new Promise((r) => setTimeout(r, 40));
    }
    await emit("import-progress", { jobId: "puzzles", phase: "Importing puzzles", seen: 20000, imported: 9000, bytes: 0 });
    puzzleCount = 1_500_000;
    return puzzleCount;
  },
  puzzles_themes: () => [
    { theme: "endgame", count: 400000 },
    { theme: "mateIn2", count: 120000 },
  ],
  puzzle_next: () => MOCK_PUZZLE,
  puzzle_check: (a: Record<string, unknown>) => {
    const index = a.index as number;
    const correct = a.uci === MOCK_PUZZLE.moves[index];
    const last = index === MOCK_PUZZLE.moves.length - 1;
    return { correct, reply: correct && !last ? MOCK_PUZZLE.moves[index + 1] : null, done: correct && last };
  },
  puzzle_finish: (a: Record<string, unknown>) => {
    const before = { ...puzzleRating };
    puzzleRating = { ...puzzleRating, rating: puzzleRating.rating + (a.solved ? 12 : -12), deviation: 300 };
    puzzleAttempts.push({ solved: a.solved as boolean, rating: puzzleRating.rating });
    return { before, after: puzzleRating };
  },
  progress_summary: () => ({
    totals: {
      games: 3,
      wins: 2,
      draws: 0,
      losses: 1,
      gamesLast30Days: 3,
      puzzlesAttempted: puzzleAttempts.length,
      puzzlesSolved: puzzleAttempts.filter((p) => p.solved).length,
    },
    levels: [{ level: "maia-1500", name: "Maia 1500", elo: 1500, games: 3, wins: 2, draws: 0, losses: 1, performance: 1633 }],
    strength: [
      { at: 1_790_000_000, rating: 1450 },
      { at: 1_791_000_000, rating: 1560 },
      { at: 1_792_000_000, rating: 1633 },
    ],
    quality: [
      { gameId: 1, at: 1_790_000_000, opponent: "Maia 1500", accuracy: 78, inaccuracies: 3, mistakes: 2, blunders: 1 },
      { gameId: 2, at: 1_791_000_000, opponent: "Maia 1500", accuracy: 85, inaccuracies: 2, mistakes: 1, blunders: 0 },
    ],
    weakestOpenings: [{ family: "Sicilian Defense", eco: "B20", games: 3, score: 0.33, accuracy: 74 }],
    puzzleRating,
    puzzleHistory: puzzleAttempts.map((p, i) => ({ at: 1_790_000_000 + i, rating: p.rating })),
  }),
});

/** Register additional mock commands (used by later features). */
export function registerMock(name: string, handler: Handler) {
  handlers[name] = handler;
}

export function installMockBackend() {
  mockWindows("main");
  mockIPC(
    async (cmd, payload) => {
      if (cmd.startsWith("plugin:")) return null;
      const handler = handlers[cmd];
      if (!handler) throw `mock backend has no command ${cmd}`;
      return handler((payload ?? {}) as Record<string, unknown>);
    },
    { shouldMockEvents: true },
  );
  setInterval(() => {
    for (const s of sessions.values()) {
      if (s.clock?.running && !s.snap.outcome) {
        void emit("play-clock", { sessionId: s.snap.sessionId, clock: clockState(s) });
      }
    }
  }, 100);
  (window as unknown as { __GRANDIE_MOCK__: boolean }).__GRANDIE_MOCK__ = true;
}
