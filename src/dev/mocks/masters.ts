// Mock masters library for browser previews and Playwright tests: a few
// players with short games, the legends, profiles, and coach scripts.

import { Chess } from "chessops/chess";
import { makeFen } from "chessops/fen";
import { parseSan } from "chessops/san";
import { makeUci } from "chessops/util";
import type { CoachScript, CoachStep, GameQuery, GameRow, GuessResult, PlayerStats, Side, StyleMetrics } from "../../bindings";
import type { MockHelpers, Register } from "./index";

interface MockMasterGame {
  row: GameRow;
  pgn: string;
  subject: string;
  /** The subject's colour in the game. */
  color: Side | null;
  sans: string[];
}

interface Spec {
  white: string;
  black: string;
  result: string;
  date: string;
  eco: string;
  opening: string;
  moves: string;
}

const PLAYERS: { id: string; name: string; gamesAvailable: number }[] = [
  { id: "Capablanca", name: "José Raúl Capablanca", gamesAvailable: 597 },
  { id: "Carlsen", name: "Magnus Carlsen", gamesAvailable: 6615 },
  { id: "Fischer", name: "Robert James Fischer", gamesAvailable: 827 },
  { id: "Tal", name: "Mikhail Tal", gamesAvailable: 2431 },
];

const RUY = "e4 e5 Nf3 Nc6 Bb5 a6 Ba4 Nf6 O-O Be7 Re1 b5 Bb3 d6 c3 O-O h3 Nb8 d4 Nbd7 Nbd2 Bb7 Bc2 Re8";
const KID = "c4 Nf6 Nf3 g6 g3 Bg7 Bg2 O-O d4 d6 Nc3 Nbd7 O-O e5 e4 c6 h3 Qb6 d5 cxd5 cxd5 Nc5 Ne1 Bd7";
const NIMZO = "d4 Nf6 c4 e6 Nc3 Bb4 e3 O-O Bd3 d5 Nf3 c5 O-O Nc6 a3 Bxc3 bxc3 dxc4 Bxc4 Qc7 Bd3 e5 Qc2 Re8";
const SICILIAN = "e4 c5 Nf3 d6 d4 cxd4 Nxd4 Nf6 Nc3 a6 Be3 e5 Nb3 Be6 f3 Be7 Qd2 O-O O-O-O Nbd7 g4 b5 g5 b4";

const GAMES: Record<string, Spec[]> = {
  Tal: [
    { white: "Tal, Mikhail", black: "Smyslov, Vassily", result: "1-0", date: "1959.09.20", eco: "C95", opening: "Ruy Lopez: Breyer", moves: RUY },
    { white: "Botvinnik, Mikhail", black: "Tal, Mikhail", result: "0-1", date: "1960.03.26", eco: "E69", opening: "King's Indian Defense", moves: KID },
    { white: "Tal, Mikhail", black: "Spassky, Boris", result: "1/2-1/2", date: "1965.05.12", eco: "E53", opening: "Nimzo-Indian Defense", moves: NIMZO },
  ],
  Carlsen: [
    { white: "Carlsen, Magnus", black: "Anand, Viswanathan", result: "1-0", date: "2013.11.15", eco: "E53", opening: "Nimzo-Indian Defense", moves: NIMZO },
    { white: "Caruana, Fabiano", black: "Carlsen, Magnus", result: "1/2-1/2", date: "2018.11.09", eco: "B90", opening: "Sicilian Defense: Najdorf", moves: SICILIAN },
    { white: "Carlsen, Magnus", black: "Karjakin, Sergey", result: "1-0", date: "2016.11.30", eco: "C95", opening: "Ruy Lopez: Breyer", moves: RUY },
  ],
  Fischer: [
    { white: "Fischer, Robert James", black: "Spassky, Boris", result: "1-0", date: "1972.07.23", eco: "B90", opening: "Sicilian Defense: Najdorf", moves: SICILIAN },
    { white: "Byrne, Donald", black: "Fischer, Robert James", result: "0-1", date: "1956.10.17", eco: "E69", opening: "King's Indian Defense", moves: KID },
    { white: "Fischer, Robert James", black: "Petrosian, Tigran", result: "1-0", date: "1971.10.07", eco: "C95", opening: "Ruy Lopez: Breyer", moves: RUY },
  ],
  Capablanca: [
    { white: "Capablanca, Jose Raul", black: "Marshall, Frank", result: "1-0", date: "1918.10.23", eco: "C95", opening: "Ruy Lopez", moves: RUY },
    { white: "Lasker, Emanuel", black: "Capablanca, Jose Raul", result: "0-1", date: "1921.03.15", eco: "E53", opening: "Nimzo-Indian Defense", moves: NIMZO },
  ],
};

