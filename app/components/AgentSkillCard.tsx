"use client";
import { useState } from "react";

/** Copy, download or view the Ghostline skill. The file is served from /skill.md and generated from skills/ghostline/SKILL.md. */
export function AgentSkillCard() {
  const [state, setState] = useState<"idle" | "copied" | "failed">("idle");
  const copy = async () => {
    try {
      const res = await fetch("/skill.md");
      if (!res.ok) throw new Error(String(res.status));
      await navigator.clipboard.writeText(await res.text());
      setState("copied");
    } catch { setState("failed"); }
    setTimeout(() => setState("idle"), 1800);
  };
  const btn = "rounded-md border border-rule px-3.5 py-2 text-sm transition-colors duration-[var(--tick)] hover:bg-paper-2";
  return (
    <section aria-labelledby="skill-card" className="mt-8 rounded-lg border border-rule bg-night p-5">
      <p className="eyebrow">Agent skill</p>
      <h2 id="skill-card" className="mt-1 font-mono text-base font-semibold">SKILL.md</h2>
      <p className="mt-1.5 max-w-prose text-sm text-muted">Teaches an agent what Ghostline is, which MCP tools exist, the money rules, and the safety rules: quote before submitting, no secrets in chat, never change the policy to get a payment through.</p>
      <div className="mt-4 flex flex-wrap gap-2.5">
        <button type="button" onClick={copy} className={`${btn} bg-ink font-semibold text-on-ink hover:bg-ink/90`}>
          {state === "copied" ? "Copied ✓" : state === "failed" ? "Copy failed" : "Copy skill"}
        </button>
        <a href="/skill.md" download="SKILL.md" className={btn}>Download SKILL.md</a>
        <a href="/skill.md" target="_blank" rel="noreferrer" className={btn}>View<span className="sr-only"> (opens in a new tab)</span></a>
      </div>
      <span className="sr-only" role="status">{state === "copied" ? "Skill copied." : state === "failed" ? "Copy failed." : ""}</span>
    </section>
  );
}
