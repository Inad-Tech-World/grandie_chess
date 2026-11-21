// One master: how they play (from their games in the library), the coach's
// style report, and their games with study actions.

import { useCallback, useEffect, useState } from "react";
import {
  Alert,
  Anchor,
  Badge,
  Button,
  Card,
  Group,
  Loader,
  Pagination,
  Progress,
  SegmentedControl,
  Select,
  SimpleGrid,
  Stack,
  Table,
  Text,
  TextInput,
  Title,
  Tooltip as HelpTip,
} from "@mantine/core";
import { IconBook2, IconDownload, IconEye, IconPlayerPlay, IconQuestionMark, IconSchool, IconVolume } from "@tabler/icons-react";
import { Bar, BarChart, ResponsiveContainer, Tooltip, XAxis, YAxis } from "recharts";
import {
  commands,
  errorKind,
  errorMessage,
  events,
  unwrap,
  type ColorStats,
  type GameRow,
  type MasterMoveStat,
  type MasterOpeningStat,
  type MasterPlayer,
  type PlayerStats,
  type Side,
  type StyleMetrics,
  type UserOutcome,
} from "../lib/api";
import { openDbGame } from "../lib/actions";
import { useApp } from "../stores/app";
import { useCoach } from "../stores/coach";
import { gameTitle, masterColorIn, shortName, useMasters, yearOf } from "./store";

const PAGE = 50;
const OWNER = "masters";

function pct(n: number | null | undefined): string {
  return n === null || n === undefined ? "–" : `${Math.round(n)}%`;
}

function ResultsCard({ title, stats }: { title: string; stats: ColorStats }) {
  return (
    <Card withBorder radius="md" padding="sm">
      <Text size="xs" c="dimmed">
        {title}
      </Text>
      <Text fw={700} size="lg" className="tabular">
        {pct(stats.score)}
      </Text>
      <Text size="xs" c="dimmed" className="tabular">
        {stats.games} {stats.games === 1 ? "game" : "games"} · +{stats.wins} ={stats.draws} −{stats.losses}
      </Text>
    </Card>
  );
}

function OpeningList({ title, list }: { title: string; list: MasterOpeningStat[] }) {
  return (
    <Card withBorder radius="md" padding="sm">
      <Text size="sm" fw={600} mb={4}>
        {title}
      </Text>
      {list.length === 0 && (
        <Text size="xs" c="dimmed">
          No named openings in these games.
        </Text>
      )}
      {list.map((o) => (
        <Group key={`${o.eco}-${o.name}`} justify="space-between" gap={6} wrap="nowrap">
          <Text size="sm" truncate>
            {o.name}
          </Text>
          <Text size="xs" c="dimmed" className="tabular" style={{ flexShrink: 0 }}>
            {o.games} · {pct(o.score)}
          </Text>
        </Group>
      ))}
    </Card>
  );
}

function MoveList({ title, list }: { title: string; list: MasterMoveStat[] }) {
  if (list.length === 0) return null;
  return (
    <div>
      <Text size="xs" c="dimmed" mb={2}>
        {title}
      </Text>
      <Group gap={6}>
        {list.map((m) => (
          <Badge key={m.san} variant="light" color="gray" size="md" style={{ textTransform: "none" }}>
            {m.san} · {m.games} · {pct(m.score)}
          </Badge>
        ))}
      </Group>
    </div>
  );
}

