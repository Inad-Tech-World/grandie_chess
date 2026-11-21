// Import PGN files as one master's games: for example games exported from
// your own ChessBase, which has no public API for downloading.

import { useState } from "react";
import { Alert, Autocomplete, Button, Group, Modal, Progress, Stack, Text } from "@mantine/core";
import { IconFileImport } from "@tabler/icons-react";
import { open } from "@tauri-apps/plugin-dialog";
import { commands, errorMessage, events, unwrap, type ImportProgress, type ImportSummary, type MasterPlayer } from "../lib/api";
import { newId } from "../stores/app";

interface Props {
  opened: boolean;
  players: MasterPlayer[];
  onClose: () => void;
  /** Called with the catalogue id (or the typed name) the games were filed under. */
  onImported: (player: string) => void;
}

/** A catalogue player matching a typed name, by full name or unique surname. */
function catalogueId(players: MasterPlayer[], name: string): string | null {
  const n = name.trim().toLowerCase();
  const exact = players.find((p) => p.name.toLowerCase() === n || p.id.toLowerCase() === n);
  if (exact) return exact.id;
  const last = n.split(/\s+/).pop() ?? n;
  const bySurname = players.filter((p) => p.name.toLowerCase().split(/\s+/).pop() === last);
  return bySurname.length === 1 ? bySurname[0].id : null;
}

export function ImportMastersModal({ opened, players, onClose, onImported }: Props) {
  const [player, setPlayer] = useState("");
  const [busy, setBusy] = useState(false);
  const [progress, setProgress] = useState<ImportProgress | null>(null);
  const [result, setResult] = useState<ImportSummary | null>(null);
  const [error, setError] = useState<string | null>(null);

  async function run() {
    const typed = player.trim();
    if (!typed) return;
    // File the games under the catalogue entry when there is one, so the
    // player shows up in the list with their profile.
    const name = catalogueId(players, typed) ?? typed;
    const picked = await open({ multiple: true, filters: [{ name: "PGN", extensions: ["pgn"] }] });
    if (!picked) return;
    const jobId = newId("masters-import");
    setBusy(true);
    setError(null);
    setResult(null);
    setProgress(null);
    // Listen before the import starts, so its first progress events aren't missed.
    const unlisten = await events.importProgress.listen((e) => {
      if (e.payload.jobId === jobId) setProgress(e.payload);
    });
    try {
      const summary = await unwrap(commands.mastersImportFiles(jobId, Array.isArray(picked) ? picked : [picked], name));
      setResult(summary);
      onImported(name);
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      unlisten();
      setBusy(false);
    }
  }

  function close() {
    if (busy) return;
    setResult(null);
    setError(null);
    setProgress(null);
    onClose();
  }

  return (
    <Modal opened={opened} onClose={close} title="Import PGN as master games" centered>
      <Stack gap="sm">
        <Text size="sm" c="dimmed">
          Add a famous player's games from PGN files, kept apart from your own games. Games exported from ChessBase work too (ChessBase's online database has no
          public API, so the app can't download from it directly).
        </Text>
        <Autocomplete
          label="Whose games are these?"
          placeholder="e.g. Mikhail Tal"
          value={player}
          onChange={setPlayer}
          data={players.map((p) => p.name)}
          limit={8}
          disabled={busy}
          data-autofocus
        />
        {busy && (
          <Stack gap={2}>
            <Text size="xs" c="dimmed">
              {progress ? `${progress.phase}… ${progress.seen.toLocaleString()} read, ${progress.imported.toLocaleString()} added` : "Starting…"}
            </Text>
            <Progress value={100} animated size="xs" />
          </Stack>
        )}
        {result && (
          <Alert color="green" variant="light">
            Added {result.imported.toLocaleString()} games{result.duplicates ? ` (${result.duplicates} were already there)` : ""}.
          </Alert>
        )}
        {error && (
          <Alert color="red" variant="light">
            {error}
          </Alert>
        )}
        <Group justify="flex-end">
          <Button variant="default" onClick={close} disabled={busy}>
            {result ? "Done" : "Cancel"}
          </Button>
          <Button leftSection={<IconFileImport size={16} />} onClick={() => void run()} loading={busy} disabled={!player.trim()}>
            Choose PGN files
          </Button>
        </Group>
      </Stack>
    </Modal>
  );
}
