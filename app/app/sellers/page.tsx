import { pageMetadata } from "@/lib/metadata";
import Link from "next/link";
import { CodeBlock } from "@/components/CodeBlock";
import { fees } from "@/lib/data";
import { microTari } from "@/lib/format";

export const metadata = pageMetadata("For API sellers · Ghostline", "Add the Ghostline HTTP 402 paywall, quote prices, and settle to your wallet.", "/sellers/");

const QUOTE = `HTTP/1.1 402 Payment Required

{
  "scheme": "ghostline.channel.voucher.v1",
  "price": 300,
  "basis": "metered",
  "unit_price": 100,
  "units": 3,
  "channel_id": "<64 hex>",
  "recipient_key": "<64 hex>",
  "service_output_key": "<64 hex>",
  "min_expiry_epoch": 38578,
  "max_expiry_epoch": 38778
}`;

const SERVE = `ghostline-paid-api serve --bind 127.0.0.1:8787 \\
  --ledger ledger.json --price 1000 \\
  --service-key-hex <service-public> \\
  --service-output-key-hex <payout-key> \\
  --min-expiry-epoch <epoch> --max-expiry-epoch <epoch + 200>`;

const SETTLE = `ghostline channel settle --file channel.json --force --submit --dry-run \\
  --seal-key-id-file service-key-id.json --max-fee 100000

ghostline channel settle --file channel.json --force --submit \\
  --seal-key-id-file service-key-id.json --max-fee <required_fees + margin>`;

export default function Sellers() {
  return (
    <>
      <section className="ledger-bg">
        <div className="mx-auto max-w-[1200px] px-4 pb-16 pt-14 md:px-6 md:pt-20">
          <p className="eyebrow">For API sellers</p>
          <h1 className="voice mt-4 max-w-4xl text-5xl leading-[.95] md:text-[76px]">Charge per call. <em className="text-chain">Settle once.</em></h1>
          <p className="mt-6 max-w-2xl text-[17px] text-muted">Put the Ghostline paywall in front of an API. Agents pay each call with a voucher you verify locally, with no chain round-trip. When you&rsquo;re ready, one transaction collects everything.</p>
          <dl className="mt-12 grid border-t border-rule sm:grid-cols-2 lg:grid-cols-4">
            {[["Pricing", "Fixed, per request or metered"], ["Price step", microTari(fees.priceStep)], ["Protocol fee", "1% of your payout"], ["Settle fee", `≈ ${microTari(fees.settle)}, paid by you`]].map(([k, v]) => (
              <div key={k} className="border-b border-rule py-4 pr-4"><dt className="eyebrow">{k}</dt><dd className="mt-1 text-lg">{v}</dd></div>
            ))}
          </dl>
        </div>
      </section>

      <section className="border-t border-rule bg-paper">
        <div className="mx-auto grid max-w-[1200px] gap-10 px-4 py-20 md:grid-cols-[1fr_1.3fr] md:px-6">
          <div>
            <p className="eyebrow">1 · Quote</p>
            <h2 className="voice mt-3 text-4xl md:text-5xl">Answer with a 402.</h2>
            <p className="mt-4 text-muted">An unpaid request gets a quote. This example uses placeholder keys and illustrative expiry bounds. The payer checks the scheme, the channel and your key before it signs anything, and binds its voucher to your payout key.</p>
            <p className="mt-4 text-muted">Use the reference server below, or the <span className="font-mono text-ink">ghostline-paywall</span> Rust crate for your own routes and pricing.</p>
          </div>
          <CodeBlock code={QUOTE} lang="http" />
        </div>
      </section>

      <section className="border-t border-rule">
        <div className="mx-auto grid max-w-[1200px] gap-10 px-4 py-20 md:grid-cols-[1fr_1.3fr] md:px-6">
          <div>
            <p className="eyebrow">2 · Serve</p>
            <h2 className="voice mt-3 text-4xl md:text-5xl">Verify, record, respond.</h2>
            <p className="mt-4 text-muted">Each voucher must raise the total and stay under the deposit. The server writes it to the ledger before it serves the response. Replaying the latest voucher charges nothing.</p>
          </div>
          <CodeBlock code={SERVE} />
        </div>
      </section>

      <section className="border-t border-rule bg-paper">
        <div className="mx-auto grid max-w-[1200px] gap-10 px-4 py-20 md:grid-cols-[1fr_1.3fr] md:px-6">
          <div>
            <p className="eyebrow">3 · Settle</p>
            <h2 className="voice mt-3 text-4xl md:text-5xl">Collect <em>before expiry.</em></h2>
            <p className="mt-4 text-muted">Dry-run, then submit with the quoted fee. Without <span className="font-mono text-ink">--force</span>, settle waits until the channel is close to expiry. Your payout lands on a one-time key your own wallet recognises, so it shows up like any other payment.</p>
            <div role="note" className="mt-6 rounded-[3px] border border-held/50 bg-held/5 px-4 py-3 text-sm">
              <strong className="text-held">Settle before the expiry epoch.</strong> After it, the payer can refund the whole deposit and your accepted calls are unpaid.
            </div>
            <Link href="/docs/settle/" className="mt-6 inline-block text-chain underline underline-offset-4">Settling and payouts →</Link>
          </div>
          <CodeBlock code={SETTLE} />
        </div>
      </section>
    </>
  );
}