/** Plain-English reading of the engine-measured style. */
function StyleView({ style }: { style: StyleMetrics }) {
  const items: { label: string; value: string; help: string }[] = [
    { label: "Accuracy", value: pct(style.accuracy), help: "How close the moves were to the engine's, on the Lichess scale (90%+ is world class)." },
    {
      label: "Sharpness",
      value: style.sharpness === null ? "–" : style.sharpness.toFixed(1),
      help: "How much the winning chances swing per move. Above about 4 means sharp, tactical games; below 2, quiet positional ones.",
    },
    { label: "Sacrifices / game", value: (style.sacrificesPerGame ?? 0).toFixed(2), help: "Sound sacrifices the engine rates as brilliant." },
    { label: "Only moves / game", value: (style.greatPerGame ?? 0).toFixed(2), help: "Positions where only one move kept the advantage, and they found it." },
    { label: "Mistakes / game", value: (style.mistakesPerGame ?? 0).toFixed(2), help: "Moves that gave away a good part of the winning chances." },
    { label: "Blunders / game", value: (style.blundersPerGame ?? 0).toFixed(2), help: "Serious errors. Even world champions make some." },
  ];
  return (
    <>
      <SimpleGrid cols={{ base: 2, md: 3 }} spacing={8}>
        {items.map((i) => (
          <HelpTip key={i.label} label={i.help} multiline w={260}>
            <Card withBorder radius="md" padding="xs">
              <Text size="xs" c="dimmed">
                {i.label}
              </Text>
              <Text fw={700} className="tabular">
                {i.value}
              </Text>
            </Card>
          </HelpTip>
        ))}
      </SimpleGrid>
      <Text size="xs" c="dimmed" mt={4}>
        Measured by {style.engine} over {style.gamesAnalysed} decisive games.
      </Text>
    </>
  );
}

interface Props {
  player: MasterPlayer;
  downloadPhase: string | null | undefined;
  onDownload: () => void;
}

