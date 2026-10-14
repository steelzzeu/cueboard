export function formatDuration(ms: number): string {
  if (!ms || ms < 0) return "0:00";
  const total = Math.round(ms / 1000);
  const m = Math.floor(total / 60);
  const s = total % 60;
  return `${m}:${s.toString().padStart(2, "0")}`;
}

export function formatVolume(value: number): string {
  return `${Math.round(value * 100)}%`;
}

export function reportImport(imported: number, failed: number): string {
  if (imported && failed) {
    return `Imported ${imported} sound${imported === 1 ? "" : "s"}. ${failed} could not be added.`;
  }
  if (imported) {
    return `Imported ${imported} sound${imported === 1 ? "" : "s"}.`;
  }
  if (failed) {
    return `Could not import ${failed} file${failed === 1 ? "" : "s"}.`;
  }
  return "No files were imported.";
}
