"use client";
import { useState } from "react";

// Quiet JSON colouring: keys read first, values second, punctuation last. No colour carries meaning here.
function Json({ code }: { code: string }) {
  const parts = code.split(/("(?:[^"\\]|\\.)*"\s*:|"(?:[^"\\]|\\.)*"|[{}[\],])/g).filter((p) => p !== "");
  return (
    <>
      {parts.map((p, i) =>
        /^"[\s\S]*"\s*:$/.test(p) ? <span key={i} className="text-ink">{p}</span>
        : /^"/.test(p) ? <span key={i} className="text-ink/75">{p}</span>
        : /^[{}[\],]$/.test(p) ? <span key={i} className="text-muted/70">{p}</span>
        : <span key={i}>{p}</span>)}
    </>
  );
}

/** A copyable code block. Sizes to its content (never stretches to a taller neighbour) and scrolls sideways only if a line is wider than the box. */
export function CodeBlock({ code, lang = "bash", title }: { code: string; lang?: string; title?: string }) {
  const [state, setState] = useState<"idle" | "copied" | "failed">("idle");
  const label = title ?? lang;
  const copy = async () => {
    try { await navigator.clipboard.writeText(code); setState("copied"); } catch { setState("failed"); }
    setTimeout(() => setState("idle"), 1600);
  };
  return (
    <figure className="w-full min-w-0 self-start overflow-hidden rounded-lg border border-rule bg-night">
      <figcaption className="flex items-center justify-between gap-3 border-b border-rule bg-paper px-4 py-2">
        <span className="truncate font-mono text-[12px] text-muted">{label}</span>
        <button type="button" onClick={copy} aria-label={`Copy ${label}`}
          className="shrink-0 rounded-md border border-rule px-2.5 py-1 text-[12px] text-muted transition-colors duration-[var(--tick)] hover:bg-paper-2 hover:text-ink">
          {state === "copied" ? "Copied ✓" : state === "failed" ? "Copy failed" : "Copy"}
        </button>
      </figcaption>
      <pre tabIndex={0} className="thin-scroll overflow-x-auto px-4 py-4 font-mono text-[13px] leading-[1.7]"><code>{lang === "json" ? <Json code={code} /> : code}</code></pre>
      <span className="sr-only" role="status">{state === "copied" ? "Copied." : state === "failed" ? "Copy failed." : ""}</span>
    </figure>
  );
}