export function PlayerPage({ player, downloadPhase, onDownload }: Props) {
  const openStudy = useMasters((s) => s.openStudy);
  const setSection = useApp((s) => s.setSection);
  const [stats, setStats] = useState<PlayerStats | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [styleRun, setStyleRun] = useState<{ done: number; total: number } | null>(null);
  const [report, setReport] = useState<string | null>(null);
  const [reportBusy, setReportBusy] = useState(false);
  const [reportError, setReportError] = useState<{ message: string; needsKey: boolean } | null>(null);
  const [color, setColor] = useState<"any" | Side>("any");
  const [outcome, setOutcome] = useState<string>("any");
  const [opening, setOpening] = useState("");
  const [sort, setSort] = useState<"new" | "old">("new");
  const [page, setPage] = useState(1);
  const [games, setGames] = useState<{ total: number; rows: GameRow[] } | null>(null);
  const downloaded = player.gamesInLibrary > 0;

  const loadStats = useCallback(async () => {
    if (!downloaded) return;
    try {
      const s = await unwrap(commands.mastersProfile(player.id));
      setStats(s);
      setReport(s.report);
    } catch (e) {
      setError(errorMessage(e));
    }
  }, [player.id, downloaded]);

  useEffect(() => {
    setStats(null);
    setReport(null);
    setReportError(null);
    setError(null);
    setPage(1);
    void loadStats();
  }, [loadStats]);

  useEffect(() => setPage(1), [color, outcome, opening, sort]);

  useEffect(() => {
    if (!downloaded) return;
    let live = true;
    const t = setTimeout(async () => {
      const r = await commands.dbQuery({
        subject: player.id,
        subjectColor: color === "any" ? null : color,
        subjectOutcome: outcome === "any" ? null : (outcome as UserOutcome),
        text: opening.trim() || null,
        sort: "date",
        desc: sort === "new",
        offset: (page - 1) * PAGE,
        limit: PAGE,
      });
      if (!live) return;
      if (r.status === "ok") setGames({ total: r.data.total, rows: r.data.rows });
      else setError(r.error.message);
    }, 200);
    return () => {
      live = false;
      clearTimeout(t);
    };
  }, [player.id, downloaded, color, outcome, opening, sort, page]);

  async function measureStyle() {
    const id = `style:${player.id}`;
    setStyleRun({ done: 0, total: 12 });
    const unlisten = await events.reviewProgress.listen((e) => {
      if (e.payload.reviewId === id) setStyleRun({ done: e.payload.done, total: e.payload.total });
    });
    try {
      await unwrap(commands.mastersAnalyseStyle(player.id, 12, null));
      await loadStats();
    } catch (e) {
      if (errorKind(e) !== "cancelled") setError(errorMessage(e));
    } finally {
      unlisten();
      setStyleRun(null);
    }
  }

  async function writeReport(refresh: boolean) {
    setReportBusy(true);
    setReportError(null);
    try {
      setReport(await unwrap(commands.mastersStyleReport(player.id, refresh)));
    } catch (e) {
      const kind = errorKind(e);
      setReportError({ message: errorMessage(e), needsKey: kind === "noApiKey" || kind === "aiQuota" });
    } finally {
      setReportBusy(false);
    }
  }

  function study(mode: "replay" | "guess", g: GameRow) {
    openStudy({ mode, gameId: g.id, subject: player.id, masterName: player.name, masterColor: masterColorIn(player.name, g.white, g.black), title: gameTitle(g.white, g.black, g.date) });
  }

  if (!downloaded) {
    return (
      <div className="screen" style={{ maxWidth: 760, paddingTop: 8 }}>
        <Title order={2} mb={4}>
          {player.name}
        </Title>
        <Text c="dimmed" mb="md">
          {player.gamesAvailable.toLocaleString()} games available. Download them to see how {shortName(player.name)} plays: favourite openings, results with each
          colour, the style measured by the engine, and every game to replay with the coach or to guess move by move.
        </Text>
        <Button leftSection={<IconDownload size={16} />} loading={downloadPhase !== undefined} onClick={onDownload}>
          Download {shortName(player.name)}'s games
        </Button>
        {downloadPhase && (
          <Text size="sm" c="dimmed" mt={6}>
            {downloadPhase}
          </Text>
        )}
        <Text size="xs" c="dimmed" mt="lg">
          Games from PGN Mentor (pgnmentor.com), downloaded when you ask for them.
        </Text>
      </div>
    );
  }

  return (
    <div className="screen" style={{ paddingTop: 8 }}>
      <Group justify="space-between" align="flex-end" mb="sm">
        <div>
          <Title order={2}>{player.name}</Title>
          <Text c="dimmed" size="sm">
            {(stats?.games ?? player.gamesInLibrary).toLocaleString()} games in the library
            {stats?.firstYear ? ` · ${stats.firstYear}–${stats.lastYear}` : ""}
            {stats?.averageMoves ? ` · ${stats.averageMoves} moves on average · ${pct(stats.decisive)} decisive` : ""}
          </Text>
        </div>
      </Group>
      {error && (
        <Alert color="red" variant="light" mb="sm" withCloseButton onClose={() => setError(null)}>
          {error}
        </Alert>
      )}
      {!stats ? (
        <Loader size="sm" />
      ) : (
        <Stack gap="md">
          <SimpleGrid cols={{ base: 2, md: 4 }} spacing={8}>
            <ResultsCard title="Results as White" stats={stats.asWhite} />
            <ResultsCard title="Results as Black" stats={stats.asBlack} />
            <Card withBorder radius="md" padding="sm">
              <Text size="xs" c="dimmed" mb={4}>
                Games per decade
              </Text>
              <div style={{ height: 70 }}>
                <ResponsiveContainer>
                  <BarChart data={stats.decades.map(([decade, n]) => ({ decade: `${decade}s`, games: n }))} margin={{ top: 0, right: 0, bottom: 0, left: -28 }}>
                    <XAxis dataKey="decade" tick={{ fontSize: 10, fill: "var(--text-dim)" }} axisLine={false} tickLine={false} />
                    <YAxis tick={{ fontSize: 10, fill: "var(--text-dim)" }} axisLine={false} tickLine={false} allowDecimals={false} />
                    <Tooltip contentStyle={{ background: "var(--surface-raised)", border: "1px solid var(--hairline)", fontSize: 12 }} />
                    <Bar dataKey="games" fill="var(--mantine-color-teal-5)" isAnimationActive={false} />
                  </BarChart>
                </ResponsiveContainer>
              </div>
            </Card>
            <Card withBorder radius="md" padding="sm">
              <Text size="xs" c="dimmed" mb={4}>
                Most frequent opponents
              </Text>
              {stats.topOpponents.slice(0, 4).map((o) => (
                <Group key={o.name} justify="space-between" gap={4} wrap="nowrap">
                  <Text size="xs" truncate>
                    {o.name}
                  </Text>
                  <Text size="xs" c="dimmed" className="tabular">
                    {o.games} · {pct(o.score)}
                  </Text>
                </Group>
              ))}
            </Card>
          </SimpleGrid>

          <SimpleGrid cols={{ base: 1, md: 2 }} spacing={8}>
            <OpeningList title="Favourite openings with White" list={stats.openingsWhite} />
            <OpeningList title="Favourite openings with Black" list={stats.openingsBlack} />
          </SimpleGrid>
          <Stack gap={6}>
            <MoveList title="First moves with White (games · score)" list={stats.firstMoves} />
            <MoveList title="Replies to 1.e4" list={stats.vsE4} />
            <MoveList title="Replies to 1.d4" list={stats.vsD4} />
          </Stack>

          <Card withBorder radius="md" padding="sm">
            <Group justify="space-between" mb={6}>
              <Text fw={600}>Style, measured by the engine</Text>
              {styleRun ? (
                <Button size="xs" variant="default" onClick={() => void commands.reviewCancel(`style:${player.id}`)}>
                  Cancel
                </Button>
              ) : (
                <Button size="xs" variant="light" onClick={() => void measureStyle()}>
                  {stats.style ? "Measure again" : "Measure style with the engine"}
                </Button>
              )}
            </Group>
            {styleRun && (
              <Stack gap={2} mb={6}>
                <Text size="xs" c="dimmed">
                  Analysing game {Math.min(styleRun.done + 1, styleRun.total)} of {styleRun.total}…
                </Text>
                <Progress size="xs" value={(styleRun.done / Math.max(1, styleRun.total)) * 100} />
              </Stack>
            )}
            {stats.style ? (
              <StyleView style={stats.style} />
            ) : (
              !styleRun && (
                <Text size="sm" c="dimmed">
                  The coach's engine reviews a sample of {shortName(player.name)}'s decisive games to measure accuracy, sharpness and sacrifices. It takes a few minutes.
                </Text>
              )
            )}
          </Card>

          <Card withBorder radius="md" padding="sm">
            <Group justify="space-between" mb={6}>
              <Text fw={600}>How {shortName(player.name)} plays</Text>
              <Group gap={6}>
                {report && (
                  <Button size="xs" variant="subtle" leftSection={<IconVolume size={14} />} onClick={() => useCoach.getState().speak(OWNER, report)}>
                    Read aloud
                  </Button>
                )}
                <Button size="xs" variant="light" loading={reportBusy} onClick={() => void writeReport(!!report)}>
                  {report ? "Write again" : "Coach's style report"}
                </Button>
              </Group>
            </Group>
            {report ? (
              <Text size="sm" style={{ whiteSpace: "pre-wrap" }}>
                {report}
              </Text>
            ) : (
              !reportError && (
                <Text size="sm" c="dimmed">
                  The coach writes a short profile from these statistics: what makes the style distinctive and what to learn from it.
                </Text>
              )
            )}
            {reportError && (
              <Text size="sm" c="red" role="alert">
                {reportError.message}{" "}
                {reportError.needsKey && (
                  <Anchor size="sm" component="button" onClick={() => setSection("settings")}>
                    Add an OpenRouter key in Settings
                  </Anchor>
                )}
              </Text>
            )}
          </Card>
        </Stack>
      )}

      <Title order={4} mt="lg" mb="xs">
        Games
      </Title>
      <Group gap={8} mb="xs">
        <SegmentedControl
          size="xs"
          value={color}
          onChange={(v) => setColor(v as "any" | Side)}
          data={[
            { value: "any", label: "Both colours" },
            { value: "white", label: "White" },
            { value: "black", label: "Black" },
          ]}
        />
        <Select
          size="xs"
          w={130}
          value={outcome}
          onChange={(v) => setOutcome(v ?? "any")}
          allowDeselect={false}
          aria-label="Result"
          data={[
            { value: "any", label: "Any result" },
            { value: "win", label: "Wins" },
            { value: "loss", label: "Losses" },
            { value: "draw", label: "Draws" },
          ]}
        />
        <TextInput size="xs" w={200} placeholder="Opening, event or opponent" value={opening} onChange={(e) => setOpening(e.currentTarget.value)} aria-label="Filter games" />
        <SegmentedControl
          size="xs"
          value={sort}
          onChange={(v) => setSort(v as "new" | "old")}
          data={[
            { value: "new", label: "Newest" },
            { value: "old", label: "Oldest" },
          ]}
        />
        {games && (
          <Text size="xs" c="dimmed">
            {games.total.toLocaleString()} games
          </Text>
        )}
      </Group>
      <Table striped highlightOnHover verticalSpacing={4} aria-label={`${player.name}'s games`}>
        <Table.Thead>
          <Table.Tr>
            <Table.Th>Game</Table.Th>
            <Table.Th>Opening</Table.Th>
            <Table.Th>Result</Table.Th>
            <Table.Th>Moves</Table.Th>
            <Table.Th />
          </Table.Tr>
        </Table.Thead>
        <Table.Tbody>
          {games?.rows.map((g) => (
            <Table.Tr key={g.id}>
              <Table.Td>
                <Text size="sm">
                  {g.white ?? "?"} – {g.black ?? "?"}
                </Text>
                <Text size="xs" c="dimmed">
                  {yearOf(g.date)}
                </Text>
              </Table.Td>
              <Table.Td>
                <Text size="xs">{[g.eco, g.opening].filter(Boolean).join(" ")}</Text>
              </Table.Td>
              <Table.Td>
                <Text size="sm" className="tabular">
                  {g.result}
                </Text>
              </Table.Td>
              <Table.Td>
                <Text size="sm" className="tabular">
                  {Math.ceil(g.plyCount / 2)}
                </Text>
              </Table.Td>
              <Table.Td>
                <Group gap={4} justify="flex-end" wrap="nowrap">
                  <Button size="compact-xs" variant="light" color="teal" leftSection={<IconSchool size={12} />} onClick={() => study("replay", g)}>
                    Guided replay
                  </Button>
                  <Button size="compact-xs" variant="light" leftSection={<IconQuestionMark size={12} />} onClick={() => study("guess", g)}>
                    Guess the move
                  </Button>
                  <Button size="compact-xs" variant="subtle" leftSection={<IconEye size={12} />} onClick={() => void openDbGame(g.id)}>
                    Open
                  </Button>
                </Group>
              </Table.Td>
            </Table.Tr>
          ))}
        </Table.Tbody>
      </Table>
      {games && games.total > PAGE && <Pagination mt="sm" size="sm" total={Math.ceil(games.total / PAGE)} value={page} onChange={setPage} />}
      {games && games.total === 0 && (
        <Text size="sm" c="dimmed" mt="xs">
          No games match these filters.
        </Text>
      )}
      <Group gap={6} mt="md">
        <IconBook2 size={14} />
        <Text size="xs" c="dimmed">
          Tip: in Guess the move you can add the master's opening to your own repertoire.
        </Text>
      </Group>
      <Group gap={6} mt={4}>
        <IconPlayerPlay size={14} />
        <Text size="xs" c="dimmed">
          Games from PGN Mentor (pgnmentor.com), downloaded when you asked for them.
        </Text>
      </Group>
    </div>
  );
}