const LEGENDS: (Spec & { title: string; why: string })[] = [
  {
    title: "The Opera Game",
    why: "Morphy's model of rapid development: every piece joins the attack and the game ends with a queen sacrifice.",
    white: "Morphy, Paul",
    black: "Duke Karl / Count Isouard",
    result: "1-0",
    date: "1858.??.??",
    eco: "C41",
    opening: "Philidor Defense",
    moves: "e4 e5 Nf3 d6 d4 Bg4 dxe5 Bxf3 Qxf3 dxe5 Bc4 Nf6 Qb3 Qe7 Nc3 c6 Bg5 b5 Nxb5 cxb5 Bxb5+ Nbd7 O-O-O Rd8 Rxd7 Rxd7 Rd1 Qe6 Bxd7+ Nxd7 Qb8+ Nxb8 Rd8#",
  },
  {
    title: "The Immortal Game",
    why: "Anderssen gives up both rooks, a bishop and his queen, then mates with his three remaining minor pieces.",
    white: "Anderssen, Adolf",
    black: "Kieseritzky, Lionel",
    result: "1-0",
    date: "1851.??.??",
    eco: "C33",
    opening: "King's Gambit Accepted",
    moves:
      "e4 e5 f4 exf4 Bc4 Qh4+ Kf1 b5 Bxb5 Nf6 Nf3 Qh6 d3 Nh5 Nh4 Qg5 Nf5 c6 g4 Nf6 Rg1 cxb5 h4 Qg6 h5 Qg5 Qf3 Ng8 Bxf4 Qf6 Nc3 Bc5 Nd5 Qxb2 Bd6 Bxg1 e5 Qxa1+ Ke2 Na6 Nxg7+ Kd8 Qf6+ Nxf6 Be7#",
  },
  {
    title: "Légal's mate",
    why: "The queen is offered on move five and the minor pieces mate the king in the middle of the board.",
    white: "Legall de Kermeur",
    black: "Saint Brie",
    result: "1-0",
    date: "1750.??.??",
    eco: "C41",
    opening: "Philidor Defense",
    moves: "e4 e5 Nf3 d6 Bc4 Bg4 Nc3 g6 Nxe5 Bxd1 Bxf7+ Ke7 Nd5#",
  },
];

/** Legal SAN moves of a line, cut at the first move chessops rejects. */
function legal(moves: string): string[] {
  const pos = Chess.default();
  const out: string[] = [];
  for (const san of moves.split(" ")) {
    const m = parseSan(pos, san);
    if (!m) break;
    out.push(san);
    pos.play(m);
  }
  return out;
}

let nextId = 50_000;
function makeGame(spec: Spec, subject: string, color: Side | null): MockMasterGame {
  const sans = legal(spec.moves);
  const numbered = sans.map((s, i) => (i % 2 === 0 ? `${i / 2 + 1}. ${s}` : s)).join(" ");
  const pgn = `[Event "Mock"]\n[Date "${spec.date}"]\n[White "${spec.white}"]\n[Black "${spec.black}"]\n[Result "${spec.result}"]\n[ECO "${spec.eco}"]\n[Opening "${spec.opening}"]\n\n${numbered} ${spec.result}\n`;
  return {
    row: {
      id: nextId++,
      source: subject === "legends" ? "legends" : "pgnmentor",
      white: spec.white,
      black: spec.black,
      whiteElo: null,
      blackElo: null,
      result: spec.result,
      date: spec.date,
      eco: spec.eco,
      opening: spec.opening,
      timeControl: null,
      speed: null,
      userColor: null,
      opponentLevel: null,
      plyCount: sans.length,
      reviewed: false,
      userAccuracy: null,
    },
    pgn,
    subject,
    color,
    sans,
  };
}

const library: MockMasterGame[] = LEGENDS.map((l) => makeGame(l, "legends", null));
const legendInfo = new Map(library.map((g, i) => [g.row.id, LEGENDS[i]]));
const styles = new Map<string, StyleMetrics>();
const reports = new Map<string, string>();

function surname(name: string) {
  return name.split(" ").pop()!.toLowerCase();
}

function addPlayerGames(id: string) {
  if (library.some((g) => g.subject === id)) return 0;
  const name = PLAYERS.find((p) => p.id === id)?.name ?? id;
  const specs = GAMES[id] ?? GAMES.Tal.map((s) => ({ ...s, white: s.white.replace("Tal, Mikhail", name), black: s.black.replace("Tal, Mikhail", name) }));
  for (const s of specs) {
    const color: Side | null = s.white.toLowerCase().includes(surname(name)) ? "white" : s.black.toLowerCase().includes(surname(name)) ? "black" : null;
    library.push(makeGame(s, id, color));
  }
  return specs.length;
}

