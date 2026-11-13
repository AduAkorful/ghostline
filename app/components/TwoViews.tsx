"use client";
// Two views: the signature moment and the simulator (plans 45/46).
// - Server render = the finished mainnet run 5 (5 paid, 1 held, settled), so no-JS and reduced motion show the end state.
// - On view it replays that run. The full size then hands control to the visitor: pick a price, pay, settle, start over.
// - Simulation only: no wallet, no network. Hashes shown are run 5's real records, labelled as such.
import { useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";
import gsap from "gsap";
import { Hash } from "./Hash";
import { sim, txUrl } from "@/lib/data";
import { microTari, num } from "@/lib/format";

import { EMPTY, pay, type State } from "@/lib/simulator";
import { settlementFee } from "@ghostline/sdk/fee";

const PRICES = [100, 500, 1000];
const run = Array.from({ length: sim.attempts }).reduce<State>(
  (state) => pay(state, sim.price, sim.budget), EMPTY,
);
const FINISHED: State = { ...run, settled: true };

const reduced = () => typeof window !== "undefined" && matchMedia("(prefers-reduced-motion: reduce)").matches;

export function TwoViews({ size = "compact" }: { size?: "compact" | "full" }) {
  const full = size === "full";
  const root = useRef<HTMLDivElement>(null);
  const [state, setState] = useState<State>(FINISHED);
  const [mode, setMode] = useState<"replay" | "visitor">("replay");
  const [price, setPrice] = useState(sim.price);
  const [tryRequested, setTryRequested] = useState(false);
  const startOver = useRef<HTMLButtonElement>(null);
  const payButton = useRef<HTMLButtonElement>(null);
  useEffect(() => { if (tryRequested) payButton.current?.focus(); }, [tryRequested]);
  const timers = useRef<number[]>([]);
  const lastCount = useRef(FINISHED.calls.length);
  const wasSettled = useRef(true);

  const accepted = state.calls.filter((c) => c.status === "paid").reduce((s, c) => s + c.price, 0);
  const paidCount = state.calls.filter((c) => c.status === "paid").length;
  const heldCount = state.calls.length - paidCount;
  const protocolFee = settlementFee(BigInt(accepted));

  const clear = () => { timers.current.forEach(clearTimeout); timers.current = []; };

  // Replays mainnet run 5 at reading pace.
  const replay = useCallback(() => {
    clear();
    setMode("replay");
    if (reduced()) { setState(FINISHED); if (full) setMode("visitor"); return; }
    setState(EMPTY);
    run.calls.forEach((_, i) => timers.current.push(window.setTimeout(() => setState((s) => pay(s, sim.price, sim.budget)), 500 + i * 650)));
    timers.current.push(window.setTimeout(() => setState((s) => ({ ...s, settled: true })), 500 + sim.attempts * 650 + 400));
    if (full) timers.current.push(window.setTimeout(() => setMode("visitor"), 500 + sim.attempts * 650 + 1400));
  }, [full]);

  useEffect(() => {
    const el = root.current;
    if (!el) return;
    const io = new IntersectionObserver(([e]) => { if (e.isIntersecting) { replay(); io.disconnect(); } }, { threshold: 0.35 });
    io.observe(el);
    return () => { io.disconnect(); clear(); };
  }, [replay]);

  // Animate only what changed: a new ledger row, or the settle record arriving.
  useLayoutEffect(() => {
    const el = root.current;
    if (!el || reduced()) { lastCount.current = state.calls.length; wasSettled.current = state.settled; return; }
    const ctx = gsap.context(() => {
      if (state.calls.length > lastCount.current) {
        const row = el.querySelector<HTMLElement>(`[data-row="${state.calls.length}"]`);
        if (row) {
          gsap.fromTo(row, { opacity: 0, y: 4 }, { opacity: 1, y: 0, duration: 0.25, ease: "power2.out" });
          gsap.fromTo(row.querySelector("[data-ln]"), { scaleX: 0 }, { scaleX: 1, duration: 0.25, ease: "power2.out" });
        }
      }
      if (state.settled && !wasSettled.current) {
        // The product's one overshoot and one glow.
        gsap.fromTo("[data-settle]", { opacity: 0, scaleX: 0.15 }, { opacity: 1, scaleX: 1, duration: 0.65, ease: "back.out(1.6)" });
        gsap.fromTo("[data-settle]", { boxShadow: "0 0 0 1px var(--chain), 0 0 64px var(--glow)" },
          { boxShadow: "0 0 0 1px var(--chain), 0 0 32px var(--glow)", duration: 1.2, delay: 0.4 });
        gsap.fromTo("[data-ln]", { opacity: 1 }, { opacity: 0.35, duration: 0.4, stagger: 0.04 });
      }
    }, el);
    lastCount.current = state.calls.length;
    wasSettled.current = state.settled;
    void ctx; // tweens finish on their own; reverting here would snap rows back mid-animation
  }, [state]);

  const visitor = mode === "visitor";
  const statusText = state.calls.length === 0 ? "channel open · 0 calls" : `${paidCount} paid${heldCount ? ` · ${heldCount} refused` : ""}`;

  return (
    <div ref={root} className="overflow-hidden rounded-[3px] border border-rule bg-paper">
      <div className="grid md:grid-cols-2">
        {/* Your view: private, off-chain, lilac */}
        <section aria-label="Your view" className="p-4 md:p-5">
          <header className="flex items-baseline justify-between border-b border-rule pb-2.5">
            <h2 className="voice text-xl italic">Your view</h2>
            <span className="eyebrow" aria-live={visitor && tryRequested ? "polite" : "off"} aria-atomic="true">{statusText}</span>
          </header>
          <ol className={`mt-2 font-mono text-[12.5px] ${full ? "min-h-[220px]" : "min-h-[200px]"}`}>
            {state.calls.length === 0 && <li className="py-6 text-center text-muted">No calls yet. {visitor ? "Pay for one below." : ""}</li>}
            {state.calls.map((c) => c.status === "paid" ? (
              <li data-row={c.n} key={c.n} className="grid grid-cols-[22px_1fr_auto] items-center gap-2 border-b border-rule/70 py-[7px]">
                <span data-ln className="h-px origin-left bg-ghost" />
                <span>#{c.n} {sim.path} · {microTari(c.price)}</span>
                <span className="figures text-muted">{num(c.total)}</span>
              </li>
            ) : (
              <li data-row={c.n} key={c.n} className="grid grid-cols-[22px_1fr_auto] items-center gap-2 border-b border-rule/70 py-[7px] text-held">
                <span data-ln className="h-px origin-left bg-held" />
                <span className="line-through decoration-1">#{c.n} {sim.path} · over {num(sim.budget)} µT budget</span>
                <span>held</span>
              </li>
            ))}
          </ol>
          {full && (
            <dl className="mt-4 grid grid-cols-2 gap-y-1 text-[13px]">
              <dt className="text-muted">Accepted total</dt><dd className="text-right font-mono">{microTari(accepted)}</dd>
              <dt className="text-muted">Policy budget</dt><dd className="text-right font-mono">{microTari(sim.budget)}</dd>
              <dt className="text-muted">Protocol fee at settle (1%)</dt><dd className="text-right font-mono">{microTari(protocolFee)}</dd>
            </dl>
          )}
          <p className="mt-3 text-[11px] text-muted">Simulation · {visitor ? "your calls, no funds move" : "replaying mainnet run 5"}</p>
        </section>

        {/* The chain's view: public, cyan where checkable */}
        <section aria-label="The chain's view" className="border-t border-dashed border-rule p-4 md:border-l md:border-t-0 md:p-5">
          <header className="flex items-baseline justify-between border-b border-rule pb-2.5">
            <h2 className="voice text-xl italic">The chain&rsquo;s view</h2>
            <span className="eyebrow">public</span>
          </header>
          <div className="mt-3 rounded-[3px] border border-rule px-3 py-2.5 font-mono text-[12.5px]">
            <div className="flex items-center justify-between gap-2"><span>open</span>{visitor ? <span className="text-muted">simulated</span> : <Hash value={sim.openHash} href={txUrl(sim.openHash)} />}</div>
            <div className="mt-1 flex justify-between text-muted"><span>amount</span><span><span className="sr-only">Hidden</span><span aria-hidden="true">▓▓▓▓▓▓</span></span></div>
            <div className="flex justify-between text-muted"><span>fee</span><span>{microTari(sim.openFee)}</span></div>
          </div>
          <p className={`my-4 text-center text-[12px] text-muted transition-opacity duration-300 ${state.settled ? "opacity-35" : ""}`}>
            {state.calls.length > 0 && !state.settled
              ? `${state.calls.length} call${state.calls.length > 1 ? "s" : ""} made. Still nothing new on chain.`
              : full ? "No records while your agent makes calls. Vouchers never touch the chain." : "No records while your agent makes calls."}
          </p>
          {state.settled ? (
            <div data-settle className="settle-glow origin-left rounded-[3px] px-3 py-2.5 font-mono text-[12.5px] text-chain">
              <div className="flex items-center justify-between gap-2"><span>settle</span>{visitor ? <span className="opacity-80">simulated</span> : <Hash value={sim.settleHash} href={txUrl(sim.settleHash)} />}</div>
              <div className="mt-1 flex justify-between opacity-80"><span>amounts</span><span><span className="sr-only">Hidden</span><span aria-hidden="true">▓▓▓▓▓▓</span></span></div>
              <div className="flex justify-between opacity-80"><span>fee</span><span>{microTari(sim.settleFee)}</span></div>
            </div>
          ) : (
            <div className="rounded-[3px] border border-dashed border-rule px-3 py-[22px] text-center font-mono text-[12px] text-muted">settle · not yet</div>
          )}

          <span className="sr-only" role="status">{visitor && state.settled ? "Simulated channel settled." : ""}</span>
          {visitor ? (
            <div className="mt-4 space-y-3 border-t border-rule pt-4">
              <fieldset className="flex flex-wrap items-center gap-2">
                <legend className="eyebrow mb-2 w-full">Price per call</legend>
                {PRICES.map((p) => (
                  <button key={p} onClick={() => setPrice(p)} aria-pressed={price === p} disabled={state.settled}
                    className={`rounded-sm border px-2.5 py-1 font-mono text-[12px] disabled:opacity-40 ${price === p ? "border-ghost text-ghost" : "border-rule text-muted hover:text-ink"}`}>{num(p)} µT</button>
                ))}
              </fieldset>
              <div className="flex flex-wrap gap-2">
                <button ref={payButton} onClick={() => setState((s) => pay(s, price, sim.budget))} disabled={state.settled}
                  className="rounded-sm bg-ink px-3.5 py-2 text-sm font-semibold text-on-ink disabled:opacity-40">Pay for a call</button>
                <button onClick={() => { setState((s) => ({ ...s, settled: true })); startOver.current?.focus(); }} disabled={state.settled || paidCount === 0}
                  className="rounded-sm border border-chain px-3.5 py-2 text-sm text-chain disabled:opacity-40">Settle</button>
                <button ref={startOver} onClick={() => { clear(); setState(EMPTY); }} className="px-2 py-2 text-sm text-muted underline underline-offset-4 hover:text-ink">Start over</button>
              </div>
              <p className="text-[12px] text-muted">The policy refuses any call that would take the total past {microTari(sim.budget)}. Network fees shown are the measured mainnet fees.</p>
            </div>
          ) : (
            <div className="mt-3 flex items-center justify-end gap-4">
              <button onClick={replay} className="text-[12px] text-muted underline underline-offset-4 hover:text-ink">Replay</button>
              {full && (
                <button onClick={() => { clear(); setState(EMPTY); setMode("visitor"); setTryRequested(true); }}
                  className="rounded-sm border border-ghost px-3 py-1.5 text-[13px] text-ghost">Try it yourself →</button>
              )}
            </div>
          )}
        </section>
      </div>
    </div>
  );
}
