import { create } from "zustand";
import { commands, type DownloadProgress, type NetworkEntry } from "../bindings";
import { unwrap } from "../lib/api";

interface DownloadsStore {
  progress: Record<string, DownloadProgress>;
  networks: NetworkEntry[];
  refresh: () => Promise<NetworkEntry[]>;
  onProgress: (p: DownloadProgress) => void;
  download: (id: string) => Promise<void>;
}

export const useDownloads = create<DownloadsStore>()((set, get) => ({
  progress: {},
  networks: [],
  refresh: async () => {
    const networks = await commands.networksList();
    set({ networks });
    return networks;
  },
  onProgress: (p) => set((s) => ({ progress: { ...s.progress, [p.id]: p } })),
  download: async (id) => {
    set((s) => ({ progress: { ...s.progress, [id]: { id, received: 0, total: null, done: false, error: null } } }));
    try {
      await unwrap(commands.networkDownload(id));
    } finally {
      await get().refresh();
    }
  },
}));

export function downloadFraction(p: DownloadProgress | undefined): number {
  if (!p) return 0;
  if (p.done && !p.error) return 1;
  const total = p.total ?? 0;
  return total > 0 ? Math.min(1, (p.received ?? 0) / total) : 0;
}
