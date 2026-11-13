"use client";
import Link from "next/link";
import { usePathname } from "next/navigation";
import { useEffect, useState } from "react";
import { Mark } from "./Mark";
import { OPEN_SEARCH_EVENT, SearchDialog, SearchIcon, preloadSearch } from "./SearchDialog";

const NAV = [
  { href: "/how-it-works/", label: "How it works" },
  { href: "/privacy/", label: "Privacy" },
  { href: "/proof/", label: "Proof" },
  { href: "/fees/", label: "Fees" },
  { href: "/docs/", label: "Docs" },
];

export function Header() {
  const path = usePathname();
  const [open, setOpen] = useState(false);
  const [searchOpen, setSearchOpen] = useState(false);
  const [mac, setMac] = useState(false);
  useEffect(() => setOpen(false), [path]);
  useEffect(() => {
    setMac(/Mac|iPhone|iPad/.test(navigator.platform));
    const onKey = (e: KeyboardEvent) => {
      const typing = e.target instanceof HTMLElement && e.target.closest("input, textarea, select, [contenteditable='true']");
      if ((e.key === "k" || e.key === "K") && (e.metaKey || e.ctrlKey)) { e.preventDefault(); setSearchOpen((o) => !o); }
      else if (e.key === "/" && !typing && !e.metaKey && !e.ctrlKey && !e.altKey) { e.preventDefault(); setSearchOpen(true); }
    };
    const onOpen = () => setSearchOpen(true);
    addEventListener("keydown", onKey);
    addEventListener(OPEN_SEARCH_EVENT, onOpen);
    return () => { removeEventListener("keydown", onKey); removeEventListener(OPEN_SEARCH_EVENT, onOpen); };
  }, []);
  const active = (href: string) => (href === "/docs/" ? path.startsWith("/docs") : path === href);

  return (
    <header className="sticky top-0 z-40 border-b border-rule bg-night/85 backdrop-blur-md">
      <div className={`mx-auto flex h-16 items-center justify-between px-4 md:px-6 ${path.startsWith("/docs") ? "" : "max-w-[1200px]"}`}>
        <Link href="/" className="flex items-center gap-2.5" aria-label="Ghostline home">
          <Mark className="h-6 w-6 text-ink" />
          <span className="voice text-[22px] italic leading-none">Ghostline</span>
          <span className="eyebrow hidden sm:inline">on Ootle mainnet</span>
        </Link>
        <nav aria-label="Main" className="hidden items-center gap-1 md:flex">
          {NAV.map((n) => (
            <Link key={n.href} href={n.href} aria-current={active(n.href) ? "page" : undefined}
              className={`rounded-sm px-3 py-2 text-sm transition-colors duration-[var(--tick)] ${active(n.href) ? "text-ink" : "text-muted hover:text-ink"}`}>
              {n.label}
            </Link>
          ))}
        </nav>
        <div className="flex items-center gap-2">
          <button type="button" onClick={() => setSearchOpen(true)} onPointerEnter={() => void preloadSearch().catch(() => {})} onFocus={() => void preloadSearch().catch(() => {})}
            aria-label="Search" aria-keyshortcuts="Control+K Meta+K"
            className="flex h-9 w-9 items-center justify-center gap-2 rounded-md border border-rule text-muted transition-colors duration-[var(--tick)] hover:border-ink/40 hover:text-ink lg:w-60 lg:justify-start lg:px-3">
            <SearchIcon />
            <span className="hidden text-sm lg:inline">Search docs</span>
            <kbd className="ml-auto hidden rounded border border-rule px-1.5 py-px font-mono text-[11px] lg:inline">{mac ? "⌘K" : "Ctrl K"}</kbd>
          </button>
          <Link href="/agents/" className="hidden rounded-sm bg-ink px-3.5 py-2 text-sm font-semibold text-on-ink sm:inline-block">Set up your agent →</Link>
          <button className="grid h-9 w-9 place-items-center rounded-sm border border-rule md:hidden" aria-expanded={open} aria-controls="mnav"
            aria-label="Menu" onClick={() => setOpen((o) => !o)}>{open ? "✕" : "≡"}</button>
        </div>
      </div>
      <nav hidden={!open} id="mnav" aria-label="Main" className="border-t border-rule px-4 pb-4 md:hidden">
          {NAV.concat({ href: "/agents/", label: "Set up your agent →" }).map((n) => (
            <Link key={n.href} href={n.href} aria-current={active(n.href) ? "page" : undefined} className="block border-b border-rule py-3 text-[15px]">{n.label}</Link>
          ))}
      </nav>
      <SearchDialog open={searchOpen} onClose={() => setSearchOpen(false)} />
    </header>
  );
}
