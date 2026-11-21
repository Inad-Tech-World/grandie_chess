import { create } from "zustand";
import { persist } from "zustand/middleware";
import type { GameTree, ReviewResult, Side } from "../bindings";

export interface AnalysisDoc {
  tree: GameTree;
  current: number;
  orientation: Side;
  /** Review of the main line (as far as the tree still matches its moves). */
  review?: ReviewResult;
  /** Start a review as soon as the tab opens (after a game). */
  autoReview?: boolean;
  /** Database id of the game, so its review is stored with it. */
  gameId?: number;
  /** When the doc was last opened or changed, for trimming old docs. */
  touched?: number;
}

/** How many analysis docs are kept in local storage (tabs beyond this lose
 *  their board after a restart, oldest first). */
export const MAX_STORED_DOCS = 30;

interface AnalysisStore {
  docs: Record<string, AnalysisDoc>;
  create: (id: string, doc: AnalysisDoc) => void;
  update: (id: string, patch: Partial<AnalysisDoc> | ((doc: AnalysisDoc) => Partial<AnalysisDoc>)) => void;
  remove: (id: string) => void;
}

/** The newest docs only, so local storage can't grow without bound. */
export function newestDocs(docs: Record<string, AnalysisDoc>, max = MAX_STORED_DOCS): Record<string, AnalysisDoc> {
  const entries = Object.entries(docs);
  if (entries.length <= max) return docs;
  entries.sort(([, a], [, b]) => (b.touched ?? 0) - (a.touched ?? 0));
  return Object.fromEntries(entries.slice(0, max));
}

// Writing every doc to local storage on each step through a game is slow
// for long PGNs, so writes are coalesced.
const WRITE_DELAY_MS = 500;
const pendingWrites = new Map<string, string>();
let writeTimer: ReturnType<typeof setTimeout> | null = null;

function flushWrites() {
  writeTimer = null;
  for (const [key, value] of pendingWrites) {
    try {
      localStorage.setItem(key, value);
    } catch {
      // Storage full: keep the in-memory docs; they just won't survive a restart.
    }
  }
  pendingWrites.clear();
}

if (typeof window !== "undefined") window.addEventListener("beforeunload", flushWrites);

const debouncedStorage = {
  getItem: (key: string) => pendingWrites.get(key) ?? localStorage.getItem(key),
  setItem: (key: string, value: string) => {
    pendingWrites.set(key, value);
    if (!writeTimer) writeTimer = setTimeout(flushWrites, WRITE_DELAY_MS);
  },
  removeItem: (key: string) => {
    pendingWrites.delete(key);
    localStorage.removeItem(key);
  },
};

export const useAnalysis = create<AnalysisStore>()(
  persist(
    (set) => ({
      docs: {},
      create: (id, doc) => set((s) => ({ docs: { ...s.docs, [id]: { ...doc, touched: Date.now() } } })),
      update: (id, patch) =>
        set((s) => {
          const doc = s.docs[id];
          if (!doc) return s;
          const p = typeof patch === "function" ? patch(doc) : patch;
          return { docs: { ...s.docs, [id]: { ...doc, ...p, touched: Date.now() } } };
        }),
      remove: (id) =>
        set((s) => {
          const { [id]: _, ...docs } = s.docs;
          return { docs };
        }),
    }),
    {
      name: "grandie-analysis",
      version: 1,
      storage: {
        getItem: (name) => {
          const raw = debouncedStorage.getItem(name);
          return raw ? JSON.parse(raw) : null;
        },
        setItem: (name, value) => debouncedStorage.setItem(name, JSON.stringify(value)),
        removeItem: (name) => debouncedStorage.removeItem(name),
      },
      partialize: (s) => ({ docs: newestDocs(s.docs) }) as AnalysisStore,
    },
  ),
);
