import { useAppStore } from "@/store/app";

export function DropOverlay() {
  const dragging = useAppStore((s) => s.dragging);
  if (!dragging) return null;
  return (
    <div className="pointer-events-none absolute inset-0 z-40 flex items-center justify-center bg-background/80">
      <div className="rounded-md border border-dashed bg-card px-8 py-6 text-center">
        <p className="text-sm font-medium">Drop audio files to import</p>
        <p className="mt-1 text-xs text-muted-foreground">
          Multiple files are added in one pass. Unsupported or damaged files are skipped.
        </p>
      </div>
    </div>
  );
}
