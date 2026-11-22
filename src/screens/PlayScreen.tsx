import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Alert, Button, Group, Loader, SimpleGrid, Stack, Text, Tooltip } from "@mantine/core";
import { modals } from "@mantine/modals";
import { IconArrowBackUp, IconBulb, IconDeviceFloppy, IconFlag, IconHeartHandshake, IconMicroscope, IconRefresh } from "@tabler/icons-react";
import { notifications } from "@mantine/notifications";
import { Board, useBoardSize, type Arrow } from "../board/Board";
import { BoardNavButtons, useBoardNav } from "../board/BoardNav";
import { PlayerStrip } from "../board/PlayerStrip";
import { MoveList } from "../moves/MoveList";
import { commands, notifyError, tryCommand, unwrap } from "../lib/api";
import { describeOutcome, formatTimeControl } from "../lib/format";
import { usePlay } from "../stores/play";
import { useSettings } from "../stores/settings";
import { grandieGameId, openAnalysisFromPgn, savePgnText } from "../lib/actions";
import { playSound, soundForSan } from "../lib/sounds";
import { useApp, newId } from "../stores/app";
import { useCoach } from "../stores/coach";
import { CoachPanel } from "../coach/CoachPanel";
import { useCoachBoard } from "../coach/useCoachBoard";
import { announce, sanToSpeech } from "../coach/speech";
import { CLASS_STYLE } from "../lib/score";
import type { CoachVerdict, VoiceCommand } from "../bindings";

