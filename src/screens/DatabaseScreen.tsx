import { useCallback, useEffect, useRef, useState } from "react";
import { Badge, Button, CloseButton, Group, Select, Switch, Text, TextInput, Title, Tooltip } from "@mantine/core";
import { useDebouncedValue } from "@mantine/hooks";
import { modals } from "@mantine/modals";
import { IconChessBishop, IconDatabaseImport, IconSearch, IconTrash } from "@tabler/icons-react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { commands, notifyError, unwrap, type DbStats, type GameQuery, type GameRow, type PositionQuery, type SortColumn } from "../lib/api";
import { ImportModal } from "../database/ImportModal";
import { PositionSearchModal } from "../database/PositionSearchModal";
import { openDbGame } from "../lib/actions";

const PAGE = 200;

function resultClass(row: GameRow): string {
  if (!row.userColor || row.result === "1/2-1/2" || row.result === "*") return "";
  const won = (row.userColor === "white" && row.result === "1-0") || (row.userColor === "black" && row.result === "0-1");
  return won ? "result-win" : "result-loss";
}

function player(name: string | null, elo: number | null, isUser: boolean) {
  return (
    <span>
      <span style={{ fontWeight: isUser ? 650 : 400 }}>{name ?? "?"}</span>
      {elo ? <span className="muted"> {elo}</span> : null}
    </span>
  );
}

