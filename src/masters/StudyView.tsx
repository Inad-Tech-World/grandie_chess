// Studying one master game, two ways:
// - Guided replay: the coach plays through the game and stops at the moments
//   that show the master's ideas, demonstrating alternatives on the board.
// - Guess the move: you play the master's side; each guess is scored against
//   the master's move (3 points for the same move, 2 for one the engine rates
//   as just as good) and the coach shows the master's idea when you differ.

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { notifications } from "@mantine/notifications";
import { ActionIcon, Alert, Badge, Button, Group, Loader, Paper, Progress, SegmentedControl, Stack, Text, Title } from "@mantine/core";
import { IconArrowLeft, IconBook2, IconChevronLeft, IconChevronRight, IconQuestionMark, IconSchool } from "@tabler/icons-react";
import { Board, useBoardSize } from "../board/Board";
import { CoachPanel } from "../coach/CoachPanel";
import { useCoachBoard } from "../coach/useCoachBoard";
import { commands, errorKind, events, notifyError, unwrap, type GuessResult, type Side, type VoiceCommand } from "../lib/api";
import { playSound, soundForSan } from "../lib/sounds";
import { mainline } from "../lib/tree";
import { usePositionInfo } from "../lib/usePositionInfo";
import { useCoach } from "../stores/coach";
import { useSettings } from "../stores/settings";
import { shortName, useMasters, type Study, type StudyMode } from "./store";

export const OWNER = "masters";

interface Line {
  fens: string[];
  ucis: string[];
  sans: string[];
  white: string | null;
  black: string | null;
}

type GuessPhase = "setup" | "playing" | "checking" | "result" | "done";

interface Score {
  points: number;
  max: number;
  exact: number;
  guesses: number;
}

const NO_SCORE: Score = { points: 0, max: 0, exact: 0, guesses: 0 };

const sideToMove = (fen: string): Side => (fen.split(" ")[1] === "b" ? "black" : "white");

