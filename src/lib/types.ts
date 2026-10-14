export interface Hotkey {
  ctrl: boolean;
  alt: boolean;
  shift: boolean;
  win: boolean;
  code: string;
  label: string;
}

export interface Sound {
  id: string;
  name: string;
  file: string;
  originalName: string;
  durationMs: number;
  volume: number;
  loop: boolean;
  favorite: boolean;
  color: string | null;
  folderId: string | null;
  hotkey: Hotkey | null;
  addedAt: number;
}

export interface Folder {
  id: string;
  name: string;
}

export interface Library {
  version: number;
  sounds: Sound[];
  folders: Folder[];
}

export type MinimizeBehavior = "disable" | "strict" | "keepActive";
export type RetriggerMode = "overlap" | "restart" | "toggle";

export interface Settings {
  version: number;
  outputDevice: string | null;
  outputDeviceName: string | null;
  monitorDevice: string | null;
  monitorDeviceName: string | null;
  masterVolume: number;
  monitorVolume: number;
  toggleHotkey: Hotkey | null;
  stopAllHotkey: Hotkey | null;
  minimizeBehavior: MinimizeBehavior;
  restoreOnShow: boolean;
  closeToTray: boolean;
  startWithWindows: boolean;
  startMinimized: boolean;
  startActive: boolean;
  stopOnDisable: boolean;
  retriggerMode: RetriggerMode;
  maxVoices: number;
  compactAlwaysOnTop: boolean;
  viewMode: "grid" | "list";
  trayNoticeShown: boolean;
}

export interface PlayingVoice {
  voiceId: number;
  soundId: string;
  startedAt: number;
  durationMs: number;
  looping: boolean;
  preview: boolean;
}

export interface OutputStatus {
  requested: string | null;
  requestedName: string | null;
  activeName: string | null;
  sampleRate: number;
  channels: number;
  fallback: boolean;
  error: string | null;
}

export interface AudioStatus {
  primary: OutputStatus;
  monitor: OutputStatus | null;
}

export interface DeviceInfo {
  id: string;
  name: string;
  isDefault: boolean;
}

export interface AudioPayload {
  status: AudioStatus;
  devices: DeviceInfo[];
}

export interface HotkeyIssue {
  target: string;
  label: string;
  message: string;
}

export interface Snapshot {
  library: Library;
  settings: Settings;
  active: boolean;
  hidden: boolean;
  compact: boolean;
  soundsLive: boolean;
  warnings: HotkeyIssue[];
}

export interface ImportFailure {
  name: string;
  message: string;
}

export interface ImportReport {
  imported: Sound[];
  failed: ImportFailure[];
}

export interface YoutubePreview {
  url: string;
  videoId: string;
  title: string;
  author: string;
  thumbnail: string | null;
}

export interface YoutubeProgress {
  stage: "preparing" | "fetching" | "downloading" | "converting" | "importing" | "done" | string;
  percent: number;
  message: string;
}

export interface HotkeyConflict {
  kind: string;
  name: string;
}

export interface HotkeyCheck {
  reserved: string | null;
  risky: string | null;
  conflict: HotkeyConflict | null;
  unsupported: boolean;
}

export const COLOR_LABELS = [
  { id: "red", label: "Red", swatch: "#b45454" },
  { id: "orange", label: "Orange", swatch: "#c47a3a" },
  { id: "yellow", label: "Yellow", swatch: "#c4b04a" },
  { id: "green", label: "Green", swatch: "#5a9a62" },
  { id: "teal", label: "Teal", swatch: "#4a8f88" },
  { id: "blue", label: "Blue", swatch: "#4a73a3" },
  { id: "purple", label: "Purple", swatch: "#7a5a9a" },
  { id: "pink", label: "Pink", swatch: "#a35a7a" },
] as const;

export function colorSwatch(id: string | null | undefined): string | null {
  return COLOR_LABELS.find((c) => c.id === id)?.swatch ?? null;
}
