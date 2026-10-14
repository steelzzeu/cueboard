import { useEffect, useState } from "react";
import { Heart, MoreHorizontal, Pencil, Play, Square, Trash2 } from "lucide-react";
import { api } from "@/lib/api";
import { formatDuration, formatVolume } from "@/lib/format";
import { formatHotkey } from "@/lib/hotkey";
import { colorSwatch, type Sound } from "@/lib/types";
import { voicesFor, useAppStore } from "@/store/app";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Kbd } from "@/components/ui/kbd";
import { Slider } from "@/components/ui/slider";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";

interface Props {
  sound: Sound;
  compact?: boolean;
  list?: boolean;
}

export function SoundCard({ sound, compact, list }: Props) {
  const playback = useAppStore((s) => s.playback);
  const setEditing = useAppStore((s) => s.setEditing);
  const setDeleting = useAppStore((s) => s.setDeleting);
  const [volume, setVolume] = useState(sound.volume);
  useEffect(() => {
    setVolume(sound.volume);
  }, [sound.volume]);
  const voices = voicesFor(sound.id, playback);
  const playing = voices.length > 0;
  const voice = voices[0];
  const swatch = colorSwatch(sound.color);

  if (list) {
    return (
      <div className="group flex items-center gap-3 border-b px-3 py-2 hover:bg-accent/40">
        {swatch ? <span className="size-2 rounded-full" style={{ background: swatch }} /> : <span className="size-2" />}
        <button
          type="button"
          className="min-w-0 flex-1 truncate text-left text-sm font-medium"
          onClick={() => (playing ? api.stop(sound.id) : api.play(sound.id))}
        >
          {sound.name}
        </button>
        <span className="w-12 text-xs text-muted-foreground tabular-nums">{formatDuration(sound.durationMs)}</span>
        {sound.hotkey ? <Kbd>{formatHotkey(sound.hotkey)}</Kbd> : <span className="w-16" />}
        <Button size="icon-xs" variant={playing ? "secondary" : "ghost"} onClick={() => (playing ? api.stop(sound.id) : api.play(sound.id))}>
          {playing ? <Square /> : <Play />}
        </Button>
        <Button size="icon-xs" variant="ghost" onClick={() => setEditing(sound)}>
          <Pencil />
        </Button>
      </div>
    );
  }

  return (
    <div
      className={`relative flex flex-col overflow-hidden rounded-md border bg-card ${
        playing ? "border-foreground/25" : "border-border"
      } ${compact ? "p-2.5" : "p-3"}`}
    >
      {swatch ? <span className="absolute inset-x-0 top-0 h-0.5" style={{ background: swatch }} /> : null}
      {playing && voice ? (
        <span
          className="cue-progress pointer-events-none absolute inset-x-0 bottom-0 h-0.5 bg-foreground/40"
          data-loop={voice.looping ? "true" : "false"}
          style={{ animationDuration: `${Math.max(voice.durationMs, 200)}ms` }}
        />
      ) : null}

      <div className="flex items-start gap-2">
        <div className="min-w-0 flex-1">
          <p className="truncate text-sm font-medium">{sound.name}</p>
          <div className="mt-1 flex flex-wrap items-center gap-1.5">
            {sound.hotkey ? (
              <Kbd>{formatHotkey(sound.hotkey)}</Kbd>
            ) : (
              <span className="text-[11px] text-muted-foreground">No keybind</span>
            )}
            <span className="text-[11px] text-muted-foreground tabular-nums">
              {formatDuration(sound.durationMs)}
            </span>
            {sound.loop ? <span className="text-[11px] text-muted-foreground">Loop</span> : null}
            {playing ? (
              <span className="text-[11px] text-foreground">
                {voices.length > 1 ? `${voices.length} playing` : "Playing"}
              </span>
            ) : null}
          </div>
        </div>
        <Button
          type="button"
          size="icon-xs"
          variant="ghost"
          aria-label={sound.favorite ? "Remove from favourites" : "Add to favourites"}
          onClick={() => api.updateSound({ ...sound, favorite: !sound.favorite })}
        >
          <Heart className={sound.favorite ? "fill-foreground" : ""} />
        </Button>
      </div>

      {!compact ? (
        <div className="mt-3 flex items-center gap-2">
          <span className="w-8 text-[11px] text-muted-foreground tabular-nums">{formatVolume(volume)}</span>
          <Slider
            min={0}
            max={100}
            step={1}
            value={[Math.round(volume * 100)]}
            onValueChange={([v]) => setVolume((v ?? 100) / 100)}
            onValueCommit={([v]) => {
              const next = (v ?? 100) / 100;
              setVolume(next);
              api.updateSound({ ...sound, volume: next });
            }}
            aria-label={`${sound.name} volume`}
          />
        </div>
      ) : null}

      <div className={`flex items-center gap-1 ${compact ? "mt-2" : "mt-3"}`}>
        <Tooltip>
          <TooltipTrigger asChild>
            <Button
              type="button"
              size={compact ? "sm" : "sm"}
              variant={playing ? "secondary" : "default"}
              onClick={() => (playing ? api.stop(sound.id) : api.play(sound.id))}
            >
              {playing ? <Square /> : <Play />}
              {playing ? "Stop" : "Play"}
            </Button>
          </TooltipTrigger>
          <TooltipContent>{playing ? "Stop this sound" : "Play this sound"}</TooltipContent>
        </Tooltip>
        <Button type="button" size="sm" variant="ghost" onClick={() => setEditing(sound)}>
          <Pencil />
          Edit
        </Button>
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button type="button" size="icon-sm" variant="ghost" aria-label="More actions">
              <MoreHorizontal />
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end">
            <DropdownMenuItem onClick={() => api.preview(sound.id)}>Preview</DropdownMenuItem>
            <DropdownMenuItem onClick={() => api.updateSound({ ...sound, loop: !sound.loop })}>
              {sound.loop ? "Disable loop" : "Enable loop"}
            </DropdownMenuItem>
            <DropdownMenuSeparator />
            <DropdownMenuItem variant="destructive" onClick={() => setDeleting(sound)}>
              <Trash2 />
              Remove
            </DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
      </div>
    </div>
  );
}
