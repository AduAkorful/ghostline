"use client";
import { useEffect, useRef, useState } from "react";
import type { TocItem } from "@/lib/docs-meta";

/** "On this page" with scroll-spy: the heading currently under the header is highlighted. */
export function DocToc({ items }: { items: TocItem[] }) {
  const [active, setActive] = useState<string | null>(items[0]?.id ?? null);
  const list = useRef<HTMLUListElement>(null);
  useEffect(() => {
    const els = items.map((i) => document.getElementById(i.id)).filter((e): e is HTMLElement => !!e);
    let frame = 0;
    const update = () => {
      frame = 0;
      let current = els[0]?.id ?? null;
      for (const el of els) { if (el.getBoundingClientRect().top <= 110) current = el.id; else break; }
      if (innerHeight + scrollY >= document.documentElement.scrollHeight - 4 && els.length) current = els[els.length - 1].id;
      setActive(current);
    };
    const onScroll = () => { if (!frame) frame = requestAnimationFrame(update); };
    update();
    addEventListener("scroll", onScroll, { passive: true });
    addEventListener("resize", onScroll);
    return () => { removeEventListener("scroll", onScroll); removeEventListener("resize", onScroll); if (frame) cancelAnimationFrame(frame); };
  }, [items]);
  useEffect(() => {
    const el = list.current?.querySelector<HTMLElement>('[aria-current="location"]');
    const box = list.current?.parentElement;
    if (el && box) {
      const top = el.offsetTop - box.clientHeight / 2;
      box.scrollTo({ top: Math.max(0, top), behavior: matchMedia("(prefers-reduced-motion: reduce)").matches ? "auto" : "smooth" });
    }
  }, [active]);
  if (!items.length) return null;
  return (
    <nav aria-label="On this page" className="hidden xl:block">
      <div className="sticky top-24 max-h-[calc(100dvh-8rem)] overflow-y-auto pb-6 thin-scroll">
        <p className="eyebrow mb-3 flex items-center gap-2"><span aria-hidden="true">≡</span>On this page</p>
        <ul ref={list} className="relative space-y-0.5 border-l border-rule text-[13px] leading-snug">
          {items.map((t) => (
            <li key={t.id}>
              <a href={`#${t.id}`} aria-current={active === t.id ? "location" : undefined}
                className={`block py-1 transition-colors duration-[var(--tick)] ${t.depth === 3 ? "pl-7" : "pl-4"} ${active === t.id ? "font-medium text-ink" : "text-muted hover:text-ink"}`}>{t.label}</a>
            </li>
          ))}
        </ul>
      </div>
    </nav>
  );
}
