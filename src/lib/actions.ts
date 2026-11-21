// App-level actions shared by menus, hotkeys and screens.

import { open, save } from "@tauri-apps/plugin-dialog";
import { notifications } from "@mantine/notifications";
import { commands, notifyError, unwrap, type GameTree, type ReviewResult, type Side } from "./api";
import { emptyTree, header, lastOfLine } from "./tree";
import { activeTab, newId, useApp } from "../stores/app";
import { useAnalysis } from "../stores/analysis";
import { usePlay } from "../stores/play";
import { stopTabEngines, useEngines } from "../stores/engines";
import { useReviews } from "../stores/reviews";

export function openNewGame() {
  const existing = useApp.getState().tabs.find((t) => t.kind === "new-game");
  if (existing) useApp.getState().activate(existing.id);
  else useApp.getState().openTab({ id: newId("tab"), kind: "new-game", title: "New game" });
}

function treeTitle(tree: GameTree, fallback: string): string {
  const white = header(tree, "White");
  const black = header(tree, "Black");
  if (white && black && white !== "?" && black !== "?") return `${white} – ${black}`;
  return fallback;
}

interface OpenOptions {
  autoReview?: boolean;
  gameId?: number;
  review?: ReviewResult | null;
  orientation?: Side;
}

export function openAnalysis(tree: GameTree = emptyTree(), title?: string, current = 0, opts: OpenOptions = {}) {
  const analysisId = newId("analysis");
  const orientation = opts.orientation ?? (header(tree, "GrandieUserSide") === "black" ? "black" : "white");
  const review = opts.review ?? undefined;
  useAnalysis.getState().create(analysisId, { tree, current, orientation, autoReview: opts.autoReview, gameId: opts.gameId, review });
  useApp.getState().openTab({ id: newId("tab"), kind: "analysis", analysisId, title: title ?? treeTitle(tree, "Analysis") });
  return analysisId;
}

export async function openAnalysisFromPgn(pgn: string, title?: string, opts: OpenOptions & { atStart?: boolean } = {}) {
  let parsed;
  try {
    parsed = await commands.pgnParse(pgn);
  } catch (e) {
    notifyError("Could not read the PGN", e);
    return;
  }
  if (parsed.games.length === 0) {
    notifyError("No game found", parsed.errors.join("\n") || "The PGN contains no games.");
    return;
  }
  const tree = parsed.games[0];
  const last = tree.nodes.length > 1 && !opts.atStart ? lastOfLine(tree) : 0;
  openAnalysis(tree, title, last, opts);
}

export async function openPgnFiles() {
  const selected = await open({ multiple: true, filters: [{ name: "PGN", extensions: ["pgn"] }] });
  if (!selected) return;
  const paths = Array.isArray(selected) ? selected : [selected];
  for (const path of paths) {
    try {
      const parsed = await unwrap(commands.pgnReadFile(path));
      parsed.games.slice(0, 20).forEach((g) => openAnalysis(g));
      if (parsed.games.length > 20) {
        notifications.show({
          title: "Large PGN",
          message: `Opened the first 20 of ${parsed.games.length} games. Import the file into the database to browse them all.`,
        });
      }
      if (parsed.errors.length) {
        notifications.show({ color: "yellow", title: "Some moves were skipped", message: parsed.errors.slice(0, 3).join("\n") });
      }
    } catch (e) {
      notifyError("Could not open the PGN", e);
    }
  }
}

export async function savePgnText(pgn: string, suggestedName: string) {
  const path = await save({ defaultPath: suggestedName.replace(/[/:]/g, "-"), filters: [{ name: "PGN", extensions: ["pgn"] }] });
  if (!path) return;
  try {
    await unwrap(commands.fileWriteText(path, pgn));
    notifications.show({ title: "Saved", message: path, color: "green" });
  } catch (e) {
    notifyError("Could not save", e);
  }
}

/** Save the active tab's game as PGN. */
export async function saveActiveTab() {
  const tab = activeTab();
  if (!tab) return;
  if (tab.kind === "play") {
    const pgn = await unwrap(commands.playPgn(tab.sessionId)).catch(() => null);
    if (pgn) await savePgnText(pgn, `${tab.title}.pgn`);
  } else if (tab.kind === "analysis") {
    const doc = useAnalysis.getState().docs[tab.analysisId];
    if (!doc) return;
    try {
      await savePgnText(await commands.pgnWrite(doc.tree), `${tab.title}.pgn`);
    } catch (e) {
      notifyError("Could not save", e);
    }
  }
}

export function activeFen(): string | null {
  const tab = activeTab();
  if (tab?.kind === "play") return usePlay.getState().snapshots[tab.sessionId]?.position.fen ?? null;
  if (tab?.kind === "analysis") {
    const doc = useAnalysis.getState().docs[tab.analysisId];
    return doc ? doc.tree.nodes[doc.current].fen : null;
  }
  return null;
}

export async function closeTab(id: string) {
  const closed = useApp.getState().closeTab(id);
  if (closed?.kind === "play") {
    usePlay.getState().remove(closed.sessionId);
    const r = await commands.playClose(closed.sessionId).catch((e) => ({ status: "error" as const, error: e }));
    if (r.status === "error") notifyError("The game's engine didn't close cleanly", r.error);
  } else if (closed?.kind === "analysis") {
    // A review still running for this tab would only be thrown away.
    if (useReviews.getState().progress[closed.analysisId]) void commands.reviewCancel(closed.analysisId);
    useAnalysis.getState().remove(closed.analysisId);
    stopTabEngines(closed.id);
    useEngines.getState().removeTab(closed.id);
  }
}

export async function pasteFenOrPgn() {
  let text: string;
  try {
    text = (await navigator.clipboard.readText()).trim();
  } catch {
    notifyError("Clipboard unavailable", "Grandie could not read the clipboard.");
    return;
  }
  if (!text) return;
  const asFen = await commands.chessPositionInfo(text);
  if (asFen.status === "ok") {
    openAnalysis(emptyTree(asFen.data.fen), "Position");
  } else {
    await openAnalysisFromPgn(text);
  }
}

export async function copyFen() {
  const fen = activeFen();
  if (!fen) return;
  await navigator.clipboard.writeText(fen);
  notifications.show({ message: "FEN copied", color: "green", autoClose: 1500 });
}

/** Open a database game in an analysis tab (with its stored review, if any). */
export async function openDbGame(id: number) {
  try {
    const game = await unwrap(commands.dbGame(id));
    const orientation: Side = game.row.userColor === "black" ? "black" : "white";
    openAnalysis(game.tree, undefined, 0, { gameId: id, review: game.review, orientation });
  } catch (e) {
    notifyError("Could not open the game", e);
  }
}

/** Make sure Grandie's own saved games are in the database, and find one by its file. */
export async function grandieGameId(pgnPath: string | null): Promise<number | undefined> {
  await commands.dbImportGrandie();
  if (!pgnPath) return undefined;
  const name = pgnPath.split("/").pop() ?? pgnPath;
  const r = await commands.dbFind("grandie", name);
  return r.status === "ok" ? (r.data ?? undefined) : undefined;
}
