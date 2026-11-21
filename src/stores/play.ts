import { create } from "zustand";
import type { ClockState, PlaySnapshot, Side } from "../bindings";

interface PlayView {
  orientation: Side;
  /** Ply being viewed while scrolling back; null follows the live game. */
  viewPly: number | null;
}

/** How the coach joins a game: not at all, after the user's mistakes, or
 *  commenting on every move. */
export type CoachMode = "off" | "mistakes" | "every";

/** One-time things a game tab has done. Kept here, not in the screen,
 *  because the screen unmounts whenever another tab is shown. */
export interface PlayFlags {
  /** The review of the finished game was opened. */
  reviewOpened?: boolean;
  /** The game-end sound was played. */
  endSoundPlayed?: boolean;
}

interface PlayStore {
  snapshots: Record<number, PlaySnapshot>;
  clocks: Record<number, ClockState>;
  views: Record<number, PlayView>;
  flags: Record<number, PlayFlags>;
  coachModes: Record<number, CoachMode>;
  setCoachMode: (sessionId: number, mode: CoachMode) => void;
  /** Store a snapshot, unless a newer one (higher `seq`) is already here:
   *  the reply to a move and the engine's next update can arrive in either order. */
  setSnapshot: (s: PlaySnapshot) => void;
  setClock: (sessionId: number, clock: ClockState) => void;
  setView: (sessionId: number, view: Partial<PlayView>) => void;
  setFlag: (sessionId: number, flag: keyof PlayFlags) => void;
  remove: (sessionId: number) => void;
}

export const usePlay = create<PlayStore>()((set) => ({
  snapshots: {},
  clocks: {},
  views: {},
  flags: {},
  coachModes: {},
  setCoachMode: (sessionId, mode) => set((state) => ({ coachModes: { ...state.coachModes, [sessionId]: mode } })),
  setSnapshot: (s) =>
    set((state) => {
      const prev = state.snapshots[s.sessionId];
      if (prev && s.seq < prev.seq) return state;
      return {
        snapshots: { ...state.snapshots, [s.sessionId]: s },
        clocks: s.clock ? { ...state.clocks, [s.sessionId]: s.clock } : state.clocks,
        views: state.views[s.sessionId]
          ? {
              ...state.views,
              // A new move snaps the board back to the live position.
              [s.sessionId]: {
                ...state.views[s.sessionId],
                viewPly: prev?.moves.length !== s.moves.length ? null : state.views[s.sessionId].viewPly,
              },
            }
          : { ...state.views, [s.sessionId]: { orientation: s.userSide, viewPly: null } },
      };
    }),
  setClock: (sessionId, clock) => set((state) => ({ clocks: { ...state.clocks, [sessionId]: clock } })),
  setView: (sessionId, view) =>
    set((state) => ({
      views: {
        ...state.views,
        [sessionId]: { ...(state.views[sessionId] ?? { orientation: "white", viewPly: null }), ...view },
      },
    })),
  setFlag: (sessionId, flag) => set((state) => ({ flags: { ...state.flags, [sessionId]: { ...state.flags[sessionId], [flag]: true } } })),
  remove: (sessionId) =>
    set((state) => {
      const { [sessionId]: _s, ...snapshots } = state.snapshots;
      const { [sessionId]: _c, ...clocks } = state.clocks;
      const { [sessionId]: _v, ...views } = state.views;
      const { [sessionId]: _f, ...flags } = state.flags;
      const { [sessionId]: _m, ...coachModes } = state.coachModes;
      return { snapshots, clocks, views, flags, coachModes };
    }),
}));
