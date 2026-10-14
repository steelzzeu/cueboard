import { useEffect, useMemo, useState } from "react";
import { Plus, Search, Square } from "lucide-react";
import { toast } from "sonner";
import { api } from "@/lib/api";
import { formatDuration } from "@/lib/format";
import type { CatalogItem } from "@/lib/types";
import { voicesFor, useAppStore } from "@/store/app";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Badge } from "@/components/ui/badge";

const SUGGESTIONS = [
  "airhorn",
  "laugh",
  "boom",
  "whoosh",
  "punch",
  "click",
  "horn",
  "drum",
  "scream",
  "coin",
  "laser",
  "notification",
];

export function CatalogView() {
  const sounds = useAppStore((s) => s.snapshot?.library.sounds ?? []);
  const playback = useAppStore((s) => s.playback);
  const [query, setQuery] = useState("airhorn");
  const [draft, setDraft] = useState("airhorn");
  const [page, setPage] = useState(1);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [results, setResults] = useState<CatalogItem[]>([]);
  const [pageCount, setPageCount] = useState(1);
  const [resultCount, setResultCount] = useState(0);
  const [adding, setAdding] = useState<string | null>(null);
  const [previewing, setPreviewing] = useState<string | null>(null);

  const added = useMemo(() => {
    const ids = new Set<string>();
    for (const sound of sounds) {
      if (sound.originalName.startsWith("openverse:")) {
        ids.add(sound.originalName.slice("openverse:".length));
      }
    }
    return ids;
  }, [sounds]);

  useEffect(() => {
    const handle = window.setTimeout(() => {
      const next = draft.trim();
      if (next.length >= 2) {
        setQuery(next);
        setPage(1);
      }
    }, 400);
    return () => window.clearTimeout(handle);
  }, [draft]);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    setError(null);
    api
      .catalogSearch(query, page)
      .then((data) => {
        if (cancelled) return;
        setResults(data.results);
        setPageCount(data.pageCount);
        setResultCount(data.resultCount);
      })
      .catch((err) => {
        if (cancelled) return;
        setResults([]);
        setError(String(err));
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [query, page]);

  async function preview(item: CatalogItem) {
    const id = `cat:${item.id}`;
    const playing = voicesFor(id, playback).length > 0;
    if (playing) {
      await api.stop(id);
      return;
    }
    setPreviewing(item.id);
    try {
      await api.catalogPreview(item);
    } catch (err) {
      toast.error(String(err));
    } finally {
      setPreviewing((cur) => (cur === item.id ? null : cur));
    }
  }

  async function add(item: CatalogItem) {
    if (added.has(item.id)) {
      toast.message("Already on your board.");
      return;
    }
    setAdding(item.id);
    try {
      await api.catalogImport(item);
      toast.success(`Added ${item.name} to Catalog.`);
    } catch (err) {
      toast.error(String(err));
    } finally {
      setAdding((cur) => (cur === item.id ? null : cur));
    }
  }

  return (
    <section className="flex min-w-0 flex-1 flex-col">
      <div className="flex flex-wrap items-center gap-2 border-b px-3 py-2">
        <h1 className="mr-auto text-sm font-medium">Catalog</h1>
        <div className="relative min-w-48 flex-1">
          <Search className="pointer-events-none absolute top-1/2 left-2.5 size-3.5 -translate-y-1/2 text-muted-foreground" />
          <Input
            value={draft}
            onChange={(e) => setDraft(e.target.value)}
            placeholder="Search the catalog"
            className="h-8 pl-8"
          />
        </div>
        <Button size="sm" variant="ghost" onClick={() => api.stopAll()}>
          <Square />
          Stop all
        </Button>
      </div>

      <div className="flex flex-wrap gap-1.5 border-b px-3 py-2">
        {SUGGESTIONS.map((tag) => (
          <button
            key={tag}
            type="button"
            onClick={() => {
              setDraft(tag);
              setQuery(tag);
              setPage(1);
            }}
            className={`rounded-full border px-2.5 py-0.5 text-xs ${
              query.toLowerCase() === tag
                ? "border-foreground/30 bg-secondary text-foreground"
                : "border-border text-muted-foreground hover:text-foreground"
            }`}
          >
            {tag}
          </button>
        ))}
      </div>

      <p className="border-b px-3 py-2 text-xs text-muted-foreground">
        Search Creative Commons clips from Freesound. Preview, then add a copy to your local board.
        Sound Buttons Lab cannot be mirrored here.
      </p>

      {error ? (
        <div className="flex flex-1 items-center justify-center p-8 text-sm text-muted-foreground">
          {error}
        </div>
      ) : loading && results.length === 0 ? (
        <div className="flex flex-1 items-center justify-center p-8 text-sm text-muted-foreground">
          Searching catalog
        </div>
      ) : results.length === 0 ? (
        <div className="flex flex-1 items-center justify-center p-8 text-sm text-muted-foreground">
          No short clips matched. Try another word.
        </div>
      ) : (
        <ScrollArea className="flex-1">
          <div className="grid grid-cols-[repeat(auto-fill,minmax(180px,1fr))] gap-2 p-3">
            {results.map((item) => {
              const id = `cat:${item.id}`;
              const playing = voicesFor(id, playback).length > 0;
              const onBoard = added.has(item.id);
              return (
                <article
                  key={item.id}
                  className={`flex flex-col rounded-md border bg-card p-3 ${
                    playing ? "border-foreground/25" : "border-border"
                  }`}
                >
                  <button
                    type="button"
                    className="min-w-0 text-left"
                    onClick={() => preview(item)}
                    disabled={previewing === item.id}
                  >
                    <p className="truncate text-sm font-medium">{item.name}</p>
                    <p className="mt-1 truncate text-[11px] text-muted-foreground">
                      {item.creator || "Unknown"} · {formatDuration(item.durationMs)}
                    </p>
                  </button>
                  <div className="mt-2 flex items-center gap-1.5">
                    <Badge variant="outline" className="text-[10px] uppercase">
                      {item.license}
                    </Badge>
                    <span className="ml-auto" />
                    <Button
                      size="xs"
                      variant={onBoard ? "secondary" : "outline"}
                      disabled={onBoard || adding === item.id}
                      onClick={() => add(item)}
                    >
                      <Plus />
                      {onBoard ? "Added" : adding === item.id ? "Adding" : "Add"}
                    </Button>
                  </div>
                </article>
              );
            })}
          </div>
          <div className="flex items-center justify-between px-3 pb-3 text-xs text-muted-foreground">
            <span>
              {resultCount} result{resultCount === 1 ? "" : "s"} · page {page} of {pageCount}
            </span>
            <div className="flex gap-1">
              <Button
                size="xs"
                variant="outline"
                disabled={page <= 1 || loading}
                onClick={() => setPage((p) => Math.max(1, p - 1))}
              >
                Previous
              </Button>
              <Button
                size="xs"
                variant="outline"
                disabled={page >= pageCount || loading}
                onClick={() => setPage((p) => p + 1)}
              >
                Next
              </Button>
            </div>
          </div>
        </ScrollArea>
      )}
    </section>
  );
}
