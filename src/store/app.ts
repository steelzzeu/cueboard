import { create } from "zustand";
import { api } from "@/lib/api";
import type {
  AudioPayload,
  DeviceInfo,
  PlayingVoice,
  Settings,
  Snapshot,
  Sound,
} from "@/lib/types";

export type Page = "board" | "settings";
export type Filter = "all" | "favorites" | "unfiled" | string;

interface AppStore {
  loaded: boolean;
  page: Page;
  filter: Filter;
  query: string;
  snapshot: Snapshot | null;
  playback: PlayingVoice[];
  devices: DeviceInfo[];
  audio: AudioPayload | null;
  dragging: boolean;
  editing: Sound | null;
  deleting: Sound | null;
  youtubeOpen: boolean;
  youtubeSeed: string;
  setPage: (page: Page) => void;
  setFilter: (filter: Filter) => void;
  setQuery: (query: string) => void;
  setDragging: (dragging: boolean) => void;
  setEditing: (sound: Sound | null) => void;
  setDeleting: (sound: Sound | null) => void;
  openYoutube: (seed?: string) => void;
  closeYoutube: () => void;
  applySnapshot: (snapshot: Snapshot) => void;
  applyPlayback: (playback: PlayingVoice[]) => void;
  applyAudio: (audio: AudioPayload) => void;
  bootstrap: () => Promise<void>;
}

const emptyAudio: AudioPayload = {
  status: {
    primary: {
      requested: null,
      requestedName: null,
      activeName: null,
      sampleRate: 0,
      channels: 0,
      fallback: false,
      error: null,
    },
    monitor: null,
  },
  devices: [],
};

export const useAppStore = create<AppStore>((set) => ({
  loaded: false,
  page: "board",
  filter: "all",
  query: "",
  snapshot: null,
  playback: [],
  devices: [],
  audio: emptyAudio,
  dragging: false,
  editing: null,
  deleting: null,
  youtubeOpen: false,
  youtubeSeed: "",
  setPage: (page) => set({ page }),
  setFilter: (filter) => set({ filter }),
  setQuery: (query) => set({ query }),
  setDragging: (dragging) => set({ dragging }),
  setEditing: (editing) => set({ editing }),
  setDeleting: (deleting) => set({ deleting }),
  openYoutube: (seed = "") => set({ youtubeOpen: true, youtubeSeed: seed }),
  closeYoutube: () => set({ youtubeOpen: false, youtubeSeed: "" }),
  applySnapshot: (snapshot) => set({ snapshot, loaded: true }),
  applyPlayback: (playback) => set({ playback }),
  applyAudio: (audio) => set({ audio, devices: audio.devices }),
  bootstrap: async () => {
    const [snapshot, playback, audio] = await Promise.all([
      api.snapshot(),
      api.playback(),
      api.audio(),
    ]);
    set({
      snapshot,
      playback,
      audio,
      devices: audio.devices,
      loaded: true,
    });
  },
}));

export function useSettings(): Settings | null {
  return useAppStore((s) => s.snapshot?.settings ?? null);
}

export function voicesFor(soundId: string, playback: PlayingVoice[]): PlayingVoice[] {
  return playback.filter((v) => v.soundId === soundId);
}
