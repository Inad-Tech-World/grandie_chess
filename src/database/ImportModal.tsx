import { useEffect, useState } from "react";
import { Alert, Button, Group, Modal, Progress, Stack, Tabs, Text, TextInput } from "@mantine/core";
import { IconChessQueen, IconChessKnight, IconFileImport } from "@tabler/icons-react";
import { open } from "@tauri-apps/plugin-dialog";
import { commands, events, unwrap, type ImportProgress, type ImportSummary } from "../lib/api";
import { formatBytes } from "../lib/format";
import { newId } from "../stores/app";
import { useSettings } from "../stores/settings";

interface Props {
  opened: boolean;
  onClose: () => void;
  onImported: () => void;
}

type Source = "files" | "lichess" | "chesscom";

export function ImportModal({ opened, onClose, onImported }: Props) {
  const { settings, update } = useSettings();
  const [tab, setTab] = useState<Source>("lichess");
  const [lichess, setLichess] = useState(settings.lichessUsername ?? "");
  const [chesscom, setChesscom] = useState(settings.chesscomUsername ?? "");
  const [job, setJob] = useState<string | null>(null);
  const [progress, setProgress] = useState<ImportProgress | null>(null);
  const [result, setResult] = useState<ImportSummary | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    setLichess(settings.lichessUsername ?? "");
    setChesscom(settings.chesscomUsername ?? "");
  }, [settings.lichessUsername, settings.chesscomUsername]);

  useEffect(() => {
    if (!job) return;
    const unlisten = events.importProgress.listen((e) => {
      if (e.payload.jobId === job) setProgress(e.payload);
    });
    return () => void unlisten.then((f) => f());
  }, [job]);

  async function run(source: Source) {
    const jobId = newId("import");
    setJob(jobId);
    setProgress(null);
    setResult(null);
    setError(null);
    try {
      let summary: ImportSummary;
      if (source === "files") {
        const picked = await open({ multiple: true, filters: [{ name: "PGN", extensions: ["pgn"] }] });
        if (!picked) {
          setJob(null);
          return;
        }
        summary = await unwrap(commands.dbImportFiles(jobId, Array.isArray(picked) ? picked : [picked]));
      } else if (source === "lichess") {
        await update({ lichessUsername: lichess.trim() || null });
        summary = await unwrap(commands.dbImportLichess(jobId, lichess.trim()));
      } else {
        await update({ chesscomUsername: chesscom.trim() || null });
        summary = await unwrap(commands.dbImportChesscom(jobId, chesscom.trim()));
      }
      setResult(summary);
      onImported();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setJob(null);
    }
  }

  const busy = job !== null;

  return (
    <Modal opened={opened} onClose={busy ? () => {} : onClose} title="Import games" centered size="lg">
      <Tabs value={tab} onChange={(v) => v && setTab(v as Source)} mb="md">
        <Tabs.List>
          <Tabs.Tab value="lichess" leftSection={<IconChessKnight size={14} />} disabled={busy}>
            Lichess
          </Tabs.Tab>
          <Tabs.Tab value="chesscom" leftSection={<IconChessQueen size={14} />} disabled={busy}>
            Chess.com
          </Tabs.Tab>
          <Tabs.Tab value="files" leftSection={<IconFileImport size={14} />} disabled={busy}>
            PGN files
          </Tabs.Tab>
        </Tabs.List>
      </Tabs>

      {tab === "lichess" && (
        <Stack gap="sm">
          <Text size="sm" c="dimmed">
            Downloads your public Lichess games. Next time, only new games are fetched.
          </Text>
          <Group align="flex-end">
            <TextInput label="Lichess username" value={lichess} onChange={(e) => setLichess(e.currentTarget.value)} style={{ flex: 1 }} disabled={busy} />
            <Button loading={busy} disabled={!lichess.trim()} onClick={() => void run("lichess")}>
              Import
            </Button>
          </Group>
        </Stack>
      )}
      {tab === "chesscom" && (
        <Stack gap="sm">
          <Text size="sm" c="dimmed">
            Downloads your games from Chess.com's public archives, month by month.
          </Text>
          <Group align="flex-end">
            <TextInput label="Chess.com username" value={chesscom} onChange={(e) => setChesscom(e.currentTarget.value)} style={{ flex: 1 }} disabled={busy} />
            <Button loading={busy} disabled={!chesscom.trim()} onClick={() => void run("chesscom")}>
              Import
            </Button>
          </Group>
        </Stack>
      )}
      {tab === "files" && (
        <Stack gap="sm">
          <Text size="sm" c="dimmed">
            Choose one or more PGN files. Large files are read as a stream, and games already in the database are skipped.
          </Text>
          <Group justify="flex-end">
            <Button loading={busy} onClick={() => void run("files")}>
              Choose files…
            </Button>
          </Group>
        </Stack>
      )}

      {busy && (
        <Stack gap={4} mt="md" aria-live="polite">
          <Text size="sm">
            {progress?.phase ?? "Starting…"}
            {progress && progress.seen > 0 ? ` · ${progress.seen} games` : ""}
            {progress && (progress.bytes ?? 0) > 0 ? ` · ${formatBytes(progress.bytes)}` : ""}
          </Text>
          <Progress value={100} animated striped size="sm" />
        </Stack>
      )}
      {result && (
        <Alert mt="md" color="green" variant="light" title="Import finished">
          {result.imported} new game{result.imported === 1 ? "" : "s"} added
          {result.duplicates ? `, ${result.duplicates} already in the database` : ""}.
          {result.errors.length > 0 && (
            <Text size="xs" c="dimmed" mt={4}>
              {result.errors.length} problem{result.errors.length === 1 ? "" : "s"}, e.g. {result.errors[0]}
            </Text>
          )}
        </Alert>
      )}
      {error && (
        <Alert mt="md" color="red" variant="light" title="Import failed">
          {error}
        </Alert>
      )}
    </Modal>
  );
}
