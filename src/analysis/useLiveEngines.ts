import { useEffect, useMemo, useRef } from "react";
import { useShallow } from "zustand/react/shallow";
import { commands, notifyError, type AnalysisUpdate } from "../lib/api";
import { sameFen } from "../lib/fen";
import { DEFAULT_TAB_ENGINES, stopTabEngines, useEngines, type TabEngines } from "../stores/engines";

interface LiveEngines {
  engineTab: TabEngines;
  /** The first engine update for the shown position that has lines. */
  live: AnalysisUpdate | undefined;
  toggleRunning: () => void;
}

/** Runs the tab's engines on the position shown (`rootFen` + `moves`) and
 *  stops them when the screen goes away. */
export function useLiveEngines(tabId: string, rootFen: string | undefined, moves: string[], fen: string): LiveEngines {
  const engineTab = useEngines((s) => s.tabs[tabId]) ?? DEFAULT_TAB_ENGINES;
  const setEngineTab = useEngines((s) => s.setTab);
  // Only this tab's lines, so other tabs' engine output doesn't re-render us.
  const updates = useEngines(useShallow((s) => engineTab.enabled.map((e) => s.lines[`${tabId}:${e}`])));

  const running = engineTab.running;
  const enabledKey = engineTab.enabled.join(",");
  const movesKey = moves.join(" ");
  useEffect(() => {
    if (!rootFen) return;
    const engines = enabledKey ? enabledKey.split(",") : [];
    if (!running) {
      engines.forEach((e) => void commands.analysisStop(e));
      return;
    }
    const t = setTimeout(() => {
      for (const engineId of engines) {
        void commands
          .analysisStart({ tabId, engineId, rootFen, moves: movesKey ? movesKey.split(" ") : [], multipv: engineTab.multipv })
          .then((r) => {
            if (r.status === "error") {
              notifyError("Engine error", r.error);
              setEngineTab(tabId, { running: false });
            }
          });
      }
    }, 120);
    return () => clearTimeout(t);
  }, [running, rootFen, movesKey, enabledKey, engineTab.multipv, tabId, setEngineTab]);

  // Leaving the tab stops whatever is running for it now.
  useEffect(() => () => stopTabEngines(tabId), [tabId]);

  // An engine that stopped with an error says so once.
  const reported = useRef<AnalysisUpdate | null>(null);
  useEffect(() => {
    const failed = updates.find((u) => u?.done && u.error);
    if (!failed || reported.current === failed) return;
    reported.current = failed;
    notifyError("The engine stopped", failed.error);
    setEngineTab(tabId, { running: false });
  }, [updates, tabId, setEngineTab]);

  const live = useMemo(() => updates.find((u) => u && sameFen(u.fen, fen) && u.lines.length > 0), [updates, fen]);

  return { engineTab, live, toggleRunning: () => setEngineTab(tabId, { running: !engineTab.running }) };
}
