import { useCallback, useEffect, useMemo } from "react";
import { commands, errorKind, notifyError, unwrap, type GameTree, type MoveReview, type PositionEval, type ReviewDepth, type ReviewResult } from "../lib/api";
import { header, mainline, toggleNag } from "../lib/tree";
import { useSettings } from "../stores/settings";
import { useAnalysis, type AnalysisDoc } from "../stores/analysis";
import { useReviews } from "../stores/reviews";

/** NAG for a move class, used to mark reviewed moves in the tree. */
const CLASS_NAG: Partial<Record<MoveReview["class"], number>> = { inaccuracy: 6, mistake: 2, blunder: 4, brilliant: 3, great: 1 };

/** How many leading main-line moves still match the reviewed moves. The tree
 *  can change after a review (a variation promoted, moves deleted); only that
 *  prefix may show review data. */
export function reviewedPrefix(tree: GameTree, main: number[], review: ReviewResult | undefined): number {
  if (!review) return 0;
  let n = 0;
  while (n < main.length && n < review.moves.length && tree.nodes[main[n]].uci === review.moves[n].uci) n++;
  return n;
}

/** Put the review's annotation glyphs on the main line, matching moves by
 *  their place in the line (node ids may have changed since it started). */
export function markReviewedMoves(tree: GameTree, review: ReviewResult): GameTree {
  const main = mainline(tree);
  const matched = reviewedPrefix(tree, main, review);
  let t = tree;
  for (let i = 0; i < matched; i++) {
    const id = main[i];
    const nag = CLASS_NAG[review.moves[i].class];
    if (nag && !t.nodes[id].nags.some((n) => n <= 6)) t = toggleNag(t, id, nag);
  }
  return t;
}

/** The user's colour in a game: from Grandie's own tag, or by name. */
export function userSideOf(tree: GameTree): "white" | "black" | null {
  const tag = header(tree, "GrandieUserSide");
  if (tag === "white" || tag === "black") return tag;
  const s = useSettings.getState().settings;
  const names = [s.playerName, s.lichessUsername, s.chesscomUsername].filter((n): n is string => !!n && n !== "You").map((n) => n.toLowerCase());
  if (names.includes((header(tree, "White") ?? "").toLowerCase())) return "white";
  if (names.includes((header(tree, "Black") ?? "").toLowerCase())) return "black";
  return null;
}

interface ReviewFlow {
  main: number[];
  progress: { done: number; total: number } | null;
  /** Node of the reviewed move at `ply`, if the tree still has it. */
  reviewNodeOf: (ply: number) => number | null;
  /** Review of the move leading to `id`, if `id` is on the reviewed main line. */
  reviewOfNode: (id: number) => MoveReview | null;
  /** Ply of the current node if it is on the main line (0 at the start). */
  currentPly: number | null;
  /** Review evaluation of the current position, if it is still valid. */
  reviewEval: PositionEval | undefined;
  startReview: (depth: ReviewDepth) => Promise<void>;
  cancelReview: () => void;
}

/** Reviewing a doc's main line, and relating the review to the tree. */
export function useReviewFlow(analysisId: string, doc: AnalysisDoc | undefined, onStarted: () => void): ReviewFlow {
  const update = useAnalysis((s) => s.update);
  const progress = useReviews((s) => s.progress[analysisId] ?? null);
  const tree = doc?.tree;
  const review = doc?.review;

  const main = useMemo(() => (tree ? mainline(tree) : []), [tree]);
  const matched = useMemo(() => (tree ? reviewedPrefix(tree, main, review) : 0), [tree, main, review]);
  const indexOfNode = useMemo(() => new Map(main.map((id, i) => [id, i])), [main]);

  const reviewNodeOf = useCallback((ply: number) => (ply >= 1 && ply <= matched ? main[ply - 1] : null), [main, matched]);
  const reviewOfNode = useCallback(
    (id: number): MoveReview | null => {
      const i = indexOfNode.get(id);
      return review && i !== undefined && i < matched ? review.moves[i] : null;
    },
    [indexOfNode, matched, review],
  );

  const current = doc?.current;
  const currentPly = current === 0 ? 0 : current !== undefined && indexOfNode.has(current) ? indexOfNode.get(current)! + 1 : null;
  const reviewEval = review && currentPly !== null && currentPly <= matched ? review.evals[currentPly] : undefined;

  const startReview = useCallback(
    async (depth: ReviewDepth) => {
      // One review per tab: the screen remounts on every tab switch, and the
      // running review is tracked outside it.
      if (useReviews.getState().progress[analysisId]) return;
      const d = useAnalysis.getState().docs[analysisId];
      if (!d) return;
      const ucis = mainline(d.tree).map((i) => d.tree.nodes[i].uci ?? "");
      if (ucis.length === 0) {
        notifyError("Nothing to review", "Play or load some moves first.");
        return;
      }
      onStarted();
      useReviews.getState().set(analysisId, 0, ucis.length + 1);
      try {
        const result = await unwrap(commands.reviewStart(analysisId, d.tree.nodes[0].fen, ucis, depth, null));
        update(analysisId, (cur) => ({ review: result, tree: markReviewedMoves(cur.tree, result), autoReview: false }));
        // Store the review with the game so Progress can use it.
        const gameId = useAnalysis.getState().docs[analysisId]?.gameId;
        if (gameId !== undefined) {
          const saved = await commands.dbSaveReview(gameId, result);
          if (saved.status === "error") notifyError("The review wasn't saved with the game", saved.error);
        }
        // The user's own mistakes go into the "Learn from your mistakes" deck.
        const side = userSideOf(d.tree);
        if (side) void commands.mistakesAdd(gameId ?? null, result, side);
      } catch (e) {
        if (errorKind(e) !== "cancelled") notifyError("Review failed", e);
        update(analysisId, { autoReview: false });
      } finally {
        useReviews.getState().clear(analysisId);
      }
    },
    [analysisId, update, onStarted],
  );

  // After a game the review starts by itself, once.
  const autoReview = doc?.autoReview;
  const hasReview = !!review;
  useEffect(() => {
    if (autoReview && !hasReview && !useReviews.getState().progress[analysisId]) void startReview("quick");
  }, [autoReview, hasReview, analysisId, startReview]);

  const cancelReview = useCallback(() => void commands.reviewCancel(analysisId), [analysisId]);

  return { main, progress, reviewNodeOf, reviewOfNode, currentPly, reviewEval, startReview, cancelReview };
}
