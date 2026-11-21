import { useCallback, useEffect, useRef, useState } from "react";
import { Alert, Anchor, Badge, Button, Card, Group, Progress, SegmentedControl, Select, Stack, Text, Title } from "@mantine/core";
import { useHotkeys } from "@mantine/hooks";
import { openUrl } from "@tauri-apps/plugin-opener";
import { IconBulb, IconCheck, IconDownload, IconPlayerTrackNext, IconRefresh, IconX } from "@tabler/icons-react";
import { Board, useBoardSize, type Arrow } from "../board/Board";
import { commands, events, notifyError, unwrap, type ImportProgress, type Puzzle, type PuzzleInfo, type Side } from "../lib/api";

const wait = (ms: number) => new Promise((r) => setTimeout(r, ms));
import { usePositionInfo } from "../lib/usePositionInfo";
import { applyMove } from "../lib/applyMove";
import { playSound } from "../lib/sounds";
import { formatBytes } from "../lib/format";
import { downloadFraction, useDownloads } from "../stores/downloads";
import { useSettings } from "../stores/settings";
import { useCoach } from "../stores/coach";
import { CoachPanel } from "../coach/CoachPanel";
import { MistakesTrainer } from "../coach/MistakesTrainer";
import { useCoachBoard } from "../coach/useCoachBoard";
import { IconSchool } from "@tabler/icons-react";
import type { ReactNode } from "react";

type Status = "loading" | "opponent" | "solving" | "wrong" | "solved" | "failed" | "revealing";

/** "mateIn2" → "Mate in 2", "hangingPiece" → "Hanging piece". */
export function themeLabel(t: string): string {
  const words = t.replace(/([a-z])([A-Z0-9])/g, "$1 $2").replace(/(\d)/g, " $1").replace(/\s+/g, " ").trim().toLowerCase();
  return words.charAt(0).toUpperCase() + words.slice(1);
}

export function PuzzlesScreen() {
  const [mode, setMode] = useState<"puzzles" | "mistakes">("puzzles");
  const [due, setDue] = useState<number | null>(null);
  useEffect(() => {
    void commands.mistakesStats().then((r) => r.status === "ok" && setDue(r.data.due));
  }, [mode]);
  const modeSwitch = (
    <SegmentedControl
      size="xs"
      value={mode}
      onChange={(v) => setMode(v as "puzzles" | "mistakes")}
      data={[
        { value: "puzzles", label: "Puzzles" },
        { value: "mistakes", label: due ? `My mistakes (${due})` : "My mistakes" },
      ]}
      aria-label="Training mode"
    />
  );
  return mode === "mistakes" ? <MistakesTrainer modeSwitch={modeSwitch} /> : <PuzzleTrainer modeSwitch={modeSwitch} />;
}

const COACH = "puzzles";

