"use client";
import Link from "next/link";
import { usePathname } from "next/navigation";
import { useEffect, useRef, useState } from "react";
import { DOCS, GROUPS } from "@/lib/docs-meta";

const item = (active: boolean) =>
  `block rounded-md px-3 py-[7px] text-[14px] leading-snug transition-colors duration-[var(--tick)] ${
    active ? "bg-paper-2 font-medium text-ink" : "text-muted hover:bg-paper-2/60 hover:text-ink"}`;

/** The docs sidebar. It lives in the docs layout, so it keeps its scroll position between pages. */
export function DocsNav({ onNavigate }: { onNavigate?: () => void }) {
  const path = usePathname();
  const root = useRef<HTMLElement>(null);
  const activeGroup = DOCS.find((d) => path === `/docs/${d.slug}/`)?.group;
  const [open, setOpen] = useState<Set<string>>(() => new Set(activeGroup ? [activeGroup] : [GROUPS[0]]));
  // Landing on a page always opens its section; sections the reader opened by hand stay open.
  useEffect(() => { if (activeGroup) setOpen((o) => (o.has(activeGroup) ? o : new Set(o).add(activeGroup))); }, [activeGroup]);
  const toggle = (g: string) => setOpen((o) => { const n = new Set(o); if (n.has(g)) n.delete(g); else n.add(g); return n; });
  useEffect(() => {
    root.current?.querySelector<HTMLElement>('[aria-current="page"]')?.scrollIntoView({ block: "nearest" });
  }, [path]);
  return (
    <nav ref={root} aria-label="Docs">
      <Link href="/docs/" onClick={onNavigate} aria-current={path === "/docs/" ? "page" : undefined} className={item(path === "/docs/")}>Overview</Link>
      {GROUPS.map((g) => {
        const expanded = open.has(g);
        return (
          <div key={g} className="mt-1">
            <button type="button" onClick={() => toggle(g)} aria-expanded={expanded} aria-controls={`nav-${g.replace(/\s+/g, "-")}`}
              className="flex w-full items-center justify-between rounded-md px-3 py-[7px] text-left text-[14px] font-medium transition-colors duration-[var(--tick)] hover:bg-paper-2/60">
              {g}
              <span aria-hidden="true" className={`text-muted transition-transform duration-[var(--quick)] ${expanded ? "rotate-90" : ""}`}>›</span>
            </button>
            <ul id={`nav-${g.replace(/\s+/g, "-")}`} hidden={!expanded} className="ml-3 border-l border-rule pl-2">
              {DOCS.filter((d) => d.group === g).map((d) => {
                const href = `/docs/${d.slug}/`;
                return (
                  <li key={d.slug}>
                    <Link href={href} onClick={onNavigate} aria-current={path === href ? "page" : undefined} className={item(path === href)}>{d.label}</Link>
                  </li>
                );
              })}
            </ul>
          </div>
        );
      })}
    </nav>
  );
}
