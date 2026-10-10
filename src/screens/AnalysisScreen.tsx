import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { ActionIcon, Alert, Button, Group, Paper, SegmentedControl, Stack, Text, Textarea, Tooltip } from "@mantine/core";
import { useHotkeys } from "@mantine/hooks";
import {
  IconChevronLeft,
  IconChevronRight,
  IconChevronsLeft,
  IconChevronsRight,
  IconSwitchVertical,
  IconX,
} from "@tabler/icons-react";
import { Board, useBoardSize, type Arrow } from "../board/Board";
import { EvalBar } from "../board/EvalBar";
import { MoveTree } from "../moves/MoveTree";
import { EnginePanel } from "../analysis/EnginePanel";
import { EvalGraph } from "../analysis/EvalGraph";
import { ReviewPanel } from "../analysis/ReviewPanel";
import { commands, notifyError, unwrap, type MoveReview, type OpeningName, type ReviewDepth } from "../lib/api";
import { addApplied, deleteFrom, header, mainline, promoteToMainline, survivorAfterDelete, toggleNag, uciPath, updateNode } from "../lib/tree";
import { CLASS_STYLE } from "../lib/score";
import { useAnalysis } from "../stores/analysis";
import { useSettings } from "../stores/settings";
import { sameFen, useEngines } from "../stores/engines";
import { useReviews } from "../stores/reviews";
import { useApp } from "../stores/app";
import { primePositionInfo, usePositionInfo } from "../lib/usePositionInfo";
import { playSound, soundForSan } from "../lib/sounds";

interface RetryState {
  move: MoveReview;
  /** Node whose position the user must find a better move in. */
  nodeId: number;
  status: "waiting" | "checking" | "correct" | "wrong";
  tried?: { san: string; winAfter: number };
}

const NAGS: { nag: number; label: string }[] = [
  { nag: 1, label: "!" },
  { nag: 2, label: "?" },
  { nag: 3, label: "!!" },
  { nag: 4, label: "??" },
  { nag: 5, label: "!?" },
  { nag: 6, label: "?!" },
];

