import { useEffect } from "react";
import { MantineProvider } from "@mantine/core";
import { ModalsProvider } from "@mantine/modals";
import { Notifications } from "@mantine/notifications";
import { events } from "./bindings";
import { theme } from "./theme";
import { Shell } from "./layout/Shell";
import { Onboarding } from "./screens/Onboarding";
import { useSettings } from "./stores/settings";
import { usePlay } from "./stores/play";
import { useDownloads } from "./stores/downloads";
import { activeTab, useApp } from "./stores/app";
import { closeTab, copyFen, openAnalysis, openNewGame, openPgnFiles, pasteFenOrPgn, saveActiveTab } from "./lib/actions";
import { useAnalysis } from "./stores/analysis";
import { useEngines } from "./stores/engines";
import { useReviews } from "./stores/reviews";
import { HelpModals, useHelp } from "./layout/HelpModals";
import { checkForUpdates } from "./lib/updates";

function flipActiveBoard() {
  const tab = activeTab();
  if (tab?.kind === "play") {
    const view = usePlay.getState().views[tab.sessionId];
    usePlay.getState().setView(tab.sessionId, { orientation: view?.orientation === "black" ? "white" : "black" });
  } else if (tab?.kind === "analysis") {
    useAnalysis.getState().update(tab.analysisId, (d) => ({ orientation: d.orientation === "white" ? "black" : "white" }));
  }
}

function handleMenu(action: string) {
  switch (action) {
    case "new_game":
      openNewGame();
      break;
    case "new_analysis":
      openAnalysis();
      break;
    case "open_pgn":
      void openPgnFiles();
      break;
    case "save_pgn":
      void saveActiveTab();
      break;
    case "close_tab": {
      const tab = activeTab();
      if (tab) void closeTab(tab.id);
      break;
    }
    case "settings":
      useApp.getState().setSection("settings");
      break;
    case "flip_board":
      flipActiveBoard();
      break;
    case "copy_fen":
      void copyFen();
      break;
    case "paste_fen":
      void pasteFenOrPgn();
      break;
    case "help_credits":
      useHelp.getState().show("credits");
      break;
    case "help_shortcuts":
      useHelp.getState().show("shortcuts");
      break;
    case "check_updates":
      void checkForUpdates(true);
      break;
  }
}

/** Subscribes to events pushed from Rust for the lifetime of the app. */
function useBackendEvents() {
  useEffect(() => {
    const unlisten = [
      events.playUpdate.listen((e) => usePlay.getState().setSnapshot(e.payload)),
      events.playClock.listen((e) => usePlay.getState().setClock(e.payload.sessionId, e.payload.clock)),
      events.downloadProgress.listen((e) => useDownloads.getState().onProgress(e.payload)),
      events.menuAction.listen((e) => handleMenu(e.payload)),
      events.analysisEvent.listen((e) => useEngines.getState().onUpdate(e.payload)),
      events.reviewProgress.listen((e) => useReviews.getState().set(e.payload.reviewId, e.payload.done, e.payload.total)),
    ];
    return () => {
      unlisten.forEach((p) => void p.then((f) => f()));
    };
  }, []);
}

export default function App() {
  const { settings, load } = useSettings();
  useBackendEvents();
  useEffect(() => {
    void load();
  }, [load]);

  // Quietly look for updates in release builds (not in dev or browser tests).
  useEffect(() => {
    if (import.meta.env.PROD && import.meta.env.VITE_MOCK_TAURI !== "1") void checkForUpdates(false);
  }, []);

  // Board theme and piece set are CSS classes on <body> (see styles/boards.css, pieces.css).
  useEffect(() => {
    document.body.classList.forEach((c) => (c.startsWith("board-") || c.startsWith("pieces-")) && document.body.classList.remove(c));
    document.body.classList.add(`board-${settings.boardTheme}`, `pieces-${settings.pieceSet}`);
  }, [settings.boardTheme, settings.pieceSet]);

  const forced = settings.colorScheme === "auto" ? undefined : settings.colorScheme;
  return (
    <MantineProvider theme={theme} defaultColorScheme="auto" forceColorScheme={forced}>
      <ModalsProvider>
        <Notifications position="top-right" />
        <Shell />
        <Onboarding />
        <HelpModals />
      </ModalsProvider>
    </MantineProvider>
  );
}