const count = (id: string) => library.filter((g) => g.subject === id).length;

/** Master games for the main mock's db_query (when `subject` is set). */
export function masterQuery(q: GameQuery) {
  const text = q.text?.toLowerCase();
  let games = library.filter((g) => {
    if (g.subject !== q.subject) return false;
    if (q.subjectColor && g.color !== q.subjectColor) return false;
    if (q.subjectOutcome) {
      const won = (g.color === "white" && g.row.result === "1-0") || (g.color === "black" && g.row.result === "0-1");
      const outcome = g.row.result === "1/2-1/2" ? "draw" : won ? "win" : "loss";
      if (outcome !== q.subjectOutcome) return false;
    }
    if (text && ![g.row.white, g.row.black, g.row.opening, g.row.eco].some((v) => v?.toLowerCase().includes(text))) return false;
    return true;
  });
  games = games.sort((a, b) => (a.row.date ?? "").localeCompare(b.row.date ?? "") * (q.desc ? -1 : 1));
  const offset = q.offset ?? 0;
  return { total: games.length, rows: games.slice(offset, offset + (q.limit || 50)).map((g) => g.row), elapsedMs: 2 };
}

/** A master game by id, for the main mock's db_game. */
export function masterGameById(id: number): { row: GameRow; pgn: string } | undefined {
  return library.find((g) => g.row.id === id);
}

/** FENs along a game, from the start (index 0) to the end. */
function fensOf(sans: string[]): { fens: string[]; ucis: string[] } {
  const pos = Chess.default();
  const fens = [makeFen(pos.toSetup())];
  const ucis: string[] = [];
  for (const san of sans) {
    const m = parseSan(pos, san)!;
    ucis.push(makeUci(m));
    pos.play(m);
    fens.push(makeFen(pos.toSetup()));
  }
  return { fens, ucis };
}

function profile(id: string): PlayerStats {
  const games = library.filter((g) => g.subject === id);
  const name = PLAYERS.find((p) => p.id === id)?.name ?? id;
  const side = (c: Side) => {
    const list = games.filter((g) => g.color === c);
    const wins = list.filter((g) => g.row.result === (c === "white" ? "1-0" : "0-1")).length;
    const draws = list.filter((g) => g.row.result === "1/2-1/2").length;
    const losses = list.length - wins - draws;
    return { games: list.length, wins, draws, losses, score: list.length ? ((wins + draws / 2) / list.length) * 100 : 0 };
  };
  const openings = (c: Side) =>
    Object.values(
      games
        .filter((g) => g.color === c)
        .reduce<Record<string, { eco: string | null; name: string; games: number; score: number }>>((acc, g) => {
          const fam = (g.row.opening ?? "?").split(":")[0];
          acc[fam] ??= { eco: g.row.eco?.slice(0, 1) ?? null, name: fam, games: 0, score: 50 };
          acc[fam].games++;
          return acc;
        }, {}),
    );
  const years = games.map((g) => Number(g.row.date?.slice(0, 4))).filter((y) => y > 0);
  const decades = new Map<number, number>();
  for (const y of years) decades.set(Math.floor(y / 10) * 10, (decades.get(Math.floor(y / 10) * 10) ?? 0) + 1);
  return {
    id,
    name,
    games: games.length,
    firstYear: years.length ? Math.min(...years) : null,
    lastYear: years.length ? Math.max(...years) : null,
    asWhite: side("white"),
    asBlack: side("black"),
    openingsWhite: openings("white"),
    openingsBlack: openings("black"),
    firstMoves: [{ san: "e4", games: 1, score: 100 }],
    vsE4: [{ san: "c5", games: 1, score: 50 }],
    vsD4: [{ san: "Nf6", games: 1, score: 100 }],
    averageMoves: 12,
    decisive: 67,
    topOpponents: games.map((g) => ({ name: (g.color === "white" ? g.row.black : g.row.white) ?? "?", games: 1, score: 50 })),
    decades: [...decades.entries()].sort(),
    style: styles.get(id) ?? null,
    report: reports.get(id) ?? null,
  };
}

function script(id: string, title: string, startFen: string, steps: CoachStep[], returnFen: string, returnLastMove: string | null): CoachScript {
  return { id, coachId: "stockfish", title, startFen, startLastMove: null, returnFen, returnLastMove, steps, aiNarrated: false, notice: null };
}

