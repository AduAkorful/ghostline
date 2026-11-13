import manifest from "../../deploy/mainnet.json";
import runs from "../data/mainnet-proof.json";
import { FEE_DIVISOR } from "@ghostline/sdk/fee";

export const INDEXER = "https://indexer-a.ootle.tari.com";
export const txUrl = (hash: string) => `${INDEXER}/transactions/${hash}`;
export const substateUrl = (id: string) => `${INDEXER}/substates/${id}`;
export const RECORDED_ON = new Intl.DateTimeFormat("en-GB", {
  dateStyle: "medium", timeZone: "UTC",
}).format(new Date(runs.recordedOn));

export const deployment = {
  channelTemplate: manifest.channel.template,
  feeConfigTemplate: manifest.feeConfig.template,
  feeConfig: manifest.feeConfig.component,
};

export type ProofKind = "publish" | "open" | "settle" | "refund";
type ProofRecord = {
  run: string; kind: ProofKind; label: string; hash: string;
  status: "Accepted" | "Finalized"; feeMicroTari: number; note: string;
};

function publish(label: string, hash: string, feeMicroTari: number, note: string): ProofRecord {
  return { run: "Deploy", kind: "publish", label, hash, status: "Accepted", feeMicroTari, note };
}

export const proof: ProofRecord[] = [
  publish("Publish fee-config template", manifest.feeConfig.publishTransaction, manifest.feeConfig.publishFeeMicroTari, "GhostlineFeeConfig"),
  publish("Create fee config (treasury)", manifest.feeConfig.createTransaction, manifest.feeConfig.createFeeMicroTari, "Treasury read on chain at every settle"),
  publish("Publish channel template", manifest.channel.publishTransaction, manifest.channel.publishFeeMicroTari, "GhostlineChannel"),
  ...(runs.records as ProofRecord[]),
];

function record(run: string, kind: ProofKind) {
  const match = proof.find((p) => p.run === run && p.kind === kind);
  if (!match) throw new Error(`Missing ${kind} record for ${run}`);
  return match;
}

export const sim = {
  openHash: record("Run 5", "open").hash,
  settleHash: record("Run 5", "settle").hash,
  openFee: record("Run 5", "open").feeMicroTari,
  settleFee: record("Run 5", "settle").feeMicroTari,
  price: runs.run5.priceMicroTari,
  budget: runs.run5.budgetMicroTari,
  path: runs.run5.path,
  attempts: runs.run5.paidCalls + runs.run5.refusedCalls,
};

export const fees = {
  open: record("Run 1", "open").feeMicroTari,
  settle: record("Run 5", "settle").feeMicroTari,
  refund: record("Run 3", "refund").feeMicroTari,
  protocolPercent: 100 / Number(FEE_DIVISOR),
  priceStep: Number(FEE_DIVISOR),
};

/**
 * Stats derived only from the recorded transactions and run records. Nothing here is a live chain total: amounts of
 * payouts and protocol fees are hidden commitments, so the site cannot count them. Fee figures are deliberately not part of the stats.
 */
export const stats = (() => {
  const count = (kind: ProofKind) => proof.filter((p) => p.kind === kind).length;
  const settledRuns = new Set(proof.filter((p) => p.kind === "settle").map((p) => p.run));
  const paidCalls = Object.values(runs.paidCallsByRun).reduce((a, b) => a + b, 0);
  return {
    transactions: proof.length,
    channelsOpened: count("open"),
    settles: count("settle"),
    refunds: count("refund"),
    paidCalls,
    refusedCalls: runs.run5.refusedCalls,
    // Opens and settles of the channels that carried the paid calls: what those calls cost the chain in records.
    onChainRecordsForCalls: proof.filter((p) => settledRuns.has(p.run) && (p.kind === "open" || p.kind === "settle")).length,
  };
})();
