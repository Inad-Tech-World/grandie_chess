import { useCallback, useEffect, useMemo, useState } from "react";
import { ActionIcon, Badge, Button, Group, SegmentedControl, Stack, Text, Title, Tooltip } from "@mantine/core";
import { useHotkeys } from "@mantine/hooks";
import { open } from "@tauri-apps/plugin-dialog";
import { notifications } from "@mantine/notifications";
import { IconChevronLeft, IconChevronRight, IconChevronsLeft, IconDeviceFloppy, IconFileImport, IconPlus, IconTrash } from "@tabler/icons-react";
import { Board, useBoardSize, type Arrow } from "../board/Board";
import { commands, notifyError, tryCommand, unwrap, type AppliedMove, type OpeningName, type RepMove, type RepertoireStats, type Side } from "../lib/api";
import { START_FEN } from "../lib/tree";
import { primePositionInfo, usePositionInfo } from "../lib/usePositionInfo";
import { savePgnText } from "../lib/actions";
import { useSettings } from "../stores/settings";
import { ExplorerPanel, type ExplorerSource } from "../openings/ExplorerPanel";
import { TrainPanel } from "../openings/TrainPanel";
import { moveNumber } from "../lib/format";

type Mode = "explore" | "build" | "train";

const plural = (n: number, word: string) => `${n} ${word}${n === 1 ? "" : "s"}`;

const sameSpot = (a: string, b: string) => a.split(" ").slice(0, 4).join(" ") === b.split(" ").slice(0, 4).join(" ");