export function DatabaseScreen() {
  const [stats, setStats] = useState<DbStats | null>(null);
  const [text, setText] = useState("");
  const [debouncedText] = useDebouncedValue(text, 250);
  const [filters, setFilters] = useState<Omit<GameQuery, "offset" | "limit" | "text">>({
    color: null,
    outcome: null,
    onlyMine: false,
    source: null,
    speed: null,
    eco: null,
    dateFrom: null,
    dateTo: null,
    position: null,
    sort: null,
    desc: true,
  });
  const [rows, setRows] = useState<GameRow[]>([]);
  const [total, setTotal] = useState(0);
  const [elapsed, setElapsed] = useState<number | null>(null);
  const [loading, setLoading] = useState(false);
  const [selected, setSelected] = useState<Set<number>>(new Set());
  const [importOpen, setImportOpen] = useState(false);
  const [positionOpen, setPositionOpen] = useState(false);
  const generation = useRef(0);
  const scrollRef = useRef<HTMLDivElement>(null);

  const query: GameQuery = { ...filters, text: debouncedText || null, offset: 0, limit: PAGE };
  const queryKey = JSON.stringify(query);

  const refreshStats = useCallback(() => void commands.dbStats().then((r) => r.status === "ok" && setStats(r.data)), []);

  const load = useCallback(
    async (offset: number) => {
      const gen = offset === 0 ? ++generation.current : generation.current;
      setLoading(true);
      try {
        const page = await unwrap(commands.dbQuery({ ...(JSON.parse(queryKey) as GameQuery), offset }));
        if (gen !== generation.current) return;
        setTotal(page.total);
        setElapsed(page.elapsedMs);
        setRows((prev) => (offset === 0 ? page.rows : [...prev, ...page.rows]));
      } catch (e) {
        notifyError("Search failed", e);
      } finally {
        setLoading(false);
      }
    },
    [queryKey],
  );

  useEffect(() => {
    void commands.dbImportGrandie().then(() => {
      refreshStats();
      void load(0);
    });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    setSelected(new Set());
    scrollRef.current?.scrollTo({ top: 0 });
    void load(0);
  }, [load]);

  const virtualizer = useVirtualizer({
    count: rows.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => 34,
    overscan: 12,
  });
  const items = virtualizer.getVirtualItems();
  const lastIndex = items.length ? items[items.length - 1].index : 0;
  useEffect(() => {
    if (!loading && rows.length < total && lastIndex > rows.length - 40) void load(rows.length);
  }, [lastIndex, rows.length, total, loading, load]);

  const set = (patch: Partial<typeof filters>) => setFilters((f) => ({ ...f, ...patch }));
  const sortBy = (col: SortColumn) =>
    set(filters.sort === col ? { desc: !filters.desc } : { sort: col, desc: col === "date" || col === "whiteElo" || col === "blackElo" });
  const header = (label: string, col?: SortColumn) =>
    col ? (
      <button onClick={() => sortBy(col)} aria-label={`Sort by ${label}`}>
        {label}
        {filters.sort === col || (!filters.sort && col === "date") ? (filters.desc || !filters.sort ? " ↓" : " ↑") : ""}
      </button>
    ) : (
      <span>{label}</span>
    );

  function toggle(id: number, additive: boolean) {
    setSelected((s) => {
      const next = new Set(additive ? s : []);
      if (s.has(id) && additive) next.delete(id);
      else next.add(id);
      return next;
    });
  }

  function deleteSelected() {
    modals.openConfirmModal({
      title: `Delete ${selected.size} game${selected.size === 1 ? "" : "s"}?`,
      children: <Text size="sm">They are removed from Grandie's database (your PGN files and online accounts are not touched).</Text>,
      labels: { confirm: "Delete", cancel: "Cancel" },
      confirmProps: { color: "red" },
      onConfirm: async () => {
        await unwrap(commands.dbDelete([...selected])).catch((e) => notifyError("Delete failed", e));
        setSelected(new Set());
        refreshStats();
        void load(0);
      },
    });
  }

  const positionLabel = (p: PositionQuery) => (p.exact ? "Exact position" : "Piece pattern");

  return (
    <div style={{ height: "100%", display: "flex", flexDirection: "column", padding: "18px 24px 12px", boxSizing: "border-box" }}>
      <Group justify="space-between" mb={10}>
        <div>
          <Title order={2}>Database</Title>
          {stats && (
            <Text size="sm" c="dimmed">
              {stats.games.toLocaleString()} games · {stats.mine.toLocaleString()} yours · {stats.reviewed.toLocaleString()} reviewed
            </Text>
          )}
        </div>
        <Group gap={8}>
          {selected.size > 0 && (
            <Button variant="default" color="red" leftSection={<IconTrash size={16} />} onClick={deleteSelected}>
              Delete {selected.size}
            </Button>
          )}
          <Button leftSection={<IconDatabaseImport size={16} />} onClick={() => setImportOpen(true)}>
            Import games
          </Button>
        </Group>
      </Group>

      <Group gap={8} mb={10} wrap="wrap">
        <TextInput
          placeholder="Player, event or opening"
          leftSection={<IconSearch size={15} />}
          value={text}
          onChange={(e) => setText(e.currentTarget.value)}
          w={230}
          size="xs"
          aria-label="Search games"
        />
        <Select size="xs" w={110} placeholder="Colour" clearable value={filters.color} onChange={(v) => set({ color: v as GameQuery["color"] })} data={[{ value: "white", label: "As White" }, { value: "black", label: "As Black" }]} aria-label="Colour" />
        <Select size="xs" w={100} placeholder="Result" clearable value={filters.outcome} onChange={(v) => set({ outcome: v as GameQuery["outcome"] })} data={[{ value: "win", label: "Wins" }, { value: "loss", label: "Losses" }, { value: "draw", label: "Draws" }]} aria-label="Result" />
        <Select size="xs" w={120} placeholder="Speed" clearable value={filters.speed} onChange={(v) => set({ speed: v })} data={["bullet", "blitz", "rapid", "classical", "correspondence"].map((s) => ({ value: s, label: s[0].toUpperCase() + s.slice(1) }))} aria-label="Speed" />
        <Select size="xs" w={120} placeholder="Source" clearable value={filters.source} onChange={(v) => set({ source: v })} data={[{ value: "grandie", label: "Grandie" }, { value: "lichess", label: "Lichess" }, { value: "chesscom", label: "Chess.com" }, { value: "pgn", label: "PGN files" }]} aria-label="Source" />
        <TextInput size="xs" w={70} placeholder="ECO" value={filters.eco ?? ""} onChange={(e) => set({ eco: e.currentTarget.value || null })} aria-label="ECO code" />
        <TextInput size="xs" type="date" w={130} value={filters.dateFrom ?? ""} onChange={(e) => set({ dateFrom: e.currentTarget.value || null })} aria-label="From date" />
        <TextInput size="xs" type="date" w={130} value={filters.dateTo ?? ""} onChange={(e) => set({ dateTo: e.currentTarget.value || null })} aria-label="To date" />
        <Switch size="xs" label="Only my games" checked={filters.onlyMine} onChange={(e) => set({ onlyMine: e.currentTarget.checked })} />
        {filters.position ? (
          <Badge size="lg" variant="light" rightSection={<CloseButton size="xs" onClick={() => set({ position: null })} aria-label="Clear position search" />}>
            {positionLabel(filters.position)}
          </Badge>
        ) : (
          <Button size="xs" variant="default" leftSection={<IconChessBishop size={14} />} onClick={() => setPositionOpen(true)}>
            Position…
          </Button>
        )}
      </Group>

      <div className="panel db-table" style={{ flex: 1, minHeight: 0, display: "flex", flexDirection: "column", overflow: "hidden" }}>
        <div className="db-row db-head" role="row">
          {header("Date", "date")}
          {header("White", "white")}
          {header("Black", "black")}
          {header("Result", "result")}
          {header("Opening", "eco")}
          <span>Time</span>
          {header("Moves", "moves")}
          <span>Accuracy</span>
        </div>
        <div ref={scrollRef} style={{ flex: 1, overflow: "auto" }} role="rowgroup" aria-label="Games">
          {rows.length === 0 && !loading ? (
            <Group justify="center" p="xl">
              <Text c="dimmed" size="sm">
                {stats?.games ? "No games match these filters." : "No games yet. Play a game in Grandie or import your games from Lichess, Chess.com or a PGN file."}
              </Text>
            </Group>
          ) : (
            <div style={{ height: virtualizer.getTotalSize(), position: "relative" }}>
              {items.map((item) => {
                const row = rows[item.index];
                return (
                  <div
                    key={row.id}
                    className="db-row"
                    role="row"
                    data-selected={selected.has(row.id)}
                    style={{ position: "absolute", top: 0, left: 0, right: 0, transform: `translateY(${item.start}px)` }}
                    onClick={(e) => (e.metaKey || e.shiftKey ? toggle(row.id, true) : toggle(row.id, false))}
                    onDoubleClick={() => void openDbGame(row.id)}
                    title="Double-click to open"
                  >
                    <span className="tabular">{row.date?.replace(/\./g, "-") ?? ""}</span>
                    {player(row.white, row.whiteElo, row.userColor === "white")}
                    {player(row.black, row.blackElo, row.userColor === "black")}
                    <span className={`tabular ${resultClass(row)}`}>{row.result}</span>
                    <span>
                      {row.eco && <span className="muted">{row.eco} </span>}
                      {row.opening ?? ""}
                    </span>
                    <span className="muted">{row.timeControl ?? row.speed ?? ""}</span>
                    <span className="tabular">{Math.ceil(row.plyCount / 2)}</span>
                    <span className="tabular">
                      {row.reviewed ? (
                        <Tooltip label="Your accuracy in the review">
                          <span>{row.userAccuracy !== null ? `${Math.round(row.userAccuracy)}%` : "✓"}</span>
                        </Tooltip>
                      ) : (
                        ""
                      )}
                    </span>
                  </div>
                );
              })}
            </div>
          )}
        </div>
      </div>
      <Group justify="space-between" mt={6}>
        <Text size="xs" c="dimmed" aria-live="polite">
          {total.toLocaleString()} game{total === 1 ? "" : "s"}
          {elapsed !== null ? ` · ${elapsed < 1 ? "<1" : Math.round(elapsed)} ms` : ""}
        </Text>
        <Text size="xs" c="dimmed">
          Double-click a game to open it · ⌘-click to select several
        </Text>
      </Group>

      <ImportModal
        opened={importOpen}
        onClose={() => setImportOpen(false)}
        onImported={() => {
          refreshStats();
          void load(0);
        }}
      />
      <PositionSearchModal opened={positionOpen} onClose={() => setPositionOpen(false)} onSearch={(position) => set({ position })} />
    </div>
  );
}
