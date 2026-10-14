import { useEffect, useState } from "react";
import { toast } from "sonner";
import { api } from "@/lib/api";
import { formatVolume } from "@/lib/format";
import { COLOR_LABELS, type Sound } from "@/lib/types";
import { useAppStore } from "@/store/app";
import { HotkeyField } from "@/components/HotkeyField";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogFooter,
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
import { Slider } from "@/components/ui/slider";
import { Switch } from "@/components/ui/switch";

export function SoundEditor() {
  const sound = useAppStore((s) => s.editing);
  const setEditing = useAppStore((s) => s.setEditing);
  const folders = useAppStore((s) => s.snapshot?.library.folders ?? []);
  const [draft, setDraft] = useState<Sound | null>(null);

  useEffect(() => {
    setDraft(sound);
  }, [sound]);

  if (!draft) return null;

  return (
    <Dialog open={Boolean(sound)} onOpenChange={(open) => !open && setEditing(null)}>
      <DialogContent className="sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>Edit sound</DialogTitle>
        </DialogHeader>
        <div className="grid gap-4">
          <div className="grid gap-2">
            <Label htmlFor="sound-name">Name</Label>
            <Input
              id="sound-name"
              value={draft.name}
              onChange={(e) => setDraft({ ...draft, name: e.target.value })}
            />
          </div>

          <div className="grid gap-2">
            <Label>Folder</Label>
            <Select
              value={draft.folderId ?? "none"}
              onValueChange={(value) =>
                setDraft({ ...draft, folderId: value === "none" ? null : value })
              }
            >
              <SelectTrigger className="w-full">
                <SelectValue placeholder="Unfiled" />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="none">Unfiled</SelectItem>
                {folders.map((folder) => (
                  <SelectItem key={folder.id} value={folder.id}>
                    {folder.name}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>

          <div className="grid gap-2">
            <Label>Colour label</Label>
            <div className="flex flex-wrap gap-1.5">
              <button
                type="button"
                className={`h-7 rounded-md border px-2 text-xs ${
                  !draft.color ? "border-foreground/40 bg-muted" : "border-border"
                }`}
                onClick={() => setDraft({ ...draft, color: null })}
              >
                None
              </button>
              {COLOR_LABELS.map((color) => (
                <button
                  key={color.id}
                  type="button"
                  title={color.label}
                  aria-label={color.label}
                  className={`size-7 rounded-md border ${
                    draft.color === color.id ? "border-foreground" : "border-border"
                  }`}
                  style={{ background: color.swatch }}
                  onClick={() => setDraft({ ...draft, color: color.id })}
                />
              ))}
            </div>
          </div>

          <div className="grid gap-2">
            <div className="flex items-center justify-between">
              <Label>Volume</Label>
              <span className="text-xs text-muted-foreground tabular-nums">{formatVolume(draft.volume)}</span>
            </div>
            <Slider
              min={0}
              max={100}
              value={[Math.round(draft.volume * 100)]}
              onValueChange={([v]) => setDraft({ ...draft, volume: (v ?? 100) / 100 })}
            />
          </div>

          <div className="flex items-center justify-between rounded-md border px-3 py-2">
            <div>
              <p className="text-sm font-medium">Loop</p>
              <p className="text-xs text-muted-foreground">Keep playing until stopped</p>
            </div>
            <Switch
              checked={draft.loop}
              onCheckedChange={(loop) => setDraft({ ...draft, loop })}
            />
          </div>

          <div className="flex items-center justify-between rounded-md border px-3 py-2">
            <div>
              <p className="text-sm font-medium">Favourite</p>
              <p className="text-xs text-muted-foreground">Pin this sound in the Favourites list</p>
            </div>
            <Switch
              checked={draft.favorite}
              onCheckedChange={(favorite) => setDraft({ ...draft, favorite })}
            />
          </div>

          <div className="grid gap-2">
            <Label>Keyboard shortcut</Label>
            <HotkeyField
              value={draft.hotkey}
              ignore={draft.id}
              onChange={(hotkey) => setDraft({ ...draft, hotkey })}
            />
          </div>
        </div>
        <DialogFooter>
          <Button variant="outline" onClick={() => api.preview(draft.id)}>
            Preview
          </Button>
          <Button variant="outline" onClick={() => setEditing(null)}>
            Cancel
          </Button>
          <Button
            onClick={async () => {
              try {
                await api.updateSound(draft);
                setEditing(null);
                toast.success("Sound updated");
              } catch (err) {
                toast.error(String(err));
              }
            }}
          >
            Save
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
