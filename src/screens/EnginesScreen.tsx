import { useEffect, useState } from "react";
import { Badge, Button, Card, Group, NumberInput, Progress, Radio, Stack, Table, Text, Title, Tooltip } from "@mantine/core";
import { IconCpu, IconDownload, IconFolderOpen, IconGauge, IconTrash } from "@tabler/icons-react";
import { open } from "@tauri-apps/plugin-dialog";
import { commands, notifyError, tryCommand, type AppInfo, type BackendResult, type EngineConfig, type NetworkEntry } from "../lib/api";
import { useEngines } from "../stores/engines";
import { formatBytes } from "../lib/format";
import { downloadFraction, useDownloads } from "../stores/downloads";
import { useSettings } from "../stores/settings";

export function EnginesScreen() {
  const { networks, progress, refresh, download } = useDownloads();
  const { settings, update, load } = useSettings();
  const [info, setInfo] = useState<AppInfo | null>(null);
  const [bench, setBench] = useState<BackendResult[] | null>(null);
  const [benchRunning, setBenchRunning] = useState(false);
  const engines = useEngines((s) => s.engines);
  const refreshEngines = useEngines((s) => s.refresh);
  const [installing, setInstalling] = useState(false);

  useEffect(() => {
    void refresh();
    void refreshEngines();
    void commands.appInfo().then(setInfo);
  }, [refresh, refreshEngines]);

  async function installStockfish() {
    setInstalling(true);
    await tryCommand(commands.engineInstallStockfish(), "Could not install Stockfish");
    setInstalling(false);
    await refreshEngines();
  }

  async function addEngine() {
    const path = await open({ multiple: false, title: "Choose a UCI engine executable" });
    if (!path || Array.isArray(path)) return;
    await tryCommand(commands.engineAdd(path), "That file isn't a UCI chess engine");
    await refreshEngines();
  }

  async function setOption(engine: EngineConfig, name: string, value: string) {
    const options = [...engine.options.filter((o) => o.name !== name), { name, value }];
    await tryCommand(commands.engineUpdate({ ...engine, options }));
    await refreshEngines();
  }

  async function detect() {
    setBenchRunning(true);
    const results = await tryCommand(commands.engineDetectBackend(), "Benchmark failed");
    setBenchRunning(false);
    if (results) {
      setBench(results);
      await load();
    }
  }

  async function remove(n: NetworkEntry) {
    await tryCommand(commands.networkDelete(n.id), "Could not delete");
    await refresh();
    await load();
  }

  const leela = networks.filter((n) => n.kind === "leela");
  const maia = networks.filter((n) => n.kind === "maia");
  const backendLabel = settings.backend
    ? `${settings.backend}${settings.backendOpts ? ` (${settings.backendOpts})` : ""}`
    : "lc0 default (not benchmarked)";

  const row = (n: NetworkEntry, selectable: boolean) => {
    const p = progress[n.id];
    const downloading = p && !p.done;
    return (
      <Table.Tr key={n.id}>
        {selectable && (
          <Table.Td w={36}>
            <Radio
              aria-label={`Use ${n.name} for full strength`}
              disabled={!n.installed}
              checked={settings.defaultNetwork === n.id}
              onChange={() => void update({ defaultNetwork: n.id })}
            />
          </Table.Td>
        )}
        <Table.Td>
          <Text size="sm" fw={500}>
            {n.name}
          </Text>
          <Text size="xs" c="dimmed">
            {n.description}
          </Text>
        </Table.Td>
        <Table.Td w={90}>
          <Text size="xs" c="dimmed" className="tabular">
            {formatBytes(n.size)}
          </Text>
        </Table.Td>
        <Table.Td w={170}>
          {n.installed ? (
            <Group gap={6} justify="flex-end">
              <Badge color="green" variant="light">
                Installed
              </Badge>
              <Tooltip label="Delete">
                <Button variant="subtle" color="gray" size="compact-sm" onClick={() => void remove(n)} aria-label={`Delete ${n.name}`}>
                  <IconTrash size={15} />
                </Button>
              </Tooltip>
            </Group>
          ) : downloading ? (
            <Progress value={downloadFraction(p) * 100} size="sm" />
          ) : (
            <Group justify="flex-end">
              <Button
                size="compact-sm"
                variant="light"
                leftSection={<IconDownload size={14} />}
                onClick={() => void download(n.id).catch((e) => notifyError("Download failed", e))}
              >
                Download
              </Button>
            </Group>
          )}
        </Table.Td>
      </Table.Tr>
    );
  };

  return (
    <div className="screen">
      <Title order={2} mb={4}>
        Engines
      </Title>
      <Text c="dimmed" mb="lg">
        Leela Chess Zero runs as a separate process inside Grandie. Networks are its "brains": download the ones you need.
      </Text>

      <Card withBorder radius="lg" padding="lg" mb="lg">
        <Group justify="space-between" align="flex-start">
          <div>
            <Group gap={8}>
              <IconCpu size={20} />
              <Text fw={600}>Leela Chess Zero 0.32</Text>
            </Group>
            <Text size="sm" c="dimmed" mt={4}>
              Backend: <b>{backendLabel}</b>
              {info && ` · ${info.arch === "aarch64" ? "Apple Silicon" : "Intel"} Mac`}
            </Text>
            {!info?.lc0Path && (
              <Text size="sm" c="red" mt={4}>
                The lc0 engine binary is missing from this build.
              </Text>
            )}
          </div>
          <Button leftSection={<IconGauge size={16} />} variant="default" loading={benchRunning} onClick={() => void detect()} disabled={!leela.some((n) => n.installed)}>
            Find fastest backend
          </Button>
        </Group>
        {benchRunning && (
          <Text size="xs" c="dimmed" mt="sm">
            Timing every GPU and the CPU. This takes about a minute.
          </Text>
        )}
        {bench && (
          <Table mt="md" withRowBorders={false} verticalSpacing={4}>
            <Table.Tbody>
              {bench.map((r) => (
                <Table.Tr key={`${r.backend}-${r.backendOpts}`}>
                  <Table.Td>
                    <Text size="sm">
                      {r.backend}
                      {r.backendOpts ? ` (${r.backendOpts})` : ""}
                    </Text>
                  </Table.Td>
                  <Table.Td>
                    <Text size="sm" c="dimmed">
                      {r.device ?? (r.backend === "blas" ? "CPU (Accelerate)" : "")}
                    </Text>
                  </Table.Td>
                  <Table.Td ta="right">
                    <Text size="sm" className="tabular" c={r.error ? "dimmed" : undefined}>
                      {r.error ? "unavailable" : `${Math.round(r.nps ?? 0)} nps`}
                    </Text>
                  </Table.Td>
                </Table.Tr>
              ))}
            </Table.Tbody>
          </Table>
        )}
      </Card>

      <Stack gap="lg">
        <div>
          <Group justify="space-between" mb={6}>
            <Text fw={600}>Analysis engines</Text>
            <Group gap={6}>
              {!engines.some((e) => e.id === "stockfish") && (
                <Button size="compact-sm" variant="light" leftSection={<IconDownload size={14} />} loading={installing} onClick={() => void installStockfish()}>
                  Install Stockfish
                </Button>
              )}
              <Button size="compact-sm" variant="default" leftSection={<IconFolderOpen size={14} />} onClick={() => void addEngine()}>
                Add engine…
              </Button>
            </Group>
          </Group>
          {installing && <Progress value={downloadFraction(progress["stockfish"]) * 100} size="xs" mb={6} />}
          <Card withBorder radius="lg" p={0}>
            <Table verticalSpacing="sm" horizontalSpacing="md">
              <Table.Tbody>
                {engines.map((e) => {
                  const hasThreads = e.kind === "uci" && e.availableOptions.some((o) => o.name === "Threads");
                  const hasHash = e.kind === "uci" && e.availableOptions.some((o) => o.name === "Hash");
                  return (
                    <Table.Tr key={e.id}>
                      <Table.Td>
                        <Text size="sm" fw={500}>
                          {e.name}
                        </Text>
                        <Text size="xs" c="dimmed" truncate maw={420}>
                          {e.kind === "lc0" ? `Bundled · network ${settings.defaultNetwork ?? "not chosen"}` : e.path}
                        </Text>
                      </Table.Td>
                      <Table.Td w={120}>
                        {hasThreads && (
                          <NumberInput size="xs" label="Threads" min={1} max={64} value={Number(e.options.find((o) => o.name === "Threads")?.value ?? 1)} onChange={(v) => void setOption(e, "Threads", String(v || 1))} />
                        )}
                      </Table.Td>
                      <Table.Td w={120}>
                        {hasHash && (
                          <NumberInput size="xs" label="Hash (MB)" min={16} max={8192} step={64} value={Number(e.options.find((o) => o.name === "Hash")?.value ?? 16)} onChange={(v) => void setOption(e, "Hash", String(v || 16))} />
                        )}
                      </Table.Td>
                      <Table.Td w={60}>
                        {e.kind !== "lc0" && (
                          <Tooltip label="Remove">
                            <Button variant="subtle" color="gray" size="compact-sm" aria-label={`Remove ${e.name}`} onClick={() => void tryCommand(commands.engineRemove(e.id)).then(refreshEngines)}>
                              <IconTrash size={15} />
                            </Button>
                          </Tooltip>
                        )}
                      </Table.Td>
                    </Table.Tr>
                  );
                })}
              </Table.Tbody>
            </Table>
          </Card>
        </div>
        <div>
          <Text fw={600} mb={6}>
            Leela networks
          </Text>
          <Text size="xs" c="dimmed" mb="xs">
            The selected network is used for full-strength play, hints and analysis.
          </Text>
          <Card withBorder radius="lg" p={0}>
            <Table verticalSpacing="sm" horizontalSpacing="md">
              <Table.Tbody>{leela.map((n) => row(n, true))}</Table.Tbody>
            </Table>
          </Card>
        </div>
        <div>
          <Text fw={600} mb={6}>
            Maia networks (human-like opponents)
          </Text>
          <Card withBorder radius="lg" p={0}>
            <Table verticalSpacing="sm" horizontalSpacing="md">
              <Table.Tbody>{maia.map((n) => row(n, false))}</Table.Tbody>
            </Table>
          </Card>
        </div>
      </Stack>
    </div>
  );
}
