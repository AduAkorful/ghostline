// SPDX-License-Identifier: MIT
// Renders evidence.json for people; the JSON stays the single source of every id and number.

const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
const number = new Intl.NumberFormat("en-US");

export function formatRunDate(isoDate) {
  const match = /^(\d{4})-(\d{2})-(\d{2})$/.exec(String(isoDate || ""));
  if (!match || !MONTHS[Number(match[2]) - 1]) return "date not recorded";
  return `${Number(match[3])} ${MONTHS[Number(match[2]) - 1]} ${match[1]}`;
}

export function explorerStatus(evidence, transactionId) {
  return evidence.explorerAvailability?.transactions?.[transactionId] || "not checked";
}

// Each run lists only fields the record has; nothing is filled in from elsewhere.
export function describeRuns(evidence) {
  const chain = evidence.chainRecord;
  const service = evidence.serviceRecord;
  const live = evidence.liveRunRecord;
  const refund = evidence.refundRecord;
  const batch = evidence.batchRecord;
  const template = evidence.templateRecord;
  const runs = [];
  if (live) {
    runs.push({
      title: `Hosted Sapient run · ${formatRunDate(live.runDate)}`,
      summary: `${live.successfulCalls} paid calls at ${live.unitPriceMicroTari} µT · ${live.acceptedTotalMicroTari} µT accepted · ${live.depositMicroTari} µT deposit. ${live.custody}`,
      transactions: [
        ["Private funding", live.fundingTransactionId, live.fundingTransactionUrl],
        [`Channel open · fee ${number.format(live.openFeeMicroTari)} µT`, live.openTransactionId, live.openTransactionUrl],
        [`Settlement · fee ${number.format(live.settlementFeeMicroTari)} µT`, live.settlementTransactionId, live.settlementTransactionUrl],
      ],
    });
  }
  if (chain) {
    runs.push({
      title: `Command-line run · ${formatRunDate(evidence.runDate)}`,
      summary: `${service.successfulCalls} paid calls at ${service.unitPriceMicroTari} µT · ${service.acceptedTotalMicroTari} µT accepted (service ledger). Deposit and service amount hidden in public fields.`,
      transactions: [
        [`Channel open · fee ${number.format(chain.openFeeMicroTari)} µT`, chain.openTransactionId, chain.openTransactionUrl],
        [`Settlement · fee ${number.format(chain.settlementFeeMicroTari)} µT`, chain.settlementTransactionId, chain.settlementTransactionUrl],
      ],
    });
  }
  if (refund) {
    runs.push({
      title: "Expiry refund · latest template",
      summary: `Payer refund after expiry epoch ${refund.expiryEpoch}; fee ${number.format(refund.feePaidMicroTari)} µT. The recipient output was not unblinded to check the amount received.`,
      transactions: [
        ["Channel open", refund.channelOpenTransactionId, refund.channelOpenTransactionUrl],
        ["Refund", refund.refundTransactionId, refund.refundTransactionUrl],
      ],
    });
  }
  if (batch) {
    runs.push({
      title: "Pooled settlement · trusted-local prototype",
      summary: `Two channels with ${batch.channels.map((channel) => channel.successfulCalls).join(" and ")} calls closed in one settlement (fee ${number.format(batch.feeMicroTari)} µT). ${batch.coordinatorPrivacy}`,
      transactions: [
        ...batch.channels.map((channel, index) => [`Channel ${index + 1} open`, channel.openTransactionId, channel.openTransactionUrl]),
        ["Pooled settlement", batch.settlementTransactionId, batch.settlementTransactionUrl],
      ],
    });
  }
  const previousRefund = evidence.historicalPreviousTemplateRefundRecord;
  if (previousRefund) {
    runs.push({
      title: "Expiry refund · previous template (historical)",
      summary: previousRefund.testDescription,
      transactions: [
        ["Channel open", previousRefund.channelOpenTransactionId, previousRefund.channelOpenTransactionUrl],
        ["Refund", previousRefund.refundTransactionId, previousRefund.refundTransactionUrl],
      ],
    });
  }
  if (template) {
    runs.push({
      title: "Channel template publish",
      summary: `Template ${template.address}; fee ${number.format(template.publishFeeMicroTari)} µT.`,
      transactions: [["Template publish", template.publishTransactionId, template.publishTransactionUrl]],
    });
  }
  return runs;
}

function render(evidence) {
  const availability = evidence.explorerAvailability;
  document.getElementById("availability-note").textContent = availability
    ? `Explorer status checked ${formatRunDate(availability.checkedOn)}. ${availability.note}`
    : "Explorer availability was not recorded.";
  const container = document.getElementById("runs");
  container.replaceChildren();
  for (const run of describeRuns(evidence)) {
    const article = document.createElement("article");
    article.className = "run";
    const title = document.createElement("h2");
    title.textContent = run.title;
    const summary = document.createElement("p");
    summary.textContent = run.summary;
    const list = document.createElement("ul");
    list.className = "run-transactions";
    for (const [label, transactionId, url] of run.transactions) {
      const item = document.createElement("li");
      const name = document.createElement("span");
      name.textContent = label;
      const link = document.createElement("a");
      link.href = url;
      link.target = "_blank";
      link.rel = "noopener noreferrer";
      link.textContent = `${transactionId} ↗`;
      const state = explorerStatus(evidence, transactionId);
      const badge = document.createElement("span");
      badge.className = state === "loads" ? "badge badge-ok" : "badge";
      badge.textContent = state === "loads" ? "Explorer: loads" : `Explorer: ${state}`;
      item.append(name, link, badge);
      list.append(item);
    }
    article.append(title, summary, list);
    container.append(article);
  }
  const notes = document.getElementById("notes");
  notes.replaceChildren(...(evidence.notes || []).map((text) => {
    const item = document.createElement("li");
    item.textContent = text;
    return item;
  }));
}

if (typeof document !== "undefined" && document.getElementById("runs")) {
  fetch("evidence.json")
    .then((response) => {
      if (!response.ok) throw new Error("evidence fetch failed");
      return response.json();
    })
    .then(render)
    .catch(() => {
      document.getElementById("availability-note").textContent = "The evidence record could not be loaded. Open evidence.json directly, or reload this page.";
    });
}
