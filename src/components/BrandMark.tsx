export function BrandMark({ className }: { className?: string }) {
  return (
    <svg
      viewBox="0 0 24 24"
      aria-hidden="true"
      className={className}
      fill="currentColor"
    >
      <rect x="4" y="8" width="4" height="10" rx="1.2" />
      <rect x="10" y="4" width="4" height="16" rx="1.2" />
      <rect x="16" y="10" width="4" height="8" rx="1.2" />
    </svg>
  );
}
