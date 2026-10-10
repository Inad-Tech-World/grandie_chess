import { useEffect, useMemo, useState } from "react";
import {
  Badge,
  Button,
  Card,
  Group,
  Modal,
  NumberInput,
  Progress,
  SegmentedControl,
  Select,
  Stack,
  Switch,
  Text,
  TextInput,
  Title,
} from "@mantine/core";
import { IconChessKing, IconChessKingFilled, IconArrowsShuffle, IconPlayerPlay } from "@tabler/icons-react";
import { commands, notifyError, unwrap, type OpponentLevel, type PlaySetup, type SideChoice, type TimeControl } from "../lib/api";
import { formatBytes, formatTimeControl } from "../lib/format";
import { useApp } from "../stores/app";
import { downloadFraction, useDownloads } from "../stores/downloads";
import { usePlay } from "../stores/play";

type OpponentType = "maia" | "leela" | "full";

const PRESETS: { label: string; tc: TimeControl | null }[] = [
  { label: "Untimed", tc: null },
  { label: "1+0", tc: { initialMs: 60_000, incrementMs: 0 } },
  { label: "3+0", tc: { initialMs: 180_000, incrementMs: 0 } },
  { label: "3+2", tc: { initialMs: 180_000, incrementMs: 2_000 } },
  { label: "5+3", tc: { initialMs: 300_000, incrementMs: 3_000 } },
  { label: "10+0", tc: { initialMs: 600_000, incrementMs: 0 } },
  { label: "10+5", tc: { initialMs: 600_000, incrementMs: 5_000 } },
  { label: "15+10", tc: { initialMs: 900_000, incrementMs: 10_000 } },
  { label: "30+0", tc: { initialMs: 1_800_000, incrementMs: 0 } },
];

interface StoredSetup {
  type: OpponentType;
  maia: string;
  leela: string;
  side: SideChoice;
  preset: string;
  customMinutes: number;
  customIncrement: number;
  takebacks: boolean;
}

const STORAGE_KEY = "grandie-last-setup";

function loadSetup(): StoredSetup {
  const fallback: StoredSetup = {
    type: "maia",
    maia: "maia-1500",
    leela: "leela-3",
    side: "random",
    preset: "10+5",
    customMinutes: 10,
    customIncrement: 5,
    takebacks: true,
  };
  try {
    return { ...fallback, ...JSON.parse(localStorage.getItem(STORAGE_KEY) ?? "{}") };
  } catch {
    return fallback;
  }
}

const OPPONENT_INFO: Record<OpponentType, string> = {
  maia: "Maia networks were trained on millions of human games at each rating. They make the mistakes real players make, which is what you want to practise against.",
  leela: "Leela with a cap on how far she searches each move, plus some variety in the opening. Ratings are approximate.",
  full: "Leela at full strength on your Mac's fastest backend. Expect to lose; use it for a challenge.",
};

