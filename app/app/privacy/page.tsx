import { pageMetadata } from "@/lib/metadata";
import { TwoViews } from "@/components/TwoViews";

export const metadata = pageMetadata("Privacy · Ghostline", "Compare the private call ledger with the public Tari Ootle records and see what observers can link.", "/privacy/");

const ROWS: { what: string; status: "hidden" | "public" | "linked"; detail: string }[] = [
  { what: "Each call and its price", status: "hidden", detail: "The payer sends each voucher privately to the service." },
  { what: "How many calls were made", status: "hidden", detail: "Only the latest cumulative voucher is settled." },
  { what: "Deposit amount", status: "hidden", detail: "The channel output is a commitment." },
  { what: "Payout, protocol fee and change amounts", status: "hidden", detail: "The chain shows commitments. The treasury key holder can recover the protocol fee and infer the payout." },
  { what: "The payer's account", status: "hidden", detail: "The open names no account component." },
  { what: "Which payout key belongs to the service", status: "hidden", detail: "The public fields do not identify the owner of the one-time payout key." },
  { what: "Network fees", status: "public", detail: "Every transaction's fee is visible." },
  { what: "That an open and a settle belong together", status: "linked", detail: "The settle spends the channel output the open created." },
  { what: "The service's fee account", status: "linked", detail: "The settle pays its network fee from a named account." },
  { what: "That a settle is a Ghostline settle", status: "linked", detail: "The protocol fee goes to the treasury key, so settlements are countable." },
  { what: "Funding links", status: "linked", detail: "If one wallet funds both sides, the funding graph connects them. Use independent funding." },
];

const BADGE = { hidden: "text-ghost border-ghost/50", public: "text-chain border-chain/50", linked: "text-held border-held/50" } as const;

export default function Privacy() {
  return (
    <>
      <section className="ledger-bg">
        <div className="mx-auto max-w-[1200px] px-4 pb-16 pt-14 md:px-6 md:pt-20">
          <p className="eyebrow">Privacy</p>
          <h1 className="voice mt-4 max-w-4xl text-5xl leading-[.95] md:text-[76px]">One channel, <em className="text-ghost">seen twice.</em></h1>
          <p className="mt-6 max-w-2xl text-[17px] leading-relaxed text-muted">
            On the left is what you know: every call, every price, every running total. On the right are the public open and close records for the same channel. The replay uses recorded mainnet data.
          </p>
          <div className="mt-12"><TwoViews size="full" /></div>
        </div>
      </section>

      <section className="border-t border-rule">
        <div className="mx-auto grid max-w-[1200px] gap-10 px-4 py-20 md:grid-cols-[1fr_1.6fr] md:px-6">
          <div>
            <p className="eyebrow">What stays public</p>
            <h2 className="voice mt-3 text-4xl md:text-5xl">Private is not invisible.</h2>
            <p className="mt-4 text-muted">Ghostline hides amounts and call patterns. It does not hide that a channel exists, what it paid in network fees, or links you create by funding both sides from one wallet. Here is the full list, from the mainnet observer test on 13 Nov 2026.</p>
            <ul className="mt-6 space-y-2 text-sm">
              <li><span className={`mr-2 rounded-sm border px-1.5 py-0.5 font-mono text-[11px] ${BADGE.hidden}`}>hidden</span>absent from public chain records</li>
              <li><span className={`mr-2 rounded-sm border px-1.5 py-0.5 font-mono text-[11px] ${BADGE.public}`}>public</span>anyone can read it</li>
              <li><span className={`mr-2 rounded-sm border px-1.5 py-0.5 font-mono text-[11px] ${BADGE.linked}`}>linked</span>an observer can connect it to something else</li>
            </ul>
          </div>
          <table className="w-full text-[15px]">
            <thead><tr className="border-b border-rule text-left"><th className="eyebrow py-2 font-normal">What</th><th className="eyebrow py-2 font-normal">Status</th><th className="eyebrow hidden py-2 font-normal sm:table-cell">Why</th></tr></thead>
            <tbody>
              {ROWS.map((r) => (
                <tr key={r.what} className="border-b border-rule align-top">
                  <td className="py-3 pr-3">{r.what}<p className="mt-1 text-sm text-muted sm:hidden">{r.detail}</p></td>
                  <td className="py-3 pr-3"><span className={`rounded-sm border px-1.5 py-0.5 font-mono text-[11px] ${BADGE[r.status]}`}>{r.status}</span></td>
                  <td className="hidden py-3 text-muted sm:table-cell">{r.detail}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </section>
    </>
  );
}
