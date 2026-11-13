import { pageMetadata } from "@/lib/metadata";
import { Hash } from "@/components/Hash";
import { deployment, proof, RECORDED_ON, substateUrl, txUrl } from "@/lib/data";
import { microTari, num } from "@/lib/format";

export const metadata = pageMetadata("Mainnet proof · Ghostline", "Check the Ghostline deployment and recorded Tari Ootle mainnet opens, settles, and refund.", "/proof/");

const KIND = { publish: "text-muted", open: "text-ink", settle: "text-chain", refund: "text-ghost" } as const;

export default function Proof() {
  const runs = Array.from(new Set(proof.map((p) => p.run)));
  return (
    <>
      <section className="ledger-bg">
        <div className="mx-auto max-w-[1200px] px-4 pb-14 pt-14 md:px-6 md:pt-20">
          <p className="eyebrow">Mainnet proof · recorded {RECORDED_ON}</p>
          <h1 className="voice mt-4 text-5xl leading-[.95] md:text-[76px]">Don&rsquo;t trust the page.<br /><em className="text-chain">Check the chain.</em></h1>
          <p className="mt-6 max-w-2xl text-[17px] text-muted">Deployment and channel transactions from five recorded runs on Tari Ootle mainnet. Each hash opens the public indexer. The amounts shown are recorded network fees.</p>
        </div>
      </section>

      <section className="border-t border-rule bg-paper">
        <div className="mx-auto max-w-[1200px] px-4 py-14 md:px-6">
          <h2 className="eyebrow">Deployment</h2>
          <dl className="mt-4 grid gap-x-8 md:grid-cols-3">
            {[["Channel template", deployment.channelTemplate], ["Fee-config template", deployment.feeConfigTemplate], ["Fee config (treasury)", deployment.feeConfig]].map(([k, v]) => (
              <div key={k} className="border-b border-rule py-3"><dt className="text-sm text-muted">{k}</dt><dd className="mt-1"><Hash value={v} href={substateUrl(v)} /></dd></div>
            ))}
          </dl>
          <p className="mt-3 text-sm text-muted">Compare against the substate on the indexer, not a local build hash: walletd optimises WASM before publishing.</p>
        </div>
      </section>

      <section className="border-t border-rule">
        <div className="mx-auto max-w-[1200px] px-4 py-14 md:px-6">
          {runs.map((run) => (
            <div key={run} className="mb-10">
              <h2 className="voice text-3xl italic">{run}</h2>
              <ul className="mt-3 border-t border-rule">
                {proof.filter((p) => p.run === run).map((p) => (
                  <li key={p.hash} className="grid gap-1 border-b border-rule py-3.5 md:grid-cols-[90px_1.4fr_180px_120px_130px] md:items-center md:gap-4">
                    <span className={`font-mono text-xs uppercase tracking-wider ${KIND[p.kind]}`}>{p.kind}</span>
                    <span>{p.label}{p.note && <span className="text-muted"> · {p.note}</span>}</span>
                    <Hash value={p.hash} href={txUrl(p.hash)} />
                    <span className="font-mono text-xs text-chain">{p.status}</span>
                    <span className="font-mono text-[13px] md:text-right">{microTari(p.feeMicroTari)}</span>
                  </li>
                ))}
              </ul>
            </div>
          ))}
          <p className="text-sm text-muted">Fees are network fees paid in TARI (1 TARI = 1,000,000 µT). The indexer transaction summary gives the final amount paid.</p>
          <p className="mt-2 text-sm text-muted">Public indexers do not keep every record forever; on mainnet the short result view of older transactions is already gone. If a link stops resolving, the hash and fee recorded here remain the record, and the template and fee-config addresses above stay readable on the indexer.</p>
        </div>
      </section>
    </>
  );
}
