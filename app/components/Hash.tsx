"use client";
import { useState } from "react";
import { shortHash } from "@/lib/format";

/** A shortened, copyable, linked hash. Cyan because it points at something checkable on chain. */
export function Hash({ value, href, className = "" }: { value: string; href?: string; className?: string }) {
  const [copied, setCopied] = useState(false);
  const copy = async () => {
    try { await navigator.clipboard.writeText(value); setCopied(true); setTimeout(() => setCopied(false), 1400); } catch {}
  };
  return (
    <span className={`inline-flex items-center gap-1.5 font-mono text-[13px] ${className}`}>
      {href ? (
        <a href={href} target="_blank" rel="noreferrer" title={value} className="text-chain underline decoration-chain/40 underline-offset-4 hover:decoration-chain">
          {shortHash(value)}<span className="sr-only"> (opens the indexer record)</span>
        </a>
      ) : <span title={value}>{shortHash(value)}</span>}
      <button onClick={copy} className="rounded-sm px-1 text-muted hover:text-ink" aria-label={`Copy ${value}`}>{copied ? "✓" : "⧉"}</button>
      <span aria-live="polite" className="sr-only">{copied ? "Copied" : ""}</span>
    </span>
  );
}
