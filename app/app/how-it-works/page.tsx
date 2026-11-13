import { pageMetadata } from "@/lib/metadata";
import Link from "next/link";
import { fees } from "@/lib/data";
import { microTari } from "@/lib/format";

export const metadata = pageMetadata("How it works · Ghostline", "Follow a Ghostline channel from deposit through signed off-chain calls to settlement or an expiry refund.", "/how-it-works/");

const LIFECYCLE = [
  { stage: "Open", where: "chain", who: "Payer's wallet", body: "Locks a deposit in a channel output. The chain sees one transaction with the amount hidden; the fee is public.", fee: `≈ ${microTari(fees.open)}` },
  { stage: "Pay per call", where: "off-chain", who: "Agent → API", body: "Each call carries a signed voucher for the new running total. Nothing is published. It repeats as often as the agent calls.", fee: "0 µT on chain" },
  { stage: "Settle", where: "chain", who: "Service", body: "Before expiry, the service spends the channel with the latest voucher: its payout, the 1% protocol fee, and the payer's change, all as hidden amounts.", fee: `≈ ${microTari(fees.settle)}` },
  { stage: "Refund at expiry", where: "chain", who: "Payer", body: "If the service never settles, the payer takes the whole deposit back once the expiry epoch is reached.", fee: `≈ ${microTari(fees.refund)}` },
];

const HANDSHAKE = [
  { who: "Agent", text: "Calls the API with no payment." },
  { who: "API", text: "Answers HTTP 402 with a quote: price, channel id, the service's payout key, and how long the quote is valid." },
  { who: "Policy", text: "Checks the quote outside the agent: budget, per-call cap, vendor allowlist. New vendors and large spends wait for a human." },
  { who: "Agent", text: "Signs a voucher for the new cumulative total and retries the call with it." },
  { who: "API", text: "Verifies the voucher, stores it as the latest, and serves the response." },
];

const VOUCHER = [
  { field: "channel_id", text: "Binds the network, payer key, deposit, spend conditions and a nonce. A voucher for one channel is useless on another.", where: "both" },
  { field: "service_output_key", text: "The one-time key the payout will be locked to, so it lands in the service's wallet.", where: "both" },
  { field: "output_commitment", text: "A commitment to the cumulative amount. The chain checks it; it never sees the number.", where: "both" },
  { field: "signature", text: "The payer's signature over the three fields above, under a versioned domain.", where: "both" },
  { field: "amount + mask", text: "The opening of the commitment. Only the service gets these, so it can verify the total and later find its payout.", where: "off-chain" },
];

const GUARANTEES = [
  "Settle works only before the expiry epoch. Refund works only at or after it.",
  "A settle can pay out at most floor((deposit − 1) / 101) × 100 µT. The 1% fee and 1 µT of change are reserved.",
  "The service accepts only a higher cumulative total than its last voucher, so replaying an old voucher pays nothing extra.",
  "The protocol fee goes to the treasury key read from the on-chain fee config at settle time.",
];

