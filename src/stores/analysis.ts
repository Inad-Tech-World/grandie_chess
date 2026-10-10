import { create } from "zustand";
import { persist } from "zustand/middleware";
import type { GameTree, ReviewResult, Side } from "../bindings";

export interface AnalysisDoc {
  tree: GameTree;
  current: number;
  orientation: Side;
  /** Review of the main line, computed for exactly `reviewMoves`. */
  review?: ReviewResult;
  reviewMoves?: string[];
  /** Start a review as soon as the tab opens (after a game). */
  autoReview?: boolean;
  /** Database id of the game, so its review is stored with it. */
  gameId?: number;
}

interface AnalysisStore {
  docs: Record<string, AnalysisDoc>;
  create: (id: string, doc: AnalysisDoc) => void;
  update: (id: string, patch: Partial<AnalysisDoc> | ((doc: AnalysisDoc) => Partial<AnalysisDoc>)) => void;
  remove: (id: string) => void;
}

export const useAnalysis = create<AnalysisStore>()(
  persist(
    (set) => ({
      docs: {},
      create: (id, doc) => set((s) => ({ docs: { ...s.docs, [id]: doc } })),
      update: (id, patch) =>
        set((s) => {
          const doc = s.docs[id];
          if (!doc) return s;
          const p = typeof patch === "function" ? patch(doc) : patch;
          return { docs: { ...s.docs, [id]: { ...doc, ...p } } };
        }),
      remove: (id) =>
        set((s) => {
          const { [id]: _, ...docs } = s.docs;
          return { docs };
        }),
    }),
    { name: "grandie-analysis", version: 1 },
  ),
);
