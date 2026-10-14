import { api } from "@/lib/api";
import { formatHotkey } from "@/lib/hotkey";
import { useAppStore } from "@/store/app";
import { Button } from "@/components/ui/button";

export function StatusBar() {
  const snapshot = useAppStore((s) => s.snapshot);
  const audio = useAppStore((s) => s.audio);
  const playback = useAppStore((s) => s.playback);
  const compact = snapshot?.compact ?? false;
  const device = audio?.status.primary.activeName ?? "No output";
  const rate = audio?.status.primary.sampleRate;
  const warning = audio?.status.primary.error;
  const toggle = snapshot?.settings.toggleHotkey;

  return (
    <footer className="flex h-8 shrink-0 items-center gap-3 border-t bg-sidebar px-3 text-[11px] text-muted-foreground">
      <span className="truncate">
        {device}
        {rate ? ` · ${Math.round(rate / 1000)} kHz` : ""}
      </span>
      {warning ? <span className="truncate text-warning">{warning}</span> : null}
      <span className="tabular-nums">{playback.length} playing</span>
      {toggle ? <span className="hidden sm:inline">Toggle {formatHotkey(toggle)}</span> : null}
      <span className="ml-auto hidden truncate md:inline">
        Headphones do not send audio to a livestream. See Settings for routing.
      </span>
      <Button
        type="button"
        size="xs"
        variant="ghost"
        className="ml-auto md:ml-0"
        onClick={() => api.setCompact(!compact)}
      >
        {compact ? "Full view" : "Compact"}
      </Button>
    </footer>
  );
}
