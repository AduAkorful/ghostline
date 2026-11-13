"use client";
import { usePathname } from "next/navigation";
import { useEffect, useState } from "react";
import { DOCS } from "@/lib/docs-meta";
import { DocsNav } from "./DocsNav";

/** Below the lg breakpoint the sidebar becomes a menu under the header. */
export function DocsMobileNav() {
  const path = usePathname();
  const [open, setOpen] = useState(false);
  useEffect(() => setOpen(false), [path]);
  const current = DOCS.find((d) => path === `/docs/${d.slug}/`)?.label ?? "Overview";
  return (
    <div className="sticky top-16 z-30 border-b border-rule bg-paper/95 backdrop-blur lg:hidden">
      <button type="button" onClick={() => setOpen((o) => !o)} aria-expanded={open} aria-controls="docs-menu"
        className="flex w-full items-center justify-between px-5 py-3 text-left text-sm md:px-10">
        <span><span className="text-muted">Docs · </span><span className="font-medium">{current}</span></span>
        <span aria-hidden="true" className={`text-muted transition-transform duration-[var(--quick)] ${open ? "rotate-180" : ""}`}>⌄</span>
      </button>
      <div id="docs-menu" hidden={!open} className="max-h-[70vh] overflow-y-auto border-t border-rule px-3 py-4 thin-scroll">
        <DocsNav onNavigate={() => setOpen(false)} />
      </div>
    </div>
  );
}
