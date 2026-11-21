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
  reviewEngineId: null,
  coach: {
    coachId: null,
    verbosity: "normal",
    voiceOutput: "system",
    systemVoice: null,
    speechRate: 1,
    announceMoves: false,
    language: "English",
  },
  ai: {
    enabled: false,
    narrateModel: "anthropic/claude-haiku-5.5",
    explainModel: "anthropic/claude-sonnet-5.5",
    sttModel: "openai/gpt-4o-mini-transcribe",
    ttsModel: "mistralai/voxtral-mini-tts-3-0",
    ttsVoice: "en_paul_confident",
  },
};

interface SettingsStore {
  settings: FullSettings;
  loaded: boolean;
  load: () => Promise<void>;
  /** Apply a change at once and save it; on failure the saved copy is reloaded. */
  update: (patch: Partial<FullSettings>) => Promise<void>;
  /** Take settings the backend changed by itself (the `SettingsChanged` event). */
  replace: (settings: Settings) => void;
}

const withDefaults = (s: Settings): FullSettings => ({ ...DEFAULT_SETTINGS, ...s }) as FullSettings;

export const useSettings = create<SettingsStore>()((set, get) => ({
  settings: DEFAULT_SETTINGS,
  loaded: false,
  load: async () => {
    try {
      set({ settings: withDefaults(await commands.settingsGet()), loaded: true });
    } catch (e) {
      // Run with defaults rather than leaving the app (and onboarding) waiting forever.
      set({ loaded: true });
      notifyError("Could not load settings", e);
    }
  },
  update: async (patch) => {
    // Build on the latest state, so quick successive changes don't undo each other.
    set((s) => ({ settings: { ...s.settings, ...patch } }));
    try {
      await unwrap(commands.settingsSet(get().settings));
    } catch (e) {
      notifyError("Could not save settings", e);
      await get().load();
    }
  },
  replace: (settings) => set({ settings: withDefaults(settings), loaded: true }),
}));
