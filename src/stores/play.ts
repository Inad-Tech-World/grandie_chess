import { create } from "zustand";
import type { ClockState, PlaySnapshot, Side } from "../bindings";

interface PlayView {
  orientation: Side;
  /** Ply being viewed while scrolling back; null follows the live game. */
  viewPly: number | null;
}

interface PlayStore {
  snapshots: Record<number, PlaySnapshot>;
  clocks: Record<number, ClockState>;
  views: Record<number, PlayView>;
  setSnapshot: (s: PlaySnapshot) => void;
  setClock: (sessionId: number, clock: ClockState) => void;
  setView: (sessionId: number, view: Partial<PlayView>) => void;
  remove: (sessionId: number) => void;
}

export const usePlay = create<PlayStore>()((set) => ({
  snapshots: {},
  clocks: {},
  views: {},
  setSnapshot: (s) =>
    set((state) => ({
      snapshots: { ...state.snapshots, [s.sessionId]: s },
      clocks: s.clock ? { ...state.clocks, [s.sessionId]: s.clock } : state.clocks,
      views: state.views[s.sessionId]
        ? {
            ...state.views,
            // A new move snaps the board back to the live position.
            [s.sessionId]: {
              ...state.views[s.sessionId],
              viewPly:
                state.snapshots[s.sessionId]?.moves.length !== s.moves.length ? null : state.views[s.sessionId].viewPly,
            },
          }
        : { ...state.views, [s.sessionId]: { orientation: s.userSide, viewPly: null } },
    })),
  setClock: (sessionId, clock) => set((state) => ({ clocks: { ...state.clocks, [sessionId]: clock } })),
  setView: (sessionId, view) =>
    set((state) => ({
      views: {
        ...state.views,
        [sessionId]: { ...(state.views[sessionId] ?? { orientation: "white", viewPly: null }), ...view },
      },
    })),
  remove: (sessionId) =>
    set((state) => {
      const { [sessionId]: _s, ...snapshots } = state.snapshots;
      const { [sessionId]: _c, ...clocks } = state.clocks;
      const { [sessionId]: _v, ...views } = state.views;
      return { snapshots, clocks, views };
    }),
}));