export default function HowItWorks() {
  return (
    <>
      <section className="ledger-bg">
        <div className="mx-auto max-w-[1200px] px-4 pb-16 pt-14 md:px-6 md:pt-20">
          <p className="eyebrow">How it works</p>
          <h1 className="voice mt-4 max-w-4xl text-5xl leading-[.95] md:text-[76px]">Four moves. <em className="text-ghost">Only two touch the chain.</em></h1>
          <p className="mt-6 max-w-2xl text-[17px] text-muted">A Ghostline channel is a deposit both sides can trust: the payer can&rsquo;t take it back early, and the service can&rsquo;t take more than the payer signed for.</p>

          <ol className="mt-12 border-t border-rule">
            {LIFECYCLE.map((s, i) => (
              <li key={s.stage} className="grid gap-2 border-b border-rule py-6 md:grid-cols-[48px_220px_1fr_170px] md:gap-6">
                <span className="font-mono text-sm text-muted">0{i + 1}</span>
                <div>
                  <h2 className="text-xl font-semibold">{s.stage}</h2>
                  <span className={`mt-1 inline-block rounded-sm border px-1.5 py-0.5 font-mono text-[11px] ${s.where === "chain" ? "border-chain/50 text-chain" : "border-ghost/50 text-ghost"}`}>{s.where === "chain" ? "on chain" : "off-chain"}</span>
                  <p className="mt-1 text-sm text-muted">{s.who}</p>
                </div>
                <p className="text-muted">{s.body}</p>
                <p className="font-mono text-sm md:text-right">{s.fee}</p>
              </li>
            ))}
          </ol>
          <p className="mt-3 text-sm text-muted">Network fees measured on mainnet, 13 Nov 2026. Re-run the dry-run before you rely on them.</p>
        </div>
      </section>

      <section className="border-t border-rule bg-paper">
        <div className="mx-auto grid max-w-[1200px] gap-10 px-4 py-20 md:grid-cols-[1fr_1.4fr] md:px-6">
          <div>
            <p className="eyebrow">One paid call</p>
            <h2 className="voice mt-3 text-4xl md:text-5xl">Quote. Check. Sign. <em>Call.</em></h2>
            <p className="mt-4 text-muted">Ghostline speaks HTTP 402 Payment Required. The whole exchange happens between the agent and the API, in milliseconds, with no chain access.</p>
          </div>
          <ol className="border-t border-rule">
            {HANDSHAKE.map((h, i) => (
              <li key={i} className="grid grid-cols-[28px_76px_1fr] gap-3 border-b border-rule py-3.5">
                <span className="font-mono text-sm text-muted">{i + 1}</span>
                <span className={`font-mono text-[13px] ${h.who === "Policy" ? "text-held" : h.who === "Agent" ? "text-ghost" : "text-ink"}`}>{h.who}</span>
                <span>{h.text}</span>
              </li>
            ))}
          </ol>
        </div>
      </section>

      <section className="border-t border-rule">
        <div className="mx-auto grid max-w-[1200px] gap-10 px-4 py-20 md:grid-cols-[1fr_1.4fr] md:px-6">
          <div>
            <p className="eyebrow">Inside a voucher</p>
            <h2 className="voice mt-3 text-4xl md:text-5xl">A signed promise <em>with a hidden number.</em></h2>
            <p className="mt-4 text-muted">Each voucher replaces the last. Only the latest matters, so the service settles once no matter how many calls were made.</p>
          </div>
          <dl className="border-t border-rule">
            {VOUCHER.map((v) => (
              <div key={v.field} className="grid gap-1 border-b border-rule py-3.5 sm:grid-cols-[180px_1fr] sm:gap-4">
                <dt className={`font-mono text-[13px] ${v.where === "off-chain" ? "text-ghost" : "text-chain"}`}>{v.field}</dt>
                <dd className="text-muted">{v.text}</dd>
              </div>
            ))}
          </dl>
        </div>
      </section>

      <section className="border-t border-rule bg-paper">
        <div className="mx-auto max-w-[1200px] px-4 py-20 md:px-6">
          <p className="eyebrow">Enforced by the channel template</p>
          <h2 className="voice mt-3 text-4xl md:text-5xl">Rules nobody can bend.</h2>
          <ul className="mt-8 grid gap-px overflow-hidden rounded-[3px] border border-rule bg-rule md:grid-cols-2">
            {GUARANTEES.map((g) => <li key={g} className="bg-paper p-5">{g}</li>)}
          </ul>
          <div className="mt-10 flex flex-wrap gap-3">
            <Link href="/privacy/" className="rounded-sm bg-ink px-5 py-3 font-semibold text-on-ink">See both views →</Link>
            <Link href="/docs/" className="rounded-sm border border-rule px-5 py-3 hover:border-ink">Read the docs</Link>
          </div>
        </div>
      </section>
    </>
  );
}