export function OpeningsScreen() {
  const settings = useSettings((s) => s.settings);
  const [mode, setMode] = useState<Mode>("explore");
  const [color, setColor] = useState<Side>("white");
  const [line, setLine] = useState<AppliedMove[]>([]);
  const [cursor, setCursor] = useState(0);
  const [source, setSource] = useState<ExplorerSource>("database");
  const [repMoves, setRepMoves] = useState<RepMove[]>([]);
  const [stats, setStats] = useState<Record<Side, RepertoireStats | null>>({ white: null, black: null });
  const [opening, setOpening] = useState<OpeningName | null>(null);
  const { ref, size } = useBoardSize(70, 0);

  const fen = cursor === 0 ? START_FEN : line[cursor - 1].position.fen;
  const info = usePositionInfo(fen);
  const ucis = useMemo(() => line.slice(0, cursor).map((m) => m.uci), [line, cursor]);

  const refresh = useCallback(async () => {
    const [moves, w, b] = await Promise.all([
      commands.repertoireMoves(color),
      commands.repertoireStats("white"),
      commands.repertoireStats("black"),
    ]);
    if (moves.status === "ok") setRepMoves(moves.data);
    setStats({ white: w.status === "ok" ? w.data : null, black: b.status === "ok" ? b.data : null });
  }, [color]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  useEffect(() => {
    void commands.openingName(START_FEN, ucis).then(setOpening);
  }, [ucis]);

  const play = useCallback(
    async (uci: string): Promise<boolean> => {
      const r = await commands.chessApplyMove(fen, uci);
      if (r.status !== "ok") return false;
      primePositionInfo(r.data.position);
      // Following the existing line just moves forward.
      if (line[cursor]?.uci === r.data.uci) {
        setCursor(cursor + 1);
        return true;
      }
      setLine([...line.slice(0, cursor), r.data]);
      setCursor(cursor + 1);
      return true;
    },
    [fen, line, cursor],
  );

  useHotkeys([
    ["ArrowLeft", () => setCursor((c) => Math.max(0, c - 1))],
    ["ArrowRight", () => setCursor((c) => Math.min(line.length, c + 1))],
    ["Home", () => setCursor(0)],
    ["End", () => setCursor(line.length)],
  ]);

  const here = repMoves.filter((m) => sameSpot(m.parentFen, fen));
  const yourTurn = info?.turn === color;
  const arrows: Arrow[] =
    mode === "build" ? here.map((m) => ({ uci: m.uci, brush: yourTurn ? "green" : "blue" })) : [];

  async function addLine() {
    if (ucis.length === 0) return;
    const added = await tryCommand(commands.repertoireAddLine(color, ucis), "Could not save the line");
    if (added !== undefined) {
      notifications.show({ color: "green", message: added ? `Added ${added} move${added === 1 ? "" : "s"} to your ${color} repertoire` : "That line is already in your repertoire" });
      await refresh();
    }
  }

  async function importPgn() {
    const path = await open({ multiple: false, filters: [{ name: "PGN", extensions: ["pgn"] }] });
    if (!path || Array.isArray(path)) return;
    try {
      const text = await unwrap(commands.fileReadText(path));
      const added = await unwrap(commands.repertoireImportPgn(color, text));
      notifications.show({ color: "green", message: `Imported ${added} moves into your ${color} repertoire` });
      await refresh();
    } catch (e) {
      notifyError("Import failed", e);
    }
  }

  async function exportPgn() {
    const pgn = await tryCommand(commands.repertoireExportPgn(color));
    if (pgn) await savePgnText(pgn, `Grandie ${color} repertoire.pgn`);
  }

  const lineText = line.map((m, i) => {
    const before = i === 0 ? START_FEN : line[i - 1].position.fen;
    const { number, white } = moveNumber(before);
    return (
      <span key={i} className="move" data-current={i === cursor - 1} onClick={() => setCursor(i + 1)} style={{ opacity: i < cursor ? 1 : 0.5 }}>
        {white ? `${number}. ` : i === 0 ? `${number}… ` : ""}
        {m.san}
      </span>
    );
  });

  return (
    <div className="workspace" style={{ gridTemplateColumns: "minmax(0, 1fr) minmax(320px, 420px)" }}>
      <div className="board-column" ref={ref}>
        {mode === "train" ? (
          <TrainPanel size={size} onDone={() => void refresh()} />
        ) : (
          <>
            <Group justify="space-between" style={{ width: size }} mb={6}>
              <Text size="sm" fw={600} truncate>
                {opening ? `${opening.eco} ${opening.name}` : "Starting position"}
              </Text>
              <Group gap={2}>
                <ActionIcon variant="subtle" color="gray" onClick={() => setCursor(0)} aria-label="Start">
                  <IconChevronsLeft size={18} />
                </ActionIcon>
                <ActionIcon variant="subtle" color="gray" onClick={() => setCursor(Math.max(0, cursor - 1))} aria-label="Back">
                  <IconChevronLeft size={18} />
                </ActionIcon>
                <ActionIcon variant="subtle" color="gray" onClick={() => setCursor(Math.min(line.length, cursor + 1))} aria-label="Forward">
                  <IconChevronRight size={18} />
                </ActionIcon>
              </Group>
            </Group>
            <Board
              fen={fen}
              orientation={color}
              turn={info?.turn ?? "white"}
              dests={info?.dests}
              movableColor={info?.turn}
              lastMove={cursor > 0 ? line[cursor - 1].uci : null}
              check={info?.check}
              arrows={arrows}
              coordinates={settings.showCoordinates}
              animationMs={settings.animationMs}
              size={size}
              onMove={play}
            />
            <div className="tree" style={{ width: size, padding: "8px 0 0", minHeight: 32 }} aria-label="Current line">
              {line.length ? lineText : <span className="muted">Play moves on the board, or click a move in the explorer.</span>}
            </div>
          </>
        )}
      </div>

      <div className="side-panel" style={{ overflow: "auto" }}>
        <Group justify="space-between">
          <Title order={3}>Openings</Title>
          <SegmentedControl
            size="xs"
            value={mode}
            onChange={(v) => setMode(v as Mode)}
            data={[
              { value: "explore", label: "Explore" },
              { value: "build", label: "Build" },
              { value: "train", label: "Train" },
            ]}
          />
        </Group>

        {mode !== "train" && (
          <Group justify="space-between">
            <SegmentedControl
              size="xs"
              value={color}
              onChange={(v) => setColor(v as Side)}
              data={[
                { value: "white", label: "As White" },
                { value: "black", label: "As Black" },
              ]}
            />
            {stats[color] && (
              <Text size="xs" c="dimmed">
                {plural(stats[color]!.moves, "move")} · {plural(stats[color]!.cards, "position")} to know
              </Text>
            )}
          </Group>
        )}

        {mode === "build" && (
          <div className="panel" style={{ padding: 12 }} aria-label="Repertoire builder">
            <Text fw={600} size="sm" mb={4}>
              Your {color} repertoire here
            </Text>
            {here.length === 0 ? (
              <Text size="xs" c="dimmed" mb={8}>
                {yourTurn ? "No move prepared for this position yet." : "No opponent replies prepared here yet."}
              </Text>
            ) : (
              <Stack gap={2} mb={8}>
                {here.map((m) => (
                  <Group key={m.id} justify="space-between" className="engine-line" onClick={() => void play(m.uci)}>
                    <Group gap={6}>
                      <Badge size="xs" variant="light" color={yourTurn ? "green" : "blue"}>
                        {yourTurn ? "your move" : "their reply"}
                      </Badge>
                      <Text size="sm" fw={600}>
                        {m.san}
                      </Text>
                    </Group>
                    <Tooltip label="Remove this move and everything after it">
                      <ActionIcon
                        size="sm"
                        variant="subtle"
                        color="gray"
                        aria-label={`Remove ${m.san}`}
                        onClick={(e) => {
                          e.stopPropagation();
                          void tryCommand(commands.repertoireRemoveMove(m.id)).then(refresh);
                        }}
                      >
                        <IconTrash size={14} />
                      </ActionIcon>
                    </Tooltip>
                  </Group>
                ))}
              </Stack>
            )}
            <Group gap={6}>
              <Button size="xs" leftSection={<IconPlus size={14} />} disabled={cursor === 0} onClick={() => void addLine()}>
                Add this line
              </Button>
              <Button size="xs" variant="default" leftSection={<IconFileImport size={14} />} onClick={() => void importPgn()}>
                Import PGN
              </Button>
              <Button size="xs" variant="default" leftSection={<IconDeviceFloppy size={14} />} onClick={() => void exportPgn()}>
                Export
              </Button>
            </Group>
          </div>
        )}

        {mode !== "train" && <ExplorerPanel fen={fen} color={color} onPlay={(u) => void play(u)} source={source} onSourceChange={setSource} />}

        {mode === "train" && (
          <div className="panel" style={{ padding: 12 }}>
            <Text size="sm" fw={600} mb={4}>
              How training works
            </Text>
            <Text size="xs" c="dimmed">
              Grandie plays each line of your repertoire up to a position where you must find your prepared move. Moves you get right come back
              later and later; mistakes come back soon. A few minutes a day keeps your openings sharp.
            </Text>
            <Stack gap={4} mt="sm">
              {(["white", "black"] as Side[]).map((c) => (
                <Text key={c} size="xs">
                  As {c}: {stats[c]?.dueNow ?? 0} due · {stats[c]?.newCards ?? 0} new · {stats[c]?.cards ?? 0} total
                </Text>
              ))}
            </Stack>
          </div>
        )}
      </div>
    </div>
  );
}
