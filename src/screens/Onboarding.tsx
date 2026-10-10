import { useEffect, useState } from "react";
import { Button, Group, Image, Modal, Progress, Stack, Stepper, Text, TextInput, Title } from "@mantine/core";
import { notifications } from "@mantine/notifications";
import { commands, notifyError, unwrap, type BackendResult } from "../lib/api";
import { formatBytes } from "../lib/format";
import { downloadFraction, useDownloads } from "../stores/downloads";
import { useSettings } from "../stores/settings";

/** Networks every new install needs: one full-strength net, the small net for
 *  weak Leela levels, and a spread of Maia levels. */
const ESSENTIALS = ["791556", "703810", "maia-1100", "maia-1300", "maia-1500", "maia-1700", "maia-1900"];

export function Onboarding() {
  const { settings, loaded, update, load } = useSettings();
  const { networks, progress, refresh, download } = useDownloads();
  const [step, setStep] = useState(0);
  const [busy, setBusy] = useState(false);
  const [bench, setBench] = useState<BackendResult | null>(null);
  const [name, setName] = useState("");
  const [lichess, setLichess] = useState("");
  const [chesscom, setChesscom] = useState("");

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const opened = loaded && !settings.onboardingDone;
  const essentials = networks.filter((n) => ESSENTIALS.includes(n.id));
  const missing = essentials.filter((n) => !n.installed);
  const missingBytes = missing.reduce((s, n) => s + (n.size ?? 0), 0);

  async function downloadAll() {
    setBusy(true);
    try {
      for (const n of missing) await download(n.id);
      if (!settings.defaultNetwork) await update({ defaultNetwork: "791556" });
      setStep(2);
    } catch (e) {
      notifyError("Download failed", e);
    } finally {
      setBusy(false);
    }
  }

  async function detect() {
    setBusy(true);
    try {
      const results = await unwrap(commands.engineDetectBackend());
      const best = results.filter((r) => !r.error).sort((a, b) => (b.nps ?? 0) - (a.nps ?? 0))[0] ?? null;
      setBench(best);
      await load();
    } catch (e) {
      notifyError("Benchmark failed", e);
    } finally {
      setBusy(false);
    }
  }

  async function finish() {
    await update({
      onboardingDone: true,
      ...(name.trim() ? { playerName: name.trim() } : {}),
      lichessUsername: lichess.trim() || null,
      chesscomUsername: chesscom.trim() || null,
    });
    // Bring in the user's online games in the background.
    const imports: [string, () => ReturnType<typeof commands.dbImportLichess>][] = [];
    if (lichess.trim()) imports.push(["Lichess", () => commands.dbImportLichess("onboarding-lichess", lichess.trim())]);
    if (chesscom.trim()) imports.push(["Chess.com", () => commands.dbImportChesscom("onboarding-chesscom", chesscom.trim())]);
    for (const [site, run] of imports) {
      void run().then((r) =>
        r.status === "ok"
          ? notifications.show({ color: "green", title: `${site} games imported`, message: `${r.data.imported} games added to your database.` })
          : notifyError(`${site} import failed`, r.error),
      );
    }
  }

  return (
    <Modal opened={opened} onClose={() => void finish()} size="lg" centered withCloseButton={false} closeOnClickOutside={false} radius="lg">
      <Stepper active={step} onStepClick={setStep} size="sm" mb="lg" allowNextStepsSelect={false}>
        <Stepper.Step label="Welcome" />
        <Stepper.Step label="Networks" />
        <Stepper.Step label="Your Mac" />
        <Stepper.Step label="You" />
      </Stepper>

      {step === 0 && (
        <Stack align="center" gap="sm">
          <Image src="/grandie-logo.jpg" alt="Grandie Chess" w={150} h={225} fit="cover" radius="lg" />
          <Title order={2}>Welcome to Grandie Chess</Title>
          <Text c="dimmed" ta="center" maw={440}>
            Play Leela Chess Zero and human-like Maia opponents at your level, review every game, and train your openings and tactics.
          </Text>
          <Button mt="md" onClick={() => setStep(1)}>
            Get started
          </Button>
        </Stack>
      )}

      {step === 1 && (
        <Stack>
          <Text>
            Grandie needs a few neural networks to play. They download once
            {missing.length ? ` (${formatBytes(missingBytes)})` : ""}.
          </Text>
          {essentials.map((n) => (
            <div key={n.id}>
              <Group justify="space-between">
                <Text size="sm">{n.name}</Text>
                <Text size="xs" c="dimmed">
                  {n.installed ? "Installed" : formatBytes(n.size)}
                </Text>
              </Group>
              <Progress value={n.installed ? 100 : downloadFraction(progress[n.id]) * 100} size="xs" mt={4} color={n.installed ? "green" : undefined} />
            </div>
          ))}
          <Group justify="flex-end">
            {missing.length === 0 ? (
              <Button onClick={() => setStep(2)}>Continue</Button>
            ) : (
              <Button loading={busy} onClick={() => void downloadAll()}>
                Download {formatBytes(missingBytes)}
              </Button>
            )}
          </Group>
        </Stack>
      )}

      {step === 2 && (
        <Stack>
          <Text>
            Grandie times your Mac's graphics chips and processor to find the fastest way to run Leela. This takes about a minute.
          </Text>
          {bench && (
            <Text fw={600}>
              Using {bench.backend}
              {bench.device ? ` on ${bench.device}` : bench.backend === "blas" ? " on the CPU" : ""} · {Math.round(bench.nps ?? 0)} positions/second
            </Text>
          )}
          <Group justify="flex-end">
            {bench ? (
              <Button onClick={() => setStep(3)}>Continue</Button>
            ) : (
              <>
                <Button variant="default" onClick={() => setStep(3)} disabled={busy}>
                  Skip
                </Button>
                <Button loading={busy} onClick={() => void detect()}>
                  Run benchmark
                </Button>
              </>
            )}
          </Group>
        </Stack>
      )}

      {step === 3 && (
        <Stack>
          <TextInput label="What should Grandie call you?" description="Used in your saved games." placeholder="You" value={name} onChange={(e) => setName(e.currentTarget.value)} autoFocus />
          <Group grow>
            <TextInput label="Lichess username" description="Optional" value={lichess} onChange={(e) => setLichess(e.currentTarget.value)} />
            <TextInput label="Chess.com username" description="Optional" value={chesscom} onChange={(e) => setChesscom(e.currentTarget.value)} />
          </Group>
          <Text size="xs" c="dimmed">
            With a username, Grandie imports your games so you can review them, find your weak openings and build your repertoire.
          </Text>
          <Group justify="flex-end">
            <Button onClick={() => void finish()}>Start playing</Button>
          </Group>
        </Stack>
      )}
    </Modal>
  );
}
