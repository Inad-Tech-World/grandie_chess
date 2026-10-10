import { create } from "zustand";
import { persist } from "zustand/middleware";

export type Section = "boards" | "database" | "openings" | "puzzles" | "engines" | "progress" | "settings";

export type Tab =
  | { id: string; kind: "new-game"; title: string }
  | { id: string; kind: "play"; title: string; sessionId: number }
  | { id: string; kind: "analysis"; title: string; analysisId: string };

interface AppStore {
  section: Section;
  tabs: Tab[];
  activeTabId: string | null;
  setSection: (section: Section) => void;
  openTab: (tab: Tab) => void;
  replaceTab: (id: string, tab: Tab) => void;
  closeTab: (id: string) => Tab | undefined;
  activate: (id: string) => void;
  renameTab: (id: string, title: string) => void;
}

export function newId(prefix: string): string {
  return `${prefix}-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 7)}`;
}

export const useApp = create<AppStore>()(
  persist(
    (set, get) => ({
      section: "boards",
      tabs: [],
      activeTabId: null,
      setSection: (section) => set({ section }),
      openTab: (tab) => set((s) => ({ tabs: [...s.tabs, tab], activeTabId: tab.id, section: "boards" })),
      replaceTab: (id, tab) =>
        set((s) => ({
          tabs: s.tabs.map((t) => (t.id === id ? tab : t)),
          activeTabId: s.activeTabId === id ? tab.id : s.activeTabId,
        })),
      closeTab: (id) => {
        const { tabs, activeTabId } = get();
        const index = tabs.findIndex((t) => t.id === id);
        if (index < 0) return undefined;
        const closed = tabs[index];
        const rest = tabs.filter((t) => t.id !== id);
        const nextActive =
          activeTabId === id ? (rest[Math.min(index, rest.length - 1)]?.id ?? null) : activeTabId;
        set({ tabs: rest, activeTabId: nextActive });
        return closed;
      },
      activate: (id) => set({ activeTabId: id, section: "boards" }),
      renameTab: (id, title) => set((s) => ({ tabs: s.tabs.map((t) => (t.id === id ? { ...t, title } : t)) })),
    }),
    {
      name: "grandie-tabs",
      version: 1,
      // Live games don't survive a restart (the engine process is gone).
      partialize: (s) => ({
        section: s.section,
        tabs: s.tabs.filter((t) => t.kind !== "play"),
        activeTabId: s.tabs.find((t) => t.id === s.activeTabId)?.kind === "play" ? null : s.activeTabId,
      }),
    },
  ),
);

export function activeTab(): Tab | undefined {
  const { tabs, activeTabId } = useApp.getState();
  return tabs.find((t) => t.id === activeTabId);
}