export function StudyView({ study }: { study: Study }) {
  const closeStudy = useMasters((s) => s.closeStudy);
  const openStudy = useMasters((s) => s.openStudy);
  const settings = useSettings((s) => s.settings);
  const { ref, size } = useBoardSize(40, 0);
  const coachBoard = useCoachBoard(OWNER);
  const coachStatus = useCoach((s) => (s.owner === OWNER ? s.status : "idle"));
  const [line, setLine] = useState<Line | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [ply, setPly] = useState(0);
  const mode: StudyMode = study.mode;

  // Guess the move.
  const [side, setSide] = useState<Side>(study.masterColor ?? "white");
  const [startAt, setStartAt] = useState<"start" | "middle">("middle");
  const [phase, setPhase] = useState<GuessPhase>("setup");
  const [score, setScore] = useState<Score>(NO_SCORE);
  const [result, setResult] = useState<GuessResult | null>(null);

  const total = line ? line.ucis.length : 0;
  const fen = line ? line.fens[ply] : "";
  const info = usePositionInfo(fen || undefined);
  const master = study.masterColor ?? side;
  const masterShort = shortName(study.masterName);

  // Load the game (again when switching between replay and guessing).
  const replayStarted = useRef<number | null>(null);
  useEffect(() => {
    let live = true;
    useCoach.getState().stop();
    if (study.mode !== "replay") replayStarted.current = null;
    setLine(null);
    setError(null);
    void commands.dbGame(study.gameId).then((r) => {
      if (!live) return;
      if (r.status === "error") {
        setError(r.error.message);
        return;
      }
      const tree = r.data.tree;
      const ids = mainline(tree);
      setLine({
        fens: [tree.nodes[0].fen, ...ids.map((i) => tree.nodes[i].fen)],
        ucis: ids.map((i) => tree.nodes[i].uci ?? ""),
        sans: ids.map((i) => tree.nodes[i].san ?? ""),
        white: r.data.row.white,
        black: r.data.row.black,
      });
      setPly(study.mode === "replay" ? ids.length : 0);
      setPhase("setup");
      setScore(NO_SCORE);
      setResult(null);
      setStartAt(ids.length >= 24 ? "middle" : "start");
    });
    return () => {
      live = false;
    };
  }, [study.gameId, study.mode]);

  // A move sound whenever the game advances on its own (not the coach's demos).
  const lastPly = useRef(ply);
  useEffect(() => {
    if (line && ply === lastPly.current + 1 && !coachBoard.active) playSound(soundForSan(line.sans[ply - 1]));
    lastPly.current = ply;
  }, [ply, line, coachBoard.active]);

  // ---- guided replay ----
  const startReplay = useCallback(async () => {
    const coach = useCoach.getState();
    const reviewId = `walk:${study.gameId}`;
    coach.setLoading(OWNER, "Preparing the replay…");
    const unlisten = await events.reviewProgress.listen((e) => {
      if (e.payload.reviewId === reviewId) {
        const pct = Math.round((e.payload.done / Math.max(1, e.payload.total)) * 100);
        useCoach.getState().setLoading(OWNER, `Reviewing the game with the engine… ${pct}%`);
      }
    });
    try {
      const script = await unwrap(commands.mastersWalkthrough({ gameId: study.gameId, subject: study.subject, coachId: null }));
      useCoach.getState().setLoading(OWNER, null);
      useCoach.getState().play(script, OWNER);
    } catch (e) {
      useCoach.getState().setLoading(OWNER, null);
      if (errorKind(e) !== "cancelled") notifyError("The replay couldn't be prepared", e);
    } finally {
      unlisten();
    }
  }, [study.gameId, study.subject]);

  useEffect(() => {
    if (mode === "replay" && line && replayStarted.current !== study.gameId) {
      replayStarted.current = study.gameId;
      void startReplay();
    }
  }, [mode, line, study.gameId, startReplay]);

  // ---- guess the move ----
  const startGuessing = () => {
    useCoach.getState().stop();
    setScore(NO_SCORE);
    setResult(null);
    setPly(startAt === "middle" ? Math.min(16, total) : 0);
    setPhase("playing");
  };

  // Opponent moves play themselves; the game ends after the last move.
  useEffect(() => {
    if (mode !== "guess" || phase !== "playing" || !line) return;
    if (ply >= total) {
      setPhase("done");
      return;
    }
    if (sideToMove(line.fens[ply]) !== master) {
      const t = setTimeout(() => setPly((p) => p + 1), 650);
      return () => clearTimeout(t);
    }
  }, [mode, phase, ply, total, line, master]);

  const advance = useCallback(() => {
    useCoach.getState().stop();
    setResult(null);
    setPly((p) => p + 1);
    setPhase("playing");
  }, []);

  async function guess(uci: string): Promise<boolean> {
    if (!line || phase !== "playing" || sideToMove(line.fens[ply]) !== master) return false;
    const at = ply;
    setPhase("checking");
    try {
      const r = await unwrap(commands.mastersGuess(line.fens[at], uci, line.ucis[at], study.masterName, null));
      setScore((s) => ({ points: s.points + r.points, max: s.max + 3, exact: s.exact + (r.verdict === "exact" ? 1 : 0), guesses: s.guesses + 1 }));
      setResult(r);
      setPhase("result");
      playSound(r.verdict === "miss" ? "wrong" : "correct");
      if (r.script) useCoach.getState().play(r.script, OWNER);
    } catch (e) {
      notifyError("Couldn't check the move", e);
      setPhase("playing");
    }
    return false; // the master's move is played on Continue
  }

  // After the same move as the master, carry on by itself.
  useEffect(() => {
    if (phase === "result" && result?.verdict === "exact") {
      const t = setTimeout(advance, 900);
      return () => clearTimeout(t);
    }
  }, [phase, result, advance]);

  async function addToRepertoire() {
    if (!line) return;
    try {
      const added = await unwrap(commands.repertoireAddLine(master, line.ucis.slice(0, 16)));
      notifications.show({
        color: "green",
        message: added ? `Added ${masterShort}'s opening (${added} moves) to your ${master} repertoire` : "That opening is already in your repertoire",
      });
    } catch (e) {
      notifyError("Couldn't add the opening", e);
    }
  }

  function voiceCommand(c: VoiceCommand): boolean {
    if (c === "next") {
      if (mode === "guess" && phase === "result") advance();
      else if (mode === "replay") setPly((p) => Math.min(total, p + 1));
      return true;
    }
    if (c === "previous" && mode === "replay") {
      setPly((p) => Math.max(0, p - 1));
      return true;
    }
    return false;
  }

  const recent = useMemo(() => (line ? line.sans.slice(Math.max(0, ply - 10), ply) : []), [line, ply]);

  if (error) {
    return (
      <div className="screen">
        <Button variant="subtle" leftSection={<IconArrowLeft size={14} />} onClick={closeStudy} mb="sm">
          Back
        </Button>
        <Alert color="red" variant="light">
          {error}
        </Alert>
      </div>
    );
  }

  const myTurn = mode === "guess" && phase === "playing" && !!line && sideToMove(line.fens[ply]) === master;
  const coaching = coachBoard.active;

  return (
    <div className="workspace">
      <div className="board-column" ref={ref}>
        {line ? (
          <Board
            fen={coachBoard.fen ?? fen}
            orientation={mode === "guess" ? master : (study.masterColor ?? "white")}
            turn={info?.turn ?? "white"}
            dests={myTurn && !coaching ? info?.dests : {}}
            movableColor={myTurn && !coaching ? master : undefined}
            lastMove={coaching ? coachBoard.lastMove : ply > 0 ? line.ucis[ply - 1] : null}
            check={coaching ? false : info?.check}
            arrows={coaching ? coachBoard.arrows : []}
            circles={coachBoard.circles}
            coaching={coaching}
            coordinates={settings.showCoordinates}
            animationMs={settings.animationMs}
            size={size}
            onMove={guess}
          />
        ) : (
          <Loader />
        )}
      </div>

      <div className="side-panel" style={{ overflow: "auto" }}>
        <Group justify="space-between" wrap="nowrap">
          <Button size="xs" variant="subtle" leftSection={<IconArrowLeft size={14} />} onClick={closeStudy}>
            Back
          </Button>
          <SegmentedControl
            size="xs"
            value={mode}
            onChange={(v) => openStudy({ ...study, mode: v as StudyMode })}
            data={[
              { value: "replay", label: "Guided replay" },
              { value: "guess", label: "Guess the move" },
            ]}
          />
        </Group>
        <div>
          <Title order={4}>{study.title}</Title>
          {line && (
            <Text size="xs" c="dimmed">
              {line.white} – {line.black} · {Math.ceil(total / 2)} moves
            </Text>
          )}
        </div>

        {mode === "replay" && line && (
          <Paper withBorder radius="md" p="sm">
            <Group justify="space-between">
              <Group gap={4}>
                <ActionIcon variant="default" aria-label="Previous move" disabled={coaching || ply === 0} onClick={() => setPly((p) => Math.max(0, p - 1))}>
                  <IconChevronLeft size={16} />
                </ActionIcon>
                <ActionIcon variant="default" aria-label="Next move" disabled={coaching || ply >= total} onClick={() => setPly((p) => Math.min(total, p + 1))}>
                  <IconChevronRight size={16} />
                </ActionIcon>
                <Text size="xs" c="dimmed" className="tabular">
                  {ply === 0 ? "Start" : `Move ${Math.ceil(ply / 2)}${ply % 2 ? "" : "…"} ${line.sans[ply - 1]}`}
                </Text>
              </Group>
              <Button size="xs" variant="light" color="teal" leftSection={<IconSchool size={14} />} disabled={coachStatus === "loading"} onClick={() => void startReplay()}>
                {coachStatus === "idle" || coachStatus === "done" ? "Start the replay again" : "Restart"}
              </Button>
            </Group>
          </Paper>
        )}

        {mode === "guess" && line && (
          <Paper withBorder radius="md" p="sm" aria-label="Guess the move">
            {phase === "setup" && (
              <Stack gap={8}>
                <Text size="sm">
                  Play {study.masterColor ? masterShort : "one side"}'s moves. Each guess scores 3 points for {masterShort}'s move, 2 for one the engine rates as just as
                  good.
                </Text>
                {!study.masterColor && (
                  <SegmentedControl
                    size="xs"
                    value={side}
                    onChange={(v) => setSide(v as Side)}
                    data={[
                      { value: "white", label: `Guess ${shortName(line.white)}'s moves` },
                      { value: "black", label: `Guess ${shortName(line.black)}'s moves` },
                    ]}
                  />
                )}
                <SegmentedControl
                  size="xs"
                  value={startAt}
                  onChange={(v) => setStartAt(v as "start" | "middle")}
                  data={[
                    { value: "start", label: "From move 1" },
                    { value: "middle", label: "From move 9" },
                  ]}
                />
                <Button size="sm" leftSection={<IconQuestionMark size={16} />} onClick={startGuessing}>
                  Start guessing
                </Button>
              </Stack>
            )}
            {phase !== "setup" && (
              <Group justify="space-between" mb={6}>
                <Text fw={650} className="tabular" aria-label="Score">
                  Score: {score.points} / {score.max}
                </Text>
                <Text size="xs" c="dimmed" className="tabular">
                  {score.exact} of {score.guesses} like {masterShort}
                </Text>
              </Group>
            )}
            {phase === "playing" && (
              <Text size="sm" aria-live="polite">
                {myTurn ? `What did ${masterShort} play here?` : "The opponent moves…"}
              </Text>
            )}
            {phase === "checking" && (
              <Group gap={8}>
                <Loader size="xs" />
                <Text size="sm">Checking your move with the engine…</Text>
              </Group>
            )}
            {phase === "result" && result && (
              <Stack gap={6}>
                <Group gap={8}>
                  <Badge color={result.verdict === "exact" ? "green" : result.verdict === "alsoGood" ? "blue" : "orange"} variant="light" size="lg">
                    +{result.points}
                  </Badge>
                  <Text size="sm" aria-live="polite">
                    {result.verdict === "exact"
                      ? `${result.masterSan}: exactly what ${masterShort} played!`
                      : result.verdict === "alsoGood"
                        ? `${result.guessSan} is just as good. ${masterShort} played ${result.masterSan}.`
                        : `You chose ${result.guessSan}; ${masterShort} played ${result.masterSan}.`}
                  </Text>
                </Group>
                {result.verdict !== "exact" && (
                  <Button size="xs" onClick={advance}>
                    Continue
                  </Button>
                )}
              </Stack>
            )}
            {phase === "done" && (
              <Stack gap={8}>
                <Text size="sm" fw={600}>
                  Game over. You scored {score.points} of {score.max} points
                  {score.guesses ? `, finding ${masterShort}'s move ${Math.round((score.exact / score.guesses) * 100)}% of the time` : ""}.
                </Text>
                {score.max > 0 && <Progress value={(score.points / score.max) * 100} />}
                <Group gap={6}>
                  <Button size="xs" variant="light" onClick={() => setPhase("setup")}>
                    Play again
                  </Button>
                  <Button size="xs" variant="subtle" color="teal" onClick={() => openStudy({ ...study, mode: "replay" })}>
                    Guided replay
                  </Button>
                </Group>
              </Stack>
            )}
          </Paper>
        )}

        <CoachPanel
          owner={OWNER}
          fen={() => (useCoach.getState().fen ? null : fen || null)}
          context={() => ({ fen, recentMoves: recent, game: study.title, player: study.masterName })}
          onVoiceMove={(uci) => void guess(uci)}
          onVoiceCommand={voiceCommand}
          placeholder={`Ask about ${masterShort}'s ideas…`}
        />

        {line && (
          <Button size="xs" variant="default" leftSection={<IconBook2 size={14} />} onClick={() => void addToRepertoire()}>
            Add this opening to my {master} repertoire
          </Button>
        )}
      </div>
    </div>
  );
}
