import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Alert, Button, Group, SegmentedControl, Text, Textarea } from "@mantine/core";
import { Board, useBoardSize, type Arrow } from "../board/Board";
import { BoardNavButtons, useBoardNav } from "../board/BoardNav";
import { EvalBar } from "../board/EvalBar";
import { MoveTree } from "../moves/MoveTree";
import { EnginePanel } from "../analysis/EnginePanel";
import { EvalGraph } from "../analysis/EvalGraph";
import { ReviewPanel } from "../analysis/ReviewPanel";
import { MoveContextMenu } from "../analysis/MoveContextMenu";
import { useLiveEngines } from "../analysis/useLiveEngines";
import { useReviewFlow } from "../analysis/useReviewFlow";
import { useRetry } from "../analysis/useRetry";
import { commands, errorKind, notifyError, type AnalysisLine, type CoachScript, type ErrorPayload, type MoveReview, type OpeningName, type VoiceCommand } from "../lib/api";
import { applyMove as applyRustMove } from "../lib/applyMove";
import { addApplied, deleteFrom, header, lastOfLine, promoteToMainline, survivorAfterDelete, toggleNag, uciPath, updateNode } from "../lib/tree";
import { CLASS_STYLE } from "../lib/score";
import { usePositionInfo } from "../lib/usePositionInfo";
import { useAnalysis } from "../stores/analysis";
import { useSettings } from "../stores/settings";
import { useCoach } from "../stores/coach";
import { CoachPanel } from "../coach/CoachPanel";
import { useCoachBoard } from "../coach/useCoachBoard";
import { IconSchool } from "@tabler/icons-react";

