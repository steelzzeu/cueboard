export function YoutubeMark({ className }: { className?: string }) {
  return (
    <svg viewBox="0 0 24 24" aria-hidden="true" className={className ?? "size-4"}>
      <rect x="2.5" y="6.5" width="19" height="11" rx="3" fill="currentColor" />
      <path d="M10 9.25v5.5L15.5 12 10 9.25z" fill="var(--background)" />
    </svg>
  );
}