function PuzzleTrainer({ modeSwitch }: { modeSwitch: ReactNode }) {
  const settings = useSettings((s) => s.settings);
  const coachBoard = useCoachBoard(COACH);
  const [wrongTry, setWrongTry] = useState<{ fen: string; uci: string; lastMove: string | null } | null>(null);
  const [info, setInfo] = useState<PuzzleInfo | null>(null);
  const [themes, setThemes] = useState<{ value: string; label: string }[]>([]);
  const [theme, setTheme] = useState<string | null>(null);
  const [difficulty, setDifficulty] = useState<"easier" | "normal" | "harder">("normal");
  const [puzzle, setPuzzle] = useState<Puzzle | null>(null);
  const [fen, setFen] = useState<string>("");
  const [lastMove, setLastMove] = useState<string | null>(null);
  const [index, setIndex] = useState(1);
  const [status, setStatus] = useState<Status>("loading");
  const [delta, setDelta] = useState<number | null>(null);
  const [arrows, setArrows] = useState<Arrow[]>([]);
  const [importing, setImporting] = useState<ImportProgress | null>(null);
  const [downloading, setDownloading] = useState(false);
  const recorded = useRef(false);
  // Bumped whenever a new puzzle starts (or the screen closes): async steps of
  // an older puzzle check it and stop instead of touching the new one.
  const generation = useRef(0);
  const progress = useDownloads((s) => s.progress["puzzles"]);
  const { ref, size } = useBoardSize(40, 0);
  const pos = usePositionInfo(fen || undefined);
  const solver: Side = puzzle ? (puzzle.fen.split(" ")[1] === "w" ? "black" : "white") : "white";

  const refreshInfo = useCallback(async () => {
    const r = await commands.puzzlesInfo();
    if (r.status === "ok") setInfo(r.data);
    else notifyError("Could not read the puzzle database", r.error);
    const t = await commands.puzzlesThemes();
    if (t.status === "ok") setThemes(t.data.filter((x) => x.count >= 50).map((x) => ({ value: x.theme, label: `${themeLabel(x.theme)} (${x.count.toLocaleString()})` })));
  }, []);

  useEffect(
    () => () => {
      generation.current++;
    },
    [],
  );

  useEffect(() => {
    void refreshInfo();
    const un = events.importProgress.listen((e) => e.payload.jobId === "puzzles" && setImporting(e.payload));
    return () => void un.then((f) => f());
  }, [refreshInfo]);

  /** Play a move on the board, unless puzzle `gen` has been replaced. */
  const apply = useCallback(async (gen: number, from: string, uci: string): Promise<string | null> => {
    const r = await applyMove(from, uci);
    if (generation.current !== gen) return null;
    setFen(r.position.fen);
    setLastMove(r.uci);
    return r.position.fen;
  }, []);

  const load = useCallback(async () => {
    const gen = ++generation.current;
    setStatus("loading");
    setDelta(null);
    setArrows([]);
    setWrongTry(null);
    useCoach.getState().stop();
    recorded.current = false;
    try {
      const offset = { easier: -200, normal: 0, harder: 200 }[difficulty];
      const target = offset && info ? (info.rating.rating ?? 1500) + offset : null;
      const p = await unwrap(commands.puzzleNext(theme, target));
      if (generation.current !== gen) return;
      if (!p) {
        notifyError("No puzzle found", "Try another theme.");
        return;
      }
      setPuzzle(p);
      setFen(p.fen);
      setLastMove(null);
      setIndex(1);
      setStatus("opponent");
      await wait(650);
      if (generation.current !== gen) return;
      if ((await apply(gen, p.fen, p.moves[0])) !== null) setStatus("solving");
    } catch (e) {
      if (generation.current === gen) notifyError("Could not load a puzzle", e);
    }
  }, [theme, difficulty, info, apply]);

  useEffect(() => {
    if (info && info.count > 0 && !puzzle) void load();
  }, [info, puzzle, load]);

  async function finish(solved: boolean) {
    if (!puzzle || recorded.current) return;
    recorded.current = true;
    const r = await commands.puzzleFinish(puzzle.id, solved);
    if (r.status === "ok") {
      setDelta(Math.round((r.data.after.rating ?? 0) - (r.data.before.rating ?? 0)));
      void refreshInfo();
    } else {
      notifyError("The result wasn't saved", r.error);
    }
  }

  async function onMove(uci: string): Promise<boolean> {
    if (!puzzle || (status !== "solving" && status !== "wrong")) return false;
    const gen = generation.current;
    try {
      const verdict = await unwrap(commands.puzzleCheck(puzzle.id, index, uci));
      if (generation.current !== gen) return false;
      if (!verdict.correct) {
        playSound("wrong");
        setWrongTry({ fen, uci, lastMove });
        setStatus("wrong");
        void finish(false);
        return false;
      }
      const after = await apply(gen, fen, uci);
      if (after === null) return false;
      setArrows([]);
      if (verdict.done) {
        playSound("correct");
        setStatus(recorded.current ? "failed" : "solved");
        void finish(true);
        return true;
      }
      if (verdict.reply) {
        setStatus("opponent");
        await wait(400);
        if (generation.current !== gen) return true;
        if ((await apply(gen, after, verdict.reply)) === null) return true;
        setIndex(index + 2);
        setStatus("solving");
      }
      return true;
    } catch (e) {
      if (generation.current === gen) notifyError("Could not play that move", e);
      return false;
    }
  }

  /** The coach plays out why the wrong move fails, then puts the pieces back. */
  async function explainWrong() {
    if (!wrongTry) return;
    const coach = useCoach.getState();
    coach.setLoading(COACH, "Showing why it doesn't work…");
    const r = await commands.coachExplainMove({
      fenBefore: wrongTry.fen,
      lastMoveBefore: wrongTry.lastMove,
      played: wrongTry.uci,
      known: null,
      returnFen: wrongTry.fen,
      returnLastMove: wrongTry.lastMove,
      coachId: null,
    });
    coach.setLoading(COACH, null);
    if (r.status === "ok") coach.play(r.data, COACH);
    else notifyError("The coach couldn't explain this", r.error);
  }

  async function reveal() {
    if (!puzzle) return;
    const gen = generation.current;
    void finish(false);
    setStatus("revealing");
    let current: string | null = fen;
    try {
      for (let i = index; i < puzzle.moves.length && current !== null; i++) {
        await wait(550);
        if (generation.current !== gen) return;
        current = await apply(gen, current, puzzle.moves[i]);
      }
      if (generation.current === gen) setStatus("failed");
    } catch (e) {
      if (generation.current === gen) {
        notifyError("Could not show the solution", e);
        setStatus("failed");
      }
    }
  }

  function hint() {
    if (!puzzle) return;
    setArrows([{ uci: puzzle.moves[index], brush: "paleGreen" }]);
    void finish(false);
  }

  useHotkeys([
    ["Enter", () => (status === "solved" || status === "failed") && void load()],
    ["n", () => (status === "solved" || status === "failed") && void load()],
  ]);

  async function download(full: boolean) {
    setDownloading(true);
    try {
      const kept = await unwrap(commands.puzzlesDownload(full ? null : { minPopularity: 85, minPlays: 300, maxDeviation: 90 }));
      setImporting(null);
      await refreshInfo();
      setPuzzle(null);
      if (kept === 0) notifyError("No puzzles imported", "The download contained no usable puzzles.");
    } catch (e) {
      notifyError("Puzzle download failed", e);
    } finally {
      setDownloading(false);
    }
  }

  if (info && info.count === 0) {
    return (
      <div className="screen" style={{ maxWidth: 720 }}>
        <div style={{ marginBottom: 12 }}>{modeSwitch}</div>
        <Title order={2} mb={4}>
          Puzzles
        </Title>
        <Text c="dimmed" mb="lg">
          Tactics from the Lichess puzzle database: millions of positions from real games, each rated by how hard players found it.
        </Text>
        <Card withBorder radius="lg" padding="lg">
          <Text fw={600} mb={6}>
            Download the puzzle database
          </Text>
          <Text size="sm" c="dimmed" mb="md">
            About {formatBytes(307_000_000)} to download, once. "Recommended" keeps the ~1.5 million most popular, well-tested puzzles and
            saves space; "Everything" keeps all of them.
          </Text>
          {downloading ? (
            <Stack gap={6}>
              <Text size="sm">
                {importing
                  ? `Importing… ${importing.seen.toLocaleString()} read, ${importing.imported.toLocaleString()} kept`
                  : `Downloading… ${formatBytes(progress?.received)} of ${formatBytes(progress?.total)}`}
              </Text>
              <Progress value={importing ? 100 : downloadFraction(progress) * 100} animated={!!importing} />
            </Stack>
          ) : (
            <Group>
              <Button leftSection={<IconDownload size={16} />} onClick={() => void download(false)}>
                Recommended
              </Button>
              <Button variant="default" onClick={() => void download(true)}>
                Everything
              </Button>
            </Group>
          )}
          <Text size="xs" c="dimmed" mt="md">
            Puzzles: Lichess puzzle database, CC0 (public domain).
          </Text>
        </Card>
      </div>
    );
  }

  const done = status === "solved" || status === "failed";
  const statusText: Record<Status, string> = {
    loading: "Finding a puzzle…",
    opponent: "Watch the opponent's move",
    solving: `Find the best move for ${solver}`,
    wrong: "Not the move. Try again, or show the solution.",
    solved: "Solved!",
    failed: "Puzzle over",
    revealing: "Showing the solution…",
  };

  return (
    <div className="workspace">
      <div className="board-column" ref={ref}>
        {fen && (
          <Board
            fen={coachBoard.fen ?? fen}
            orientation={solver}
            turn={pos?.turn ?? "white"}
            dests={!coachBoard.active && (status === "solving" || status === "wrong") ? pos?.dests : {}}
            movableColor={coachBoard.active ? undefined : solver}
            lastMove={coachBoard.active ? coachBoard.lastMove : lastMove}
            check={!coachBoard.active && pos?.check}
            arrows={coachBoard.active ? coachBoard.arrows : arrows}
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
          <Group justify="space-between" align="flex-end">
            <div>
              <Text size="xs" c="dimmed">
                Puzzle rating
              </Text>
              <Group gap={8} align="baseline">
                <Text fw={700} size="xl" className="tabular" aria-label="Puzzle rating">
                  {info ? Math.round(info.rating.rating ?? 1500) : "–"}
                </Text>
                {delta !== null && (
                  <Text fw={600} c={delta >= 0 ? "green" : "red"} className="tabular">
                    {delta >= 0 ? `+${delta}` : delta}
                  </Text>
                )}
              </Group>
            </div>
            <Text size="xs" c="dimmed">
              {info?.count.toLocaleString()} puzzles
            </Text>
          </Group>
        </div>
        <Select
          placeholder="All themes"
          clearable
          searchable
          data={themes}
          value={theme}
          onChange={(v) => {
            generation.current++;
            setTheme(v);
            setPuzzle(null);
          }}
          aria-label="Puzzle theme"
        />
        <SegmentedControl
          size="xs"
          value={difficulty}
          onChange={(v) => {
            generation.current++;
            setDifficulty(v as "easier" | "normal" | "harder");
            setPuzzle(null);
          }}
          data={[
            { value: "easier", label: "Easier" },
            { value: "normal", label: "At my level" },
            { value: "harder", label: "Harder" },
          ]}
          aria-label="Difficulty"
        />
        <Alert
          variant="light"
          color={status === "solved" ? "green" : status === "wrong" || status === "failed" ? "orange" : "blue"}
          icon={status === "solved" ? <IconCheck size={18} /> : status === "wrong" ? <IconX size={18} /> : undefined}
          aria-live="polite"
        >
          {statusText[status]}
        </Alert>
        {puzzle && (
          <div className="panel" style={{ padding: 12 }}>
            <Group justify="space-between" mb={6}>
              <Text size="sm" fw={600}>
                Puzzle {puzzle.id}
              </Text>
              <Text size="xs" c="dimmed" className="tabular" aria-label="Moves found">
                {status === "solved" ? puzzle.moves.length / 2 : Math.floor((index - 1) / 2)} / {puzzle.moves.length / 2} moves
              </Text>
              <Text size="xs" c="dimmed">
                {done ? `rated ${puzzle.rating}` : "rating hidden"}
              </Text>
            </Group>
            {done && (
              <Group gap={4} mb={6}>
                {puzzle.themes.map((t) => (
                  <Badge key={t} size="xs" variant="light" color="steel">
                    {themeLabel(t)}
                  </Badge>
                ))}
              </Group>
            )}
            {done && puzzle.gameUrl && (
              <Anchor size="xs" component="button" onClick={() => void openUrl(puzzle.gameUrl!)}>
                View the original game on Lichess
              </Anchor>
            )}
          </div>
        )}
        <Group grow gap={8}>
          {!done ? (
            <>
              <Button variant="default" leftSection={<IconBulb size={16} />} onClick={hint} disabled={status !== "solving" && status !== "wrong"}>
                Hint
              </Button>
              <Button
                variant="default"
                leftSection={<IconRefresh size={16} />}
                onClick={() => void reveal()}
                disabled={status === "loading" || status === "revealing" || status === "opponent"}
              >
                Solution
              </Button>
            </>
          ) : (
            <Button leftSection={<IconPlayerTrackNext size={16} />} onClick={() => void load()}>
              Next puzzle
            </Button>
          )}
        </Group>
        {status === "wrong" && wrongTry && (
          <Button variant="light" color="teal" leftSection={<IconSchool size={16} />} onClick={() => void explainWrong()}>
            Why doesn't that work?
          </Button>
        )}
        <Text size="xs" c="dimmed">
          Hints and revealed solutions count as a miss. Press Enter for the next puzzle.
        </Text>
        <CoachPanel
          owner={COACH}
          fen={() => (status === "solving" || status === "wrong" ? fen : null)}
          context={() => ({ fen, recentMoves: [], game: puzzle ? `a puzzle (themes: ${puzzle.themes.join(", ")})` : "a puzzle", player: null })}
          onVoiceMove={(uci) => void onMove(uci)}
        />
      </div>
    </div>
  );
}
