// "Learn from your mistakes": positions from your own reviewed games where
// you went wrong. Find the better move; the coach shows why the original
// move failed. Cards come back on a spaced-repetition schedule until the
// right idea sticks.

import { useCallback, useEffect, useState, type ReactNode } from "react";
import { Alert, Button, Group, Stack, Text } from "@mantine/core";
import { IconCheck, IconPlayerTrackNext, IconSchool, IconX } from "@tabler/icons-react";
import { Board, useBoardSize } from "../board/Board";
import { commands, errorKind, notifyError, unwrap, type Grade, type MistakeCard, type MistakeStats } from "../lib/api";
import { usePositionInfo } from "../lib/usePositionInfo";
import { playSound } from "../lib/sounds";
import { useSettings } from "../stores/settings";
import { useCoach } from "../stores/coach";
import { CoachPanel } from "./CoachPanel";
import { useCoachBoard } from "./useCoachBoard";

const OWNER = "mistakes";

type Status = "loading" | "empty" | "solving" | "checking" | "correct" | "wrong";

export function MistakesTrainer({ modeSwitch }: { modeSwitch?: ReactNode }) {
  const settings = useSettings((s) => s.settings);
  const { ref, size } = useBoardSize(40, 0);
  const coachBoard = useCoachBoard(OWNER);
  const [cards, setCards] = useState<MistakeCard[]>([]);
  const [stats, setStats] = useState<MistakeStats | null>(null);
  const [status, setStatus] = useState<Status>("loading");
  const [message, setMessage] = useState<string | null>(null);
  const [tries, setTries] = useState(0);
  const card = cards[0];
  const pos = usePositionInfo(card?.fen);

  const load = useCallback(async () => {
    setStatus("loading");
    try {
      const [due, st] = await Promise.all([unwrap(commands.mistakesDue(20)), unwrap(commands.mistakesStats())]);
      setCards(due);
      setStats(st);
      setTries(0);
      setMessage(null);
      setStatus(due.length ? "solving" : "empty");
    } catch (e) {
      notifyError("Could not load your mistakes", e);
      setStatus("empty");
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  async function grade(g: Grade) {
    if (!card) return;
    const r = await commands.mistakesGrade(card.id, g);
    if (r.status === "error") notifyError("The result wasn't saved", r.error);
  }

  async function explain(c: MistakeCard) {
    const coach = useCoach.getState();
    coach.setLoading(OWNER, "Showing why it went wrong…");
    const r = await commands.coachExplainMove({
      fenBefore: c.fen,
      lastMoveBefore: null,
      played: c.played,
      known: c.bestLine.length ? { bestLine: c.bestLine, replyLine: c.refutation, winBefore: c.winBefore ?? 50, winAfter: c.winAfter ?? 40, class: "mistake" } : null,
      returnFen: c.fen,
      returnLastMove: null,
      coachId: null,
    });
    coach.setLoading(OWNER, null);
    if (r.status === "ok") coach.play(r.data, OWNER);
    else if (errorKind(r.error) !== "cancelled") notifyError("The coach couldn't explain this", r.error);
  }

  async function onMove(uci: string): Promise<boolean> {
    if (!card || (status !== "solving" && status !== "wrong")) return false;
    if (uci === card.best) {
      playSound("correct");
      setStatus("correct");
      setMessage(`${card.bestSan} is right${tries ? "" : " first time"}.`);
      void grade(tries ? "hard" : "good");
      return false;
    }
    // Another move may be just as good: ask the coach's engine.
    setStatus("checking");
    const r = await commands.coachCheckMove(card.fen, uci, null, false, null);
    if (r.status === "error") {
      notifyError("Couldn't check the move", r.error);
      setStatus("solving");
      return false;
    }
    const v = r.data;
    if (!["inaccuracy", "mistake", "blunder"].includes(v.class)) {
      playSound("correct");
      setStatus("correct");
      setMessage(`That works too. The engine's choice was ${card.bestSan}.`);
      void grade(tries ? "hard" : "good");
      return false;
    }
    playSound("wrong");
    setTries((t) => t + 1);
    setStatus("wrong");
    setMessage(`Not quite: that's ${v.class === "inaccuracy" ? "an inaccuracy" : `a ${v.class}`}. Try again, or let the coach show you.`);
    if (tries === 0) void grade("again");
    if (v.script) useCoach.getState().play(v.script, OWNER);
    return false;
  }

  function next() {
    useCoach.getState().stop();
    setCards((c) => c.slice(1));
    setTries(0);
    setMessage(null);
    setStatus(cards.length > 1 ? "solving" : "empty");
    if (cards.length <= 1) void load();
  }

  if (status === "empty") {
    return (
      <div className="screen" style={{ maxWidth: 640 }}>
        {modeSwitch && <div style={{ marginBottom: 12 }}>{modeSwitch}</div>}
        <Alert variant="light" color="teal" icon={<IconSchool size={18} />} title="No mistakes to practise right now">
          {stats?.total
            ? `All ${stats.total} positions are scheduled for later. Come back tomorrow.`
            : "Review your games (Play a game, or open one from the Database and press Review): the positions where you went wrong are collected here, and come back until you find the better move."}
        </Alert>
      </div>
    );
  }

  return (
    <div className="workspace">
      <div className="board-column" ref={ref}>
        {card && (
          <Board
            fen={coachBoard.fen ?? card.fen}
            orientation={card.side}
            turn={pos?.turn ?? card.side}
            dests={coachBoard.active || status === "correct" || status === "checking" ? {} : pos?.dests}
            movableColor={coachBoard.active ? undefined : card.side}
            lastMove={coachBoard.active ? coachBoard.lastMove : null}
            check={!coachBoard.active && pos?.check}
            arrows={coachBoard.arrows}
            circles={coachBoard.circles}
            coaching={coachBoard.active}
            coordinates={settings.showCoordinates}
            animationMs={settings.animationMs}
            size={size}
            onMove={onMove}
          />
        )}
      </div>
      <div className="side-panel">
        {modeSwitch}
        <div className="panel" style={{ padding: 14 }}>
          <Text fw={650}>Learn from your mistakes</Text>
          <Text size="xs" c="dimmed">
            {stats ? `${stats.due} due · ${stats.total} collected from your reviewed games` : ""}
          </Text>
        </div>
        {card && (
          <Alert
            variant="light"
            color={status === "correct" ? "green" : status === "wrong" ? "orange" : "blue"}
            icon={status === "correct" ? <IconCheck size={18} /> : status === "wrong" ? <IconX size={18} /> : undefined}
            aria-live="polite"
          >
            <Stack gap={4}>
              <Text size="sm">
                In your game you played <b>{card.playedSan}</b> here, and it cost you. Find a better move for {card.side}.
              </Text>
              {status === "checking" && <Text size="sm">Checking your move…</Text>}
              {message && <Text size="sm">{message}</Text>}
            </Stack>
          </Alert>
        )}
        {card && (
          <Group gap={8} grow>
            <Button variant="light" color="teal" leftSection={<IconSchool size={16} />} onClick={() => void explain(card)}>
              Why was {card.playedSan} wrong?
            </Button>
            <Button leftSection={<IconPlayerTrackNext size={16} />} onClick={next} disabled={status === "checking"}>
              {status === "correct" ? "Next" : "Skip"}
            </Button>
          </Group>
        )}
        <CoachPanel
          owner={OWNER}
          fen={() => (status === "solving" || status === "wrong" ? card?.fen ?? null : null)}
          context={() => ({ fen: card?.fen ?? "", recentMoves: [], game: "a position from one of your games", player: null })}
          onVoiceMove={(uci) => void onMove(uci)}
        />
      </div>
    </div>
  );
}
