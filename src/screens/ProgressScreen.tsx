import { useEffect, useState } from "react";
import { Card, Group, Select, SimpleGrid, Table, Text, Title, Tooltip as MTooltip } from "@mantine/core";
import { Bar, BarChart, CartesianGrid, Line, LineChart, ReferenceLine, ResponsiveContainer, Tooltip, XAxis, YAxis } from "recharts";
import { commands, type ProgressSummary } from "../lib/api";
import { openDbGame } from "../lib/actions";

const day = (secs: number | null | undefined) =>
  secs ? new Date(secs * 1000).toLocaleDateString(undefined, { month: "short", day: "numeric" }) : "";

function Tile({ label, value, sub }: { label: string; value: string; sub?: string }) {
  return (
    <Card withBorder radius="lg" padding="md">
      <Text size="xs" c="dimmed">
        {label}
      </Text>
      <Text fw={700} size="xl" className="tabular">
        {value}
      </Text>
      {sub && (
        <Text size="xs" c="dimmed">
          {sub}
        </Text>
      )}
    </Card>
  );
}

function ChartCard({ title, children, empty }: { title: string; children: React.ReactNode; empty?: string | null }) {
  return (
    <Card withBorder radius="lg" padding="md">
      <Text fw={600} size="sm" mb="xs">
        {title}
      </Text>
      {empty ? (
        <Text size="sm" c="dimmed" py="lg" ta="center">
          {empty}
        </Text>
      ) : (
        <div style={{ height: 190 }}>{children}</div>
      )}
    </Card>
  );
}

const axis = { stroke: "var(--text-dim)", fontSize: 11, tickLine: false, axisLine: false } as const;
const tooltipStyle = {
  contentStyle: { background: "var(--surface-raised)", border: "1px solid var(--hairline)", borderRadius: 8, fontSize: 12 },
  labelStyle: { color: "var(--mantine-color-text)" },
  itemStyle: { color: "var(--mantine-color-text)" },
};

