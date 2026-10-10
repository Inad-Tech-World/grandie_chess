import { useCallback, useEffect, useMemo, useRef } from "react";
import { ActionIcon, Alert, Button, Group, Loader, SimpleGrid, Stack, Text, Tooltip } from "@mantine/core";
import { modals } from "@mantine/modals";
import { useHotkeys } from "@mantine/hooks";
import {
  IconArrowBackUp,
  IconBulb,
  IconChevronLeft,
  IconChevronRight,
  IconChevronsLeft,
  IconChevronsRight,
  IconDeviceFloppy,
  IconFlag,
  IconHeartHandshake,
  IconMicroscope,
  IconRefresh,
  IconSwitchVertical,
} from "@tabler/icons-react";
import { notifications } from "@mantine/notifications";
import { Board, useBoardSize, type Arrow } from "../board/Board";
import { PlayerStrip } from "../board/PlayerStrip";
import { MoveList } from "../moves/MoveList";
import { commands, notifyError, tryCommand, unwrap } from "../lib/api";
import { describeOutcome, formatTimeControl } from "../lib/format";
import { usePlay } from "../stores/play";
import { useSettings } from "../stores/settings";
import { grandieGameId, openAnalysisFromPgn, savePgnText } from "../lib/actions";
import { playSound, soundForSan } from "../lib/sounds";
import { useApp, newId } from "../stores/app";

export function PlayScreen({ sessionId }: { sessionId: number }) {
  const snap = usePlay((s) => s.snapshots[sessionId]);
  const clock = usePlay((s) => s.clocks[sessionId]);
  const view = usePlay((s) => s.views[sessionId]);
  const setView = usePlay((s) => s.setView);
  const settings = useSettings((s) => s.settings);
  const { ref, size } = useBoardSize(108, 0);

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

  useHotkeys([
    ["ArrowLeft", () => step(shownPly - 1)],
    ["ArrowRight", () => step(shownPly + 1)],
    ["ArrowUp", () => step(0)],
    ["ArrowDown", () => step(total)],
    ["Home", () => step(0)],
    ["End", () => step(total)],
    ["f", () => setView(sessionId, { orientation: orientation === "white" ? "black" : "white" })],
  ]);

  const outcomeReason = snap?.outcome?.reason;
  // Sounds for each new move and for the end of the game.
  const moveCount = snap?.moves.length ?? 0;
  const lastSan = snap?.moves[moveCount - 1]?.san;
  const heard = useRef<number | null>(null);
  useEffect(() => {
    if (heard.current !== null && moveCount > heard.current) playSound(soundForSan(lastSan));
    heard.current = moveCount;
  }, [moveCount, lastSan]);
  useEffect(() => {
    if (outcomeReason && outcomeReason !== "aborted") playSound("end");
  }, [outcomeReason]);

  // When a game ends, open its review automatically (once).
  const reviewOpened = useRef(false);
  useEffect(() => {
    if (!snap || !outcomeReason || outcomeReason === "aborted" || reviewOpened.current || snap.moves.length < 4) return;
    reviewOpened.current = true;
    const t = setTimeout(() => void review(), 900);
    return () => clearTimeout(t);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [outcomeReason]);

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
    try {
      const s = await unwrap(commands.playMove(sessionId, uci));
      usePlay.getState().setSnapshot(s);
      return true;
    } catch (e) {
      notifyError("Move not played", e);
      return false;
    }
  }

  async function hint() {
    const best = await tryCommand(commands.playHint(sessionId), "No hint available");
    if (!best) return;
  }

  async function takeback() {
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

  async function review() {
    reviewOpened.current = true;
    const pgn = await tryCommand(commands.playPgn(sessionId));
    const gameId = await grandieGameId(snap!.pgnPath);
    if (pgn) await openAnalysisFromPgn(pgn, `Review: ${snap!.whiteName} – ${snap!.blackName}`, { autoReview: true, atStart: true, gameId });
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
          fen={fen}
          orientation={orientation}
          turn={snap.position.turn}
          dests={live ? snap.position.dests : undefined}
          movableColor={live && !finished ? snap.userSide : undefined}
          lastMove={lastMove}
          check={live && snap.position.check}
          arrows={arrows}
          premove={live && !finished}
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
        </div>

        <div className="panel" style={{ flex: 1, minHeight: 0, display: "flex", flexDirection: "column" }}>
          <div style={{ flex: 1, minHeight: 0, overflow: "auto", padding: "6px 4px" }}>
            <MoveList moves={moves} current={shownPly - 1} onSelect={(i) => step(i + 1)} />
          </div>
          <Group justify="center" gap={4} p={6} style={{ borderTop: "1px solid var(--hairline)" }}>
            <ActionIcon variant="subtle" color="gray" onClick={() => step(0)} aria-label="First move">
              <IconChevronsLeft size={18} />
            </ActionIcon>
            <ActionIcon variant="subtle" color="gray" onClick={() => step(shownPly - 1)} aria-label="Previous move">
              <IconChevronLeft size={18} />
            </ActionIcon>
            <ActionIcon variant="subtle" color="gray" onClick={() => step(shownPly + 1)} aria-label="Next move">
              <IconChevronRight size={18} />
            </ActionIcon>
            <ActionIcon variant="subtle" color="gray" onClick={() => step(total)} aria-label="Last move">
              <IconChevronsRight size={18} />
            </ActionIcon>
            <Tooltip label="Flip board (F)">
              <ActionIcon variant="subtle" color="gray" onClick={() => setView(sessionId, { orientation: orientation === "white" ? "black" : "white" })} aria-label="Flip board">
                <IconSwitchVertical size={18} />
              </ActionIcon>
            </Tooltip>
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
