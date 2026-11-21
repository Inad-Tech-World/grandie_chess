// Masters: famous players and their games. Pick a player (or the bundled
// Legends), see how they play, and study their games with the coach.
//
// Sources: per-player PGN files from PGN Mentor, downloaded only when asked
// for, a bundled pack of famous games, and PGN files you import. ChessBase's
// online database has no public API, so it isn't used.

import { useCallback, useEffect, useState } from "react";
import { notifications } from "@mantine/notifications";
import { commands, errorMessage, events, notifyError, unwrap, type MasterPlayer } from "../lib/api";
import { ImportMastersModal } from "../masters/ImportMastersModal";
import { LegendsList } from "../masters/LegendsList";
import { PlayerList } from "../masters/PlayerList";
import { PlayerPage } from "../masters/PlayerPage";
import { StudyView } from "../masters/StudyView";
import { LEGENDS_ID, useMasters } from "../masters/store";

export function MastersScreen() {
  const { selected, study, select } = useMasters();
  const [players, setPlayers] = useState<MasterPlayer[]>([]);
  /** Player id -> import phase (null while still downloading). */
  const [downloading, setDownloading] = useState<Record<string, string | null>>({});
  const [importOpen, setImportOpen] = useState(false);

  const refresh = useCallback(async () => {
    const r = await commands.mastersPlayers();
    if (r.status === "ok") setPlayers(r.data);
    else notifyError("Couldn't load the masters", r.error);
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  async function download(id: string) {
    if (downloading[id] !== undefined) return;
    const job = `masters:${id}`;
    setDownloading((d) => ({ ...d, [id]: null }));
    const unlisten = await events.importProgress.listen((e) => {
      if (e.payload.jobId === job) setDownloading((d) => ({ ...d, [id]: `Adding games… ${e.payload.imported.toLocaleString()}` }));
    });
    try {
      const summary = await unwrap(commands.mastersDownload(id));
      const name = players.find((p) => p.id === id)?.name ?? id;
      notifications.show({ color: "green", message: `Added ${summary.imported.toLocaleString()} games of ${name} to the library` });
      await refresh();
    } catch (e) {
      notifyError("Download failed", errorMessage(e));
    } finally {
      unlisten();
      setDownloading((d) => {
        const { [id]: _done, ...rest } = d;
        return rest;
      });
    }
  }

  async function remove(id: string) {
    const r = await commands.mastersRemove(id);
    if (r.status === "error") notifyError("Couldn't remove the games", r.error);
    await refresh();
  }

  const player = players.find((p) => p.id === selected);

  return (
    <div className="masters-screen">
      <PlayerList players={players} downloading={downloading} onDownload={(id) => void download(id)} onRemove={(id) => void remove(id)} onImport={() => setImportOpen(true)} />
      <div className="masters-main">
        {study ? (
          <StudyView key={`${study.gameId}`} study={study} />
        ) : selected === LEGENDS_ID || !player ? (
          <LegendsList />
        ) : (
          <PlayerPage player={player} downloadPhase={downloading[player.id]} onDownload={() => void download(player.id)} />
        )}
      </div>
      <ImportMastersModal opened={importOpen} players={players} onClose={() => setImportOpen(false)} onImported={(id) => void refresh().then(() => select(id))} />
    </div>
  );
}