export function ProgressScreen() {
  const [data, setData] = useState<ProgressSummary | null>(null);
  const [level, setLevel] = useState<string | null>(null);
  useEffect(() => {
    void commands.progressSummary().then((r) => r.status === "ok" && setData(r.data));
  }, []);

  if (!data) return null;
  const t = data.totals;
  const winRate = t.games ? Math.round(((t.wins + t.draws / 2) / t.games) * 100) : null;
  const strength = data.strength.map((p) => ({ at: day(p.at), rating: Math.round(p.rating ?? 0) }));
  const quality = data.quality.map((q, i) => ({
    game: i + 1,
    id: q.gameId,
    label: `${day(q.at)}${q.opponent ? ` vs ${q.opponent}` : ""}`,
    accuracy: q.accuracy !== null ? Math.round(q.accuracy) : null,
    errors: q.mistakes + q.blunders,
    detail: `${q.inaccuracies} inaccuracies · ${q.mistakes} mistakes · ${q.blunders} blunders`,
  }));
  const puzzles = data.puzzleHistory.map((p, i) => ({ n: i + 1, rating: Math.round(p.rating ?? 0) }));
  const series = data.levelSeries.find((s) => s.level === level) ?? data.levelSeries[0];
  const seriesPoints = (series?.points ?? []).map((p, i) => ({ game: i + 1, at: day(p.at), rating: Math.round(p.rating ?? 0) }));

  return (
    <div className="screen">
      <Title order={2} mb={4}>
        Progress
      </Title>
      <Text c="dimmed" mb="lg">
        How you're doing against Grandie's opponents, in your reviewed games, and in puzzles.
      </Text>

      <SimpleGrid cols={{ base: 2, md: 4 }} mb="lg">
        <Tile label="Games" value={t.games.toLocaleString()} sub={`${t.wins} won · ${t.draws} drawn · ${t.losses} lost`} />
        <Tile label="Score" value={winRate !== null ? `${winRate}%` : "–"} sub={`${t.gamesLast30Days} games in the last 30 days`} />
        <Tile
          label="Estimated strength"
          value={strength.length ? String(strength[strength.length - 1].rating) : "–"}
          sub="Performance over your last 10 rated games"
        />
        <Tile
          label="Puzzle rating"
          value={String(Math.round(data.puzzleRating.rating ?? 1500))}
          sub={t.puzzlesAttempted ? `${t.puzzlesSolved} of ${t.puzzlesAttempted} solved` : "No puzzles yet"}
        />
      </SimpleGrid>

      <SimpleGrid cols={{ base: 1, md: 2 }} mb="lg">
        <ChartCard title="Estimated strength over time" empty={strength.length < 2 ? "Play a few rated games (against Grandie levels or imported) to see a trend." : null}>
          <ResponsiveContainer>
            <LineChart data={strength} margin={{ top: 6, right: 8, bottom: 0, left: -12 }}>
              <CartesianGrid stroke="var(--hairline)" vertical={false} />
              <XAxis dataKey="at" {...axis} minTickGap={24} />
              <YAxis {...axis} domain={["dataMin - 50", "dataMax + 50"]} allowDecimals={false} />
              <Tooltip {...tooltipStyle} />
              <Line type="monotone" dataKey="rating" name="Strength" stroke="var(--mantine-color-blue-5)" strokeWidth={2} dot={false} activeDot={{ r: 4 }} isAnimationActive={false} />
            </LineChart>
          </ResponsiveContainer>
        </ChartCard>
        <Card withBorder radius="lg" padding="md">
          <Group justify="space-between" mb="xs">
            <Text fw={600} size="sm">
              Against each level over time
            </Text>
            {data.levelSeries.length > 0 && (
              <Select
                size="xs"
                w={170}
                value={series?.level ?? null}
                onChange={setLevel}
                data={data.levelSeries.map((s) => ({ value: s.level, label: s.name }))}
                allowDeselect={false}
                aria-label="Opponent level"
              />
            )}
          </Group>
          {seriesPoints.length < 2 ? (
            <Text size="sm" c="dimmed" py="lg" ta="center">
              Play a few games against the same level to see how your results change.
            </Text>
          ) : (
            <div style={{ height: 190 }}>
              <ResponsiveContainer>
                <LineChart data={seriesPoints} margin={{ top: 6, right: 8, bottom: 0, left: -12 }}>
                  <CartesianGrid stroke="var(--hairline)" vertical={false} />
                  <XAxis dataKey="game" {...axis} />
                  <YAxis {...axis} domain={["dataMin - 100", "dataMax + 100"]} allowDecimals={false} />
                  {series?.elo ? (
                    <ReferenceLine y={series.elo} stroke="var(--text-dim)" strokeDasharray="4 4" label={{ value: `${series.name} ≈${series.elo}`, fill: "var(--text-dim)", fontSize: 11, position: "insideTopLeft" }} />
                  ) : null}
                  <Tooltip {...tooltipStyle} labelFormatter={(_, p) => `Game ${p?.[0]?.payload?.game ?? ""} · ${p?.[0]?.payload?.at ?? ""}`} formatter={(v) => [v, "Performance"]} />
                  <Line type="monotone" dataKey="rating" stroke="var(--mantine-color-blue-5)" strokeWidth={2} dot={{ r: 3 }} activeDot={{ r: 5 }} isAnimationActive={false} />
                </LineChart>
              </ResponsiveContainer>
            </div>
          )}
        </Card>
        <ChartCard title="Puzzle rating" empty={puzzles.length < 2 ? "Solve a few puzzles to see your rating move." : null}>
          <ResponsiveContainer>
            <LineChart data={puzzles} margin={{ top: 6, right: 8, bottom: 0, left: -12 }}>
              <CartesianGrid stroke="var(--hairline)" vertical={false} />
              <XAxis dataKey="n" {...axis} minTickGap={24} />
              <YAxis {...axis} domain={["dataMin - 30", "dataMax + 30"]} allowDecimals={false} />
              <Tooltip {...tooltipStyle} labelFormatter={(n) => `Puzzle ${n}`} />
              <Line type="monotone" dataKey="rating" name="Rating" stroke="var(--mantine-color-blue-5)" strokeWidth={2} dot={false} activeDot={{ r: 4 }} isAnimationActive={false} />
            </LineChart>
          </ResponsiveContainer>
        </ChartCard>
        <ChartCard title="Accuracy in reviewed games" empty={quality.length === 0 ? "Review your games (it starts automatically after each game) to track accuracy." : null}>
          <ResponsiveContainer>
            <LineChart data={quality} margin={{ top: 6, right: 8, bottom: 0, left: -12 }} onClick={(s) => s?.activeIndex !== undefined && quality[Number(s.activeIndex)] && void openDbGame(quality[Number(s.activeIndex)].id)}>
              <CartesianGrid stroke="var(--hairline)" vertical={false} />
              <XAxis dataKey="game" {...axis} />
              <YAxis {...axis} domain={[0, 100]} />
              <Tooltip {...tooltipStyle} labelFormatter={(_, p) => p?.[0]?.payload?.label ?? ""} formatter={(v) => [`${v}%`, "Accuracy"]} />
              <Line type="monotone" dataKey="accuracy" stroke="var(--annot-best)" strokeWidth={2} dot={{ r: 3 }} activeDot={{ r: 5 }} connectNulls isAnimationActive={false} />
            </LineChart>
          </ResponsiveContainer>
        </ChartCard>
        <ChartCard title="Mistakes and blunders per game" empty={quality.length === 0 ? "No reviewed games yet." : null}>
          <ResponsiveContainer>
            <BarChart data={quality} margin={{ top: 6, right: 8, bottom: 0, left: -12 }}>
              <CartesianGrid stroke="var(--hairline)" vertical={false} />
              <XAxis dataKey="game" {...axis} />
              <YAxis {...axis} allowDecimals={false} />
              <Tooltip
                {...tooltipStyle}
                cursor={{ fill: "var(--hairline)" }}
                labelFormatter={(_, p) => p?.[0]?.payload?.label ?? ""}
                formatter={(v, _n, item) => [`${v} (${item.payload.detail})`, "Errors"]}
              />
              <Bar dataKey="errors" fill="var(--annot-mistake)" radius={[4, 4, 0, 0]} maxBarSize={22} isAnimationActive={false} />
            </BarChart>
          </ResponsiveContainer>
        </ChartCard>
      </SimpleGrid>

      <SimpleGrid cols={{ base: 1, md: 2 }}>
        <Card withBorder radius="lg" padding="md">
          <Text fw={600} size="sm" mb="xs">
            Results against each opponent
          </Text>
          {data.levels.length === 0 ? (
            <Text size="sm" c="dimmed">
              Play games against Leela and Maia to see how you do at each level.
            </Text>
          ) : (
            <Table verticalSpacing={4} fz="sm">
              <Table.Thead>
                <Table.Tr>
                  <Table.Th>Opponent</Table.Th>
                  <Table.Th ta="right">Games</Table.Th>
                  <Table.Th ta="right">W / D / L</Table.Th>
                  <Table.Th ta="right">
                    <MTooltip label="Your performance rating against this level">
                      <span>Perf.</span>
                    </MTooltip>
                  </Table.Th>
                </Table.Tr>
              </Table.Thead>
              <Table.Tbody>
                {data.levels.map((l) => (
                  <Table.Tr key={l.level}>
                    <Table.Td>
                      {l.name} {l.elo ? <span className="muted">≈{l.elo}</span> : null}
                    </Table.Td>
                    <Table.Td ta="right" className="tabular">
                      {l.games}
                    </Table.Td>
                    <Table.Td ta="right" className="tabular">
                      {l.wins} / {l.draws} / {l.losses}
                    </Table.Td>
                    <Table.Td ta="right" className="tabular">
                      {l.performance ? Math.round(l.performance) : "–"}
                    </Table.Td>
                  </Table.Tr>
                ))}
              </Table.Tbody>
            </Table>
          )}
        </Card>
        <Card withBorder radius="lg" padding="md">
          <Text fw={600} size="sm" mb="xs">
            Weakest openings
          </Text>
          {data.weakestOpenings.length === 0 ? (
            <Text size="sm" c="dimmed">
              Openings appear here once you've played at least three games in them.
            </Text>
          ) : (
            <Table verticalSpacing={4} fz="sm">
              <Table.Thead>
                <Table.Tr>
                  <Table.Th>Opening</Table.Th>
                  <Table.Th ta="right">Games</Table.Th>
                  <Table.Th ta="right">Score</Table.Th>
                  <Table.Th ta="right">Accuracy</Table.Th>
                </Table.Tr>
              </Table.Thead>
              <Table.Tbody>
                {data.weakestOpenings.map((o) => (
                  <Table.Tr key={o.family}>
                    <Table.Td>
                      <Group gap={6} wrap="nowrap">
                        {o.eco && <span className="muted">{o.eco}</span>}
                        <span>{o.family}</span>
                      </Group>
                    </Table.Td>
                    <Table.Td ta="right" className="tabular">
                      {o.games}
                    </Table.Td>
                    <Table.Td ta="right" className="tabular">
                      {Math.round((o.score ?? 0) * 100)}%
                    </Table.Td>
                    <Table.Td ta="right" className="tabular">
                      {o.accuracy !== null ? `${Math.round(o.accuracy)}%` : "–"}
                    </Table.Td>
                  </Table.Tr>
                ))}
              </Table.Tbody>
            </Table>
          )}
        </Card>
      </SimpleGrid>
    </div>
  );
}
