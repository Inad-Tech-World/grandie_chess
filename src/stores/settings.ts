import { create } from "zustand";
import { commands, type Settings } from "../bindings";
import { notifyError, unwrap } from "../lib/api";

export type FullSettings = Required<Settings>;

export const DEFAULT_SETTINGS: FullSettings = {
  playerName: "You",
  defaultNetwork: null,
  backend: null,
  backendOpts: null,
  threads: null,
  humanThinkTime: true,
  opponentVariety: true,
  colorScheme: "auto",
  boardTheme: "brown",
  pieceSet: "cburnett",
  showCoordinates: true,
  animationMs: 200,
  showEvalBar: true,
  sound: true,
  lichessUsername: null,
  chesscomUsername: null,
  onboardingDone: false,
};

interface SettingsStore {
  settings: FullSettings;
  loaded: boolean;
  load: () => Promise<void>;
  update: (patch: Partial<FullSettings>) => Promise<void>;
}

export const useSettings = create<SettingsStore>()((set, get) => ({
  settings: DEFAULT_SETTINGS,
  loaded: false,
  load: async () => {
    const s = await commands.settingsGet();
    set({ settings: { ...DEFAULT_SETTINGS, ...s } as FullSettings, loaded: true });
  },
  update: async (patch) => {
    const next = { ...get().settings, ...patch };
    set({ settings: next });
    try {
      await unwrap(commands.settingsSet(next));
    } catch (e) {
      notifyError("Could not save settings", e);
    }
  },
}));
