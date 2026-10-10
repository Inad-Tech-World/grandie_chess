import { useEffect, useState } from "react";
import { ActionIcon, Badge, Chip, Group, Loader, SegmentedControl, Text, Tooltip } from "@mantine/core";
import { IconPlayerPlayFilled, IconPlayerStopFilled } from "@tabler/icons-react";
import { commands } from "../lib/api";
import { formatScore } from "../lib/score";
import { moveNumber } from "../lib/format";
import { sameFen, useEngines } from "../stores/engines";
import type { AnalysisLine } from "../bindings";

interface EnginePanelProps {
  tabId: string;
  fen: string;
  onPlayLine: (pv: string[]) => void;
}

function Wdl({ line }: { line: AnalysisLine }) {
  if (!line.wdl) return null;
  const total = line.wdl.w + line.wdl.d + line.wdl.l || 1;
  const pct = (n: number) => `${((n / total) * 100).toFixed(0)}%`;
  return (
    <Tooltip label={`White wins ${pct(line.wdl.w)} · draw ${pct(line.wdl.d)} · Black wins ${pct(line.wdl.l)}`}>
      <div style={{ display: "flex", width: 54, height: 6, borderRadius: 3, overflow: "hidden" }} aria-label="Win draw loss">
        <div style={{ width: pct(line.wdl.w), background: "var(--eval-white)" }} />
        <div style={{ width: pct(line.wdl.d), background: "var(--text-dim)" }} />
        <div style={{ width: pct(line.wdl.l), background: "var(--eval-black)" }} />
      </div>
    </Tooltip>
  );
}

function pvText(fen: string, san: string[]): string {
  const { number, white } = moveNumber(fen);
  let n = number;
  let w = white;
  const parts: string[] = [];
  san.forEach((m, i) => {
    if (w) parts.push(`${n}. ${m}`);
    else parts.push(i === 0 ? `${n}… ${m}` : m);
    if (!w) n += 1;
    w = !w;
  });
  return parts.join(" ");
}

export function EnginePanel({ tabId, fen, onPlayLine }: EnginePanelProps) {
  const engines = useEngines((s) => s.engines);
  const tab = useEngines((s) => s.tabs[tabId]) ?? { enabled: ["lc0"], running: false, multipv: 3 };
  const lines = useEngines((s) => s.lines);
  const setTab = useEngines((s) => s.setTab);
  const [backend, setBackend] = useState<string | null>(null);

  useEffect(() => {
    if (engines.length === 0) void useEngines.getState().refresh();
  }, [engines.length]);

  useEffect(() => {
    if (!tab.running) return;
    const t = setTimeout(() => void commands.analysisBackend("lc0").then((r) => r.status === "ok" && setBackend(r.data)), 4000);
    return () => clearTimeout(t);
  }, [tab.running]);

  const toggleEngine = (id: string) => {
    const enabled = tab.enabled.includes(id) ? tab.enabled.filter((e) => e !== id) : [...tab.enabled, id];
    if (!enabled.includes(id)) void commands.analysisStop(id);
    setTab(tabId, { enabled });
  };

  return (
    <div className="panel" style={{ padding: "10px 12px" }}>
      <Group justify="space-between" mb={6}>
        <Group gap={8}>
          <Tooltip label={tab.running ? "Stop engines (Space)" : "Start engines (Space)"}>
            <ActionIcon
              variant={tab.running ? "filled" : "light"}
              onClick={() => setTab(tabId, { running: !tab.running })}
              aria-label={tab.running ? "Stop engines" : "Start engines"}
            >
              {tab.running ? <IconPlayerStopFilled size={14} /> : <IconPlayerPlayFilled size={14} />}
            </ActionIcon>
          </Tooltip>
          <Text fw={600} size="sm">
            Engine
          </Text>
          {backend && (
            <Badge size="xs" variant="light" color="steel">
              {backend}
            </Badge>
          )}
        </Group>
        <SegmentedControl
          size="xs"
          value={String(tab.multipv)}
          onChange={(v) => setTab(tabId, { multipv: Number(v) })}
          data={["1", "2", "3", "5"]}
          aria-label="Number of lines"
        />
      </Group>
      {engines.length > 1 && (
        <Group gap={6} mb={6}>
          {engines.map((e) => (
            <Chip key={e.id} size="xs" checked={tab.enabled.includes(e.id)} onChange={() => toggleEngine(e.id)}>
              {e.name}
            </Chip>
          ))}
        </Group>
      )}
      {tab.enabled.map((engineId) => {
        const update = lines[`${tabId}:${engineId}`];
        const current = update && sameFen(update.fen, fen) ? update : undefined;
        const name = engines.find((e) => e.id === engineId)?.name ?? engineId;
        const top = current?.lines[0];
        return (
          <div key={engineId} style={{ marginTop: 4 }} aria-label={`${name} analysis`}>
            {engines.length > 1 && (
              <Text size="xs" c="dimmed" fw={600}>
                {name}
              </Text>
            )}
            {!tab.running && !current && (
              <Text size="xs" c="dimmed">
                Press Space or ▶ to analyse this position.
              </Text>
            )}
            {tab.running && !current?.lines.length && (
              <Group gap={6}>
                <Loader size={12} />
                <Text size="xs" c="dimmed">
                  Starting {name}…
                </Text>
              </Group>
            )}
            {current?.lines.map((line) => (
              <div
                key={line.multipv}
                className="engine-line"
                onClick={() => onPlayLine(line.pv)}
                role="button"
                aria-label={`Line ${line.multipv}: ${formatScore(line.score)}`}
              >
                <span className="engine-score tabular">{formatScore(line.score)}</span>
                <span className="engine-pv">{pvText(fen, line.san)}</span>
              </div>
            ))}
            {top && (
              <Group gap={10} mt={4}>
                <Wdl line={top} />
                <Text size="xs" c="dimmed" className="tabular">
                  depth {top.depth}
                  {top.seldepth ? `/${top.seldepth}` : ""} · {Intl.NumberFormat("en", { notation: "compact" }).format(top.nodes ?? 0)} nodes
                  {top.nps ? ` · ${Math.round(top.nps)} nps` : ""}
                </Text>
              </Group>
            )}
          </div>
        );
      })}
    </div>
  );
}
