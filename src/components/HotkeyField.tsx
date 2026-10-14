import { useEffect, useState } from "react";
import { Keyboard, X } from "lucide-react";
import { toast } from "sonner";
import { api } from "@/lib/api";
import { formatHotkey, fromKeyboardEvent, fromMouseEvent } from "@/lib/hotkey";
import type { Hotkey, HotkeyCheck } from "@/lib/types";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Kbd } from "@/components/ui/kbd";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";

interface Props {
  value: Hotkey | null;
  ignore?: string;
  onChange: (value: Hotkey | null) => void;
  label?: string;
}

export function HotkeyField({ value, ignore, onChange, label = "Assign keybind" }: Props) {
  const [open, setOpen] = useState(false);
  const [draft, setDraft] = useState<Hotkey | null>(null);
  const [check, setCheck] = useState<HotkeyCheck | null>(null);

  useEffect(() => {
    if (!open) return;
    setDraft(null);
    setCheck(null);

    const onKey = (event: KeyboardEvent) => {
      event.preventDefault();
      event.stopPropagation();
      if (event.key === "Escape" && !event.ctrlKey && !event.altKey && !event.shiftKey) {
        setOpen(false);
        return;
      }
      const next = fromKeyboardEvent(event);
      if (next) setDraft(next);
    };
    const onMouse = (event: MouseEvent) => {
      if (event.button === 0 || event.button === 2) return;
      event.preventDefault();
      const next = fromMouseEvent(event, {
        ctrl: event.ctrlKey,
        alt: event.altKey,
        shift: event.shiftKey,
        win: event.metaKey,
      });
      if (next) setDraft(next);
    };

    window.addEventListener("keydown", onKey, true);
    window.addEventListener("mousedown", onMouse, true);
    return () => {
      window.removeEventListener("keydown", onKey, true);
      window.removeEventListener("mousedown", onMouse, true);
    };
  }, [open]);

  useEffect(() => {
    if (!draft) {
      setCheck(null);
      return;
    }
    let cancelled = false;
    api.checkHotkey(draft, ignore).then((result) => {
      if (!cancelled) setCheck(result);
    });
    return () => {
      cancelled = true;
    };
  }, [draft, ignore]);

  const blocked = Boolean(check?.reserved || check?.unsupported);

  return (
    <div className="flex items-center gap-2">
      <Button type="button" variant="outline" size="sm" onClick={() => setOpen(true)}>
        <Keyboard />
        {value ? formatHotkey(value) : label}
      </Button>
      {value ? (
        <Button
          type="button"
          variant="ghost"
          size="icon-xs"
          aria-label="Clear keybind"
          onClick={() => onChange(null)}
        >
          <X />
        </Button>
      ) : null}

      <Dialog open={open} onOpenChange={setOpen}>
        <DialogContent className="sm:max-w-md" showCloseButton>
          <DialogHeader>
            <DialogTitle>Assign keybind</DialogTitle>
            <DialogDescription>
              Press the key or combination you want to use. Middle mouse, Mouse 4 and Mouse 5 are also supported. Escape cancels.
            </DialogDescription>
          </DialogHeader>

          <div className="flex min-h-20 items-center justify-center rounded-md border border-dashed bg-muted/40 px-4">
            {draft ? (
              <Kbd className="h-8 px-3 text-sm text-foreground">{formatHotkey(draft)}</Kbd>
            ) : (
              <p className="text-sm text-muted-foreground">Waiting for input</p>
            )}
          </div>

          {check?.reserved ? (
            <Alert variant="destructive">
              <AlertTitle>Reserved shortcut</AlertTitle>
              <AlertDescription>{check.reserved}</AlertDescription>
            </Alert>
          ) : null}
          {check?.unsupported ? (
            <Alert variant="destructive">
              <AlertTitle>Unsupported key</AlertTitle>
              <AlertDescription>Windows cannot register this key as a global shortcut.</AlertDescription>
            </Alert>
          ) : null}
          {check?.conflict ? (
            <Alert>
              <AlertTitle>Already in use</AlertTitle>
              <AlertDescription>
                {check.conflict.name} already uses this shortcut. Reassigning here will not steal it. Clear the other binding first, or choose a different combination.
              </AlertDescription>
            </Alert>
          ) : null}
          {check?.risky && !check.reserved ? (
            <Alert>
              <AlertTitle>Easy to trigger by accident</AlertTitle>
              <AlertDescription>{check.risky}</AlertDescription>
            </Alert>
          ) : null}

          <DialogFooter>
            <Button type="button" variant="outline" onClick={() => setOpen(false)}>
              Cancel
            </Button>
            <Button
              type="button"
              disabled={!draft || blocked || Boolean(check?.conflict)}
              onClick={() => {
                if (!draft || blocked || check?.conflict) return;
                onChange(draft);
                setOpen(false);
                toast.success(`Assigned ${formatHotkey(draft)}`);
              }}
            >
              Assign
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}