export function AnalysisScreen({ analysisId, tabId }: { analysisId: string; tabId: string }) {
  const doc = useAnalysis((s) => s.docs[analysisId]);
  const update = useAnalysis((s) => s.update);
  const settings = useSettings((s) => s.settings);
  const engineTab = useEngines((s) => s.tabs[tabId]) ?? { enabled: ["lc0"], running: false, multipv: 3 };
  const setEngineTab = useEngines((s) => s.setTab);
  const allLines = useEngines((s) => s.lines);
  const progress = useReviews((s) => s.progress[analysisId] ?? null);
  const isActive = useApp((s) => s.activeTabId === tabId && s.section === "boards");
  const { ref, size } = useBoardSize(doc?.review ? 116 : 16, 34);
  const [panel, setPanel] = useState<"moves" | "review">(doc?.review || doc?.autoReview ? "review" : "moves");
  const [retry, setRetry] = useState<RetryState | null>(null);
  const [menu, setMenu] = useState<{ id: number; x: number; y: number } | null>(null);
  const [editingComment, setEditingComment] = useState<number | null>(null);
  const [extraArrows, setExtraArrows] = useState<Arrow[]>([]);
  const moveQueue = useRef<Promise<boolean>>(Promise.resolve(true));
  const [opening, setOpening] = useState<OpeningName | null>(null);

  const node = doc?.tree.nodes[retry ? retry.nodeId : doc.current];
  const info = usePositionInfo(node?.fen);

  const go = useCallback(
    (id: number | null | undefined) => {
      if (id === null || id === undefined) return;
      setExtraArrows([]);
      update(analysisId, { current: id });
    },
    [analysisId, update],
  );

  // ---- review ↔ tree mapping ----
  const main = useMemo(() => (doc ? mainline(doc.tree) : []), [doc]);
  const reviewNodeOf = useCallback(
    (ply: number) => {
      if (!doc?.review) return null;
      const id = main[ply - 1];
      return id !== undefined && doc.tree.nodes[id].uci === doc.review.moves[ply - 1]?.uci ? id : null;
    },
    [doc, main],
  );
  const reviewOfNode = useCallback(
    (id: number): MoveReview | null => {
      if (!doc?.review) return null;
      const idx = main.indexOf(id);
      if (idx < 0) return null;
      const m = doc.review.moves[idx];
      return m && m.uci === doc.tree.nodes[id].uci ? m : null;
    },
    [doc, main],
  );
  const currentPly = doc && main.includes(doc.current) ? doc.tree.nodes[doc.current].ply : doc?.current === 0 ? 0 : null;

  // ---- live analysis ----
  const fen = node?.fen ?? "";
  const moves = useMemo(() => (doc && node ? uciPath(doc.tree, node.id) : []), [doc, node]);
  const running = engineTab.running && isActive;
  const enabledKey = engineTab.enabled.join(",");
  useEffect(() => {
    if (!doc || !node) return;
    const engines = enabledKey ? enabledKey.split(",") : [];
    if (!running) {
      engines.forEach((e) => void commands.analysisStop(e));
      return;
    }
    const t = setTimeout(() => {
      for (const engineId of engines) {
        void commands
          .analysisStart({ tabId, engineId, rootFen: doc.tree.nodes[0].fen, moves, multipv: engineTab.multipv })
          .then((r) => {
            if (r.status === "error") {
              notifyError("Engine error", r.error);
              setEngineTab(tabId, { running: false });
            }
          });
      }
    }, 120);
    return () => clearTimeout(t);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [running, fen, enabledKey, engineTab.multipv, tabId]);

  useEffect(() => () => engineTab.enabled.forEach((e) => void commands.analysisStop(e)), []); // eslint-disable-line react-hooks/exhaustive-deps

  // Name of the opening reached by the current line.
  const rootFen = doc?.tree.nodes[0].fen;
  const movesKey = moves.join(" ");
  useEffect(() => {
    if (!rootFen) return;
    void commands.openingName(rootFen, movesKey ? movesKey.split(" ") : []).then(setOpening);
  }, [rootFen, movesKey]);

  // ---- auto review after a game ----
  const startReview = useCallback(
    async (depth: ReviewDepth) => {
      const d = useAnalysis.getState().docs[analysisId];
      if (!d) return;
      const ids = mainline(d.tree);
      const ucis = ids.map((i) => d.tree.nodes[i].uci ?? "");
      if (ucis.length === 0) {
        notifyError("Nothing to review", "Play or load some moves first.");
        return;
      }
      setPanel("review");
      useReviews.getState().set(analysisId, 0, ucis.length + 1);
      try {
        const review = await unwrap(commands.reviewStart(analysisId, d.tree.nodes[0].fen, ucis, depth, null));
        update(analysisId, (cur) => {
          // Mark mistakes in the tree with the matching annotation glyph.
          let tree = cur.tree;
          review.moves.forEach((m, i) => {
            const id = ids[i];
            const nag = { inaccuracy: 6, mistake: 2, blunder: 4, brilliant: 3, great: 1 }[m.class as string];
            if (nag && tree.nodes[id] && !tree.nodes[id].nags.some((n) => n <= 6)) tree = toggleNag(tree, id, nag);
          });
          return { review, reviewMoves: ucis, tree, autoReview: false };
        });
        // Store the review with the game so Progress can use it.
        const gameId = useAnalysis.getState().docs[analysisId]?.gameId;
        if (gameId !== undefined) void commands.dbSaveReview(gameId, review);
      } catch (e) {
        if (!String(e).includes("cancelled")) notifyError("Review failed", e);
        update(analysisId, { autoReview: false });
      } finally {
        useReviews.getState().clear(analysisId);
      }
    },
    [analysisId, update],
  );

  const autoStarted = useRef(false);
  useEffect(() => {
    if (doc?.autoReview && !doc.review && !autoStarted.current) {
      autoStarted.current = true;
      void startReview("quick");
    }
  }, [doc?.autoReview, doc?.review, startReview]);

  // ---- keyboard ----
  useHotkeys([
    ["ArrowLeft", () => !retry && go(node?.parent)],
    ["ArrowRight", () => !retry && go(node?.children[0])],
    ["ArrowUp", () => !retry && go(0)],
    ["ArrowDown", () => !retry && doc && go(lastOf(doc.tree, doc.current))],
    ["Home", () => !retry && go(0)],
    ["End", () => !retry && doc && go(lastOf(doc.tree, doc.current))],
    ["Space", () => setEngineTab(tabId, { running: !engineTab.running })],
    ["f", () => doc && update(analysisId, { orientation: doc.orientation === "white" ? "black" : "white" })],
    ["Escape", () => setRetry(null)],
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
    const doc = useAnalysis.getState().docs[analysisId];
    const node = doc?.tree.nodes[retry ? retry.nodeId : doc.current];
    if (!doc || !node) return false;
    if (retry) {
      setRetry({ ...retry, status: "checking" });
      try {
        const check = await unwrap(commands.reviewCheckMove(node.fen, uci));
        const ok = (check.winAfter ?? 0) >= (retry.move.winBefore ?? 0) - 5 || uci === retry.move.best;
        setRetry({ ...retry, status: ok ? "correct" : "wrong", tried: { san: check.san, winAfter: check.winAfter ?? 0 } });
        playSound(ok ? "correct" : "wrong");
      } catch (e) {
        notifyError("Could not check the move", e);
        setRetry({ ...retry, status: "waiting" });
      }
      return false; // keep the retry position on the board
    }
    try {
      const applied = await unwrap(commands.chessApplyMove(node.fen, uci));
      primePositionInfo(applied.position);
      playSound(soundForSan(applied.san));
      const { tree, id } = addApplied(doc.tree, doc.current, applied);
      update(analysisId, { tree, current: id });
      setExtraArrows([]);
      return true;
    } catch (e) {
      notifyError("Illegal move", e);
      return false;
    }
  }

  async function playLine(pv: string[]) {
    if (!doc || !node || pv.length === 0) return;
    let tree = doc.tree;
    let cur = doc.current;
    let fenNow = node.fen;
    for (const uci of pv.slice(0, 1)) {
      const r = await commands.chessApplyMove(fenNow, uci);
      if (r.status !== "ok") break;
      const res = addApplied(tree, cur, r.data);
      tree = res.tree;
      cur = res.id;
      fenNow = r.data.position.fen;
    }
    update(analysisId, { tree, current: cur });
  }

  async function showBetter(m: MoveReview) {
    const parent = reviewNodeOf(m.ply) !== null ? doc!.tree.nodes[reviewNodeOf(m.ply)!].parent : null;
    if (parent === null || !m.best) return;
    setRetry(null);
    update(analysisId, { current: parent });
    setExtraArrows([{ uci: m.best, brush: "green" }]);
    // Add the engine's line as a variation so it can be stepped through.
    const r = await commands.chessApplySanLine(doc!.tree.nodes[parent].fen, m.bestLine.slice(0, 8));
    if (r.status !== "ok" || r.data.length === 0) return;
    update(analysisId, (d) => {
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
    const id = reviewNodeOf(m.ply);
    if (id === null) return;
    const parent = doc!.tree.nodes[id].parent ?? 0;
    setExtraArrows([]);
    setRetry({ move: m, nodeId: parent, status: "waiting" });
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
  const engineIds = engineTab.enabled;
  const live = engineIds
    .map((e) => allLines[`${tabId}:${e}`])
    .find((u) => u && sameFen(u.fen, node.fen) && u.lines.length > 0);
  const reviewEval = doc.review && currentPly !== null && !retry ? doc.review.evals[currentPly] : undefined;
  const barWin = live?.lines[0]?.whiteWin ?? reviewEval?.whiteWin ?? null;
  const barScore = live?.lines[0]?.score ?? reviewEval?.score ?? null;
  const engineArrows: Arrow[] =
    running && live
      ? live.lines.slice(0, 3).map((l, i) => ({ uci: l.pv[0] ?? "", brush: i === 0 ? "paleBlue" : "paleGrey" }))
      : [];
  const currentReview = retry ? null : reviewOfNode(doc.current);
  const white = header(doc.tree, "White");
  const black = header(doc.tree, "Black");

  return (
    <div className="workspace">
      <div className="board-column" ref={ref}>
        <div className="board-frame">
          {settings.showEvalBar && <EvalBar whiteWin={barWin} score={barScore} orientation={doc.orientation} height={size} />}
          <Board
            fen={node.fen}
            orientation={doc.orientation}
            turn={info?.turn ?? "white"}
            dests={retry && retry.status !== "waiting" && retry.status !== "wrong" ? {} : info?.dests}
            movableColor={info?.turn}
            lastMove={retry ? null : node.uci}
            check={info?.check}
            arrows={[...extraArrows, ...engineArrows]}
            coordinates={settings.showCoordinates}
            animationMs={settings.animationMs}
            size={size}
            onMove={move}
          />
        </div>
        {doc.review && (
          <div style={{ width: size + 30, marginTop: 10 }}>
            <EvalGraph review={doc.review} currentPly={currentPly} onSelectPly={(ply) => go(ply === 0 ? 0 : reviewNodeOf(ply))} />
          </div>
        )}
      </div>

      <div className="side-panel">
        {retry && (
          <Alert
            color={retry.status === "correct" ? "green" : retry.status === "wrong" ? "orange" : "blue"}
            variant="light"
            title={retry.status === "correct" ? "Well found!" : "Find a better move"}
            withCloseButton
            onClose={() => setRetry(null)}
            closeButtonLabel="Leave retry"
          >
            {retry.status === "waiting" && <Text size="sm">You played {retry.move.san} here. What's stronger?</Text>}
            {retry.status === "checking" && <Text size="sm">Checking your move…</Text>}
            {retry.status === "correct" && retry.tried && (
              <Text size="sm">
                {retry.tried.san} keeps your winning chances at {Math.round(retry.tried.winAfter)}%.
              </Text>
            )}
            {retry.status === "wrong" && retry.tried && (
              <>
                <Text size="sm">
                  {retry.tried.san} still drops to {Math.round(retry.tried.winAfter)}%. Try again.
                </Text>
                <Group gap={6} mt={6}>
                  <Button size="xs" variant="default" onClick={() => setRetry({ ...retry, status: "waiting", tried: undefined })}>
                    Try again
                  </Button>
                  <Button size="xs" variant="subtle" onClick={() => void showBetter(retry.move)}>
                    Show the answer
                  </Button>
                </Group>
              </>
            )}
          </Alert>
        )}

        <EnginePanel tabId={tabId} fen={node.fen} onPlayLine={(pv) => void playLine(pv)} />

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
              onStart={(depth) => void startReview(depth)}
              onCancel={() => void commands.reviewCancel(analysisId)}
              onJump={(ply) => go(reviewNodeOf(ply))}
              onShowBetter={(m) => void showBetter(m)}
              onRetry={startRetry}
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
                  const r = reviewOfNode(id);
                  if (!r || r.class === "best" || r.class === "good") return null;
                  if (doc.tree.nodes[id].nags.length) return null;
                  return (
                    <span className="annot" style={{ color: CLASS_STYLE[r.class].color }}>
                      {CLASS_STYLE[r.class].symbol}
                    </span>
                  );
                }}
              />
            </div>
            {editingComment !== null && (
              <div style={{ padding: 8, borderTop: "1px solid var(--hairline)" }}>
                <Textarea
                  autosize
                  minRows={2}
                  placeholder="Comment on this move"
                  defaultValue={doc.tree.nodes[editingComment]?.comment ?? ""}
                  autoFocus
                  onBlur={(e) => {
                    const text = e.currentTarget.value.trim();
                    update(analysisId, (d) => ({ tree: updateNode(d.tree, editingComment, { comment: text || null }) }));
                    setEditingComment(null);
                  }}
                />
              </div>
            )}
            <Group justify="center" gap={4} p={6} style={{ borderTop: "1px solid var(--hairline)" }}>
              <ActionIcon variant="subtle" color="gray" onClick={() => go(0)} aria-label="Start">
                <IconChevronsLeft size={18} />
              </ActionIcon>
              <ActionIcon variant="subtle" color="gray" onClick={() => go(node.parent)} aria-label="Previous move">
                <IconChevronLeft size={18} />
              </ActionIcon>
              <ActionIcon variant="subtle" color="gray" onClick={() => go(node.children[0])} aria-label="Next move">
                <IconChevronRight size={18} />
              </ActionIcon>
              <ActionIcon variant="subtle" color="gray" onClick={() => go(lastOf(doc.tree, doc.current))} aria-label="End of line">
                <IconChevronsRight size={18} />
              </ActionIcon>
              <Tooltip label="Flip board (F)">
                <ActionIcon
                  variant="subtle"
                  color="gray"
                  onClick={() => update(analysisId, { orientation: doc.orientation === "white" ? "black" : "white" })}
                  aria-label="Flip board"
                >
                  <IconSwitchVertical size={18} />
                </ActionIcon>
              </Tooltip>
            </Group>
          </div>
        )}
      </div>

      {menu && (
        <>
          <div style={{ position: "fixed", inset: 0, zIndex: 99 }} onClick={() => setMenu(null)} onContextMenu={(e) => { e.preventDefault(); setMenu(null); }} />
          <Paper shadow="md" withBorder p={6} style={{ position: "fixed", left: menu.x, top: menu.y, zIndex: 100, minWidth: 190 }} role="menu">
            <Stack gap={2}>
              <Group gap={2} mb={4}>
                {NAGS.map((n) => (
                  <Button
                    key={n.nag}
                    size="compact-xs"
                    variant={doc.tree.nodes[menu.id].nags.includes(n.nag) ? "filled" : "default"}
                    onClick={() => update(analysisId, (d) => ({ tree: toggleNag(d.tree, menu.id, n.nag) }))}
                    aria-label={`Annotate ${n.label}`}
                  >
                    {n.label}
                  </Button>
                ))}
              </Group>
              <Button size="xs" variant="subtle" justify="flex-start" onClick={() => { setEditingComment(menu.id); setMenu(null); }}>
                Comment…
              </Button>
              <Button size="xs" variant="subtle" justify="flex-start" onClick={() => { promote(menu.id); setMenu(null); }}>
                Promote to main line
              </Button>
              <Button size="xs" variant="subtle" color="red" justify="flex-start" leftSection={<IconX size={14} />} onClick={() => { remove(menu.id); setMenu(null); }}>
                Delete from here
              </Button>
            </Stack>
          </Paper>
        </>
      )}
    </div>
  );
}

function lastOf(tree: Parameters<typeof mainline>[0], from: number): number {
  const line = mainline(tree, from);
  return line.length ? line[line.length - 1] : from;
}
