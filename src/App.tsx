import { useEffect } from "react";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { toast } from "sonner";
import { api } from "@/lib/api";
import { reportImport } from "@/lib/format";
import type { AudioPayload, ImportReport, PlayingVoice, Snapshot } from "@/lib/types";
import { useAppStore } from "@/store/app";
import { DeleteSound } from "@/components/DeleteSound";
import { DropOverlay } from "@/components/DropOverlay";
import { SettingsView } from "@/components/SettingsView";
import { Sidebar } from "@/components/Sidebar";
import { SoundBoard } from "@/components/SoundBoard";
import { SoundEditor } from "@/components/SoundEditor";
import { YoutubeImport } from "@/components/YoutubeImport";
import { isYoutubeUrl } from "@/lib/youtube";
import { StatusBar } from "@/components/StatusBar";
import { TitleBar } from "@/components/TitleBar";
import { Toaster } from "@/components/ui/sonner";
import { TooltipProvider } from "@/components/ui/tooltip";

export function App() {
  const loaded = useAppStore((s) => s.loaded);
  const page = useAppStore((s) => s.page);
  const compact = useAppStore((s) => s.snapshot?.compact ?? false);
  const bootstrap = useAppStore((s) => s.bootstrap);
  const applySnapshot = useAppStore((s) => s.applySnapshot);
  const applyPlayback = useAppStore((s) => s.applyPlayback);
  const applyAudio = useAppStore((s) => s.applyAudio);
  const setDragging = useAppStore((s) => s.setDragging);
  const openYoutube = useAppStore((s) => s.openYoutube);
  const youtubeOpen = useAppStore((s) => s.youtubeOpen);

  useEffect(() => {
    bootstrap().catch((err) => toast.error(String(err)));
  }, [bootstrap]);

  useEffect(() => {
    const unlisten: Array<Promise<() => void>> = [];
    unlisten.push(listen<Snapshot>("snapshot", (event) => applySnapshot(event.payload)));
    unlisten.push(listen<PlayingVoice[]>("playback", (event) => applyPlayback(event.payload)));
    unlisten.push(listen<AudioPayload>("audio-status", (event) => applyAudio(event.payload)));
    unlisten.push(
      listen<string>("notice", (event) => {
        toast.warning(event.payload);
      }),
    );
    unlisten.push(
      listen<ImportReport>("import-result", (event) => {
        handleReport(event.payload);
      }),
    );

    const window = getCurrentWindow();
    unlisten.push(
      window.onDragDropEvent((event) => {
        if (event.payload.type === "enter" || event.payload.type === "over") {
          setDragging(true);
        } else if (event.payload.type === "leave") {
          setDragging(false);
        } else if (event.payload.type === "drop") {
          setDragging(false);
          api.importPaths(event.payload.paths).then(handleReport).catch((err) => toast.error(String(err)));
        }
      }),
    );

    return () => {
      unlisten.forEach((p) => p.then((fn) => fn()));
    };
  }, [applyAudio, applyPlayback, applySnapshot, setDragging]);

  useEffect(() => {
    const onPaste = (event: ClipboardEvent) => {
      if (youtubeOpen) return;
      const target = event.target as HTMLElement | null;
      if (target?.closest("input, textarea, [contenteditable='true']")) return;
      const text = event.clipboardData?.getData("text") ?? "";
      if (!isYoutubeUrl(text)) return;
      event.preventDefault();
      openYoutube(text.trim());
    };
    window.addEventListener("paste", onPaste);
    return () => window.removeEventListener("paste", onPaste);
  }, [openYoutube, youtubeOpen]);

  if (!loaded) {
    return (
      <div className="flex h-full items-center justify-center bg-background text-sm text-muted-foreground">
        Starting Cueboard
      </div>
    );
  }

  return (
    <TooltipProvider delayDuration={250}>
      <div className="relative flex h-full flex-col bg-background">
        <TitleBar />
        {compact ? (
          <SoundBoard compact />
        ) : (
          <div className="flex min-h-0 flex-1">
            <Sidebar />
            {page === "settings" ? <SettingsView /> : <SoundBoard />}
          </div>
        )}
        <StatusBar />
        <SoundEditor />
        <YoutubeImport />
        <DeleteSound />
        <DropOverlay />
        <Toaster position="bottom-right" />
      </div>
    </TooltipProvider>
  );
}

function handleReport(report: ImportReport) {
  if (report.imported.length || report.failed.length) {
    const message = reportImport(report.imported.length, report.failed.length);
    if (report.failed.length && !report.imported.length) toast.error(message);
    else if (report.failed.length) toast.warning(message);
    else toast.success(message);
  }
  report.failed.forEach((fail) => toast.error(`${fail.name}: ${fail.message}`));
}
