"use client";
import { useEffect, useState } from "react";
import { SearchIcon, openSearch, preloadSearch } from "./SearchDialog";

/** A large search field for the docs home. It opens the same dialog as the header. */
export function OpenSearchButton() {
  const [mac, setMac] = useState(false);
  useEffect(() => setMac(/Mac|iPhone|iPad/.test(navigator.platform)), []);
  return (
    <button type="button" onClick={openSearch} onPointerEnter={() => void preloadSearch().catch(() => {})}
      className="mt-8 flex w-full max-w-xl items-center gap-3 rounded-lg border border-rule bg-night px-4 py-3.5 text-left text-[15px] text-muted transition-colors duration-[var(--tick)] hover:border-ink/40 hover:text-ink">
      <SearchIcon className="h-[18px] w-[18px]" />
      <span>Search the docs</span>
      <kbd className="ml-auto hidden rounded border border-rule px-1.5 py-px font-mono text-[11px] sm:inline">{mac ? "⌘K" : "Ctrl K"}</kbd>
    </button>
  );
}
