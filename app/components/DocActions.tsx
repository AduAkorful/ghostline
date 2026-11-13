"use client";
import { useEffect, useRef, useState } from "react";
import { SITE_ORIGIN } from "@/lib/docs-meta";

const item = "block w-full rounded-md px-3 py-2 text-left text-[13.5px] transition-colors duration-[var(--tick)] hover:bg-paper-2";

/** "Copy page" with a menu: the page as Markdown for an agent, or opened in a chat assistant. */
export function DocActions({ slug }: { slug: string }) {
  const [open, setOpen] = useState(false);
  const [state, setState] = useState<"idle" | "copied" | "failed">("idle");
  const box = useRef<HTMLDivElement>(null);
  const md = `/docs/${slug}.md`;
  const prompt = encodeURIComponent(`Read ${SITE_ORIGIN}${md} and answer my questions about it.`);

  useEffect(() => {
    if (!open) return;
    const onDown = (e: MouseEvent) => { if (!box.current?.contains(e.target as Node)) setOpen(false); };
    const onKey = (e: KeyboardEvent) => { if (e.key === "Escape") setOpen(false); };
    addEventListener("mousedown", onDown);
    addEventListener("keydown", onKey);
    return () => { removeEventListener("mousedown", onDown); removeEventListener("keydown", onKey); };
  }, [open]);

  const copy = async () => {
    try {
      const res = await fetch(md);
      if (!res.ok) throw new Error(String(res.status));
      await navigator.clipboard.writeText(await res.text());
      setState("copied");
    } catch { setState("failed"); }
    setTimeout(() => setState("idle"), 1800);
  };

  return (
    <div ref={box} className="relative shrink-0">
      <div className="inline-flex rounded-md border border-rule text-[13px]">
        <button type="button" onClick={copy} className="rounded-l-md px-3 py-1.5 transition-colors duration-[var(--tick)] hover:bg-paper-2">
          {state === "copied" ? "Copied ✓" : state === "failed" ? "Copy failed" : "Copy page"}
        </button>
        <button type="button" onClick={() => setOpen((o) => !o)} aria-expanded={open} aria-controls="doc-actions-menu" aria-label="More ways to use this page"
          className="rounded-r-md border-l border-rule px-2 text-muted transition-colors duration-[var(--tick)] hover:bg-paper-2 hover:text-ink">
          <span aria-hidden="true" className={`inline-block transition-transform duration-[var(--quick)] ${open ? "rotate-180" : ""}`}>⌄</span>
        </button>
      </div>
      <span className="sr-only" role="status">{state === "copied" ? "Page copied as Markdown." : state === "failed" ? "Copy failed." : ""}</span>
      <div id="doc-actions-menu" hidden={!open} className="absolute right-0 top-full z-20 mt-1.5 w-60 rounded-lg border border-rule bg-paper p-1.5 shadow-[0_12px_40px_rgba(0,0,0,.35)]">
        <a className={item} href={md} target="_blank" rel="noreferrer">View as Markdown<span className="sr-only"> (opens in a new tab)</span></a>
        <a className={item} href={`https://claude.ai/new?q=${prompt}`} target="_blank" rel="noreferrer">Open in Claude<span className="sr-only"> (opens in a new tab)</span></a>
        <a className={item} href={`https://chatgpt.com/?q=${prompt}`} target="_blank" rel="noreferrer">Open in ChatGPT<span className="sr-only"> (opens in a new tab)</span></a>
        <div className="my-1 border-t border-rule" role="separator" />
        <a className={item} href="/llms.txt" target="_blank" rel="noreferrer">llms.txt (docs index)</a>
        <a className={item} href="/skill.md" target="_blank" rel="noreferrer">Ghostline skill (SKILL.md)</a>
      </div>
    </div>
  );
}