export function PlayScreen({ sessionId }: { sessionId: number }) {
  const snap = usePlay((s) => s.snapshots[sessionId]);
  const clock = usePlay((s) => s.clocks[sessionId]);
  const view = usePlay((s) => s.views[sessionId]);
  const setView = usePlay((s) => s.setView);
  const settings = useSettings((s) => s.settings);
  const { ref, size } = useBoardSize(108, 0);
  const coachOwner = `play:${sessionId}`;
  const coachBoard = useCoachBoard(coachOwner);
  const coachMode = usePlay((s) => s.coachModes[sessionId] ?? "off");
  const [verdict, setVerdict] = useState<{ verdict: CoachVerdict; san: string } | null>(null);
  // Bumped by every coach check and takeback: a verdict that arrives after
  // the move was taken back (or after a newer move) is dropped.
  const coachCheckId = useRef(0);

  useEffect(() => {
    if (!snap) {
      void tryCommand(commands.playSnapshot(sessionId), "This game is no longer running").then((s) => s && usePlay.getState().setSnapshot(s));
    }
  }, [snap, sessionId]);

  const total = snap?.moves.length ?? 0;
  const viewPly = view?.viewPly ?? null;
  const shownPly = viewPly ?? total;
  const live = viewPly === null || viewPly === total;
  const orientation = view?.orientation ?? snap?.userSide ?? "white";

  const step = useCallback(
    (to: number) => {
      const clamped = Math.max(0, Math.min(total, to));
      setView(sessionId, { viewPly: clamped === total ? null : clamped });
    },
    [sessionId, total, setView],
  );

  const nav = {
    start: () => step(0),
    back: () => step(shownPly - 1),
    forward: () => step(shownPly + 1),
    end: () => step(total),
    flip: () => setView(sessionId, { orientation: orientation === "white" ? "black" : "white" }),
  };
  useBoardNav(nav);

  const outcomeReason = snap?.outcome?.reason;
  // Sounds for each new move and for the end of the game.
  const moveCount = snap?.moves.length ?? 0;
  const lastSan = snap?.moves[moveCount - 1]?.san;
  const heard = useRef<number | null>(null);
  const lastMover = moveCount === 0 || !snap ? null : snap.position.turn === "white" ? "black" : "white";
  const opponentMoved = !!snap && lastMover !== null && lastMover !== snap.userSide;
  const announceMoves = settings.coach.announceMoves;
  useEffect(() => {
    if (heard.current !== null && moveCount > heard.current) {
      playSound(soundForSan(lastSan));
      // Say the opponent's moves out loud, if asked to.
      if (announceMoves && opponentMoved && lastSan) announce(sanToSpeech(lastSan));
    }
    heard.current = moveCount;
  }, [moveCount, lastSan, announceMoves, opponentMoved]);
  // The end sound and the automatic review happen once per game, not each
  // time the tab is shown again (the flags live in the store for that).
  const flags = usePlay((s) => s.flags[sessionId]);
  useEffect(() => {
    if (!outcomeReason || outcomeReason === "aborted" || usePlay.getState().flags[sessionId]?.endSoundPlayed) return;
    usePlay.getState().setFlag(sessionId, "endSoundPlayed");
    playSound("end");
  }, [outcomeReason, sessionId]);

  const finishedLongEnough = !!outcomeReason && outcomeReason !== "aborted" && moveCount >= 4;
  const autoReviewDue = finishedLongEnough && !flags?.reviewOpened;
  const review = useCallback(async () => {
    const s = usePlay.getState().snapshots[sessionId];
    if (!s) return;
    usePlay.getState().setFlag(sessionId, "reviewOpened");
    const pgn = await tryCommand(commands.playPgn(sessionId));
    const gameId = await grandieGameId(s.pgnPath);
    if (pgn) await openAnalysisFromPgn(pgn, `Review: ${s.whiteName} – ${s.blackName}`, { autoReview: true, atStart: true, gameId });
  }, [sessionId]);
  useEffect(() => {
    if (!autoReviewDue) return;
    const t = setTimeout(() => void review(), 900);
    return () => clearTimeout(t);
  }, [autoReviewDue, review]);

  const moves = useMemo(
    () =>
      (snap?.moves ?? []).map((m, i, all) => ({
        san: m.san,
        fenBefore: i === 0 ? snap!.startFen : all[i - 1].fen,
      })),
    [snap],
  );

  if (!snap) {
    return (
      <Group justify="center" h="100%">
        <Loader />
      </Group>
    );
  }

  const fen = shownPly === total ? snap.position.fen : shownPly === 0 ? snap.startFen : snap.moves[shownPly - 1].fen;
  const lastMove = shownPly === 0 ? null : snap.moves[shownPly - 1].uci;
  const userTurn = snap.position.turn === snap.userSide && !snap.outcome;
  const arrows: Arrow[] = live && snap.hint ? [{ uci: snap.hint, brush: "paleBlue" }] : [];
  const finished = !!snap.outcome;
  const engineSide = snap.userSide === "white" ? "black" : "white";

  const top = orientation === "white" ? "black" : "white";
  const bottom = orientation;
  const strip = (side: "white" | "black") => {
    const isEngine = side === engineSide;
    const ms = clock ? (side === "white" ? clock.whiteMs : clock.blackMs) : null;
    return (
      <PlayerStrip
        width={size}
        name={side === "white" ? snap.whiteName : snap.blackName}
        rating={isEngine ? snap.level.elo : null}
        isEngine={isEngine}
        thinking={isEngine && snap.engineThinking}
        clockMs={snap.setup.timeControl ? ms : null}
        clockRunning={!finished && clock?.running === side}
      />
    );
  };

  async function move(uci: string): Promise<boolean> {
    const before = usePlay.getState().snapshots[sessionId];
    try {
      const s = await unwrap(commands.playMove(sessionId, uci));
      usePlay.getState().setSnapshot(s);
      if (coachMode !== "off" && before) void coachCheck(before.position.fen, before.lastMove, uci, s.moves[s.moves.length - 1]?.san ?? uci);
      return true;
    } catch (e) {
      notifyError("Move not played", e);
      return false;
    }
  }

  /** The coach looks at the move just played; mistakes are shown on the board. */
  async function coachCheck(fenBefore: string, lastMoveBefore: string | null, uci: string, san: string) {
    const coach = useCoach.getState();
    const id = ++coachCheckId.current;
    setVerdict(null);
    coach.setLoading(coachOwner, `Looking at ${san}…`);
    const r = await commands.coachCheckMove(fenBefore, uci, lastMoveBefore, coachMode === "every", null);
    if (id !== coachCheckId.current) return;
    coach.setLoading(coachOwner, null);
    if (r.status === "error") {
      notifyError("The coach couldn't check the move", r.error);
      return;
    }
    setVerdict({ verdict: r.data, san });
    if (r.data.script) coach.play(r.data.script, coachOwner);
  }

  function voiceCommand(c: VoiceCommand): boolean {
    switch (c) {
      case "takeBack":
        void takeback();
        return true;
      case "hint":
        void hint();
        return true;
      case "flip":
        nav.flip();
        return true;
      case "resign":
        resign();
        return true;
      case "offerDraw":
        void offerDraw();
        return true;
      case "next":
        nav.forward();
        return true;
      case "previous":
        nav.back();
        return true;
      default:
        return false;
    }
  }

  // The hint arrives as part of the next snapshot.
  async function hint() {
    await tryCommand(commands.playHint(sessionId), "No hint available");
  }

  async function takeback() {
    coachCheckId.current++;
    useCoach.getState().setLoading(coachOwner, null);
    useCoach.getState().stop();
    setVerdict(null);
    const s = await tryCommand(commands.playTakeback(sessionId), "Takeback refused");
    if (s) usePlay.getState().setSnapshot(s);
  }

  async function offerDraw() {
    const accepted = await tryCommand(commands.playOfferDraw(sessionId), "Draw offer failed");
    if (accepted === false) {
      notifications.show({ title: "Draw declined", message: `${snap!.level.name} wants to play on.`, color: "steel" });
    }
  }

  function resign() {
    modals.openConfirmModal({
      title: "Resign this game?",
      children: <Text size="sm">The game is saved and you can review it afterwards.</Text>,
      labels: { confirm: "Resign", cancel: "Keep playing" },
      confirmProps: { color: "red" },
      onConfirm: () => void tryCommand(commands.playResign(sessionId)).then((s) => s && usePlay.getState().setSnapshot(s)),
    });
  }

  async function save() {
    const pgn = await tryCommand(commands.playPgn(sessionId));
    if (pgn) await savePgnText(pgn, `${snap!.whiteName} vs ${snap!.blackName}.pgn`);
  }

  function rematch() {
    useApp.getState().openTab({ id: newId("tab"), kind: "new-game", title: "New game" });
  }

  const status = snap.outcome
    ? describeOutcome(snap.outcome, snap.userSide)
    : snap.engineThinking
      ? `${snap.level.name} is thinking…`
      : userTurn
        ? "Your move"
        : "Waiting…";

  return (
    <div className="workspace">
      <div className="board-column" ref={ref}>
        {strip(top)}
        <Board
          fen={coachBoard.fen ?? fen}
          orientation={orientation}
          turn={snap.position.turn}
          dests={live && !coachBoard.active ? snap.position.dests : undefined}
          movableColor={live && !finished && !coachBoard.active ? snap.userSide : undefined}
          lastMove={coachBoard.active ? coachBoard.lastMove : lastMove}
          check={!coachBoard.active && live && snap.position.check}
          arrows={coachBoard.active ? coachBoard.arrows : arrows}
          circles={coachBoard.circles}
          coaching={coachBoard.active}
          premove={live && !finished && !coachBoard.active}
          coordinates={settings.showCoordinates}
          animationMs={settings.animationMs}
          size={size}
          onMove={move}
        />
        {strip(bottom)}
      </div>

      <div className="side-panel">
        <div className="panel" style={{ padding: 14 }}>
          <Group justify="space-between" mb={6}>
            <Text fw={650}>{snap.level.name}</Text>
            <Text size="xs" c="dimmed">
              {formatTimeControl(snap.setup.timeControl)}
            </Text>
          </Group>
          <Text size="sm" c={snap.outcome ? undefined : "dimmed"} fw={snap.outcome ? 600 : 400} aria-live="polite">
            {status}
          </Text>
          {snap.error && (
            <Alert color="red" mt="sm" variant="light" title="Engine problem">
              {snap.error}
            </Alert>
          )}
          {verdict && !finished && (
            <Group gap={6} mt={8} aria-label="Coach verdict">
              <span className="annot" style={{ color: CLASS_STYLE[verdict.verdict.class].color }}>
                {CLASS_STYLE[verdict.verdict.class].symbol}
              </span>
              <Text size="sm">
                {verdict.san}: {CLASS_STYLE[verdict.verdict.class].label.toLowerCase()}
                {verdict.verdict.bestSan && verdict.verdict.class !== "best" && verdict.verdict.class !== "brilliant" && verdict.verdict.class !== "great" ? ` · ${verdict.verdict.bestSan} was best` : ""}
              </Text>
              {verdict.verdict.script && snap.canTakeback && (
                <Button size="compact-xs" variant="light" leftSection={<IconArrowBackUp size={12} />} onClick={() => void takeback()}>
                  Take back and try again
                </Button>
              )}
            </Group>
          )}
        </div>

        <CoachPanel
          owner={coachOwner}
          fen={() => (userTurn && live ? snap.position.fen : null)}
          context={() => ({
            fen: snap.position.fen,
            recentMoves: snap.moves.slice(-10).map((m) => m.san),
            game: `your game against ${snap.level.name}`,
            player: null,
          })}
          onVoiceMove={(uci) => void move(uci)}
          onVoiceCommand={voiceCommand}
        />

        <div className="panel" style={{ flex: 1, minHeight: 0, display: "flex", flexDirection: "column" }}>
          <div style={{ flex: 1, minHeight: 0, overflow: "auto", padding: "6px 4px" }}>
            <MoveList moves={moves} current={shownPly - 1} onSelect={(i) => step(i + 1)} />
          </div>
          <Group justify="center" gap={4} p={6} style={{ borderTop: "1px solid var(--hairline)" }}>
            <BoardNavButtons nav={nav} labels={{ start: "First move", back: "Previous move", forward: "Next move", end: "Last move" }} />
          </Group>
        </div>

        {!finished ? (
          <SimpleGrid cols={2} spacing={8}>
            <Tooltip label="Show a suggestion from full-strength Leela">
              <Button variant="default" leftSection={<IconBulb size={16} />} disabled={!userTurn || !live} onClick={() => void hint()}>
                Hint
              </Button>
            </Tooltip>
            <Button variant="default" leftSection={<IconArrowBackUp size={16} />} disabled={!snap.canTakeback} onClick={() => void takeback()}>
              Takeback
            </Button>
            <Button variant="default" leftSection={<IconHeartHandshake size={16} />} disabled={!snap.canOfferDraw} onClick={() => void offerDraw()}>
              Draw
            </Button>
            <Button variant="default" color="red" leftSection={<IconFlag size={16} />} onClick={resign}>
              Resign
            </Button>
          </SimpleGrid>
        ) : (
          <Stack gap={8}>
            <Group grow gap={8}>
              <Button leftSection={<IconMicroscope size={16} />} onClick={() => void review()}>
                Review game
              </Button>
              <Button variant="default" leftSection={<IconRefresh size={16} />} onClick={rematch}>
                New game
              </Button>
            </Group>
            <Button variant="subtle" leftSection={<IconDeviceFloppy size={16} />} onClick={() => void save()}>
              Save PGN…
            </Button>
            {snap.pgnPath && (
              <Text size="xs" c="dimmed" ta="center">
                Saved automatically to Grandie's games folder.
              </Text>
            )}
          </Stack>
        )}
      </div>
    </div>
  );
}
