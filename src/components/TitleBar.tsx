import { getCurrentWindow } from "@tauri-apps/api/window";
import { Minus, Square, X, Copy } from "lucide-react";
import { useEffect, useState } from "react";
import { api } from "@/lib/api";
import { formatHotkey } from "@/lib/hotkey";
import { useAppStore } from "@/store/app";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Switch } from "@/components/ui/switch";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";

export function TitleBar() {
  const snapshot = useAppStore((s) => s.snapshot);
  const [maximized, setMaximized] = useState(false);

  useEffect(() => {
    const window = getCurrentWindow();
    window.isMaximized().then(setMaximized).catch(() => undefined);
    const un = window.onResized(() => {
      window.isMaximized().then(setMaximized).catch(() => undefined);
    });
    return () => {
      un.then((fn) => fn());
    };
  }, []);

  const active = snapshot?.active ?? false;
  const live = snapshot?.soundsLive ?? false;
  const toggle = snapshot?.settings.toggleHotkey;

  return (
    <header className="flex h-11 shrink-0 items-center border-b bg-sidebar pr-0">
      <div data-tauri-drag-region className="flex min-w-0 flex-1 items-center gap-3 px-3">
        <div className="flex size-6 items-center justify-center rounded-sm border border-border bg-card text-[11px] font-semibold tracking-tight">
          CB
        </div>
        <span className="text-[13px] font-medium tracking-tight">Cueboard</span>
        <Badge
          variant="outline"
          className={
            live
              ? "border-success/30 bg-success/10 text-success"
              : "border-border bg-muted text-muted-foreground"
          }
        >
          <span className={`size-1.5 rounded-full ${live ? "bg-success" : "bg-muted-foreground/50"}`} />
          {live ? "Active" : "Disabled"}
        </Badge>
      </div>

      <div className="flex items-center gap-2 pr-1">
        <Tooltip>
          <TooltipTrigger asChild>
            <div className="flex items-center gap-2 rounded-md border border-border bg-card px-2 py-1">
              <span className="text-[11px] text-muted-foreground">Soundboard</span>
              <Switch
                size="sm"
                checked={active}
                onCheckedChange={(checked) => api.setActive(checked)}
                aria-label="Enable or disable the soundboard"
              />
            </div>
          </TooltipTrigger>
          <TooltipContent>
            {toggle
              ? `Global toggle: ${formatHotkey(toggle)}`
              : "Assign a global enable/disable shortcut in Settings"}
          </TooltipContent>
        </Tooltip>

        <div className="flex">
          <WindowButton
            label="Minimize"
            onClick={() => getCurrentWindow().minimize()}
          >
            <Minus className="size-3.5" />
          </WindowButton>
          <WindowButton
            label={maximized ? "Restore" : "Maximize"}
            onClick={() => getCurrentWindow().toggleMaximize()}
          >
            {maximized ? <Copy className="size-3 rotate-90" /> : <Square className="size-3" />}
          </WindowButton>
          <WindowButton
            label="Close"
            danger
            onClick={() => getCurrentWindow().close()}
          >
            <X className="size-3.5" />
          </WindowButton>
        </div>
      </div>
    </header>
  );
}

function WindowButton({
  children,
  label,
  onClick,
  danger,
}: {
  children: React.ReactNode;
  label: string;
  onClick: () => void;
  danger?: boolean;
}) {
  return (
    <Button
      type="button"
      variant="ghost"
      size="icon-sm"
      aria-label={label}
      className={`rounded-none ${danger ? "hover:bg-destructive hover:text-white" : ""}`}
      onClick={onClick}
    >
      {children}
    </Button>
  );
}
