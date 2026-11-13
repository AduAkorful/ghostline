/** Ghost mark: a ghost whose hem is a ledger line. */
export function Mark({ className = "" }: { className?: string }) {
  return (
    <svg viewBox="0 0 24 24" aria-hidden="true" className={className}>
      <path d="M5 20V10a7 7 0 0 1 14 0v10" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" />
      <path d="M3 20h18" stroke="var(--chain)" strokeWidth="1.6" strokeLinecap="round" />
      <circle cx="9.5" cy="10.5" r="1.1" fill="currentColor" /><circle cx="14.5" cy="10.5" r="1.1" fill="currentColor" />
    </svg>
  );
}