export function install(register: Register, helpers: MockHelpers) {
  const wait = (ms: number) => new Promise((r) => setTimeout(r, ms));

  register("masters_players", () => PLAYERS.map((p) => ({ ...p, gamesInLibrary: count(p.id) })));

  register("masters_legends", () => library.filter((g) => g.subject === "legends").map((g) => ({ game: g.row, title: legendInfo.get(g.row.id)!.title, why: legendInfo.get(g.row.id)!.why })));

  register("masters_download", async (a) => {
    const id = a.id as string;
    const job = `masters:${id}`;
    await helpers.emit("download-progress", { id: job, received: 50, total: 100, done: false, error: null });
    await wait(60);
    await helpers.emit("download-progress", { id: job, received: 100, total: 100, done: true, error: null });
    const added = addPlayerGames(id);
    await helpers.emit("import-progress", { jobId: job, phase: "Adding games to the library", seen: added, imported: added, bytes: 0 });
    return { seen: added, imported: added, duplicates: 0, errors: [], newest: null };
  });

  register("masters_import_files", async (a) => {
    const player = a.player as string;
    await helpers.emit("import-progress", { jobId: a.jobId, phase: "Importing", seen: 2, imported: 2, bytes: 0 });
    const added = addPlayerGames(player);
    return { seen: added, imported: added, duplicates: 0, errors: [], newest: null };
  });

  register("masters_remove", (a) => {
    const id = a.id as string;
    const before = library.length;
    for (let i = library.length - 1; i >= 0; i--) if (library[i].subject === id) library.splice(i, 1);
    return before - library.length;
  });

  register("masters_profile", (a) => profile(a.id as string));

  register("masters_analyse_style", async (a) => {
    const id = a.id as string;
    for (let i = 1; i <= 3; i++) {
      await helpers.emit("review-progress", { reviewId: `style:${id}`, done: i, total: 3 });
      await wait(30);
    }
    const s: StyleMetrics = { gamesAnalysed: 3, accuracy: 88, sharpness: 4.6, sacrificesPerGame: 0.67, greatPerGame: 1.33, mistakesPerGame: 0.33, blundersPerGame: 0, engine: "Stockfish" };
    styles.set(id, s);
    return s;
  });

  register("masters_style_report", (a) => {
    const id = a.id as string;
    const name = PLAYERS.find((p) => p.id === id)?.name ?? id;
    const text = `${name} plays sharp, attacking chess: a lot of decisive games, quick development and a readiness to give material for the initiative.`;
    reports.set(id, text);
    return text;
  });

  register("masters_walkthrough", (a) => {
    const req = a.request as { gameId: number };
    const g = library.find((x) => x.row.id === req.gameId);
    if (!g) throw "game not found";
    const { fens, ucis } = fensOf(g.sans);
    const n = Math.min(4, g.sans.length);
    const steps: CoachStep[] = [{ type: "say", id: "s1", role: "intro", text: `${g.row.white} vs ${g.row.black}. Let's watch how it's played.` }];
    for (let i = 0; i < n; i++) {
      steps.push({ type: "move", uci: ucis[i], san: g.sans[i], fen: fens[i + 1] });
      // Long enough pauses that a test can see the coach at work when the
      // fast mode is switched off.
      steps.push({ type: "pause", ms: 1500 });
    }
    steps.push({ type: "say", id: "s2", role: "moment", text: `${g.sans[n - 1]} is the key idea here.` });
    steps.push({ type: "back", fen: fens[n - 1], lastMove: n > 1 ? ucis[n - 2] : null });
    steps.push({ type: "pause", ms: 1500 });
    steps.push({ type: "reset", fen: fens[fens.length - 1], lastMove: ucis[ucis.length - 1] ?? null });
    steps.push({ type: "say", id: "s3", role: "summary", text: "That's the game." });
    return script(`walk-${g.row.id}`, `${g.row.white} vs ${g.row.black}`, fens[0], steps, fens[fens.length - 1], ucis[ucis.length - 1] ?? null);
  });

  register("masters_guess", (a) => {
    const fen = a.fen as string;
    const guess = a.guess as string;
    const masterMove = a.masterMove as string;
    const masterName = (a.masterName as string) ?? "The master";
    const g = helpers.apply(fen, guess);
    const m = helpers.apply(fen, masterMove);
    const exact = g.uci === m.uci;
    const result: GuessResult = {
      points: exact ? 3 : 0,
      verdict: exact ? "exact" : "miss",
      guessSan: g.san,
      masterSan: m.san,
      guessWin: exact ? 55 : 40,
      masterWin: 55,
      script: exact
        ? null
        : script(
            `guess-${Date.now()}`,
            `${masterName}'s choice`,
            fen,
            [
              { type: "say", id: "s1", role: "intro", text: `You chose ${g.san}; ${masterName} played ${m.san}.` },
              { type: "move", uci: m.uci, san: m.san, fen: m.position.fen },
              { type: "back", fen, lastMove: null },
            ],
            fen,
            null,
          ),
    };
    return result;
  });
}
