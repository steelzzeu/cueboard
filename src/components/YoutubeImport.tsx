import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { Check, ClipboardPaste, LoaderCircle, X } from "lucide-react";
import { toast } from "sonner";
import { api } from "@/lib/api";
import { isYoutubeUrl } from "@/lib/youtube";
import type { YoutubePreview, YoutubeProgress } from "@/lib/types";
import { useAppStore } from "@/store/app";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";

const STAGES = [
  { id: "preparing", label: "Ready" },
  { id: "downloading", label: "Download" },
  { id: "converting", label: "MP3" },
  { id: "importing", label: "Board" },
] as const;

function stageIndex(stage: string): number {
  if (stage === "done") return STAGES.length;
  if (stage === "fetching") return 0;
  const i = STAGES.findIndex((s) => s.id === stage);
  return i < 0 ? 0 : i;
}

export function YoutubeImport() {
  const open = useAppStore((s) => s.youtubeOpen);
  const seed = useAppStore((s) => s.youtubeSeed);
  const closeYoutube = useAppStore((s) => s.closeYoutube);
  const filter = useAppStore((s) => s.filter);
  const folders = useAppStore((s) => s.snapshot?.library.folders ?? []);
  const inputRef = useRef<HTMLInputElement>(null);

  const [url, setUrl] = useState("");
  const [folderId, setFolderId] = useState<string>("unfiled");
  const [preview, setPreview] = useState<YoutubePreview | null>(null);
  const [looking, setLooking] = useState(false);
  const [busy, setBusy] = useState(false);
  const [progress, setProgress] = useState<YoutubeProgress | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [doneName, setDoneName] = useState<string | null>(null);
  const autoRef = useRef(0);
  const startedFor = useRef<string | null>(null);

  useEffect(() => {
    if (!open) return;
    const folder = folders.some((f) => f.id === filter) ? filter : "unfiled";
    setFolderId(folder);
    setUrl(seed);
    setPreview(null);
    setLooking(false);
    setBusy(false);
    setProgress(null);
    setError(null);
    setDoneName(null);
    startedFor.current = null;
    api.youtubePrepare().catch(() => undefined);
    const t = window.setTimeout(() => inputRef.current?.focus(), 40);
    return () => window.clearTimeout(t);
  }, [open, seed, filter, folders]);

  useEffect(() => {
    if (!open) return;
    const unlisten = listen<YoutubeProgress>("youtube-progress", (event) => {
      setProgress(event.payload);
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, [open]);

  useEffect(() => {
    if (!open || busy || doneName) return;
    const trimmed = url.trim();
    if (!isYoutubeUrl(trimmed)) {
      setPreview(null);
      setLooking(false);
      return;
    }
    const token = ++autoRef.current;
    setLooking(true);
    setError(null);
    const handle = window.setTimeout(() => {
      api
        .youtubePreview(trimmed)
        .then((data) => {
          if (token !== autoRef.current) return;
          setPreview(data);
          setLooking(false);
        })
        .catch((err) => {
          if (token !== autoRef.current) return;
          setPreview(null);
          setLooking(false);
          setError(String(err));
        });
    }, 180);
    return () => window.clearTimeout(handle);
  }, [url, open, busy, doneName]);

  useEffect(() => {
    if (!open || !preview || busy || doneName || error) return;
    if (startedFor.current === preview.url) return;
    startedFor.current = preview.url;
    const handle = window.setTimeout(() => {
      startImport();
    }, 280);
    return () => window.clearTimeout(handle);
  }, [preview, open, busy, doneName, error]);

  async function startImport() {
    const trimmed = url.trim();
    if (!isYoutubeUrl(trimmed) || busy) return;
    setBusy(true);
    setError(null);
    setDoneName(null);
    setProgress({ stage: "fetching", percent: 2, message: "Starting…" });
    try {
      const sound = await api.youtubeImport(trimmed, folderId === "unfiled" ? null : folderId);
      setDoneName(sound.name);
      setProgress({ stage: "done", percent: 100, message: "Added to your board." });
      toast.success(`Added ${sound.name}`);
    } catch (err) {
      const message = String(err);
      if (
        !message.toLowerCase().includes("cancelled") &&
        !message.toLowerCase().includes("already converting")
      ) {
        setError(message);
      }
      setProgress(null);
    } finally {
      setBusy(false);
    }
  }

  async function onOpenChange(next: boolean) {
    if (next) return;
    if (busy) {
      await api.youtubeCancel().catch(() => undefined);
    }
    closeYoutube();
  }

  const activeStage = stageIndex(progress?.stage ?? (preview ? "downloading" : "preparing"));

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="overflow-hidden sm:max-w-md" showCloseButton={!busy}>
        <DialogHeader>
          <DialogTitle>YouTube to board</DialogTitle>
          <DialogDescription>
            Paste a link. Cueboard pulls the audio, converts it to MP3, and drops it on the board.
          </DialogDescription>
        </DialogHeader>

        <div className="grid gap-4">
          <div className="grid gap-2">
            <Label htmlFor="yt-url">Link</Label>
            <div className="relative">
              <Input
                ref={inputRef}
                id="yt-url"
                value={url}
                disabled={busy}
                placeholder="youtube.com/watch?v=…"
                className="pr-9"
                onChange={(e) => {
                  setUrl(e.target.value);
                  setDoneName(null);
                  setError(null);
                }}
                onPaste={(e) => {
                  const text = e.clipboardData.getData("text");
                  if (isYoutubeUrl(text)) {
                    e.preventDefault();
                    setUrl(text.trim());
                  }
                }}
                onKeyDown={(e) => {
                  if (e.key === "Enter" && preview && !busy) startImport();
                }}
              />
              {url ? (
                <button
                  type="button"
                  className="absolute top-1/2 right-2 -translate-y-1/2 text-muted-foreground hover:text-foreground"
                  aria-label="Clear link"
                  disabled={busy}
                  onClick={() => {
                    setUrl("");
                    setPreview(null);
                    setDoneName(null);
                    setError(null);
                    inputRef.current?.focus();
                  }}
                >
                  <X className="size-3.5" />
                </button>
              ) : (
                <ClipboardPaste className="pointer-events-none absolute top-1/2 right-2.5 size-3.5 -translate-y-1/2 text-muted-foreground" />
              )}
            </div>
          </div>

          {folders.length > 0 ? (
            <div className="grid gap-2">
              <Label>Folder</Label>
              <Select value={folderId} onValueChange={setFolderId} disabled={busy}>
                <SelectTrigger className="w-full">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="unfiled">Unfiled</SelectItem>
                  {folders.map((folder) => (
                    <SelectItem key={folder.id} value={folder.id}>
                      {folder.name}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
          ) : null}

          {looking && !preview ? (
            <div className="flex items-center gap-2 rounded-md border bg-card px-3 py-3 text-sm text-muted-foreground">
              <LoaderCircle className="size-4 animate-spin" />
              Looking up that video
            </div>
          ) : null}

          {preview ? (
            <div className="overflow-hidden rounded-md border bg-card">
              {preview.thumbnail ? (
                <div className="relative aspect-video bg-muted">
                  <img
                    src={preview.thumbnail}
                    alt=""
                    className="size-full object-cover"
                  />
                  {doneName ? (
                    <div className="absolute inset-0 flex items-center justify-center bg-black/45">
                      <span className="flex items-center gap-2 rounded-full bg-background px-3 py-1 text-sm font-medium">
                        <Check className="size-4" />
                        On the board
                      </span>
                    </div>
                  ) : null}
                </div>
              ) : null}
              <div className="px-3 py-2.5">
                <p className="truncate text-sm font-medium">{preview.title}</p>
                {preview.author ? (
                  <p className="truncate text-xs text-muted-foreground">{preview.author}</p>
                ) : null}
              </div>
            </div>
          ) : null}

          {busy || progress ? (
            <div className="grid gap-2">
              <div className="h-1 overflow-hidden rounded-full bg-muted">
                <div
                  className="h-full rounded-full bg-foreground transition-[width] duration-300"
                  style={{ width: `${Math.max(progress?.percent ?? 8, 8)}%` }}
                />
              </div>
              <p className="text-xs text-muted-foreground">{progress?.message ?? "Working…"}</p>
              <ol className="flex gap-1">
                {STAGES.map((stage, index) => {
                  const on = index < activeStage || progress?.stage === "done";
                  const current = index === activeStage && progress?.stage !== "done";
                  return (
                    <li
                      key={stage.id}
                      className={`flex-1 rounded-full px-2 py-1 text-center text-[10px] font-medium tracking-wide uppercase ${
                        on
                          ? "bg-foreground text-background"
                          : current
                            ? "bg-secondary text-foreground"
                            : "bg-muted text-muted-foreground"
                      }`}
                    >
                      {stage.label}
                    </li>
                  );
                })}
              </ol>
            </div>
          ) : null}

          {error ? <p className="text-sm text-destructive">{error}</p> : null}

          <div className="flex justify-end gap-2">
            {busy ? (
              <Button
                variant="outline"
                onClick={() => {
                  api.youtubeCancel().catch(() => undefined);
                }}
              >
                Cancel
              </Button>
            ) : doneName ? (
              <>
                <Button variant="outline" onClick={() => closeYoutube()}>
                  Done
                </Button>
                <Button
                  onClick={() => {
                    setUrl("");
                    setPreview(null);
                    setDoneName(null);
                    setProgress(null);
                    setError(null);
                    inputRef.current?.focus();
                  }}
                >
                  Add another
                </Button>
              </>
            ) : (
              <Button onClick={startImport} disabled={!preview || looking}>
                Add MP3
              </Button>
            )}
          </div>
        </div>
      </DialogContent>
    </Dialog>
  );
}
