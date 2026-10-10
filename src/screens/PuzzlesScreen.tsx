import { useCallback, useEffect, useRef, useState } from "react";
import { Alert, Anchor, Badge, Button, Card, Group, Progress, SegmentedControl, Select, Stack, Text, Title } from "@mantine/core";
import { useHotkeys } from "@mantine/hooks";
import { openUrl } from "@tauri-apps/plugin-opener";
import { IconBulb, IconCheck, IconDownload, IconPlayerTrackNext, IconRefresh, IconX } from "@tabler/icons-react";
import { Board, useBoardSize, type Arrow } from "../board/Board";
import { commands, events, notifyError, unwrap, type ImportProgress, type Puzzle, type PuzzleInfo, type Side } from "../lib/api";
import { primePositionInfo, usePositionInfo } from "../lib/usePositionInfo";
import { playSound, soundForSan } from "../lib/sounds";
import { formatBytes } from "../lib/format";
import { downloadFraction, useDownloads } from "../stores/downloads";
import { useSettings } from "../stores/settings";

type Status = "loading" | "opponent" | "solving" | "wrong" | "solved" | "failed" | "revealing";

/** "mateIn2" → "Mate in 2", "hangingPiece" → "Hanging piece". */
export function themeLabel(t: string): string {
  const words = t.replace(/([a-z])([A-Z0-9])/g, "$1 $2").replace(/(\d)/g, " $1").replace(/\s+/g, " ").trim().toLowerCase();
  return words.charAt(0).toUpperCase() + words.slice(1);
}

export function PuzzlesScreen() {
  const settings = useSettings((s) => s.settings);
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
  const progress = useDownloads((s) => s.progress["puzzles"]);
  const { ref, size } = useBoardSize(40, 0);
  const pos = usePositionInfo(fen || undefined);
  const solver: Side = puzzle ? (puzzle.fen.split(" ")[1] === "w" ? "black" : "white") : "white";

  const refreshInfo = useCallback(async () => {
    const r = await commands.puzzlesInfo();
    if (r.status === "ok") setInfo(r.data);
    const t = await commands.puzzlesThemes();
    if (t.status === "ok") setThemes(t.data.filter((x) => x.count >= 50).map((x) => ({ value: x.theme, label: `${themeLabel(x.theme)} (${x.count.toLocaleString()})` })));
  }, []);

  useEffect(() => {
    void refreshInfo();
    const un = events.importProgress.listen((e) => e.payload.jobId === "puzzles" && setImporting(e.payload));
    return () => void un.then((f) => f());
  }, [refreshInfo]);

  const apply = useCallback(async (from: string, uci: string) => {
    const r = await unwrap(commands.chessApplyMove(from, uci));
    primePositionInfo(r.position);
    playSound(soundForSan(r.san));
    setFen(r.position.fen);
    setLastMove(r.uci);
    return r.position.fen;
  }, []);

  const load = useCallback(async () => {
    setStatus("loading");
    setDelta(null);
    setArrows([]);
    recorded.current = false;
    try {
      const offset = { easier: -200, normal: 0, harder: 200 }[difficulty];
      const target = offset && info ? (info.rating.rating ?? 1500) + offset : null;
      const p = await unwrap(commands.puzzleNext(theme, target));
      if (!p) {
        notifyError("No puzzle found", "Try another theme.");
        return;
      }
      setPuzzle(p);
      setFen(p.fen);
      setLastMove(null);
      setIndex(1);
      setStatus("opponent");
      await new Promise((r) => setTimeout(r, 650));
      await apply(p.fen, p.moves[0]);
      setStatus("solving");
    } catch (e) {
      notifyError("Could not load a puzzle", e);
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
    }
  }

  async function onMove(uci: string): Promise<boolean> {
    if (!puzzle || (status !== "solving" && status !== "wrong")) return false;
    const verdict = await commands.puzzleCheck(puzzle.id, index, uci);
    if (verdict.status !== "ok") return false;
    if (!verdict.data.correct) {
      playSound("wrong");
      setStatus("wrong");
      void finish(false);
      return false;
    }
    const after = await apply(fen, uci);
    setArrows([]);
    if (verdict.data.done) {
      playSound("correct");
      setStatus(recorded.current ? "failed" : "solved");
      void finish(true);
      return true;
    }
    if (verdict.data.reply) {
      setStatus("opponent");
      await new Promise((r) => setTimeout(r, 400));
      await apply(after, verdict.data.reply);
      setIndex(index + 2);
      setStatus("solving");
    }
    return true;
  }

  async function reveal() {
    if (!puzzle) return;
    void finish(false);
    setStatus("revealing");
    let current = fen;
    for (let i = index; i < puzzle.moves.length; i++) {
      await new Promise((r) => setTimeout(r, 550));
      current = await apply(current, puzzle.moves[i]);
    }
    setStatus("failed");
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
            fen={fen}
            orientation={solver}
            turn={pos?.turn ?? "white"}
            dests={status === "solving" || status === "wrong" ? pos?.dests : {}}
            movableColor={solver}
            lastMove={lastMove}
            check={pos?.check}
            arrows={arrows}
            coordinates={settings.showCoordinates}
            animationMs={settings.animationMs}
            size={size}
            onMove={onMove}
          />
        )}
      </div>
      <div className="side-panel">
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
            setTheme(v);
            setPuzzle(null);
          }}
          aria-label="Puzzle theme"
        />
        <SegmentedControl
          size="xs"
          value={difficulty}
          onChange={(v) => {
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
              <Button variant="default" leftSection={<IconRefresh size={16} />} onClick={() => void reveal()} disabled={status === "loading" || status === "revealing"}>
                Solution
              </Button>
            </>
          ) : (
            <Button leftSection={<IconPlayerTrackNext size={16} />} onClick={() => void load()}>
              Next puzzle
            </Button>
          )}
        </Group>
        <Text size="xs" c="dimmed">
          Hints and revealed solutions count as a miss. Press Enter for the next puzzle.
        </Text>
      </div>
    </div>
  );
}
