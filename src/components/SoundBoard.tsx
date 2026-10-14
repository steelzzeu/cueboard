import { Grid2x2, List, Search, Square, Upload } from "lucide-react";
import { YoutubeMark } from "@/components/YoutubeMark";
import { api } from "@/lib/api";
import { useAppStore } from "@/store/app";
import { Onboarding } from "@/components/Onboarding";
import { SoundCard } from "@/components/SoundCard";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { ScrollArea } from "@/components/ui/scroll-area";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";

export function SoundBoard({ compact = false }: { compact?: boolean }) {
  const snapshot = useAppStore((s) => s.snapshot);
  const filter = useAppStore((s) => s.filter);
  const query = useAppStore((s) => s.query);
  const setQuery = useAppStore((s) => s.setQuery);
  const openYoutube = useAppStore((s) => s.openYoutube);
  const sounds = snapshot?.library.sounds ?? [];
  const folders = snapshot?.library.folders ?? [];
  const view = snapshot?.settings.viewMode ?? "grid";

  const title =
    filter === "all"
      ? "All sounds"
      : filter === "favorites"
        ? "Favourites"
        : filter === "unfiled"
          ? "Unfiled"
          : (folders.find((f) => f.id === filter)?.name ?? "Sounds");

  const visible = sounds
    .filter((sound) => {
      if (filter === "favorites") return sound.favorite;
      if (filter === "unfiled") return !sound.folderId;
      if (filter !== "all") return sound.folderId === filter;
      return true;
    })
    .filter((sound) => {
      if (!query.trim()) return true;
      const q = query.toLowerCase();
      return sound.name.toLowerCase().includes(q) || sound.originalName.toLowerCase().includes(q);
    })
    .slice()
    .sort((a, b) => {
      if (a.favorite !== b.favorite) return a.favorite ? -1 : 1;
      return a.name.localeCompare(b.name);
    });

  return (
    <section className="flex min-w-0 flex-1 flex-col">
      <div className="flex flex-wrap items-center gap-2 border-b px-3 py-2">
        {!compact ? <h1 className="mr-auto text-sm font-medium">{title}</h1> : null}
        <div className="relative min-w-40 flex-1">
          <Search className="pointer-events-none absolute top-1/2 left-2.5 size-3.5 -translate-y-1/2 text-muted-foreground" />
          <Input
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder="Search sounds"
            className="h-8 pl-8"
          />
        </div>
        <Tooltip>
          <TooltipTrigger asChild>
            <Button size="sm" variant="outline" onClick={() => api.pickImport()}>
              <Upload />
              Import
            </Button>
          </TooltipTrigger>
          <TooltipContent>Add one or more audio files</TooltipContent>
        </Tooltip>
        {!compact ? (
          <Tooltip>
            <TooltipTrigger asChild>
              <Button size="sm" variant="outline" onClick={() => openYoutube()}>
                <YoutubeMark />
                YouTube
              </Button>
            </TooltipTrigger>
            <TooltipContent>Paste a link. Converts to MP3 and adds it to the board.</TooltipContent>
          </Tooltip>
        ) : null}
        <Tooltip>
          <TooltipTrigger asChild>
            <Button size="sm" variant="ghost" onClick={() => api.stopAll()}>
              <Square />
              Stop all
            </Button>
          </TooltipTrigger>
          <TooltipContent>
            {snapshot?.settings.stopAllHotkey
              ? "Also available as a global shortcut"
              : "Stop every playing sound"}
          </TooltipContent>
        </Tooltip>
        {!compact ? (
          <ToggleGroup
            type="single"
            size="sm"
            variant="outline"
            value={view}
            onValueChange={(value) => {
              if (value) api.setViewMode(value);
            }}
          >
            <ToggleGroupItem value="grid" aria-label="Grid view">
              <Grid2x2 />
            </ToggleGroupItem>
            <ToggleGroupItem value="list" aria-label="List view">
              <List />
            </ToggleGroupItem>
          </ToggleGroup>
        ) : null}
      </div>

      {sounds.length === 0 ? (
        <Onboarding />
      ) : visible.length === 0 ? (
        <div className="flex flex-1 items-center justify-center p-8 text-sm text-muted-foreground">
          No sounds match this filter.
        </div>
      ) : view === "list" && !compact ? (
        <ScrollArea className="flex-1">
          <div className="min-w-[640px]">
            {visible.map((sound) => (
              <SoundCard key={sound.id} sound={sound} list />
            ))}
          </div>
        </ScrollArea>
      ) : (
        <ScrollArea className="flex-1">
          <div
            className={`grid gap-2 p-3 ${
              compact
                ? "grid-cols-1"
                : "grid-cols-[repeat(auto-fill,minmax(220px,1fr))]"
            }`}
          >
            {visible.map((sound) => (
              <SoundCard key={sound.id} sound={sound} compact={compact} />
            ))}
          </div>
        </ScrollArea>
      )}
    </section>
  );
}
