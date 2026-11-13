"use client";
import { useEffect, useId, useRef } from "react";

export function DocEnhancer({ html, diagram }: { html: string; diagram: string | null }) {
  const ref = useRef<HTMLDivElement>(null);
  const diagramId = useId().replace(/[^a-zA-Z0-9_-]/g, "");
  useEffect(() => {
    const host = ref.current;
    host?.querySelectorAll<HTMLPreElement>("pre[data-lang]").forEach((pre) => {
      if (pre.dataset.enhanced) return;
      pre.dataset.enhanced = "1";
      const bar = document.createElement("div");
      bar.className = "code-bar";
      const lang = document.createElement("span");
      lang.textContent = pre.dataset.lang ?? "";
      lang.className = "eyebrow";
      const button = document.createElement("button");
      button.textContent = "copy";
      button.type = "button";
      button.setAttribute("aria-label", "Copy code");
      const status = document.createElement("span");
      status.className = "sr-only";
      status.setAttribute("role", "status");
      button.onclick = async () => {
        try {
          await navigator.clipboard.writeText(pre.querySelector("code")?.textContent ?? "");
          button.textContent = "copied ✓";
          status.textContent = "Code copied.";
          setTimeout(() => { button.textContent = "copy"; status.textContent = ""; }, 1400);
        } catch {
          status.textContent = "Copy failed. Select the code to copy it.";
        }
      };
      bar.append(lang, button, status);
      pre.before(bar);
    });
    const copyHash = async (event: MouseEvent) => {
      if (!(event.target instanceof HTMLButtonElement) || !event.target.dataset.copyHash) return;
      const button = event.target;
      const hash = button.dataset.copyHash!;
      try {
        await navigator.clipboard.writeText(hash);
        button.textContent = "✓";
        button.setAttribute("aria-label", `Copied ${hash}`);
        setTimeout(() => {
          button.textContent = "⧉";
          button.setAttribute("aria-label", `Copy ${hash}`);
        }, 1400);
      } catch {
        button.setAttribute("aria-label", `Copy failed for ${hash}`);
      }
    };
    host?.addEventListener("click", copyHash);
    const target = host?.querySelector<HTMLElement>("[data-mermaid]");
    const cleanup = () => host?.removeEventListener("click", copyHash);
    if (!target || !diagram) return cleanup;
    let cancelled = false;
    let renders = 0;
    // Mermaid needs literal colours, so read the site's tokens from CSS.
    const draw = async () => {
      const { default: mermaid } = await import("mermaid");
      if (cancelled) return;
      const css = getComputedStyle(document.documentElement);
      const c = (name: string) => css.getPropertyValue(name).trim();
      const font = getComputedStyle(document.body).fontFamily;
      mermaid.initialize({
        startOnLoad: false, securityLevel: "strict", theme: "base", fontFamily: font,
        themeVariables: {
          background: "transparent", fontFamily: font, fontSize: "14px",
          primaryColor: c("--paper-2"), primaryTextColor: c("--ink"), primaryBorderColor: c("--rule"), lineColor: c("--muted"),
          actorBkg: c("--paper-2"), actorBorder: c("--muted"), actorTextColor: c("--ink"), actorLineColor: c("--rule"),
          signalColor: c("--ink"), signalTextColor: c("--ink"),
          noteBkgColor: c("--paper-2"), noteTextColor: c("--ink"), noteBorderColor: c("--ghost"),
          labelBoxBkgColor: c("--paper-2"), labelBoxBorderColor: c("--rule"), labelTextColor: c("--ink"),
          loopTextColor: c("--ink"), activationBkgColor: c("--paper-2"), activationBorderColor: c("--muted"), sequenceNumberColor: c("--on-ink"),
        },
        // Wrapped labels and narrow actors keep the diagram about as wide as the article, so it stays readable without scrolling.
        sequence: { useMaxWidth: true, wrap: true, width: 128, actorMargin: 22, messageMargin: 34, boxMargin: 8, noteMargin: 8, mirrorActors: false, showSequenceNumbers: false },
      });
      const { svg } = await mermaid.render(`diagram-${diagramId}-${++renders}`, diagram);
      if (!cancelled) target.innerHTML = svg;
    };
    const redraw = () => { void draw().catch(() => { if (!cancelled) target.textContent = "The diagram could not load. Read the payment sequence below."; }); };
    redraw();
    return () => { cancelled = true; cleanup(); };
  }, [html, diagram, diagramId]);
  return <div ref={ref} className="prose-doc mt-6" dangerouslySetInnerHTML={{ __html: html }} />;
}
