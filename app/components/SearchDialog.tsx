"use client";
// Site-wide search dialog. The index is a static JSON file fetched the first time search opens (or is hovered).
import Link from "next/link";
import { useRouter } from "next/navigation";
import { useEffect, useId, useMemo, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { DOCS, type IndexEntry } from "@/lib/docs-meta";
import { highlight, prepare, search, snippet, type Part, type Prepared } from "@/lib/search";

let cached: Prepared[] | null = null;
let pending: Promise<Prepared[]> | null = null;

export function preloadSearch(): Promise<Prepared[]> {
  if (cached) return Promise.resolve(cached);
  pending ??= fetch("/search-index.json")
    .then((r) => { if (!r.ok) throw new Error(`search index ${r.status}`); return r.json() as Promise<IndexEntry[]>; })
    .then((entries) => (cached = prepare(entries)))
    .catch((error) => { pending = null; throw error; });
  return pending;
}

export const OPEN_SEARCH_EVENT = "gl:open-search";
export const openSearch = () => window.dispatchEvent(new Event(OPEN_SEARCH_EVENT));

const SUGGESTED = ["mainnet-quickstart", "mcp", "policy", "settle", "fees", "privacy"]
  .map((slug) => DOCS.find((d) => d.slug === slug)!);

export function SearchIcon({ className = "h-4 w-4" }: { className?: string }) {
  return (
    <svg viewBox="0 0 20 20" className={className} aria-hidden="true" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round">
      <circle cx="9" cy="9" r="5.5" /><path d="m13.5 13.5 4 4" />
    </svg>
  );
}

function Marked({ parts }: { parts: Part[] }) {
  return <>{parts.map((p, i) => p.hit ? <mark key={i} className="rounded-[2px] bg-chain/20 px-px text-ink">{p.text}</mark> : <span key={i}>{p.text}</span>)}</>;
}

type Row = { href: string; crumb: string; title: Part[]; excerpt?: Part[] };

export function SearchDialog({ open, onClose }: { open: boolean; onClose: () => void }) {
  const router = useRouter();
  const [mounted, setMounted] = useState(false);
  const [query, setQuery] = useState("");
  const [active, setActive] = useState(0);
  const [index, setIndex] = useState<Prepared[] | null>(cached);
  const [failed, setFailed] = useState(false);
  const input = useRef<HTMLInputElement>(null);
  const dialog = useRef<HTMLDivElement>(null);
  const opener = useRef<HTMLElement | null>(null);
  const id = useId();

  useEffect(() => setMounted(true), []);

  useEffect(() => {
    if (!open) return;
    opener.current = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    setQuery(""); setActive(0); setFailed(false);
    const prev = document.body.style.overflow;
    document.body.style.overflow = "hidden";
    input.current?.focus();
    preloadSearch().then(setIndex, () => setFailed(true));
    return () => { document.body.style.overflow = prev; opener.current?.focus?.(); };
  }, [open]);

  const results = useMemo(() => (index && query.trim() ? search(index, query) : []), [index, query]);
  const rows: Row[] = useMemo(() => {
    if (!query.trim()) {
      return SUGGESTED.map((d) => ({ href: `/docs/${d.slug}/`, crumb: d.group, title: [{ text: d.label, hit: false }] }));
    }
    return results.map(({ entry }) => ({
      href: entry.href,
      crumb: entry.heading ? `${entry.group} › ${entry.page}` : entry.group === "Site" ? "Site" : entry.group,
      title: highlight(entry.heading ?? entry.page, query),
      excerpt: entry.text ? snippet(entry.text, query) : undefined,
    }));
  }, [results, query]);

  useEffect(() => { document.getElementById(`${id}-${active}`)?.scrollIntoView({ block: "nearest" }); }, [active, id]);

  if (!mounted || !open) return null;

  const go = (row: Row | undefined) => { if (!row) return; onClose(); router.push(row.href); };
  const onKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Escape") { e.preventDefault(); onClose(); return; }
    if (e.key === "ArrowDown") { e.preventDefault(); setActive((a) => (rows.length ? (a + 1) % rows.length : 0)); }
    else if (e.key === "ArrowUp") { e.preventDefault(); setActive((a) => (rows.length ? (a - 1 + rows.length) % rows.length : 0)); }
    else if (e.key === "Home" && e.target !== input.current) { e.preventDefault(); setActive(0); }
    else if (e.key === "Enter") { e.preventDefault(); go(rows[active]); }
    else if (e.key === "Tab") {
      const items = [...(dialog.current?.querySelectorAll<HTMLElement>("input, button") ?? [])];
      if (!items.length) return;
      const i = items.indexOf(document.activeElement as HTMLElement);
      e.preventDefault();
      items[(i + (e.shiftKey ? -1 : 1) + items.length) % items.length].focus();
    }
  };

  const loading = !index && !failed;
  const searching = query.trim().length > 0;
  const status = loading ? "Loading search" : searching ? `${results.length} result${results.length === 1 ? "" : "s"}` : "";

  return createPortal(
    <div className="fixed inset-0 z-[60] flex items-start justify-center px-3 pt-[8vh] sm:pt-[12vh]" onMouseDown={(e) => { if (e.target === e.currentTarget) onClose(); }}>
      <div className="absolute inset-0 -z-10 bg-night/75 backdrop-blur-[3px]" aria-hidden="true" />
      <div ref={dialog} role="dialog" aria-modal="true" aria-label="Search" onKeyDown={onKeyDown}
        className="w-full max-w-[660px] overflow-hidden rounded-lg border border-rule bg-paper shadow-[0_24px_80px_rgba(0,0,0,.45)]">
        <div className="flex items-center gap-3 border-b border-rule px-4">
          <SearchIcon className="h-[18px] w-[18px] shrink-0 text-muted" />
          <input ref={input} value={query} onChange={(e) => { setQuery(e.target.value); setActive(0); }} autoComplete="off" autoCorrect="off" spellCheck={false}
            placeholder="Search the docs" aria-label="Search the docs" role="combobox" aria-expanded={rows.length > 0} aria-controls={`${id}-list`}
            aria-activedescendant={rows[active] ? `${id}-${active}` : undefined} aria-autocomplete="list"
            className="search-input h-14 min-w-0 flex-1 bg-transparent text-[16px] placeholder:text-muted" />
          {query ? (
            <button type="button" onClick={() => { setQuery(""); input.current?.focus(); }} className="rounded px-1.5 py-1 text-xs text-muted hover:text-ink">Clear</button>
          ) : null}
          <button type="button" onClick={onClose} className="hidden rounded border border-rule px-1.5 py-0.5 font-mono text-[11px] text-muted hover:text-ink sm:block" aria-label="Close search">esc</button>
          <button type="button" onClick={onClose} className="rounded px-1.5 py-1 text-muted hover:text-ink sm:hidden" aria-label="Close search">✕</button>
        </div>

        <div className="max-h-[min(56vh,520px)] overflow-y-auto overscroll-contain p-2 thin-scroll">
          {failed ? (
            <p className="px-3 py-8 text-center text-sm text-muted">Search could not load. Reload the page, or <Link href="/docs/" onClick={onClose} className="text-chain underline underline-offset-4">browse the docs</Link>.</p>
          ) : loading ? (
            <p className="px-3 py-8 text-center text-sm text-muted">Loading…</p>
          ) : searching && rows.length === 0 ? (
            <div className="px-3 py-8 text-center text-sm">
              <p>No results for <span className="font-medium">&ldquo;{query}&rdquo;</span></p>
              <p className="mt-1 text-muted">Try a command, a setting such as <span className="font-mono text-[13px]">GHOSTLINE_POLICY_BUDGET</span>, or a word like &ldquo;refund&rdquo;.</p>
            </div>
          ) : (
            <>
              {!searching && <p className="eyebrow px-3 pb-1.5 pt-1">Suggested</p>}
              <ul id={`${id}-list`} role="listbox" aria-label={searching ? "Search results" : "Suggested pages"}>
                {rows.map((row, i) => (
                  <li key={row.href} id={`${id}-${i}`} role="option" aria-selected={i === active}>
                    <Link href={row.href} tabIndex={-1} onClick={(e) => { e.preventDefault(); go(row); }} onMouseMove={() => active !== i && setActive(i)}
                      className={`flex items-start gap-3 rounded-md px-3 py-2.5 transition-colors duration-[var(--tick)] ${i === active ? "bg-paper-2" : ""}`}>
                      <span className="mt-0.5 grid h-5 w-5 shrink-0 place-items-center text-muted" aria-hidden="true">{row.href.includes("#") ? "#" : "▤"}</span>
                      <span className="min-w-0 flex-1">
                        <span className="block truncate text-[11.5px] text-muted">{row.crumb}</span>
                        <span className="block truncate text-[14.5px] font-medium"><Marked parts={row.title} /></span>
                        {row.excerpt && <span className="mt-0.5 line-clamp-2 break-words text-[12.5px] leading-snug text-muted"><Marked parts={row.excerpt} /></span>}
                      </span>
                      <span className={`mt-1 hidden shrink-0 font-mono text-xs text-muted sm:block ${i === active ? "" : "invisible"}`} aria-hidden="true">↵</span>
                    </Link>
                  </li>
                ))}
              </ul>
            </>
          )}
        </div>

        <div className="flex items-center justify-between gap-3 border-t border-rule px-4 py-2 text-[11.5px] text-muted">
          <span className="hidden items-center gap-3 sm:flex" aria-hidden="true">
            <span><kbd className="font-mono">↑↓</kbd> navigate</span><span><kbd className="font-mono">↵</kbd> open</span><span><kbd className="font-mono">esc</kbd> close</span>
          </span>
          <span role="status" aria-live="polite">{status}</span>
        </div>
      </div>
    </div>,
    document.body,
  );
}