export function NewGame({ tabId }: { tabId: string }) {
  const [levels, setLevels] = useState<OpponentLevel[]>([]);
  const [setup, setSetupState] = useState<StoredSetup>(loadSetup);
  const [fenEnabled, setFenEnabled] = useState(false);
  const [fen, setFen] = useState("");
  const [fenError, setFenError] = useState<string | null>(null);
  const [starting, setStarting] = useState(false);
  const [missing, setMissing] = useState<string[] | null>(null);
  const replaceTab = useApp((s) => s.replaceTab);
  const { networks, progress, refresh, download } = useDownloads();

  useEffect(() => {
    void commands.levelsList().then(setLevels);
    void refresh();
  }, [refresh]);

  const setSetup = (patch: Partial<StoredSetup>) => {
    setSetupState((s) => {
      const next = { ...s, ...patch };
      localStorage.setItem(STORAGE_KEY, JSON.stringify(next));
      return next;
    });
  };

  const maiaLevels = levels.filter((l) => l.kind === "maia");
  const leelaLevels = levels.filter((l) => l.kind === "leela");
  const opponentId = setup.type === "maia" ? setup.maia : setup.type === "leela" ? setup.leela : "full";
  const level = levels.find((l) => l.id === opponentId);

  const timeControl: TimeControl | null = useMemo(() => {
    if (setup.preset === "custom") {
      return { initialMs: Math.round(setup.customMinutes * 60_000), incrementMs: Math.round(setup.customIncrement * 1000) };
    }
    return PRESETS.find((p) => p.label === setup.preset)?.tc ?? null;
  }, [setup.preset, setup.customMinutes, setup.customIncrement]);

  async function validateFen(value: string) {
    setFen(value);
    if (!value.trim()) {
      setFenError(null);
      return;
    }
    const r = await commands.chessPositionInfo(value);
    setFenError(r.status === "error" ? r.error : r.data.outcome ? "That position is already finished" : null);
  }

  async function start() {
    setStarting(true);
    try {
      const need = await unwrap(commands.playMissingNetworks(opponentId));
      if (need.length) {
        setMissing(need);
        return;
      }
      const playSetup: PlaySetup = {
        opponent: opponentId,
        userSide: setup.side,
        timeControl,
        startFen: fenEnabled && fen.trim() ? fen.trim() : null,
        allowTakebacks: setup.takebacks,
      };
      const snap = await unwrap(commands.playStart(playSetup));
      usePlay.getState().setSnapshot(snap);
      replaceTab(tabId, {
        id: tabId,
        kind: "play",
        sessionId: snap.sessionId,
        title: `vs ${snap.userSide === "white" ? snap.blackName : snap.whiteName}`,
      });
    } catch (e) {
      notifyError("Could not start the game", e);
    } finally {
      setStarting(false);
    }
  }

  async function downloadMissing() {
    if (!missing) return;
    try {
      for (const id of missing) await download(id);
      setMissing(null);
      await start();
    } catch (e) {
      notifyError("Download failed", e);
    }
  }

  const missingEntries = networks.filter((n) => missing?.includes(n.id));
  const missingBytes = missingEntries.reduce((sum, n) => sum + (n.size ?? 0), 0);

  return (
    <div className="screen" style={{ maxWidth: 760 }}>
      <Title order={2} mb={4}>
        New game
      </Title>
      <Text c="dimmed" mb="lg">
        Choose an opponent at your level. Every game is saved and can be reviewed afterwards.
      </Text>

      <Stack gap="lg">
        <Card withBorder radius="lg" padding="lg">
          <Text fw={600} mb="xs">
            Opponent
          </Text>
          <SegmentedControl
            fullWidth
            value={setup.type}
            onChange={(v) => setSetup({ type: v as OpponentType })}
            data={[
              { value: "maia", label: "Human-like (Maia)" },
              { value: "leela", label: "Leela levels" },
              { value: "full", label: "Full strength" },
            ]}
          />
          <Text size="sm" c="dimmed" mt="sm">
            {OPPONENT_INFO[setup.type]}
          </Text>
          {setup.type === "maia" && (
            <SegmentedControl
              mt="md"
              fullWidth
              value={setup.maia}
              onChange={(v) => setSetup({ maia: v })}
              data={maiaLevels.map((l) => ({ value: l.id, label: String(l.elo) }))}
            />
          )}
          {setup.type === "leela" && (
            <SegmentedControl
              mt="md"
              fullWidth
              value={setup.leela}
              onChange={(v) => setSetup({ leela: v })}
              data={leelaLevels.map((l) => ({ value: l.id, label: l.name.replace("Leela ", "") }))}
            />
          )}
          {level && setup.type !== "full" && (
            <Group mt="sm" gap="xs">
              <Badge variant="light" color="steel">
                {level.name}
              </Badge>
              <Text size="sm" c="dimmed">
                about {level.elo} on a Lichess-like scale
                {level.nodes ? ` · ${level.nodes} node${level.nodes === 1 ? "" : "s"} per move` : ""}
              </Text>
            </Group>
          )}
        </Card>

        <Card withBorder radius="lg" padding="lg">
          <Group grow align="flex-start">
            <div>
              <Text fw={600} mb="xs">
                You play
              </Text>
              <SegmentedControl
                fullWidth
                value={setup.side}
                onChange={(v) => setSetup({ side: v as SideChoice })}
                data={[
                  { value: "white", label: <Group gap={6} justify="center"><IconChessKing size={16} />White</Group> },
                  { value: "random", label: <Group gap={6} justify="center"><IconArrowsShuffle size={16} />Random</Group> },
                  { value: "black", label: <Group gap={6} justify="center"><IconChessKingFilled size={16} />Black</Group> },
                ]}
              />
            </div>
            <div>
              <Text fw={600} mb="xs">
                Time control
              </Text>
              <Select
                value={setup.preset}
                onChange={(v) => v && setSetup({ preset: v })}
                data={[...PRESETS.map((p) => ({ value: p.label, label: p.label === "Untimed" ? "Untimed" : `${p.label} (minutes + seconds)` })), { value: "custom", label: "Custom…" }]}
                allowDeselect={false}
              />
            </div>
          </Group>
          {setup.preset === "custom" && (
            <Group mt="sm" grow>
              <NumberInput label="Minutes" min={0.5} max={180} step={0.5} decimalScale={1} value={setup.customMinutes} onChange={(v) => setSetup({ customMinutes: Number(v) || 1 })} />
              <NumberInput label="Increment (seconds)" min={0} max={60} value={setup.customIncrement} onChange={(v) => setSetup({ customIncrement: Number(v) || 0 })} />
            </Group>
          )}
          <Group mt="md" gap="xl">
            <Switch label="Allow takebacks" checked={setup.takebacks} onChange={(e) => setSetup({ takebacks: e.currentTarget.checked })} />
            <Switch label="Start from a position" checked={fenEnabled} onChange={(e) => setFenEnabled(e.currentTarget.checked)} />
          </Group>
          {fenEnabled && (
            <TextInput
              mt="sm"
              placeholder="Paste a FEN, e.g. r1bqkbnr/pppp1ppp/2n5/4p3/4P3/5N2/PPPP1PPP/RNBQKB1R w KQkq - 2 3"
              value={fen}
              onChange={(e) => void validateFen(e.currentTarget.value)}
              error={fenError}
              spellCheck={false}
            />
          )}
        </Card>

        <Group justify="space-between">
          <Text size="sm" c="dimmed">
            {level?.name ?? "Leela"} · {formatTimeControl(timeControl)} · {setup.side === "random" ? "random colour" : `you play ${setup.side}`}
          </Text>
          <Button size="md" leftSection={<IconPlayerPlay size={18} />} loading={starting} disabled={!!fenError || levels.length === 0} onClick={() => void start()}>
            Play
          </Button>
        </Group>
      </Stack>

      <Modal opened={!!missing} onClose={() => setMissing(null)} title="Download needed" centered>
        <Text size="sm" mb="md">
          This opponent needs a network that isn't on your Mac yet ({formatBytes(missingBytes)}). It downloads once and is stored in Grandie's
          data folder.
        </Text>
        <Stack gap="xs" mb="md">
          {missingEntries.map((n) => (
            <div key={n.id}>
              <Group justify="space-between">
                <Text size="sm">{n.name}</Text>
                <Text size="xs" c="dimmed">
                  {formatBytes(n.size)}
                </Text>
              </Group>
              <Progress value={downloadFraction(progress[n.id]) * 100} size="sm" mt={4} />
            </div>
          ))}
        </Stack>
        <Group justify="flex-end">
          <Button variant="default" onClick={() => setMissing(null)}>
            Cancel
          </Button>
          <Button onClick={() => void downloadMissing()}>Download and play</Button>
        </Group>
      </Modal>
    </div>
  );
}
