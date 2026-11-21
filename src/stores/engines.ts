import { create } from "zustand";
import { persist } from "zustand/middleware";
import { commands, type AnalysisUpdate, type EngineConfig } from "../bindings";

export interface TabEngines {
  /** Engines switched on for this tab. */
  enabled: string[];
  running: boolean;
  multipv: number;
}

export const DEFAULT_TAB_ENGINES: TabEngines = { enabled: ["lc0"], running: false, multipv: 3 };

interface EnginesStore {
  engines: EngineConfig[];
  /** Latest lines per `${tabId}:${engineId}`. */
  lines: Record<string, AnalysisUpdate>;
  tabs: Record<string, TabEngines>;
  refresh: () => Promise<void>;
  onUpdate: (u: AnalysisUpdate) => void;
  setTab: (tabId: string, patch: Partial<TabEngines>) => void;
  /** Forget a closed tab's engine settings and lines. */
  removeTab: (tabId: string) => void;
}

export const useEngines = create<EnginesStore>()(
  persist(
    (set) => ({
      engines: [],
      lines: {},
      tabs: {},
      refresh: async () => {
        set({ engines: await commands.enginesList() });
      },
      onUpdate: (u) => set((s) => ({ lines: { ...s.lines, [`${u.tabId}:${u.engineId}`]: u } })),
      setTab: (tabId, patch) => set((s) => ({ tabs: { ...s.tabs, [tabId]: { ...(s.tabs[tabId] ?? DEFAULT_TAB_ENGINES), ...patch } } })),
      removeTab: (tabId) =>
        set((s) => {
          const { [tabId]: _, ...tabs } = s.tabs;
          const lines = Object.fromEntries(Object.entries(s.lines).filter(([k]) => !k.startsWith(`${tabId}:`)));
          return { tabs, lines };
        }),
    }),
    {
      name: "grandie-engines",
      version: 1,
      // Engines never run straight after a restart.
      partialize: (s) => ({
        tabs: Object.fromEntries(Object.entries(s.tabs).map(([k, v]) => [k, { ...v, running: false }])),
      }),
    },
  ),
);

/** Stop every engine switched on for a tab (reads the store now, so engines
 *  enabled after the caller rendered are included). */
export function stopTabEngines(tabId: string) {
  const tab = useEngines.getState().tabs[tabId];
  for (const engineId of tab?.enabled ?? DEFAULT_TAB_ENGINES.enabled) void commands.analysisStop(engineId);
}
