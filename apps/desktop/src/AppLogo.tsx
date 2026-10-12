/**
 * Epoch's mark, in the family grammar (@nexis/design docs/ASSETS.md): a
 * 48×48 tile with 12px corners in currentColor, the mark knocked out in
 * var(--background). The mark is a training-loss curve settling onto the
 * point where a run converges: build, train, discover.
 */
export function AppLogo({ className }: { className?: string }) {
  return (
    <svg viewBox="0 0 48 48" className={className} aria-hidden="true">
      <rect width="48" height="48" rx="12" fill="currentColor" />
      <path
        d="M12 11c2.5 13 7 21.5 14.5 23.5 2.6.7 5.6.7 9.5.5"
        fill="none"
        stroke="var(--background)"
        strokeWidth="4.5"
        strokeLinecap="round"
      />
      <circle cx="36" cy="35" r="3.75" fill="var(--background)" />
      <path d="M12 39h12" stroke="var(--background)" strokeWidth="2.5" strokeLinecap="round" opacity="0.55" />
    </svg>
  );
}
