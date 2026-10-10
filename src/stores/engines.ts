import { create } from "zustand";
import { persist } from "zustand/middleware";
import { commands, type AnalysisUpdate, type EngineConfig } from "../bindings";

interface TabEngines {
  /** Engines switched on for this tab. */
  enabled: string[];
  running: boolean;
  multipv: number;
}

interface EnginesStore {
  engines: EngineConfig[];
  /** Latest lines per `${tabId}:${engineId}`. */
  lines: Record<string, AnalysisUpdate>;
  tabs: Record<string, TabEngines>;
  refresh: () => Promise<void>;
  onUpdate: (u: AnalysisUpdate) => void;
  tab: (tabId: string) => TabEngines;
  setTab: (tabId: string, patch: Partial<TabEngines>) => void;
  clearLines: (tabId: string) => void;
}

const DEFAULT_TAB: TabEngines = { enabled: ["lc0"], running: false, multipv: 3 };

export const useEngines = create<EnginesStore>()(
  persist(
    (set, get) => ({
      engines: [],
      lines: {},
      tabs: {},
      refresh: async () => {
        set({ engines: await commands.enginesList() });
      },
      onUpdate: (u) => set((s) => ({ lines: { ...s.lines, [`${u.tabId}:${u.engineId}`]: u } })),
      tab: (tabId) => get().tabs[tabId] ?? DEFAULT_TAB,
      setTab: (tabId, patch) => set((s) => ({ tabs: { ...s.tabs, [tabId]: { ...(s.tabs[tabId] ?? DEFAULT_TAB), ...patch } } })),
      clearLines: (tabId) =>
        set((s) => ({ lines: Object.fromEntries(Object.entries(s.lines).filter(([k]) => !k.startsWith(`${tabId}:`))) })),
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

export function linesFor(tabId: string, engineId: string, fen: string): AnalysisUpdate | undefined {
  const u = useEngines.getState().lines[`${tabId}:${engineId}`];
  return u && sameFen(u.fen, fen) ? u : undefined;
}

/** Compare FENs ignoring move counters (engines may normalise them). */
export function sameFen(a: string, b: string): boolean {
  return a.split(" ").slice(0, 4).join(" ") === b.split(" ").slice(0, 4).join(" ");
}
