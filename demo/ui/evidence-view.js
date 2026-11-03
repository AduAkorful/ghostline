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
  const template = evidence.templateRecord;
  const opened = evidence.postResetOpen;
  const runs = [];
  if (template) {
    runs.push({
      title: `Channel template publish · ${formatRunDate(template.publishedOn || evidence.runDate)}`,
      summary: `Template ${template.address}; fee ${number.format(template.publishFeeMicroTari)} µT. ${template.finality}.`,
      transactions: [["Template publish", template.publishTransactionId, template.publishTransactionUrl]],
    });
  }
  if (opened) {
    runs.push({
      title: `Channel open · ${formatRunDate(opened.runDate)}`,
      summary: `${opened.depositMicroTari} µT deposit. Fee cap ${number.format(opened.openFeeCapMicroTari)} µT. ${opened.openFeeNote || ""} ${opened.note || ""}`.replace(/\s+/g, " ").trim(),
      transactions: [["Channel open", opened.openTransactionId, opened.openTransactionUrl]],
    });
  }
  const sapient = evidence.sapientRun;
  if (sapient) {
    runs.push({
      title: `Sapient paid calls · ${formatRunDate(sapient.runDate)}`,
      summary: `${sapient.wallet}. Deposit ${sapient.depositMicroTari} µT. ${sapient.acceptedCalls} accepted calls, ${sapient.acceptedTotalMicroTari} µT, from the service ledger. ${sapient.note || ""}`.replace(/\s+/g, " ").trim(),
      transactions: [
        ["Sapient funding", sapient.fundingTransactionId, sapient.fundingTransactionUrl],
        ["Channel open", sapient.openTransactionId, sapient.openTransactionUrl],
        ["Settlement", sapient.settlementTransactionId, sapient.settlementTransactionUrl],
      ],
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
