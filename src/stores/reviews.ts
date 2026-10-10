import { create } from "zustand";

interface ReviewsStore {
  /** Progress of running reviews, by analysis id. */
  progress: Record<string, { done: number; total: number }>;
  set: (id: string, done: number, total: number) => void;
  clear: (id: string) => void;
}

export const useReviews = create<ReviewsStore>()((set) => ({
  progress: {},
  set: (id, done, total) => set((s) => ({ progress: { ...s.progress, [id]: { done, total } } })),
  clear: (id) =>
    set((s) => {
      const { [id]: _, ...progress } = s.progress;
      return { progress };
    }),
}));
