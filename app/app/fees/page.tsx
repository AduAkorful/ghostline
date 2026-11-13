import { pageMetadata } from "@/lib/metadata";
import Link from "next/link";
import { FeeCalculator } from "@/components/FeeCalculator";
import { fees, RECORDED_ON } from "@/lib/data";
import { microTari } from "@/lib/format";

export const metadata = pageMetadata("Fees · Ghostline", "Estimate channel capacity, Ghostline’s 1% protocol fee, and Tari Ootle network fees.", "/fees/");

export default function Fees() {
  return (
    <>
      <section className="ledger-bg">
        <div className="mx-auto max-w-[1200px] px-4 pb-16 pt-14 md:px-6 md:pt-20">
          <p className="eyebrow">Fees</p>
          <h1 className="voice mt-4 max-w-4xl text-5xl leading-[.95] md:text-[76px]">What a channel <em className="text-ghost">really costs.</em></h1>
          <p className="mt-6 max-w-2xl text-[17px] text-muted">Two kinds of fee: the Tari network&rsquo;s fee for each on-chain transaction, and Ghostline&rsquo;s 1% protocol fee on what the service is paid. Calls themselves cost nothing on chain.</p>
          <div className="mt-12"><FeeCalculator /></div>
          <p className="mt-3 text-sm text-muted">Simulation · uses network fees measured on mainnet on {RECORDED_ON}; they change, so dry-run before you open.</p>
        </div>
      </section>
      <section className="border-t border-rule">
        <div className="mx-auto grid max-w-[1200px] gap-10 px-4 py-20 md:grid-cols-2 md:px-6">
          <div>
            <p className="eyebrow">Network fees</p>
            <h2 className="voice mt-3 text-4xl">Paid to Tari, public on chain.</h2>
            <table className="mt-6 w-full text-[15px]">
              <thead><tr className="border-b border-rule text-left"><th className="eyebrow py-2 font-normal">Transaction</th><th className="eyebrow py-2 font-normal">Paid by</th><th className="eyebrow py-2 text-right font-normal">Fee</th></tr></thead>
              <tbody>
                {[["Open", "Payer", fees.open], ["Settle", "Service", fees.settle], ["Expiry refund", "Payer", fees.refund]].map(([t, b, f]) => (
                  <tr key={t as string} className="border-b border-rule"><td className="py-3">{t}</td><td className="py-3 text-muted">{b}</td><td className="py-3 text-right font-mono">≈ {microTari(f as number)}</td></tr>
                ))}
              </tbody>
            </table>
            <p className="mt-3 text-sm text-muted">On the private-input open path, the whole submitted cap was charged. The open helper submits at the quoted fee within your reviewed ceiling.</p>
          </div>
          <div>
            <p className="eyebrow">Protocol fee</p>
            <h2 className="voice mt-3 text-4xl">1% of the payout, at settle.</h2>
            <ul className="mt-6 space-y-3 text-muted">
              <li>Taken once, when the service settles, as a hidden-amount output to the treasury key in the on-chain fee config.</li>
              <li>Prices are multiples of {microTari(fees.priceStep)}, so 1% is always a whole number of µT.</li>
              <li>A deposit <span className="font-mono text-ink">D</span> can pay out at most <span className="font-mono text-ink">floor((D − 1) / 101) × 100</span> µT. The rest covers the fee and 1 µT of change.</li>
              <li>The protocol fee applies to settlement only.</li>
            </ul>
            <Link href="/docs/fees/" className="mt-6 inline-block text-chain underline underline-offset-4">Fee reference →</Link>
          </div>
        </div>
      </section>
    </>
  );
}
