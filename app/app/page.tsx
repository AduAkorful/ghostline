import Link from "next/link";
import { CodeBlock } from "@/components/CodeBlock";
import { TwoViews } from "@/components/TwoViews";
import { Hash } from "@/components/Hash";
import { fees, proof, RECORDED_ON, txUrl } from "@/lib/data";
import { microTari, num } from "@/lib/format";

const STEPS = [
  { n: "01", title: "Lock once", body: "Your agent opens a private channel with a deposit. One record on chain; the amount stays hidden." },
  { n: "02", title: "Sign per call", body: "Each paid API call carries a signed, cumulative voucher. Off-chain, with no block to wait for, and checked against your policy first." },
  { n: "03", title: "Settle once", body: "The service settles the latest voucher in one transaction. Settlement returns the remainder to the payer. If the channel is still open at expiry, the payer can refund it." },
];

const MCP_SNIPPET = `{
  "mcpServers": {
    "ghostline": {
      "command": "ghostline-mcp",
      "env": {
        "GHOSTLINE_CHANNEL_FILE": "/home/you/ghostline/channel.json",
        "GHOSTLINE_WALLET_COOKIE": "/home/you/wallet/.cookie",
        "GHOSTLINE_POLICY_BUDGET": "5000",
        "GHOSTLINE_POLICY_MAX_PER_CALL": "1000"
      }
    }
  }
}`;