export function AnalysisScreen({ analysisId, tabId }: { analysisId: string; tabId: string }) {
  const doc = useAnalysis((s) => s.docs[analysisId]);
  const update = useAnalysis((s) => s.update);
  const settings = useSettings((s) => s.settings);
  const { ref, size } = useBoardSize(doc?.review ? 116 : 16, 34);
  const [panel, setPanel] = useState<"moves" | "review">(doc?.review || doc?.autoReview ? "review" : "moves");
  const [menu, setMenu] = useState<{ id: number; x: number; y: number } | null>(null);
  const [editingComment, setEditingComment] = useState<number | null>(null);
  const [extraArrows, setExtraArrows] = useState<Arrow[]>([]);
  const moveQueue = useRef<Promise<boolean>>(Promise.resolve(true));
  const [opening, setOpening] = useState<OpeningName | null>(null);

  const coachOwner = `analysis:${tabId}`;
  const coachBoard = useCoachBoard(coachOwner);
  const retry = useRetry();
  const showReviewPanel = useCallback(() => setPanel("review"), []);
  const review = useReviewFlow(analysisId, doc, showReviewPanel);

  const node = doc?.tree.nodes[retry.retry ? retry.retry.nodeId : doc.current];
  const info = usePositionInfo(node?.fen);
  const fen = node?.fen ?? "";
  const moves = useMemo(() => (doc && node ? uciPath(doc.tree, node.id) : []), [doc, node]);
  const rootFen = doc?.tree.nodes[0].fen;
  const engines = useLiveEngines(tabId, rootFen, moves, fen);

  // Navigating leaves retry mode, so the board and the tree always agree.
  const { stop: stopRetry } = retry;
  const go = useCallback(
    (id: number | null | undefined) => {
      if (id === null || id === undefined) return;
      stopRetry();
      setExtraArrows([]);
      update(analysisId, { current: id });
    },
    [analysisId, update, stopRetry],
  );

  // Name of the opening reached by the current line.
  const movesKey = moves.join(" ");
  useEffect(() => {
    if (!rootFen) return;
    let live = true;
    commands
      .openingName(rootFen, movesKey ? movesKey.split(" ") : [])
      .then((o) => live && setOpening(o))
      .catch(() => live && setOpening(null));
    return () => {
      live = false;
    };
  }, [rootFen, movesKey]);

  const flip = () => update(analysisId, (d) => ({ orientation: d.orientation === "white" ? "black" : "white" }));
  const nav = {
    start: () => go(0),
    back: () => go(node?.parent),
    forward: () => go(node?.children[0]),
    end: () => doc && go(lastOfLine(doc.tree, doc.current)),
    flip,
  };
  useBoardNav(nav, [
    ["Space", engines.toggleRunning],
    ["Escape", () => (useCoach.getState().owner === coachOwner && useCoach.getState().fen ? useCoach.getState().stop() : stopRetry())],
  ]);

  if (!doc || !node) return <Text p="xl">This analysis is no longer available.</Text>;

  // ---- moves ----
  // Moves are queued and read the latest document, so quick successive moves
  // never act on a stale position.
  function move(uci: string): Promise<boolean> {
    const next = moveQueue.current.then(() => applyMove(uci));
    moveQueue.current = next.catch(() => false);
    return next;
  }

  async function applyMove(uci: string): Promise<boolean> {
    const attempt = retry.active();
    const d = useAnalysis.getState().docs[analysisId];
    const at = d?.tree.nodes[attempt ? attempt.nodeId : d.current];
    if (!d || !at) return false;
    if (attempt) {
      await retry.check(at.fen, uci);
      return false; // keep the retry position on the board
    }
    try {
      const applied = await applyRustMove(at.fen, uci);
      let added = false;
      update(analysisId, (cur) => {
        // The tree may have been edited while the move was checked; only add
        // it if the position it was played in is still there.
        if (cur.tree.nodes[d.current]?.fen !== at.fen) return {};
        added = true;
        const { tree, id } = addApplied(cur.tree, d.current, applied);
        return { tree, current: id };
      });
      setExtraArrows([]);
      return added;
    } catch (e) {
      notifyError("Illegal move", e);
      return false;
    }
  }

  /** Clicking an engine line plays its first move (through the move queue). */
  function playLine(pv: string[]) {
    if (pv.length) void move(pv[0]);
  }

  async function showBetter(m: MoveReview) {
    const id = review.reviewNodeOf(m.ply);
    const parent = id !== null ? doc!.tree.nodes[id].parent : null;
    if (parent === null || !m.best) return;
    stopRetry();
    update(analysisId, { current: parent });
    setExtraArrows([{ uci: m.best, brush: "green" }]);
    // Add the engine's line as a variation so it can be stepped through.
    const parentFen = doc!.tree.nodes[parent].fen;
    const r = await commands.chessApplySanLine(parentFen, m.bestLine.slice(0, 8));
    if (r.status !== "ok" || r.data.length === 0) return;
    update(analysisId, (d) => {
      if (d.tree.nodes[parent]?.fen !== parentFen) return {}; // the tree changed meanwhile
      let tree = d.tree;
      let cur = parent;
      for (const applied of r.data) {
        const res = addApplied(tree, cur, applied);
        tree = res.tree;
        cur = res.id;
      }
      return { tree };
    });
  }

  function startRetry(m: MoveReview) {
    const id = review.reviewNodeOf(m.ply);
    if (id === null) return;
    setExtraArrows([]);
    retry.start(m, doc!.tree.nodes[id].parent ?? 0);
  }

  // ---- the coach ----
  async function coachRun(label: string, load: () => Promise<{ status: "ok"; data: CoachScript } | { status: "error"; error: ErrorPayload }>) {
    const coach = useCoach.getState();
    stopRetry();
    coach.setLoading(coachOwner, label);
    const r = await load();
    coach.setLoading(coachOwner, null);
    if (r.status === "ok") coach.play(r.data, coachOwner);
    else if (errorKind(r.error) !== "cancelled") notifyError("The coach couldn't explain this", r.error);
  }

  function explainMove(m: MoveReview) {
    const id = review.reviewNodeOf(m.ply);
    const parent = id !== null ? doc!.tree.nodes[id].parent : null;
    if (id === null || parent === null || !doc!.review) return;
    const before = doc!.tree.nodes[parent];
    const after = doc!.tree.nodes[id];
    const replyLine = doc!.review.evals[m.ply]?.line ?? [];
    void coachRun("Looking at the move…", () =>
      commands.coachExplainMove({
        fenBefore: before.fen,
        lastMoveBefore: before.uci,
        played: m.uci,
        known: m.bestLine.length ? { bestLine: m.bestLine, replyLine, winBefore: m.winBefore ?? 50, winAfter: m.winAfter ?? 50, class: m.class } : null,
        returnFen: doc!.tree.nodes[doc!.current].fen === after.fen ? after.fen : doc!.tree.nodes[doc!.current].fen,
        returnLastMove: doc!.tree.nodes[doc!.current].uci,
        coachId: null,
      }),
    );
  }

  function walkGame() {
    if (!doc?.review) return;
    const ucis = review.main.map((i) => doc.tree.nodes[i].uci ?? "");
    void coachRun("Preparing the key moments…", () => commands.coachWalkGame(doc.review!, ucis, null, null));
  }

  function explainLine(line: AnalysisLine) {
    void coachRun("Following the engine's line…", () => commands.coachExplainLine(node!.fen, node!.uci, line.san.slice(0, 8), line.whiteWin, null));
  }

  /** Explain the move that led here (no review needed: the coach's engine checks it). */
  function explainCurrent() {
    const parent = node?.parent;
    if (!node?.uci || parent === null || parent === undefined) return;
    const currentReviewMove = review.reviewOfNode(node.id);
    if (currentReviewMove) return explainMove(currentReviewMove);
    const before = doc!.tree.nodes[parent];
    void coachRun("Checking the move with the engine…", () =>
      commands.coachExplainMove({ fenBefore: before.fen, lastMoveBefore: before.uci, played: node.uci!, known: null, returnFen: node.fen, returnLastMove: node.uci, coachId: null }),
    );
  }

  function voiceCommand(c: VoiceCommand): boolean {
    switch (c) {
      case "next":
        nav.forward();
        return true;
      case "previous":
      case "takeBack":
        nav.back();
        return true;
      case "flip":
        flip();
        return true;
      case "explain":
        explainCurrent();
        return true;
      default:
        return false;
    }
  }

  // ---- variation editing ----
  function promote(id: number) {
    update(analysisId, (d) => ({ tree: promoteToMainline(d.tree, id) }));
  }
  function remove(id: number) {
    update(analysisId, (d) => {
      // Deleting renumbers nodes, so find the surviving position again by its moves.
      const path = uciPath(d.tree, survivorAfterDelete(d.tree, id, d.current));
      const tree = deleteFrom(d.tree, id);
      let cur = 0;
      for (const uci of path) {
        const next = tree.nodes[cur].children.find((c) => tree.nodes[c].uci === uci);
        if (next === undefined) break;
        cur = next;
      }
      return { tree, current: cur };
    });
  }

  // ---- display ----
  const r = retry.retry;
  const live = engines.live;
  const reviewEval = r ? undefined : review.reviewEval;
  const barWin = live?.lines[0]?.whiteWin ?? reviewEval?.whiteWin ?? null;
  const barScore = live?.lines[0]?.score ?? reviewEval?.score ?? null;
  const engineArrows: Arrow[] =
    engines.engineTab.running && live
      ? live.lines.slice(0, 3).map((l, i) => ({ uci: l.pv[0] ?? "", brush: i === 0 ? "paleBlue" : "paleGrey" }))
      : [];
  const currentReview = r ? null : review.reviewOfNode(doc.current);
  const white = header(doc.tree, "White");
  const black = header(doc.tree, "Black");
  const progress = review.progress;

  return (
    <div className="workspace">
      <div className="board-column" ref={ref}>
        <div className="board-frame">
          {settings.showEvalBar && <EvalBar whiteWin={barWin} score={barScore} orientation={doc.orientation} height={size} />}
          <Board
            fen={coachBoard.fen ?? node.fen}
            orientation={doc.orientation}
            turn={info?.turn ?? "white"}
            dests={coachBoard.active || (r && r.status !== "waiting" && r.status !== "wrong") ? {} : info?.dests}
            movableColor={coachBoard.active ? undefined : info?.turn}
            lastMove={coachBoard.active ? coachBoard.lastMove : r ? null : node.uci}
            check={coachBoard.active ? false : info?.check}
            arrows={coachBoard.active ? coachBoard.arrows : [...extraArrows, ...engineArrows]}
            circles={coachBoard.circles}
            coaching={coachBoard.active}
            coordinates={settings.showCoordinates}
            animationMs={settings.animationMs}
            size={size}
            onMove={move}
          />
        </div>
        {doc.review && (
          <div style={{ width: size + 30, marginTop: 10 }}>
            <EvalGraph review={doc.review} currentPly={review.currentPly} onSelectPly={(ply) => go(ply === 0 ? 0 : review.reviewNodeOf(ply))} />
          </div>
        )}
      </div>

      <div className="side-panel">
        {r && (
          <Alert
            color={r.status === "correct" ? "green" : r.status === "wrong" ? "orange" : "blue"}
            variant="light"
            title={r.status === "correct" ? "Well found!" : "Find a better move"}
            withCloseButton
            onClose={stopRetry}
            closeButtonLabel="Leave retry"
          >
            {r.status === "waiting" && <Text size="sm">You played {r.move.san} here. What's stronger?</Text>}
            {r.status === "checking" && <Text size="sm">Checking your move…</Text>}
            {r.status === "correct" && r.tried && (
              <Text size="sm">
                {r.tried.san} keeps your winning chances at {Math.round(r.tried.winAfter)}%.
              </Text>
            )}
            {r.status === "wrong" && r.tried && (
              <>
                <Text size="sm">
                  {r.tried.san} still drops to {Math.round(r.tried.winAfter)}%. Try again.
                </Text>
                <Group gap={6} mt={6}>
                  <Button size="xs" variant="default" onClick={retry.again}>
                    Try again
                  </Button>
                  <Button size="xs" variant="subtle" onClick={() => void showBetter(r.move)}>
                    Show the answer
                  </Button>
                </Group>
              </>
            )}
          </Alert>
        )}

        <CoachPanel
          owner={coachOwner}
          fen={() => (useCoach.getState().fen ? null : node.fen)}
          context={() => ({
            fen: node.fen,
            recentMoves: review.main.slice(0, Math.max(0, (review.currentPly ?? 0))).slice(-10).map((i) => doc.tree.nodes[i].san ?? ""),
            game: white || black ? `${white ?? "?"} vs ${black ?? "?"}` : null,
            player: null,
          })}
          onVoiceMove={(uci) => void move(uci)}
          onVoiceCommand={voiceCommand}
          actions={
            node.uci ? (
              <Button size="compact-xs" variant="light" color="teal" leftSection={<IconSchool size={12} />} onClick={explainCurrent}>
                Explain {node.san ?? "this move"}
              </Button>
            ) : undefined
          }
        />

        <EnginePanel tabId={tabId} fen={node.fen} onPlayLine={playLine} onExplainLine={explainLine} />

        <SegmentedControl
          size="xs"
          value={panel}
          onChange={(v) => setPanel(v as "moves" | "review")}
          data={[
            { value: "moves", label: "Moves" },
            { value: "review", label: progress ? `Review ${Math.round((progress.done / progress.total) * 100)}%` : "Review" },
          ]}
        />

        {panel === "review" ? (
          <div style={{ flex: 1, minHeight: 0, overflow: "auto" }}>
            <ReviewPanel
              review={doc.review}
              progress={progress}
              whiteName={white ?? "White"}
              blackName={black ?? "Black"}
              currentMove={currentReview}
              onStart={(depth) => void review.startReview(depth)}
              onCancel={review.cancelReview}
              onJump={(ply) => go(review.reviewNodeOf(ply))}
              onShowBetter={(m) => void showBetter(m)}
              onRetry={startRetry}
              onExplain={explainMove}
              onWalkGame={walkGame}
            />
          </div>
        ) : (
          <div className="panel" style={{ flex: 1, minHeight: 0, display: "flex", flexDirection: "column" }}>
            {opening && (
              <Text size="xs" px={12} pt={8} c="dimmed" aria-label="Opening">
                <b>{opening.eco}</b> {opening.name}
              </Text>
            )}
            {(white || black) && (
              <div style={{ padding: "8px 12px 0" }}>
                <Text fw={650} size="sm">
                  {white ?? "?"} – {black ?? "?"}
                </Text>
                <Text size="xs" c="dimmed">
                  {[header(doc.tree, "Event"), header(doc.tree, "Date"), header(doc.tree, "Result")].filter((v) => v && v !== "?" && v !== "*").join(" · ")}
                </Text>
              </div>
            )}
            <div style={{ flex: 1, minHeight: 0, overflow: "auto" }}>
              <MoveTree
                tree={doc.tree}
                current={doc.current}
                onSelect={go}
                onContextMenu={(id, e) => {
                  e.preventDefault();
                  setMenu({ id, x: e.clientX, y: e.clientY });
                }}
                decorate={(id) => {
                  const rv = review.reviewOfNode(id);
                  if (!rv || rv.class === "best" || rv.class === "good") return null;
                  if (doc.tree.nodes[id].nags.length) return null;
                  return (
                    <span className="annot" style={{ color: CLASS_STYLE[rv.class].color }}>
                      {CLASS_STYLE[rv.class].symbol}
                    </span>
                  );
                }}
              />
            </div>
            {editingComment !== null && (
              <div style={{ padding: 8, borderTop: "1px solid var(--hairline)" }}>
                <Textarea
                  // Keyed by node so switching moves never shows the previous comment.
                  key={editingComment}
                  autosize
                  minRows={2}
                  placeholder="Comment on this move"
                  defaultValue={doc.tree.nodes[editingComment]?.comment ?? ""}
                  autoFocus
                  onBlur={(e) => {
                    const text = e.currentTarget.value.trim();
                    const id = editingComment;
                    update(analysisId, (d) => (d.tree.nodes[id] ? { tree: updateNode(d.tree, id, { comment: text || null }) } : {}));
                    setEditingComment(null);
                  }}
                />
              </div>
            )}
            <Group justify="center" gap={4} p={6} style={{ borderTop: "1px solid var(--hairline)" }}>
              <BoardNavButtons nav={nav} labels={{ start: "Start", back: "Previous move", forward: "Next move", end: "End of line" }} />
            </Group>
          </div>
        )}
      </div>

      {menu && doc.tree.nodes[menu.id] && (
        <MoveContextMenu
          x={menu.x}
          y={menu.y}
          nags={doc.tree.nodes[menu.id].nags}
          onToggleNag={(nag) => update(analysisId, (d) => ({ tree: toggleNag(d.tree, menu.id, nag) }))}
          onComment={() => setEditingComment(menu.id)}
          onPromote={() => promote(menu.id)}
          onDelete={() => remove(menu.id)}
          onClose={() => setMenu(null)}
        />
      )}
    </div>
  );
}
