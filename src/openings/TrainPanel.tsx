import { useEffect, useRef, useState } from "react";
import { Button, Group, Progress, SegmentedControl, Stack, Text, Title } from "@mantine/core";
import { IconCheck, IconPlayerPlay, IconX } from "@tabler/icons-react";
import { Board, type Arrow } from "../board/Board";
import { commands, notifyError, unwrap, type DrillItem, type Side } from "../lib/api";
import { START_FEN } from "../lib/tree";
import { primePositionInfo, usePositionInfo } from "../lib/usePositionInfo";
import { useSettings } from "../stores/settings";

type Phase = "idle" | "replay" | "waiting" | "correct" | "wrong" | "done";

interface Props {
  size: number;
  onDone: () => void;
}

/** Spaced-repetition drill over the user's repertoire. */
export function TrainPanel({ size, onDone }: Props) {
  const settings = useSettings((s) => s.settings);
  const [which, setWhich] = useState<"both" | Side>("both");
  const [queue, setQueue] = useState<DrillItem[]>([]);
  const [index, setIndex] = useState(0);
  const [phase, setPhase] = useState<Phase>("idle");
  const [fen, setFen] = useState(START_FEN);
  const [lastMove, setLastMove] = useState<string | null>(null);
  const [mistakes, setMistakes] = useState(0);
  const [score, setScore] = useState({ right: 0, wrong: 0 });
  const requeued = useRef(new Set<string>());
  const info = usePositionInfo(fen);
  const item = queue[index];

  async function start() {
    try {
      const items = await unwrap(commands.repertoireDue(which === "both" ? null : which, 40, 10));
      if (items.length === 0) {
        notifyError("Nothing to train", "No positions are due. Build your repertoire first, or come back later.");
        return;
      }
      requeued.current.clear();
      setQueue(items);
      setIndex(0);
      setScore({ right: 0, wrong: 0 });
      setPhase("replay");
    } catch (e) {
      notifyError("Could not start training", e);
    }
  }

  // Replay the line up to the drill position.
  useEffect(() => {
    if (phase !== "replay" || !item) return;
    let cancelled = false;
    setMistakes(0);
    (async () => {
      let current = START_FEN;
      setFen(current);
      setLastMove(null);
      for (const uci of item.line) {
        await new Promise((r) => setTimeout(r, 280));
        if (cancelled) return;
        const r = await commands.chessApplyMove(current, uci);
        if (r.status !== "ok") break;
        primePositionInfo(r.data.position);
        current = r.data.position.fen;
        setFen(current);
        setLastMove(r.data.uci);
      }
      if (!cancelled) setPhase("waiting");
    })();
    return () => {
      cancelled = true;
    };
  }, [phase, item]);

  async function next() {
    if (index + 1 >= queue.length) {
      setPhase("done");
      onDone();
      return;
    }
    setIndex(index + 1);
    setPhase("replay");
  }

  async function onMove(uci: string): Promise<boolean> {
    if (!item || (phase !== "waiting" && phase !== "wrong")) return false;
    const r = await commands.chessApplyMove(fen, uci);
    if (r.status !== "ok") return false;
    const ok = item.expected.some((e) => e.uci === r.data.uci);
    if (ok) {
      const firstTry = phase === "waiting";
      setFen(r.data.position.fen);
      setLastMove(r.data.uci);
      setPhase("correct");
      // After a mistake the card was already graded "again"; playing the shown move just continues.
      if (firstTry) {
        await commands.repertoireGrade(item.color, item.key, "good");
        setScore((s) => ({ ...s, right: s.right + 1 }));
      }
      setTimeout(() => void next(), 650);
      return true;
    }
    setMistakes((m) => m + 1);
    setPhase("wrong");
    if (mistakes === 0) {
      await commands.repertoireGrade(item.color, item.key, "again");
      setScore((s) => ({ ...s, wrong: s.wrong + 1 }));
      // Show it once more at the end of the session.
      if (!requeued.current.has(item.key)) {
        requeued.current.add(item.key);
        setQueue((q) => [...q, item]);
      }
    }
    return false;
  }

  if (phase === "idle" || phase === "done") {
    return (
      <Stack align="center" justify="center" style={{ width: size, height: size }} gap="sm">
        <Title order={3}>{phase === "done" ? "Session complete" : "Train your repertoire"}</Title>
        {phase === "done" && (
          <Text c="dimmed">
            {score.right} right · {score.wrong} to review again soon
          </Text>
        )}
        <SegmentedControl
          value={which}
          onChange={(v) => setWhich(v as "both" | Side)}
          data={[
            { value: "both", label: "Both colours" },
            { value: "white", label: "White" },
            { value: "black", label: "Black" },
          ]}
        />
        <Button leftSection={<IconPlayerPlay size={16} />} onClick={() => void start()}>
          {phase === "done" ? "Train again" : "Start"}
        </Button>
      </Stack>
    );
  }

  const arrows: Arrow[] = phase === "wrong" && item ? item.expected.map((e) => ({ uci: e.uci, brush: "green" })) : [];
  return (
    <div>
      <Group justify="space-between" style={{ width: size }} mb={6}>
        <Text size="sm" fw={600} aria-live="polite">
          {phase === "replay" && "Watch the line…"}
          {phase === "waiting" && (item?.isNew ? "New position: what's your move?" : "Your move")}
          {phase === "correct" && (
            <Group gap={4} c="green">
              <IconCheck size={16} /> Correct
            </Group>
          )}
          {phase === "wrong" && (
            <Group gap={4} c="orange">
              <IconX size={16} /> Your repertoire plays {item?.expected.map((e) => e.san).join(" or ")}
            </Group>
          )}
        </Text>
        <Text size="xs" c="dimmed">
          {Math.min(index + 1, queue.length)} / {queue.length}
        </Text>
      </Group>
      <Progress value={(index / Math.max(1, queue.length)) * 100} size="xs" mb={6} style={{ width: size }} />
      <Board
        fen={fen}
        orientation={item?.color ?? "white"}
        turn={info?.turn ?? "white"}
        dests={phase === "waiting" || phase === "wrong" ? info?.dests : {}}
        movableColor={item?.color}
        lastMove={lastMove}
        check={info?.check}
        arrows={arrows}
        coordinates={settings.showCoordinates}
        animationMs={settings.animationMs}
        size={size}
        onMove={onMove}
      />
      {phase === "wrong" && (
        <Group mt={8} gap={6}>
          <Text size="xs" c="dimmed">
            Play the green move to continue, or
          </Text>
          <Button size="compact-xs" variant="default" onClick={() => void next()}>
            Skip
          </Button>
        </Group>
      )}
    </div>
  );
}