export default function Landing() {
  const latest = proof.filter((p) => p.run === "Run 5");
  const networkPerChannel = fees.open + fees.settle;
  const breakEvenCalls = Math.ceil(networkPerChannel / fees.priceStep);

  return (
    <>
      {/* Hero */}
      <section className="ledger-bg">
        <div className="mx-auto max-w-[1200px] px-4 pb-16 pt-14 md:px-6 md:pt-20">
          <p className="eyebrow">Live on Tari Ootle mainnet · since {RECORDED_ON}</p>
          <h1 className="voice mt-4 max-w-4xl text-[44px] leading-[.95] sm:text-6xl md:text-[84px]">
            Every call, <em className="text-ghost">on your ledger.</em><br />Two records on the chain.
          </h1>
          <p className="mt-6 max-w-xl text-[17px] leading-relaxed text-muted">
            Private pay-per-call channels for AI agents. Your agent locks funds once, pays each API call with a signed voucher,
            and the service settles once. A spending policy outside the agent keeps it honest.
          </p>
          <div className="mt-8 flex flex-wrap gap-3">
            <Link href="/agents/" className="rounded-sm bg-ink px-5 py-3 font-semibold text-on-ink">Set up your agent →</Link>
            <Link href="/proof/" className="rounded-sm border border-rule px-5 py-3 font-mono text-sm hover:border-chain">check the chain ↗</Link>
          </div>
          <div id="privacy" className="mt-14"><TwoViews /></div>
        </div>
      </section>

      {/* How it works */}
      <section id="story" className="border-t border-rule">
        <div className="mx-auto max-w-[1200px] px-4 py-20 md:px-6">
          <p className="eyebrow">How it works</p>
          <h2 className="voice mt-3 text-4xl md:text-5xl">Lock once. Sign per call. Settle once.</h2>
          <ol className="mt-10 grid border-t border-rule md:grid-cols-3">
            {STEPS.map((s, i) => (
              <li key={s.n} className={`py-6 md:px-6 ${i > 0 ? "border-t border-rule md:border-l md:border-t-0" : "md:pl-0"}`}>
                <span className="font-mono text-sm text-ghost">{s.n}</span>
                <h3 className="mt-2 text-xl font-semibold">{s.title}</h3>
                <p className="mt-2 text-muted">{s.body}</p>
              </li>
            ))}
          </ol>
          <Link href="/how-it-works/" className="mt-4 inline-block text-sm text-chain underline underline-offset-4">The full channel lifecycle →</Link>
        </div>
      </section>

      {/* For agents: MCP + policy */}
      <section className="border-t border-rule bg-paper">
        <div className="mx-auto grid max-w-[1200px] gap-10 px-4 py-20 md:grid-cols-[1fr_1.2fr] md:px-6">
          <div>
            <p className="eyebrow">For agent builders</p>
            <h2 className="voice mt-3 text-4xl md:text-5xl">Give your agent a wallet <em>with a leash.</em></h2>
            <p className="mt-4 text-muted">Ghostline ships an MCP server. Your agent gets <code className="font-mono text-ink">ghostline_call_paid</code>; you keep the budget, the per-call cap, the vendor allowlist and the approval threshold. The policy runs outside the agent, so a prompt can&rsquo;t talk it out of a limit.</p>
            <ul className="mt-6 space-y-2 text-[15px]">
              <li className="flex gap-3"><span className="text-ghost">—</span>Works with Claude Desktop, Claude Code and any MCP host</li>
              <li className="flex gap-3"><span className="text-ghost">—</span>Asks you before a new vendor or a large spend</li>
              <li className="flex gap-3"><span className="text-held">—</span>On mainnet it refused call 6: <span className="font-mono text-[13px] text-held">cumulative 6000 exceeds policy budget 5000</span></li>
            </ul>
            <Link href="/agents/" className="mt-8 inline-block rounded-sm bg-ink px-5 py-3 font-semibold text-on-ink">MCP setup guide →</Link>
          </div>
          <CodeBlock code={MCP_SNIPPET} lang="json" title="claude_desktop_config.json" />
        </div>
      </section>

      {/* For sellers */}
      <section className="border-t border-rule">
        <div className="mx-auto grid max-w-[1200px] gap-10 px-4 py-20 md:grid-cols-[1fr_1.1fr] md:px-6">
          <div>
            <p className="eyebrow">For API sellers</p>
            <h2 className="voice mt-3 text-4xl md:text-5xl">Charge per call. <em>Get paid in one line.</em></h2>
            <p className="mt-4 text-muted">Put the Ghostline paywall in front of your API. It answers HTTP 402 with a quote, checks each voucher, and you settle when you choose. The payout lands in your own wallet, where your wallet finds it like any other payment.</p>
            <Link href="/sellers/" className="mt-6 inline-block text-chain underline underline-offset-4">Paywall setup →</Link>
          </div>
          <dl className="grid grid-cols-2 border-t border-rule text-[15px]">
            {[
              ["Pricing", "Fixed, per request, or metered"],
              ["Price step", `${fees.priceStep} µT`],
              ["Protocol fee", `${fees.protocolPercent}% of the payout`],
              ["Settles", "Whenever you choose, before expiry"],
            ].map(([k, v]) => (
              <div key={k} className="border-b border-rule py-4 pr-4"><dt className="eyebrow">{k}</dt><dd className="mt-1">{v}</dd></div>
            ))}
          </dl>
        </div>
      </section>

      {/* Proof strip */}
      <section id="evidence" className="border-t border-rule bg-paper">
        <div className="mx-auto max-w-[1200px] px-4 py-20 md:px-6">
          <div className="flex flex-wrap items-end justify-between gap-4">
            <div>
              <p className="eyebrow">Don&rsquo;t trust the page</p>
              <h2 className="voice mt-3 text-4xl md:text-5xl">Check the chain.</h2>
            </div>
            <Link href="/proof/" className="text-sm text-chain underline underline-offset-4">All {proof.length} mainnet records →</Link>
          </div>
          <ul className="mt-8 border-t border-rule">
            {latest.map((p) => (
              <li key={p.hash} className="grid gap-1 border-b border-rule py-4 sm:grid-cols-[110px_1fr_auto_auto] sm:items-center sm:gap-6">
                <span className="font-mono text-xs text-muted">{p.run}</span>
                <span>{p.label} <span className="text-muted">· {p.note}</span></span>
                <Hash value={p.hash} href={txUrl(p.hash)} />
                <span className="font-mono text-xs text-chain">{p.status}</span>
              </li>
            ))}
          </ul>
        </div>
      </section>

      {/* Honest fees */}
      <section className="border-t border-rule">
        <div className="mx-auto grid max-w-[1200px] gap-10 px-4 py-20 md:grid-cols-2 md:px-6">
          <div>
            <p className="eyebrow">Fees, plainly</p>
            <h2 className="voice mt-3 text-4xl md:text-5xl">Channels pay off <em>over many calls.</em></h2>
            <p className="mt-4 text-muted">Opening and settling a channel costs network fees, measured on mainnet on {RECORDED_ON}. At the smallest price step that equals about {num(breakEvenCalls)} calls, so Ghostline suits agents that call an API often, not once.</p>
            <Link href="/fees/" className="mt-6 inline-block text-chain underline underline-offset-4">Fee calculator →</Link>
          </div>
          <table className="w-full self-start font-mono text-[14px]">
            <tbody>
              {[["Open (once)", fees.open], ["Settle (once)", fees.settle], ["Each paid call", 0]].map(([k, v]) => (
                <tr key={k as string} className="border-b border-rule"><td className="py-3 font-sans">{k}</td><td className="py-3 text-right">{v === 0 ? "0 µT on chain" : microTari(v as number)}</td></tr>
              ))}
              <tr className="border-b border-rule"><td className="py-3 font-sans">Protocol fee</td><td className="py-3 text-right">1% of payout</td></tr>
              <tr><td className="py-3 font-sans text-muted">Break-even at {fees.priceStep} µT per call</td><td className="py-3 text-right text-ghost">≈ {num(breakEvenCalls)} calls</td></tr>
            </tbody>
          </table>
        </div>
      </section>

      {/* Desktop teaser ("next" tier). No waitlist: the site has no backend. */}
      <section className="border-t border-rule bg-paper">
        <div className="mx-auto grid max-w-[1200px] items-center gap-6 px-4 py-14 md:grid-cols-[auto_1fr] md:px-6">
          <span className="w-fit rounded-sm border border-ghost/50 px-2 py-1 font-mono text-[11px] uppercase tracking-wider text-ghost">Coming next</span>
          <div>
            <h2 className="text-xl font-semibold">The Ghostline desktop app</h2>
            <p className="mt-1 max-w-2xl text-muted">One window for your wallet, channels, spending policy and approvals, with nothing to configure by hand. Your keys stay on your machine. Until then, the CLI and MCP server do everything shown here.</p>
          </div>
        </div>
      </section>

      {/* Get started */}
      <section id="start" className="ledger-bg border-t border-rule">
        <div className="mx-auto max-w-[1200px] px-4 py-20 md:px-6">
          <h2 className="voice text-4xl md:text-6xl">Start with a small deposit.</h2>
          <div className="mt-8 grid gap-4 md:grid-cols-3">
            {[
              { href: "/agents/", t: "I run an agent", d: "Install, connect MCP, set a budget." },
              { href: "/sellers/", t: "I sell an API", d: "Add the paywall, set prices, settle." },
              { href: "/docs/mainnet-quickstart/", t: "Walk the whole flow", d: "The mainnet quickstart, end to end." },
            ].map((c) => (
              <Link key={c.href} href={c.href} className="group rounded-[3px] border border-rule bg-paper p-5 transition-colors duration-[var(--quick)] hover:border-ink">
                <span className="text-lg font-semibold">{c.t} <span className="inline-block transition-transform duration-[var(--quick)] group-hover:translate-x-1">→</span></span>
                <p className="mt-1 text-muted">{c.d}</p>
              </Link>
            ))}
          </div>
        </div>
      </section>
    </>
  );
}
