// SPDX-License-Identifier: MIT

import { SimulatedPaywall, PRICE } from "./sim-service.js";
import { createEphemeralChannelKeys } from "./ephemeral-keys.js";
import { channelId, publicKeyFromSecret, scalarFromWide, signVoucher, voucherToBytes } from "./vendor/ghostline.js";

const byId = (id) => document.getElementById(id);
const countLimit = 5;
// Matches the live 51 µT deposit, so the simulated spend limit and refusal match live mode.
const deposit = 51n;
let payerSecret;
let serviceKey;
let params;
let paywall;
let accepted = 0n;
let evidence;
let evidenceLoaded = false;
let settlementShown = false;
let liveChannel;
let liveCalls = 0;
let liveAccepted = 0n;
let liveRefundPending = false;
// A restored channel whose state the relay could not confirm; its live actions wait for a reload.
let liveStateUnconfirmed = false;
// Set once the live channel has settled or been refunded: { kind, url, transactionId, deposit }.
let liveResult;
let simulationMode = false;
// One over-limit attempt per channel; a refusal never changes the accepted total.
let simOverspendTried = false;
let liveOverspendTried = false;
let lastRefusal;
let liveSetupCancelled = false;
let abandonArmed = false;
// While live setup is running, refund and abandon would race the relay open (QA final rerun).
let liveOpenInFlight = false;
const LIVE_OPEN_PROGRESS_LABELS = new Set(["Checking Sapient…", "Waiting for Sapient approval…", "Waiting for finality…", "Opening channel…", "Retrying relay open…"]);

// Native Esmeralda resource. On 2026-11-02 the indexer substate for this address
// carried the symbol TARI. The old XTR alias no longer resolves.
const NATIVE_TARI_RESOURCE = `resource_${"01".repeat(32)}`;
const LIVE_PENDING_KEY = "ghostline-live-pending";
const LIVE_CHANNEL_KEY = "ghostline-live-channel";
const TARI_PROVIDER_READY_EVENT = "tari#initialized";
const TARI_PROVIDER_TIMEOUT_MS = 5_000;
// A locked Sapient can leave a request unanswered forever, so wallet lookups are capped.
const WALLET_LOOKUP_TIMEOUT_MS = 10_000;
// Funding finality plus the relay open must end in an answer the user can act on.
const LIVE_OPEN_DEADLINE_MS = 180_000;
// Above the relay's 45 s funding-discovery wait plus a CLI open.
const RELAY_OPEN_TIMEOUT_MS = 120_000;
const RELAY_RETRY_MIN_MS = 10_000;
const FUNDING_NOT_DISCOVERED = /has not discovered this Sapient funding output/i;
// The relay stops stepping past recreated outputs before this page's request timeout; walletd
// keeps its counter, so asking again continues past them.
const OUTPUT_COLLISION_EXHAUSTED = /recreated an output that already exists on chain/i;
const configuredRelayUrl = String(window.GHOSTLINE_CONFIG?.relayUrl || "").trim();

function readPendingFunding() {
  try {
    const persisted = localStorage.getItem(LIVE_PENDING_KEY);
    const tabScoped = sessionStorage.getItem(LIVE_PENDING_KEY);
    const value = JSON.parse(persisted || tabScoped || "null");
    if (value?.fundingTxId && value?.sessionId) {
      localStorage.setItem(LIVE_PENDING_KEY, JSON.stringify(value));
      sessionStorage.removeItem(LIVE_PENDING_KEY);
      return value;
    }
  } catch { /* Storage may be unavailable in a restricted browser context. */ }
  return undefined;
}

function savePendingFunding(value) {
  try {
    localStorage.setItem(LIVE_PENDING_KEY, JSON.stringify(value));
    sessionStorage.removeItem(LIVE_PENDING_KEY);
  } catch {
    sessionStorage.setItem(LIVE_PENDING_KEY, JSON.stringify(value));
  }
}

function restorePayerSecret(pending) {
  const hex = pending?.payerSecretHex || sessionStorage.getItem("ghostline-live-payer-secret") || "";
  if (/^[0-9a-f]{64}$/i.test(hex)) {
    try { sessionStorage.setItem("ghostline-live-payer-secret", hex); } catch { /* Storage may be restricted. */ }
    if (pending && pending.payerSecretHex !== hex) {
      pending.payerSecretHex = hex;
      savePendingFunding(pending);
    }
    try { return hexToBytes(hex); } catch { return new Uint8Array(); }
  }
  return new Uint8Array();
}

function fundingMemo(sessionId, payerPublicKey, refundAddress) {
  return `G2:${sessionId}${payerPublicKey}:${refundAddress}`;
}

function showPendingFundingControls(pending) {
  if (!pending?.fundingTxId) {
    byId("refund-pending-funding").hidden = true;
    byId("abandon-pending-funding").hidden = true;
    byId("pending-funding-link").hidden = true;
    abandonArmed = false;
    byId("abandon-pending-funding").textContent = "Abandon saved transfer and start over";
    return;
  }
  showPendingFunding(pending.fundingTxId);
  byId("refund-pending-funding").hidden = liveOpenInFlight;
  byId("abandon-pending-funding").hidden = liveOpenInFlight;
  byId("live-stop").hidden = false;
  byId("live-stop").textContent = "Continue with simulation";
  if (!byId("live-open").disabled) {
    byId("live-open").textContent = "Retry opening channel (no new charge)";
  }
}

function clearPendingFunding() {
  try { localStorage.removeItem(LIVE_PENDING_KEY); } catch { /* Storage may be restricted. */ }
  try { sessionStorage.removeItem(LIVE_PENDING_KEY); } catch { /* Storage may be restricted. */ }
}

const status = (message) => { byId("status").textContent = message; };

// A visible fallback when live mode cannot start; the #status line alone was easy to miss.
const RELAY_BANNER_COPY = {
  waking: "🕯️ The spirit is waking (free-tier server, up to about 75 s). The instant simulation works right now.",
  down: "👻 The live relay isn't answering. Run the instant simulation, or inspect the recorded Esmeralda runs.",
  "sapient-missing": "No Sapient wallet found in this browser. Install it from sapient.tari.mw and reload, or use the simulation.",
};

function showRelayBanner(kind) {
  if (liveSetupCancelled) return;
  byId("relay-banner-text").textContent = RELAY_BANNER_COPY[kind];
  byId("relay-banner").dataset.kind = kind;
  byId("relay-banner").hidden = false;
}

function hideRelayBanner() {
  byId("relay-banner").hidden = true;
}

function simulateFromBanner() {
  hideRelayBanner();
  if (!liveChannel) {
    liveSetupCancelled = true;
    byId("live-stop").hidden = true;
    reset();
  } else if (!simulationMode) {
    returnToSimulation();
  }
  runRemainingSimulation();
  byId("demo-title").scrollIntoView?.({ behavior: "smooth", block: "start" });
}

function randomBytes(length) {
  const out = new Uint8Array(length);
  crypto.getRandomValues(out);
  return out;
}

function bytesToHex(bytes) {
  return Array.from(bytes, (byte) => byte.toString(16).padStart(2, "0")).join("");
}

function hexToBytes(hex) {
  if (typeof hex !== "string" || hex.length % 2 !== 0 || !/^[0-9a-f]*$/i.test(hex)) throw new Error("Relay returned malformed hex data.");
  return Uint8Array.from(hex.match(/../g) || [], (byte) => Number.parseInt(byte, 16));
}

function saveLiveChannel() {
  if (!liveChannel) return;
  sessionStorage.setItem(LIVE_CHANNEL_KEY, JSON.stringify({
    channel: liveChannel,
    calls: liveCalls,
    accepted: liveAccepted.toString(),
  }));
}

const SIMULATION_CHAIN_COPY = {
  subtitle: "Recorded Esmeralda proof, separate from these clicks",
  title: "No transaction was created by this demo.",
  text: "The browser simulation never connects to a wallet or sends a transaction. After five calls, this panel shows the current template publish. That publish is a separate transaction.",
};

function explorerTxUrl(transactionId) {
  return `https://explorer.tari.mw/tx/${encodeURIComponent(transactionId)}`;
}

// Full 64-hex ids overflow the result card on narrow screens (QA UI audit #5); the full id
// stays in the copyable transaction field and the explorer links.
function shortTxId(transactionId) {
  const id = String(transactionId || "");
  return id.length > 16 ? `${id.slice(0, 8)}…${id.slice(-4)}` : id;
}

const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

function formatRunDate(isoDate) {
  const match = /^(\d{4})-(\d{2})-(\d{2})$/.exec(String(isoDate || ""));
  if (!match || !MONTHS[Number(match[2]) - 1]) return "an earlier date";
  return `${Number(match[3])} ${MONTHS[Number(match[2]) - 1]} ${match[1]}`;
}

// Every result state writes all of the card's fields, so no state inherits another's label,
// provenance note or links (QA UI audit #1 and #6).
function showResult({ label, summary, txLabel, transactionId, url, linkText, openTransactionId, openLinkText, note }) {
  byId("observer-view").hidden = true;
  byId("result-label").textContent = label;
  byId("record-summary").textContent = summary;
  byId("recorded-tx-label").textContent = txLabel;
  byId("recorded-tx-id").textContent = transactionId;
  const link = byId("explorer-link");
  link.href = url;
  link.textContent = linkText;
  link.setAttribute("aria-label", `${linkText.replace(/ ↗$/, "")}: ${transactionId}`);
  const openLink = byId("open-explorer-link");
  openLink.hidden = !openTransactionId;
  if (openTransactionId) {
    openLink.href = explorerTxUrl(openTransactionId);
    openLink.textContent = openLinkText || "Inspect the channel-open transaction ↗";
  }
  byId("result-note").textContent = note;
  // The result card carries the open link; links from earlier stages must not repeat it.
  byId("open-tx-link").hidden = true;
  if (!readPendingFunding()) byId("pending-funding-link").hidden = true;
  byId("chain-empty").hidden = true;
  byId("chain-result").hidden = false;
}

// The explorer reads this same indexer; a lookup only runs when the visitor asks for it, since
// it tells the indexer operator which transaction this browser is looking at.
const PUBLIC_INDEXER_URL = "https://ootle-indexer-a.tari.com";

function shortHex(value) {
  const hex = String(value || "");
  return hex.length > 16 ? `${hex.slice(0, 8)}…${hex.slice(-4)}` : hex;
}

// Reads only the confidential-transfer statement of a settlement: what an observer can check
// for amounts. Anything unexpected is reported as unreadable rather than guessed.
function settlementPublicFields(transaction) {
  const instructions = transaction?.transaction?.transaction?.V1?.body?.transaction?.instructions || [];
  const transfer = instructions.map((instruction) => instruction?.StealthTransfer).find(Boolean);
  const statement = transfer?.statement;
  if (!statement) throw new Error("The indexer record has no confidential transfer statement.");
  return {
    inputCommitments: (statement.inputs_statement?.inputs || []).map((input) => input.commitment),
    revealedInputAmount: String(statement.inputs_statement?.revealed_amount ?? "unknown"),
    outputCommitments: (statement.outputs_statement?.outputs || []).map((output) => output.output?.commitment),
    revealedOutput: statement.outputs_statement?.revealed_output ?? null,
    revealedInputBucket: transfer.revealed_input_bucket ?? null,
  };
}

function hiddenCell(label, commitments) {
  if (!commitments?.length) return `Hidden (${label})`;
  return `Hidden · ${commitments.map((commitment) => `c: ${shortHex(commitment)}`).join(", ")}`;
}

function observerRows({ live, fields, deposit: depositAmount, calls, paid }) {
  const amountsHidden = fields
    ? fields.revealedInputAmount === "0" && fields.revealedOutput === null && fields.revealedInputBucket === null
    : true;
  const outputsCell = fields
    ? `${hiddenCell("two outputs", fields.outputCommitments)} (payment and change; observers can't tell which)${amountsHidden ? " · no revealed amount" : " · CHECK: an amount field is revealed"}`
    : "Hidden (two outputs; load the public fields to see the commitments)";
  const rows = [
    ["Channel open and settlement", "Visible, with explorer links", "Yes", "Yes"],
    ["Deposit (the channel input)", hiddenCell("input commitment", fields?.inputCommitments),
      live ? `Yes: ${depositAmount} µT (it received the deposit)` : "Knows the declared deposit limit", live ? `Yes: ${depositAmount} µT` : "Yes"],
    ["Paid to the service", outputsCell, `Yes: ${paid} µT`, `Yes: ${paid} µT`],
    ["Number of calls and price", "Not on chain: vouchers stay off-chain", `Yes: ${calls} × ${PRICE} µT`, "Yes"],
    ["Payer change", "Hidden (one of the two outputs above)", live ? `Yes: ${BigInt(depositAmount) - BigInt(paid)} µT` : "Yes",
      live ? `${BigInt(depositAmount) - BigInt(paid)} µT kept for the fixed two-output shape; it stays with this browser's temporary key and is not returned to Sapient` : "Yes"],
    ["Voucher signing key", "No", "No (public key only)", live ? "Yes, and it never leaves this browser" : "Yes, and it stays with the payer"],
  ];
  if (live) rows.push(["Funding transfer", "Visible as a private transfer from your wallet; the deposit amount is not revealed", "Yes", "Yes"]);
  rows.push(["Fees, timing, settle branch, two-output shape", live ? "Visible, including that the relay's account paid the fees" : "Visible", "Yes", "Yes"]);
  return rows;
}

function renderObserverView({ live, settlementTransactionId, fields, deposit: depositAmount, calls, paid, sourceNote }) {
  byId("observer-service-heading").textContent = live ? "🧾 Relay (custodial service)" : "🧾 Service";
  byId("observer-payer-heading").textContent = live ? "🔑 You (this browser)" : "🔑 Payer";
  byId("observer-source").textContent = sourceNote;
  byId("observer-rows").replaceChildren(...observerRows({ live, fields, deposit: depositAmount, calls, paid }).map(([fact, ...cells]) => {
    const row = document.createElement("tr");
    const heading = document.createElement("th");
    heading.scope = "row";
    heading.textContent = fact;
    row.append(heading, ...cells.map((text, index) => {
      const cell = document.createElement("td");
      cell.textContent = text;
      cell.dataset.label = (live ? ["Chain observer", "Relay", "You"] : ["Chain observer", "Service", "Payer"])[index];
      if (index === 0 && /^Hidden|^Not on chain|^No$/.test(text)) cell.className = "is-hidden";
      if (index === 0 && /^Visible/.test(text)) cell.className = "is-visible";
      return cell;
    }));
    return row;
  }));
  byId("observer-leaks").textContent = `What still leaks: fees, timing, the settle branch, the channel id, authorization data, the output commitments and the two-output shape${live ? ", plus the funding transfer and that the relay's account paid the fees" : ""}. Call counts come from the service ledger; the chain does not prove them.`;
  const refusal = byId("observer-refusal");
  refusal.hidden = !lastRefusal;
  if (lastRefusal) {
    refusal.textContent = `Refused off-chain: a ${lastRefusal.amount} µT voucher went over the ${lastRefusal.limit} µT spend limit, so nothing reached the chain. The channel template enforces the same limit at settlement (contract test settle_amount_above_deposit_is_rejected).`;
  }
  const explorer = byId("observer-explorer");
  explorer.href = explorerTxUrl(settlementTransactionId);
  // The result card already links to this settlement. A second button repeats it.
  explorer.hidden = !byId("chain-result").hidden && byId("explorer-link").href === explorer.href;
  byId("observer-load").hidden = !live || Boolean(fields);
  byId("observer-view").hidden = false;
}

function renderLiveObserverView() {
  renderObserverView({
    live: true,
    settlementTransactionId: liveResult.transactionId,
    fields: liveResult.publicFields,
    deposit: liveChannel.deposit,
    calls: liveCalls,
    paid: liveAccepted.toString(),
    sourceNote: liveResult.publicFields
      ? `Public fields of your settlement ${shortHex(liveResult.transactionId)}, read from the Esmeralda indexer the explorer uses.`
      : `Your settlement ${shortHex(liveResult.transactionId)}. Load its public fields to see the commitments an observer sees.`,
  });
}

async function loadLiveObserverFields() {
  if (!liveResult?.transactionId) return;
  const button = byId("observer-load");
  button.disabled = true;
  button.textContent = "Loading public fields…";
  try {
    const response = await fetch(`${PUBLIC_INDEXER_URL}/transactions/${encodeURIComponent(liveResult.transactionId)}`);
    if (!response.ok) throw new Error(`the indexer answered HTTP ${response.status}`);
    liveResult.publicFields = settlementPublicFields(await response.json());
    if (!simulationMode) renderLiveObserverView();
  } catch (error) {
    status(`Could not load the settlement's public fields: ${error instanceof Error ? error.message : String(error)}. The explorer link shows the same record.`);
  } finally {
    button.disabled = false;
    button.textContent = "Load this settlement's public fields";
  }
}

function hideResult() {
  byId("observer-view").hidden = true;
  byId("chain-result").hidden = true;
  byId("chain-empty").hidden = false;
  byId("open-explorer-link").hidden = true;
  byId("chain-panel").classList.remove("is-settling");
}

// The chain panel describes whichever mode the call panel is in; a live channel must not keep
// the simulation's "no transaction" copy (QA run 16 retry 11).
function showChainPanelMode() {
  const live = Boolean(liveChannel) && !simulationMode;
  const openTx = live ? liveChannel.openTransactionId : undefined;
  byId("chain-subtitle").textContent = live
    ? "Live Esmeralda channel · one open and one settlement transaction"
    : SIMULATION_CHAIN_COPY.subtitle;
  byId("chain-empty-title").textContent = live ? "Channel opened on Esmeralda." : SIMULATION_CHAIN_COPY.title;
  byId("chain-empty-text").textContent = live
    ? "Each paid call is an off-chain signed voucher, so calls create no transactions. After five calls the relay settles the cumulative voucher and the settlement appears here."
    : SIMULATION_CHAIN_COPY.text;
  const link = byId("open-tx-link");
  link.hidden = !openTx;
  if (openTx) link.href = explorerTxUrl(openTx);
  byId("quote-note").hidden = !live;
}

function showPendingFunding(transactionId) {
  const link = byId("pending-funding-link");
  link.href = explorerTxUrl(transactionId);
  link.textContent = "Inspect pending Sapient funding transaction ↗";
  link.hidden = false;
}

async function restoreLiveChannel() {
  let saved;
  try { saved = JSON.parse(sessionStorage.getItem(LIVE_CHANNEL_KEY) || "null"); } catch { saved = undefined; }
  if (!saved?.channel?.channelId) return;
  liveChannel = saved.channel;
  byId("live-open").hidden = true;
  liveCalls = Number(saved.calls) || 0;
  liveAccepted = BigInt(saved.accepted || "0");
  accepted = liveAccepted;
  params = {
    networkTag: liveChannel.networkTag,
    payer: hexToBytes(liveChannel.payerPublicKey),
    deposit: BigInt(liveChannel.deposit),
    payerRoot: hexToBytes(liveChannel.payerRoot),
    nonce: hexToBytes(liveChannel.nonce),
  };
  payerSecret = hexToBytes(sessionStorage.getItem("ghostline-live-payer-secret") || "");
  serviceKey = hexToBytes(liveChannel.servicePublicKey);
  byId("service-subtitle").textContent = "Live Esmeralda relay · each accepted call has a real signed voucher";
  byId("live-refund").hidden = false;
  byId("live-stop").hidden = false;
  byId("live-stop").textContent = "Continue with simulation";
  byId("pay").disabled = liveCalls >= countLimit || payerSecret.length !== 32;
  byId("quote").textContent = `Restored live channel · channel id ${liveChannel.channelId} (off-chain identifier, not a transaction) · ${liveCalls}/${countLimit} calls · ${liveAccepted} µT accepted`;
  // The call log is not kept across reloads; show the restored total instead of 0 µT (QA UI audit #3).
  byId("total").textContent = `${liveAccepted} µT`;
  if (liveCalls > 0) {
    const restored = document.createElement("li");
    restored.textContent = `${liveCalls} accepted live ${liveCalls === 1 ? "call" : "calls"} restored from this tab · ${liveAccepted} µT cumulative`;
    byId("calls").replaceChildren(restored);
  }
  showChainPanelMode();
  refreshControls();
  byId("live-note").textContent = payerSecret.length === 32
    ? "Restored this tab’s live channel. Its voucher key remains local; settlement/refund status is checked with the relay."
    : "The live channel was restored, but this tab no longer has its voucher key. You can still request its expiry refund after expiry.";
  try {
    if (liveChannel.recoveryReceipt) {
      const recovered = await relayJson("/demo/recover", {
        method: "POST",
        body: JSON.stringify({ receipt: liveChannel.recoveryReceipt }),
      });
      liveChannel.recoveryReceipt = recovered.recoveryReceipt;
      saveLiveChannel();
    }
    const current = await relayJson(`/demo/status?channel=${encodeURIComponent(liveChannel.channelId)}`);
    if (current.state === "Finalized") {
      showLiveEvidence(current.explorerUrl, current.settlementTransactionId);
      byId("quote").textContent = `Restored your finished live run · ${liveCalls}/${countLimit} calls · ${liveAccepted} µT accepted. Start a new live run or continue with simulation.`;
      status("This tab’s live run already settled. Its result is shown on the right; use “Start a new live run” to begin again.");
    } else if (current.state === "Refunded") {
      liveRefundPending = false;
      liveResult = { kind: "refunded" };
      byId("live-refund").hidden = true;
      byId("live-stop").hidden = false;
      byId("quote").textContent = "Restored a live channel that was already refunded. Start a new live run or continue with simulation.";
      status("This live channel was already refunded. Use “Start a new live run” to begin again.");
    } else if (current.state === "RefundPending") {
      liveRefundPending = true;
      status("The expiry refund has started. Waiting for the private transfer to Sapient to finalize.");
    } else {
      status(liveCalls === countLimit
        ? `Restored open channel ${liveChannel.channelId}. All five calls are paid; settle the channel when ready.`
        : `Restored open channel ${liveChannel.channelId}. Continue the calls or check for expiry refund.`);
    }
  } catch (error) {
    liveStateUnconfirmed = true;
    showRelayBanner("down");
    status(`Live channel ${liveChannel.channelId} is saved in this tab, but the relay could not be reached. Its live actions stay paused until a reload reaches the relay; the simulation works now.`);
  }
  refreshControls();
}

function begin() {
  const keys = createEphemeralChannelKeys(randomBytes, scalarFromWide, publicKeyFromSecret);
  payerSecret = keys.payerSecret;
  serviceKey = keys.serviceOutputPublic;
  params = {
    networkTag: 38,
    payer: keys.payerPublic,
    deposit,
    payerRoot: new Uint8Array(32),
    nonce: randomBytes(32),
  };
  paywall = new SimulatedPaywall(params, serviceKey);
  params.channelId = bytesToHex(channelId(params));
}

function renderCall(fortune, callNumber, voucher) {
  const item = document.createElement("li");
  const label = document.createElement("span");
  label.textContent = `CALL ${String(callNumber).padStart(2, "0")} · ${PRICE} µT`;
  const text = document.createElement("span");
  text.className = "fortune";
  text.textContent = fortune;
  item.append(label, text);
  byId("calls").prepend(item);
  byId("total").textContent = `${voucher.amount.toString()} µT`;
}

// Reset is never a dead end: it resets the simulation, starts over after a finished live run,
// and only refuses while a live channel still holds an open deposit (QA UI audit #2 and #3).
function refreshResetControl() {
  const button = byId("reset");
  const finishedLive = Boolean(liveChannel && liveResult);
  const openLive = Boolean(liveChannel && !liveResult && !simulationMode);
  button.disabled = openLive;
  button.textContent = finishedLive ? "Start a new live run" : openLive ? "Reset (finish the live channel first)" : "Reset simulation";
  if (finishedLive) {
    button.classList.add("button-primary");
    button.classList.remove("button-quiet");
  } else {
    button.classList.add("button-quiet");
    button.classList.remove("button-primary");
  }
}

function refreshControls() {
  refreshResetControl();
  const callsDone = liveChannel && !simulationMode ? liveCalls === countLimit && !liveResult : paywall?.calls === countLimit;
  const live = Boolean(liveChannel && !simulationMode);
  byId("overspend").hidden = !callsDone || (live ? liveOverspendTried : simOverspendTried);
  byId("overspend").disabled = liveRefundPending || (live && liveStateUnconfirmed);
  byId("overspend").textContent = `Try to overspend · sign ${(liveChannel && !simulationMode ? BigInt(liveChannel.deposit) : deposit).toString()} µT, past the spend limit`;
  byId("settle").hidden = Boolean(liveChannel && liveResult && !simulationMode);
  if (liveChannel && !simulationMode) {
    byId("pay").disabled = liveStateUnconfirmed || liveRefundPending || liveCalls >= countLimit;
    byId("pay").textContent = liveCalls >= countLimit ? "Five live calls complete" : `Send live call ${liveCalls + 1} of ${countLimit} · sign voucher`;
    // A paused restore offers the simulation, so its button stays usable (polish-fix retest).
    byId("run-simulation").hidden = !liveStateUnconfirmed;
    byId("run-simulation").disabled = false;
    byId("settle").disabled = liveStateUnconfirmed || liveRefundPending || liveCalls !== countLimit;
    byId("settle").textContent = liveCalls === countLimit ? "Settle live channel" : "Settlement unlocks after 5 calls";
    byId("demo-progress").textContent = `${liveCalls} of ${countLimit} live calls complete · ${liveAccepted.toString()} µT accepted.`;
    return;
  }
  const remaining = countLimit - paywall.calls;
  byId("pay").disabled = !evidenceLoaded || remaining <= 0;
  byId("pay").textContent = remaining <= 0
    ? "Five simulated calls complete"
    : `Run simulated call ${paywall.calls + 1} of ${countLimit} · sign voucher`;
  byId("run-simulation").hidden = false;
  byId("run-simulation").disabled = !evidenceLoaded || remaining <= 0;
  byId("run-simulation").textContent = remaining === countLimit
    ? `Run all ${countLimit} simulated calls`
    : remaining > 0 ? `Finish the remaining ${remaining} calls` : "Simulation complete";
  byId("demo-progress").textContent = `${paywall.calls} of ${countLimit} simulated calls complete · ${accepted.toString()} µT accepted.${paywall.calls === countLimit ? " The current template publish is shown on the right. This simulation did not create it." : " Run the next call or finish the remaining calls in one click."}`;
  byId("settle").disabled = paywall.calls !== countLimit;
  byId("settle").textContent = settlementShown
    ? "Recorded publish shown"
    : paywall.calls === countLimit
      ? "Show the current template publish · separate transaction"
      : `Complete ${countLimit} calls to unlock the recorded publish`;
  if (settlementShown) byId("settle").disabled = true;
}

function askForQuote() {
  const response = paywall.quote();
  byId("quote").textContent = `HTTP ${response.status} · ${response.body.price.toString()} µT · channel ${response.body.channel_id.slice(0, 12)}…`;
}

function payToll() {
  if (liveChannel && !simulationMode) {
    void payLiveToll();
    return;
  }
  askForQuote();
  const amount = accepted + PRICE;
  try {
    const voucher = signVoucher(payerSecret, params, amount, serviceKey);
    const response = paywall.pay(voucher);
    accepted = amount;
    renderCall(response.body.fortune, paywall.calls, voucher);
    status(`The mock service verified voucher ${paywall.calls}/${countLimit}; cumulative total ${accepted.toString()} µT. The voucher was not sent anywhere.`);
    refreshControls();
    if (paywall.calls === countLimit) showRecordedEvidence();
  } catch (error) {
    status(`Voucher rejected: ${error instanceof Error ? error.message : "unknown error"}`);
    refreshControls();
  }
}

function runRemainingSimulation() {
  if (liveChannel && !simulationMode) return;
  const remaining = countLimit - paywall.calls;
  if (remaining <= 0) return;
  for (let index = 0; index < remaining; index += 1) payToll();
  status(`Simulation complete: ${paywall.calls} paid calls, ${accepted.toString()} µT cumulative. The current template publish on the right is a separate transaction. This simulation did not create it.`);
  byId("demo-progress").textContent = `${paywall.calls} of ${countLimit} simulated calls complete · ${accepted.toString()} µT accepted. That open was not created by these clicks.`;
}

function waitForTariProvider() {
  if (window.tari?.request) return Promise.resolve(window.tari);
  status("Waiting for Sapient in this browser profile…");

  return new Promise((resolve, reject) => {
    let timeout;
    const cleanup = () => {
      window.removeEventListener(TARI_PROVIDER_READY_EVENT, onReady);
      window.clearTimeout(timeout);
    };
    const onReady = () => {
      if (!window.tari?.request) return;
      cleanup();
      resolve(window.tari);
    };

    // Sapient may inject its provider after this module has started. Register
    // first, then check again to avoid missing the ready event between checks.
    window.addEventListener(TARI_PROVIDER_READY_EVENT, onReady);
    onReady();
    timeout = window.setTimeout(() => {
      cleanup();
      reject(new Error("No Tari wallet became available in this browser. Open this page in Chrome with Sapient enabled, or use Tari Universe."));
    }, TARI_PROVIDER_TIMEOUT_MS);
    onReady();
  });
}

async function tariRequest(method, params) {
  const tari = await waitForTariProvider();
  try {
    return await tari.request({ method, ...(params ? { params } : {}) });
  } catch (error) {
    if (Number(error?.code) === 4100 && method !== "tari_requestAccounts") {
      await tari.request({ method: "tari_requestAccounts" });
      return tari.request({ method, ...(params ? { params } : {}) });
    }
    throw error;
  }
}

function walletError(error) {
  const code = Number(error?.code);
  if (code === 4001) return "Wallet approval was cancelled. The simulated demo is still available.";
  if (code === 4100) return "The wallet is disconnected. Reconnect it, then try again.";
  if (code === 4200) return "This wallet account does not support a required feature. Check its Sapient capabilities and use the simulated demo meanwhile.";
  if (code === -32603) return error?.message || "The Tari network rejected the request.";
  const message = error instanceof Error ? error.message : String(error);
  if (/Indexer .+ did not respond|Network interface error: Indexer/i.test(message)) {
    return "The Tari testnet indexer did not answer in time. Wait a minute and retry opening the channel; this page will not request another wallet transfer.";
  }
  return message;
}

function isSapientUnavailable(error) {
  return /No Tari wallet (was found|became available)|Could not load Tari's Sapient connector/i.test(walletError(error));
}

function apiBase() {
  const raw = configuredRelayUrl.replace(/\/$/, "");
  if (!raw) throw new Error("Live mode is not configured for this site yet. The instant simulation is available.");
  const url = new URL(raw);
  if (url.protocol !== "https:" && url.hostname !== "localhost" && url.hostname !== "127.0.0.1") {
    throw new Error("The relay URL must use HTTPS.");
  }
  return url.origin;
}

function configureLiveRelay() {
  const button = byId("live-open");
  const note = byId("relay-config-note");
  try {
    apiBase();
    button.disabled = false;
    note.textContent = "Live Esmeralda relay is configured for this site.";
  } catch (error) {
    button.disabled = true;
    note.textContent = error instanceof Error ? error.message : String(error);
  }
}

async function relayJson(path, options = {}) {
  const { timeoutMs: timeoutOption, deadline, ...fetchOptions } = options;
  const method = (fetchOptions.method || "GET").toUpperCase();
  const retryable = method === "GET" || ["/demo/open", "/demo/settle", "/demo/refund", "/demo/recover", "/demo/refund-funding"].some((prefix) => path.startsWith(prefix));
  const requestedTimeoutMs = Number(timeoutOption) > 0 ? Number(timeoutOption) : 100_000;
  let response;
  for (let attempt = 0; ; attempt += 1) {
    // An overall deadline caps every attempt, so the one safe retry cannot double the wait.
    const remainingMs = deadline ? deadline - Date.now() : requestedTimeoutMs;
    if (remainingMs <= 0) throw new Error(`The relay did not finish ${path} in time. Retry; no extra wallet transfer was sent.`);
    const timeoutMs = Math.min(requestedTimeoutMs, remainingMs);
    const controller = new AbortController();
    const timeout = window.setTimeout(() => controller.abort(), timeoutMs);
    try {
      response = await fetch(`${apiBase()}${path}`, {
        ...fetchOptions,
        signal: controller.signal,
        headers: { "content-type": "application/json", ...(fetchOptions.headers || {}) },
      });
      if (response.status < 500 || !retryable || attempt > 0) break;
    } catch (error) {
      if (error?.name === "AbortError") {
        throw new Error(`The relay did not finish ${path} within ${Math.round(timeoutMs / 1000)} seconds. Retry; no extra wallet transfer was sent.`);
      }
      if (!retryable || attempt > 0) throw error;
    } finally {
      window.clearTimeout(timeout);
    }
    if (deadline && deadline - Date.now() < RELAY_RETRY_MIN_MS) {
      if (!response) throw new Error(`The relay did not answer ${path} in time. Retry; no extra wallet transfer was sent.`);
      break;
    }
    status("The relay did not answer. Retrying this safe request once in 3 seconds; your wallet transfer will not be repeated.");
    await new Promise((resolve) => window.setTimeout(resolve, 3000));
  }
  const text = await response.text();
  let body;
  try { body = JSON.parse(text); } catch { body = text; }
  if (!response.ok) {
    const detail = typeof body === "string" ? body : body.error || body.message;
    throw Object.assign(new Error(detail || `Relay returned HTTP ${response.status}`), { status: response.status });
  }
  if (typeof body === "string") throw new Error(body || "Relay returned an empty response.");
  return body;
}

async function relayHealth(url) {
  const deadline = Date.now() + 75_000;
  let attempt = 0;
  while (Date.now() < deadline) {
    attempt += 1;
    const controller = new AbortController();
    const timeout = window.setTimeout(() => controller.abort(), Math.min(10_000, deadline - Date.now()));
    const wakeNotice = window.setTimeout(() => {
      showRelayBanner("waking");
      status(`🕯️ Waking the spirit… (${Math.ceil((Date.now() - (deadline - 75_000)) / 1000)}s). You can switch to the instant simulation at any time.`);
    }, 2000);
    try {
      const response = await fetch(`${url}/healthz`, { mode: "cors", signal: controller.signal });
      if (!response.ok && response.status !== 503) throw new Error(`Relay health returned HTTP ${response.status}.`);
      const health = await response.json();
      if (response.ok && health.ok && health.walletReady) {
        hideRelayBanner();
        return;
      }
      if (response.status >= 500 && response.status !== 503) throw new Error(`Relay health returned HTTP ${response.status}.`);
      showRelayBanner("waking");
      status(`Relay is waking. Check ${attempt}; the instant simulation remains available.`);
    } catch (error) {
      if (error?.name !== "AbortError" && !(error instanceof TypeError)) {
        throw error;
      }
      showRelayBanner("waking");
      status(`Relay is waking. Check ${attempt}; the instant simulation remains available.`);
    } finally {
      window.clearTimeout(timeout);
      window.clearTimeout(wakeNotice);
    }
    await new Promise((resolve) => window.setTimeout(resolve, Math.min(5_000, Math.max(0, deadline - Date.now()))));
  }
  showRelayBanner("down");
  throw new Error("The relay did not become ready within 75 seconds. Continue with the instant simulation or try live mode again later.");
}

async function waitWalletRequest(requestId) {
  for (let attempt = 0; attempt < 60; attempt += 1) {
    if (attempt === 20) {
      status("Still waiting for the Sapient approval. If a lock screen is covering the request, unlock the wallet and approve it. This page will not ask for a second transfer.");
    }
    const result = await tariRequest("tari_getTransactionRequest", { requestId });
    if (result.status !== "pending") {
      if (result.status !== "approved") throw new Error(result.error || `Wallet request ${result.status}`);
      return tariRequest("tari_submitTransactionRequest", { requestId });
    }
    await new Promise((resolve) => window.setTimeout(resolve, 2000));
  }
  throw new Error("Wallet approval timed out. Unlock Sapient if it is locked, then retry from this page.");
}

function transactionLookupMissing(error) {
  return /not found|HTTP 404/i.test(walletError(error));
}

function withTimeout(promise, ms, timeoutError) {
  let timeout;
  return Promise.race([
    promise,
    new Promise((_, reject) => { timeout = window.setTimeout(() => reject(timeoutError), ms); }),
  ]).finally(() => window.clearTimeout(timeout));
}

// A saved transfer is already approved, so its retry asks only the relay: a locked Sapient
// would otherwise hold the lookup open with no visible error (QA run 16 retry 7).
async function waitFinalized(transactionId, { deadline, useWallet = true } = {}) {
  const started = Date.now();
  for (let attempt = 0; Date.now() < deadline; attempt += 1) {
    const waited = Math.max(0, Math.round((Date.now() - started) / 1000));
    status(`Waiting for the funding transfer to finalize (${waited}s). This usually takes under a minute. No second transfer will be requested.`);
    if (useWallet) {
      try {
        const result = await withTimeout(
          tariRequest("tari_getTransactionResult", { transactionId }),
          Math.min(WALLET_LOOKUP_TIMEOUT_MS, Math.max(1, deadline - Date.now())),
          Object.assign(new Error("Sapient did not answer the transaction lookup."), { lookupTimedOut: true }),
        );
        if (result?.Finalized || result?.result?.Finalized) return;
        if (result?.Rejected || result?.result?.Rejected) throw new Error("The funding transaction was rejected by Ootle.");
      } catch (error) {
        if (!error?.lookupTimedOut && !transactionLookupMissing(error)) throw error;
        if (attempt === 0 || attempt === 8) {
          status("Checking the indexer transaction record through the relay; no second transfer.");
        }
      }
    }
    try {
      const indexed = await relayJson(`/demo/tx-status?id=${encodeURIComponent(transactionId)}`, { timeoutMs: 10_000, deadline });
      if (indexed.state === "Finalized") return;
      if (indexed.state === "Rejected" || indexed.state === "FinalizedAbort") {
        throw new Error("The funding transaction was rejected by Ootle.");
      }
    } catch (error) {
      const detail = walletError(error);
      if (/rejected by Ootle/i.test(detail)) throw error;
    }
    await new Promise((resolve) => window.setTimeout(resolve, 2000));
  }
  throw new Error(`Funding transaction ${transactionId} is still pending. Keep its id and retry after it finalizes.`);
}

// The relay waits briefly for its wallet to discover the deposit; keep asking until the
// overall deadline so a slow wallet scan does not need a manual retry.
async function openFundedChannel(pending, deadline) {
  const started = Date.now();
  for (;;) {
    try {
      return await relayJson("/demo/open", {
        method: "POST",
        timeoutMs: RELAY_OPEN_TIMEOUT_MS,
        deadline,
        body: JSON.stringify({
          fundingTxId: pending.fundingTxId,
          sessionId: pending.sessionId,
        }),
      });
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      const collided = OUTPUT_COLLISION_EXHAUSTED.test(message);
      if (!collided && !FUNDING_NOT_DISCOVERED.test(message)) throw error;
      if (deadline - Date.now() < RELAY_RETRY_MIN_MS) {
        if (collided) throw error;
        throw new Error("The relay wallet has not discovered your finalized deposit yet. Esmeralda wallet scans can take up to about 10 minutes; retry opening the channel later. No second transfer will be requested");
      }
      const waited = Math.round((Date.now() - started) / 1000);
      status(collided
        ? `The relay is stepping past outputs its restarted wallet recreated (${waited}s). No second transfer will be requested.`
        : `The relay is opening your channel (${waited}s). Its wallet usually finds the deposit in about a minute. No second transfer will be requested.`);
      if (!collided) await new Promise((resolve) => window.setTimeout(resolve, 5000));
    }
  }
}

// TokenBalance amounts are raw units of 10^-divisibility TARI. µT is 10^-6 TARI.
// A missing divisibility is 6, which is native TARI's exponent; the caller still prints the raw fields.
function rawToMicroTari(raw, divisibility) {
  const value = BigInt(raw);
  const places = divisibility === undefined || divisibility === null || divisibility === "" ? 6 : Number(divisibility);
  if (!Number.isInteger(places) || places < 0 || places > 12) {
    throw new Error(`Sapient reported divisibility ${divisibility}, which this page cannot convert to µT.`);
  }
  const scale = 10n ** BigInt(places);
  const micro = 1_000_000n;
  if (places <= 6) return value * (micro / scale);
  if (value % (scale / micro) !== 0n) {
    throw new Error(`Sapient reported ${raw} at divisibility ${places}, which is not a whole number of µT.`);
  }
  return value / (scale / micro);
}

function clearLiveError() {
  const details = byId("live-error-details");
  details.hidden = true;
  details.open = false;
  byId("live-error-raw").textContent = "";
}

function showLiveError(raw) {
  const details = byId("live-error-details");
  byId("live-error-raw").textContent = raw;
  details.hidden = false;
  details.open = false;
}

// A wallet answer after the approval screen is not "still waiting". The long engine text stays behind the toggle.
function depositFailure(stage, detail) {
  if (/InsufficientFees|Insufficient fees|accepted the fee but rejected/i.test(detail)) {
    return {
      summary: "The network rejected the deposit because the fee could not cover private verification. The 51 µT was not spent. A fee may still have been charged.",
      raw: detail,
    };
  }
  if (stage === "waiting for Sapient approval") {
    const summary = detail.length > 220 ? `${detail.slice(0, 200)}…` : detail;
    return {
      summary: `Sapient rejected the deposit after approval: ${summary}`,
      raw: detail,
    };
  }
  return { summary: `Live setup stopped while ${stage}: ${detail.length > 220 ? `${detail.slice(0, 200)}…` : detail}`, raw: detail };
}

async function connectAndOpenLive() {
  let setupStage = "checking relay configuration";
  liveSetupCancelled = false;
  liveOpenInFlight = true;
  byId("live-open").disabled = true;
    byId("live-open").textContent = "Checking Sapient…";
    clearLiveError();
  byId("live-stop").hidden = false;
  byId("live-stop").textContent = "Use simulated mode instead";
  try {
    const url = apiBase();
    setupStage = "checking relay readiness";
    status("Checking that the Esmeralda relay and wallet are ready before opening Sapient.");
    await relayHealth(url);
    if (liveSetupCancelled) return;
    setupStage = "loading live demo settings";
    const relay = await relayJson("/demo/config");
    byId("funding-requirement").textContent = `Use Sapient 0.7.1 or newer. Switch it to Esmeralda. The deposit is ${relay.deposit} µT of private TARI. The wallet fee is paid from public TARI. Sapient shows a Max fee of 0.02 TARI; that is a cap, and the charge can be lower. Sapient 0.7.0 reports a private balance of 0 and rejects the send. A private-view prompt appears only when this site does not already have one.`;
    let pending = readPendingFunding();
    const retryingSavedTransfer = Boolean(pending?.fundingTxId);
    if (!pending?.fundingTxId) {
      setupStage = "checking the wallet network";
      const network = await tariRequest("tari_getNetwork");
      if (String(network).toLowerCase() !== "esmeralda") throw new Error(`Switch your Tari wallet to Esmeralda. Current network: ${network}`);
      setupStage = "connecting Sapient account";
      await tariRequest("tari_requestAccounts");
      setupStage = "checking Sapient capabilities";
      const caps = await tariRequest("tari_getCapabilities");
      if (!caps.privateSpend || !caps.transactionRequests || !caps.walletAddress) {
        throw new Error("This wallet account needs private send, transaction requests, and wallet-address support.");
      }
      setupStage = "reading the refund address";
      const refundAddress = await tariRequest("tari_getWalletAddress");
      if (typeof refundAddress !== "string" || !refundAddress.startsWith("otl_esm_1")) {
        throw new Error("Could not read an Esmeralda Sapient wallet address for private refunds.");
      }
      setupStage = "checking the testnet balance";
      // Without a private view, Sapient reports confidentialAmount as "0" even when the wallet holds stealth TARI.
      let viewGranted = caps.privateViewGranted === true;
      if (!viewGranted) {
        if (caps.privateBalanceView === false) {
          throw new Error(`This Sapient account cannot show a private balance to the page. ${relay.deposit} µT plus a wallet fee is required before approval. No deposit was requested.`);
        }
        setupStage = "requesting a private balance view";
        try {
          const access = await tariRequest("tari_requestViewAccess");
          viewGranted = access?.granted === true;
        } catch (error) {
          if (Number(error?.code) === 4001) throw error;
          viewGranted = false;
        }
        setupStage = "checking the testnet balance";
      }
      let balances;
      try {
        balances = await tariRequest("tari_getBalances");
      } catch {
        balances = undefined;
      }
      const xtr = Array.isArray(balances) ? balances.find((balance) => balance.resourceAddress === NATIVE_TARI_RESOURCE) : undefined;
      const rawBalance = xtr
        ? `raw amount ${xtr.amount ?? "missing"}, confidentialAmount ${xtr.confidentialAmount ?? "missing"}, divisibility ${xtr.divisibility ?? "missing"}`
        : "no native TARI balance row";
      if (!viewGranted) {
        throw new Error(`Sapient did not grant a private balance view, so confidentialAmount is not the spendable private balance (${rawBalance}). Approve the view prompt in Sapient, then retry. ${relay.deposit} µT plus a wallet fee is required before approval. No deposit was requested.`);
      }
      if (!xtr || xtr.confidentialAmount === undefined || xtr.confidentialAmount === null || xtr.confidentialAmount === "") {
        throw new Error(`Sapient granted a private view but did not return confidentialAmount (${rawBalance}). No deposit was requested.`);
      }
      // sendPrivately spends stealth outputs. Revealed balance cannot pay this deposit.
      const privateBalance = rawToMicroTari(xtr.confidentialAmount, xtr.divisibility);
      const revealedBalance = rawToMicroTari(xtr.amount || "0", xtr.divisibility);
      // Sapient pays this fee from public TARI. The 51 µT deposit is the private spend.
      // A 12,000 µT cap was rejected on 2026-11-03: verification needed 15,687 µT of payment.
      const fundingMaxFee = 20000n;
      const reported = `Sapient reports revealed ${revealedBalance} µT and private ${privateBalance} µT (${rawBalance})`;
      if (privateBalance < BigInt(relay.deposit)) {
        const message = `The deposit needs ${relay.deposit} µT of private TARI. Revealed TARI cannot pay this private deposit. ${reported}.`;
        byId("live-note").textContent = message;
        throw new Error(message);
      }
      if (revealedBalance < fundingMaxFee) {
        const message = `Public balance is short. The wallet fee is paid from public TARI and the cap is ${fundingMaxFee} µT. No deposit was requested. ${reported}.`;
        byId("live-note").textContent = message;
        throw new Error(message);
      }
      const channelKeys = createEphemeralChannelKeys(randomBytes, scalarFromWide, publicKeyFromSecret);
      payerSecret = channelKeys.payerSecret;
      sessionStorage.setItem("ghostline-live-payer-secret", bytesToHex(payerSecret));
      setupStage = "creating a relay session";
      const relaySession = await relayJson("/demo/session", {
        method: "POST",
        body: JSON.stringify({ payerPublicKey: bytesToHex(channelKeys.payerPublic), refundAddress }),
      });
      byId("live-note").textContent = `Approve a private send of ${relaySession.deposit} µT to the relay. Scroll the Sapient popup if Approve & Sign is below the fold; its balance may stay blurred for a few seconds. Max fee 0.02 TARI is a ceiling paid from public TARI, and the charge can be lower. Five calls spend 50 µT; Ghostline reserves 1 µT for its fixed-shape change output. The relay opens the channel, then can settle or return the full deposit privately if it expires unsettled.`;
      setupStage = "waiting for Sapient approval";
      const memo = fundingMemo(relaySession.sessionId, bytesToHex(channelKeys.payerPublic), refundAddress);
      if (memo.length > 253) throw new Error("The encrypted funding memo is too long for a stealth output. Retry live setup.");
      const operation = await tariRequest("tari_createTransactionRequest", {
        kind: "sendPrivately",
        resourceAddress: NATIVE_TARI_RESOURCE,
        recipientWalletAddress: relaySession.fundingAddress,
        amount: String(relaySession.deposit),
        maxFee: "20000",
        memo,
      });
      status("Approve the private testnet transfer in Sapient. Scroll the popup if Approve & Sign is below the fold. The balance may stay blurred for a few seconds. Max fee 0.02 TARI is a cap, not the charge.");
      byId("live-open").textContent = "Waiting for Sapient approval…";
      const funding = await waitWalletRequest(operation.requestId);
      if (!funding?.transactionId) throw new Error("Sapient did not return the funding transaction id.");
      pending = {
        sessionId: relaySession.sessionId,
        fundingTxId: funding.transactionId,
        payerPublicKey: bytesToHex(channelKeys.payerPublic),
        payerSecretHex: bytesToHex(payerSecret),
      };
      savePendingFunding(pending);
      showPendingFundingControls(pending);
      byId("live-open").textContent = "Waiting for finality…";
      byId("live-note").textContent = "Sapient approved the private transfer. Finality usually takes under a minute. The relay then opens the channel; its wallet usually finds the deposit in about a minute and can take up to about two minutes. No second transfer will be requested.";
      if (liveSetupCancelled) {
        status("The approved funding transaction is saved in this browser. Continue live setup later to open its channel without sending again.");
        return;
      }
    } else {
      payerSecret = restorePayerSecret(pending);
      showPendingFundingControls(pending);
      if (payerSecret.length !== 32) throw new Error("This browser has a funded transfer saved, but its local voucher-signing key is unavailable in this tab. Return the pending deposit to Sapient, or abandon the saved transfer to start over. Do not send another deposit.");
      byId("live-open").textContent = "Retrying relay open…";
      byId("live-note").textContent = "Retrying the open for your saved, approved transfer. The relay wallet usually finds the deposit in about a minute. No second transfer will be requested.";
      status("Retrying the relay open for your existing finalized funding transaction; no second wallet transfer will be requested.");
      showPendingFunding(pending.fundingTxId);
    }
    setupStage = "waiting for funding finality";
    const openDeadline = Date.now() + LIVE_OPEN_DEADLINE_MS;
    await waitFinalized(pending.fundingTxId, { deadline: openDeadline, useWallet: !retryingSavedTransfer });
    if (liveSetupCancelled) {
      status(`Funding transaction ${pending.fundingTxId} is finalized and saved in this browser. Continue live setup later to open its channel without paying again.`);
      return;
    }
    setupStage = "opening the funded channel through the relay";
    byId("pending-funding-link").textContent = "Inspect your finalized Sapient funding transaction ↗";
    byId("live-open").textContent = "Opening channel…";
    status("Funding is finalized. The relay is opening your channel. Its wallet usually finds the deposit in about a minute, and this can take up to about two minutes. No second transfer will be requested.");
    const opened = await openFundedChannel(pending, openDeadline);
    clearPendingFunding();
    showPendingFundingControls();
    params = {
      networkTag: opened.networkTag,
      payer: hexToBytes(pending.payerPublicKey),
      deposit: BigInt(opened.deposit),
      payerRoot: hexToBytes(opened.payerRoot),
      nonce: hexToBytes(opened.nonce),
    };
    setupStage = "checking channel parameters in this browser";
    const computedId = bytesToHex(channelId(params));
    if (computedId.toLowerCase() !== opened.channelId.toLowerCase()) throw new Error("Relay channel parameters failed the browser-side channel-id check.");
    liveChannel = opened;
    simulationMode = false;
    byId("live-open").hidden = true;
    liveChannel.payerPublicKey = pending.payerPublicKey;
    liveCalls = 0;
    liveAccepted = 0n;
    liveOverspendTried = false;
    lastRefusal = undefined;
    saveLiveChannel();
    serviceKey = hexToBytes(opened.servicePublicKey);
    byId("service-subtitle").textContent = "Live Esmeralda relay · each accepted call has a real signed voucher";
    byId("quote").textContent = `Channel id ${opened.channelId} (off-chain identifier, not a transaction) · expires at epoch ${opened.expiryEpoch}`;
    byId("live-note").textContent = `Channel open through expiry epoch ${opened.expiryEpoch}. Custodial testnet flow: five calls can settle 50 µT; 1 µT remains in Ghostline’s fixed-shape change output. If left unsettled, the relay can return the full deposit privately after expiry. Your voucher key stays in this browser.`;
    byId("live-stop").hidden = false;
    byId("live-stop").textContent = "Continue with simulation";
    byId("live-refund").hidden = false;
    liveResult = undefined;
    hideResult();
    showChainPanelMode();
    refreshControls();
    status("Channel finalized. Each click now sends a real signed voucher to the relay.");
  } catch (error) {
    liveOpenInFlight = false;
    if (setupStage === "checking relay readiness") showRelayBanner("down");
    const pending = readPendingFunding();
    const cancelled = Number(error?.code) === 4001;
    byId("live-stop").hidden = false;
    byId("live-stop").textContent = "Continue with simulation";
    if (pending) showPendingFundingControls(pending);
    if (cancelled) {
      byId("live-stop").hidden = false;
      byId("live-stop").textContent = "Continue with simulation";
      byId("live-open").textContent = pending ? "Retry opening channel (no new charge)" : "Connect Sapient and open channel";
      byId("live-note").textContent = pending
        ? "Wallet approval was cancelled. If Sapient is locked, unlock it, then retry opening the saved channel (no new charge)."
        : "Wallet approval was cancelled. If Sapient is locked, unlock it first. No funding transaction was submitted.";
      status(walletError(error));
    } else if (!pending && isSapientUnavailable(error)) {
      showRelayBanner("sapient-missing");
      byId("live-open").textContent = "Retry Sapient connection";
      byId("live-stop").textContent = "Start instant simulation";
      byId("live-note").textContent = "Sapient wasn't detected in this tab. Reload this page in the Chrome profile where Sapient is enabled, or open it in Tari Universe, then retry. No wallet transfer was requested.";
      status("No Tari wallet became available. Reload this page in the Chrome profile where Sapient is enabled, then retry.");
    } else {
      byId("live-open").textContent = "Retry live setup";
      const detail = walletError(error).replace(/\.+$/, "");
      const failure = depositFailure(setupStage, detail);
      const tail = pending
        ? "Your approved funding transaction is saved in this browser; retrying will not request another transfer."
        : "No channel was opened. The instant simulation remains available.";
      byId("live-note").textContent = `${failure.summary} ${tail}`;
      status(failure.summary);
      if (failure.raw !== failure.summary) showLiveError(failure.raw);
    }
  } finally {
    liveOpenInFlight = false;
    byId("live-open").disabled = false;
    const pending = readPendingFunding();
    if (pending && !liveChannel) {
      byId("refund-pending-funding").hidden = false;
      byId("abandon-pending-funding").hidden = false;
    }
    if (LIVE_OPEN_PROGRESS_LABELS.has(byId("live-open").textContent)) {
      byId("live-open").textContent = pending ? "Retry opening channel (no new charge)" : "Connect Sapient and open channel";
    }
  }
}

async function payLiveToll() {
  byId("pay").disabled = true;
  try {
    const channelHex = bytesToHex(channelId(params));
    const quoteResponse = await fetch(`${apiBase()}/fortune`, { headers: { "ghostline-channel": channelHex } });
    if (quoteResponse.status !== 402) throw new Error(`Expected an HTTP 402 quote, got ${quoteResponse.status}.`);
    const quote = await quoteResponse.json();
    if (quote.scheme !== "ghostline.channel.voucher.v1" || quote.channel_id.toLowerCase() !== channelHex.toLowerCase()) throw new Error("The service returned a quote for a different channel or payment scheme.");
    const outputKeyHex = quote.service_output_key;
    if (!outputKeyHex) throw new Error("The quote omitted its one-time settlement-output key.");
    const total = liveAccepted + BigInt(quote.price);
    const outputKey = hexToBytes(outputKeyHex);
    const voucher = signVoucher(payerSecret, params, total, outputKey);
    const paidHeaders = {
        "ghostline-channel": channelHex,
        "ghostline-voucher": bytesToHex(voucherToBytes(voucher)),
        ...(liveChannel.recoveryReceipt ? { "ghostline-recovery": liveChannel.recoveryReceipt } : {}),
      };
    let paid;
    for (let attempt = 0; ; attempt += 1) {
      try {
        paid = await fetch(`${apiBase()}/fortune`, { headers: paidHeaders });
        if (paid.status < 500 || attempt > 0) break;
      } catch (error) {
        if (attempt > 0) throw error;
      }
      status("The paid call response was lost. Retrying the same signed voucher once in 3 seconds.");
      await new Promise((resolve) => window.setTimeout(resolve, 3000));
    }
    const body = await paid.json();
    if (!paid.ok) throw new Error(body.reason || `Paid call returned HTTP ${paid.status}.`);
    const recoveryReceipt = paid.headers.get("ghostline-recovery");
    if (!recoveryReceipt) throw new Error("The service accepted this call but did not return its updated recovery receipt. Retry the same call before closing this tab.");
    liveAccepted = total;
    liveCalls += 1;
    accepted = total;
    liveChannel.recoveryReceipt = recoveryReceipt;
    saveLiveChannel();
    renderCall(body.fortune, liveCalls, voucher);
    byId("quote").textContent = `Live voucher accepted · ${liveCalls}/5 · cumulative ${liveAccepted} µT`;
    status(`Ootle has not seen this call. The service accepted voucher ${liveCalls}/5; cumulative amount ${liveAccepted} µT.`);
    // Settlement waits for a click so the overspend attempt can run against the open channel.
    if (liveCalls === countLimit) status("Five live calls complete. Settle the channel, or first try to overspend and watch the service refuse it.");
  } catch (error) {
    if (liveChannel && !simulationMode) returnToSimulation();
    byId("live-stop").hidden = false;
    byId("live-stop").textContent = "Continue with simulation";
    status(`Live call failed: ${walletError(error)} Showing the instant simulation. The open live channel, recovery receipt and voucher key remain saved for retry or expiry refund.`);
  } finally {
    refreshControls();
  }
}

function renderRefusal(amount, limit, reason) {
  lastRefusal = { amount, limit, reason };
  const item = document.createElement("li");
  item.className = "refused";
  const label = document.createElement("span");
  label.textContent = `REFUSED · ${amount} µT`;
  const text = document.createElement("span");
  text.className = "fortune";
  text.textContent = `✖ Refused: a voucher for ${amount} µT exceeds the ${limit} µT spend limit. The service answered "${reason}". Accepted total stays ${accepted} µT.`;
  item.append(label, text);
  byId("calls").prepend(item);
  status(`The service refused the voucher: "${reason}". Nothing reached the chain, and the accepted total is unchanged.`);
}

// Signs the whole deposit. The browser signer refuses anything above the deposit, so the
// smallest overspend that reaches the service is the 1 µT reserved for the fixed-shape settlement.
function trySimulatedOverspend() {
  simOverspendTried = true;
  const amount = params.deposit;
  const voucher = signVoucher(payerSecret, params, amount, serviceKey);
  try {
    paywall.pay(voucher);
    status("The simulated service accepted an over-limit voucher. This is a bug; please report it.");
  } catch (error) {
    renderRefusal(amount, amount - 1n, error instanceof Error ? error.message : String(error));
    if (settlementShown && evidence?.chainRecord?.settlementPublicFields) renderRecordedObserverView();
  } finally {
    refreshControls();
  }
}

async function tryLiveOverspend() {
  liveOverspendTried = true;
  byId("overspend").disabled = true;
  try {
    const channelHex = bytesToHex(channelId(params));
    const quoteResponse = await fetch(`${apiBase()}/fortune`, { headers: { "ghostline-channel": channelHex } });
    if (quoteResponse.status !== 402) throw new Error(`Expected an HTTP 402 quote, got ${quoteResponse.status}.`);
    const quote = await quoteResponse.json();
    if (quote.channel_id?.toLowerCase() !== channelHex.toLowerCase() || !quote.service_output_key) throw new Error("The service returned a quote for a different channel.");
    const amount = params.deposit;
    const voucher = signVoucher(payerSecret, params, amount, hexToBytes(quote.service_output_key));
    const response = await fetch(`${apiBase()}/fortune`, {
      headers: { "ghostline-channel": channelHex, "ghostline-voucher": bytesToHex(voucherToBytes(voucher)) },
    });
    const body = await response.json().catch(() => ({}));
    if (response.ok) {
      status("The live service accepted an over-limit voucher. This is a bug; do not settle, and report it.");
      return;
    }
    renderRefusal(amount, amount - 1n, body.reason || `HTTP ${response.status}`);
  } catch (error) {
    liveOverspendTried = false;
    status(`The overspend attempt could not reach the service: ${walletError(error)} Your five accepted calls are unchanged.`);
  } finally {
    refreshControls();
  }
}

function tryOverspend() {
  if (liveChannel && !simulationMode) void tryLiveOverspend();
  else trySimulatedOverspend();
}

async function settleLive() {
  byId("settle").disabled = true;
  byId("chain-panel").classList.add("is-settling");
  status("The relay is submitting the cumulative voucher settlement to Esmeralda.");
  const result = await relayJson(`/demo/settle?channel=${encodeURIComponent(liveChannel.channelId)}`, { method: "POST" });
  for (let attempt = 0; result.state === "Pending" && attempt < 60; attempt += 1) {
    await new Promise((resolve) => window.setTimeout(resolve, 2000));
    Object.assign(result, await relayJson(`/demo/status?channel=${encodeURIComponent(liveChannel.channelId)}`));
  }
  if (result.state !== "Finalized") throw new Error(`Settlement status: ${result.state}. Keep channel ${liveChannel.channelId} and retry later.`);
  if (result.recoveryReceipt) {
    liveChannel.recoveryReceipt = result.recoveryReceipt;
  } else if (liveChannel.recoveryReceipt) {
    const recovered = await relayJson("/demo/recover", {
      method: "POST",
      body: JSON.stringify({ receipt: liveChannel.recoveryReceipt }),
    });
    liveChannel.recoveryReceipt = recovered.recoveryReceipt;
  }
  saveLiveChannel();
  showLiveEvidence(result.explorerUrl, result.settlementTransactionId);
}

async function requestLiveRefund() {
  if (!liveChannel) return;
  byId("live-refund").disabled = true;
  try {
    let result = await relayJson(`/demo/refund?channel=${encodeURIComponent(liveChannel.channelId)}`, { method: "POST" });
    for (let attempt = 0; result.state === "pending" && attempt < 60; attempt += 1) {
      liveRefundPending = true;
      refreshControls();
      await new Promise((resolve) => window.setTimeout(resolve, 2000));
      result = await relayJson(`/demo/refund?channel=${encodeURIComponent(liveChannel.channelId)}`, { method: "POST" });
    }
    if (result.state !== "refunded") throw new Error(`Refund state is ${result.state}; you can retry this check later.`);
    if (result.recoveryReceipt) {
      liveChannel.recoveryReceipt = result.recoveryReceipt;
    } else if (liveChannel.recoveryReceipt) {
      const recovered = await relayJson("/demo/recover", {
        method: "POST",
        body: JSON.stringify({ receipt: liveChannel.recoveryReceipt }),
      });
      liveChannel.recoveryReceipt = recovered.recoveryReceipt;
    }
    saveLiveChannel();
    liveRefundPending = false;
    liveResult = { kind: "refunded", url: result.explorerUrl, transactionId: result.refundTransactionId };
    showLiveRefund();
    refreshControls();
    status("The channel expired and the private refund transfer finalized. Check Sapient for the returned testnet funds.");
    byId("live-refund").hidden = true;
    byId("live-stop").hidden = false;
  } catch (error) {
    if (!liveRefundPending) refreshControls();
    if (liveChannel && !simulationMode) returnToSimulation();
    byId("live-stop").hidden = false;
    byId("live-stop").textContent = "Continue with simulation";
    status(`Refund check failed: ${error instanceof Error ? error.message : String(error)} Showing the instant simulation; the live channel and recovery receipt remain saved so you can retry after expiry.`);
  } finally {
    byId("live-refund").disabled = false;
  }
}

function showLiveRefund() {
  if (!liveResult?.transactionId) {
    hideResult();
    return;
  }
  showResult({
    label: "YOUR LIVE RUN · REFUND FINALIZED",
    summary: `The relay returned the unspent ${liveChannel.deposit} µT channel deposit privately to your Sapient wallet.`,
    txLabel: "Refund transaction",
    transactionId: liveResult.transactionId,
    url: liveResult.url,
    linkText: "Inspect the private refund transfer ↗",
    openTransactionId: liveChannel.openTransactionId,
    note: "Your live channel created this refund after expiry. The relay indexer reports it finalized; check Sapient for the returned testnet funds.",
  });
}

function showLiveSettlement() {
  const openTx = liveChannel.openTransactionId;
  showResult({
    label: "YOUR LIVE RUN · SETTLEMENT FINALIZED",
    summary: `Your live run: ${liveCalls} paid calls · ${liveAccepted} µT cumulative${openTx ? ` · open ${shortTxId(openTx)}` : ""} · settlement ${shortTxId(liveResult.transactionId)}.`,
    txLabel: "Settlement transaction",
    transactionId: liveResult.transactionId,
    url: liveResult.url,
    linkText: "Inspect your live Esmeralda settlement ↗",
    openTransactionId: openTx,
    openLinkText: "Inspect your channel-open transaction ↗",
    note: `Your browser and Sapient created this channel, these calls and this settlement on Esmeralda. The service knows the requests, prices and vouchers; observers can still see transaction metadata and fees. The chain does not publish the off-chain call count.${liveResult.sapientUnconfirmed ? " Sapient has not independently reported finality yet; the relay indexer reports Finalized." : ""}`,
  });
  renderLiveObserverView();
}

function showLiveEvidence(url, transactionId) {
  liveRefundPending = false;
  byId("chain-panel").classList.remove("is-settling");
  liveResult = { kind: "settled", url, transactionId };
  showLiveSettlement();
  void verifySapientFinality(transactionId);
  refreshControls();
  byId("live-refund").hidden = true;
  byId("live-stop").hidden = false;
  byId("live-stop").textContent = "Continue with simulation";
  status("Live settlement finalized. The explorer link below is from this run.");
}

async function verifySapientFinality(transactionId) {
  try {
    const capabilities = await tariRequest("tari_getCapabilities");
    if (!capabilities.transactionResultLookup) return;
    const result = await tariRequest("tari_getTransactionResult", { transactionId });
    if (!(result?.Finalized || result?.result?.Finalized) && liveResult?.transactionId === transactionId) {
      liveResult.sapientUnconfirmed = true;
      if (!simulationMode) showLiveSettlement();
    }
  } catch {
    // The relay indexer already checked finality; this wallet lookup is an optional cross-check.
  }
}

function returnToSimulation() {
  if (!liveChannel) {
    liveSetupCancelled = true;
    byId("live-stop").hidden = true;
    simulationMode = false;
    byId("service-subtitle").textContent = "Instant browser simulation · no wallet or testnet funds needed";
    reset();
    status("Simulation ready. Use “Ask and pay the toll” below; this will not contact Sapient or send a transaction.");
    byId("demo-title").scrollIntoView?.({ behavior: "smooth", block: "start" });
    return;
  }
  simulationMode = !simulationMode;
  lastRefusal = undefined;
  if (simulationMode) {
    simOverspendTried = false;
    begin();
    paywall = new SimulatedPaywall(params, serviceKey);
    accepted = 0n;
    byId("calls").replaceChildren();
    byId("total").textContent = "0 µT";
    byId("quote").textContent = "Ask for a quote to receive a simulated HTTP 402.";
    // The simulation must not sit next to the live result (QA UI audit #4, run 17 step 7).
    settlementShown = false;
    hideResult();
    byId("service-subtitle").textContent = "Separate browser simulation · the saved live channel was not changed";
    byId("live-stop").textContent = liveResult ? "Return to your live result" : "Return to live session";
    byId("live-stop").hidden = false;
    status(liveResult
      ? "Simulation is separate from your finished live run, which stays saved in this tab."
      : "Simulation is separate. Your live channel, recovery receipt, and local voucher key are still saved; settle it or claim the expiry refund later.");
  } else {
    params = {
      networkTag: liveChannel.networkTag,
      payer: hexToBytes(liveChannel.payerPublicKey),
      deposit: BigInt(liveChannel.deposit),
      payerRoot: hexToBytes(liveChannel.payerRoot),
      nonce: hexToBytes(liveChannel.nonce),
    };
    payerSecret = hexToBytes(sessionStorage.getItem("ghostline-live-payer-secret") || "");
    serviceKey = hexToBytes(liveChannel.servicePublicKey);
    byId("live-stop").textContent = "Continue with simulation";
    byId("service-subtitle").textContent = "Live Esmeralda relay · each accepted call has a real signed voucher";
    accepted = liveAccepted;
    byId("total").textContent = `${liveAccepted} µT`;
    byId("calls").replaceChildren();
    byId("quote").textContent = `Live channel · ${liveCalls}/${countLimit} calls · ${liveAccepted} µT accepted`;
    if (liveResult?.kind === "settled") showLiveSettlement();
    else if (liveResult?.kind === "refunded") showLiveRefund();
    else hideResult();
    status("Back in the live session. Its channel and local voucher key remain saved.");
  }
  showChainPanelMode();
  refreshControls();
  byId("demo-title").scrollIntoView?.({ behavior: "smooth", block: "start" });
}

async function refundPendingFunding() {
  const pending = readPendingFunding();
  if (!pending) {
    showPendingFundingControls();
    status("There is no pending funded transfer to return in this browser.");
    return;
  }
  const button = byId("refund-pending-funding");
  button.disabled = true;
  button.textContent = "Checking pending deposit…";
  const progress = window.setTimeout(() => {
    status("Still checking the pending deposit with the relay. This can take up to about 90 seconds; the page will report an error if it stalls.");
  }, 8000);
  try {
    const result = await relayJson("/demo/refund-funding", {
      method: "POST",
      timeoutMs: 100_000,
      body: JSON.stringify({ fundingTxId: pending.fundingTxId, sessionId: pending.sessionId }),
    });
    if (result.state === "pending") {
      showPendingFunding(result.refundTransactionId);
      byId("pending-funding-link").textContent = "Inspect pending private refund transaction ↗";
      status(`The private refund is pending. Keep this page open and retry; transaction ${result.refundTransactionId}.`);
      button.textContent = "Check pending deposit refund";
      button.disabled = false;
      return;
    }
    clearPendingFunding();
    showPendingFundingControls();
    showResult({
      label: "YOUR DEPOSIT · REFUND FINALIZED",
      summary: `The relay returned the un-opened ${result.deposit} µT deposit privately to the Sapient address bound to this session.`,
      txLabel: "Refund transaction",
      transactionId: result.refundTransactionId,
      url: result.explorerUrl,
      linkText: "Inspect the pending-deposit refund ↗",
      note: "No channel was opened from this deposit. The relay indexer reports the private refund as finalized; check Sapient for the returned testnet funds.",
    });
    status("The pending deposit was privately returned to the Sapient wallet. Check the wallet after finality.");
  } catch (error) {
    button.textContent = "Retry pending deposit refund";
    status(`Could not return the pending deposit: ${error instanceof Error ? error.message : String(error)}. The saved transaction remains available to retry.`);
  } finally {
    window.clearTimeout(progress);
    button.disabled = false;
  }
}

function renderRecordedObserverView() {
  const chain = evidence.chainRecord;
  if (!chain?.settlementPublicFields) return;
  const service = evidence.serviceRecord || {};
  renderObserverView({
    live: false,
    settlementTransactionId: chain.settlementTransactionId,
    fields: chain.settlementPublicFields,
    calls: service.successfulCalls ?? countLimit,
    paid: String(service.acceptedTotalMicroTari ?? Number(PRICE) * countLimit),
    sourceNote: `This is the recorded ${formatRunDate(evidence.runDate)} command-line run, not your clicks. The public fields of settlement ${shortHex(chain.settlementTransactionId)} were read from the Esmeralda indexer the explorer uses.`,
  });
}

function showRecordedEvidence() {
  if (!evidence?.templateRecord || settlementShown) {
    if (settlementShown) return;
    status("Could not load the current Esmeralda record. Refresh and try again.");
    return;
  }
  settlementShown = true;
  const template = evidence.templateRecord;
  const runDate = formatRunDate(template.publishedOn || evidence.runDate);
  showResult({
    label: "RECORDED PUBLISH · NOT CREATED BY THESE CLICKS",
    summary: `${runDate}: the Esmeralda channel template was published. This simulation did not submit it.`,
    txLabel: "Template publish transaction",
    transactionId: template.publishTransactionId,
    url: template.publishTransactionUrl,
    linkText: "Inspect the template publish ↗",
    note: `Your simulated clicks did not create or submit this transaction. It is the ${runDate} template publish.`,
  });
  byId("chain-panel").classList.add("is-settling");
  status(`Recorded template publish: ${template.finality}. The simulated run did not submit it.`);
  if (window.matchMedia("(max-width: 760px)").matches) {
    byId("chain-panel").scrollIntoView({ behavior: window.matchMedia("(prefers-reduced-motion: reduce)").matches ? "auto" : "smooth", block: "center" });
  }
  window.setTimeout(() => byId("chain-panel").classList.remove("is-settling"), 420);
  byId("settle").disabled = true;
  byId("settle").textContent = "Recorded publish shown";
}

function abandonPendingFunding() {
  const pending = readPendingFunding();
  const button = byId("abandon-pending-funding");
  if (!pending) {
    showPendingFundingControls();
    status("There is no saved funding transfer in this browser.");
    return;
  }
  if (!abandonArmed) {
    abandonArmed = true;
    button.textContent = "Click again to abandon without refunding";
    status("Abandoning only clears this browser. Prefer returning the deposit first; the 51 µT may still sit at the relay and cannot be recovered from a new live setup.");
    window.setTimeout(() => {
      abandonArmed = false;
      if (!button.hidden) button.textContent = "Abandon saved transfer and start over";
    }, 8000);
    return;
  }
  abandonArmed = false;
  clearPendingFunding();
  try { sessionStorage.removeItem("ghostline-live-payer-secret"); } catch { /* Storage may be restricted. */ }
  showPendingFundingControls();
  if (!liveChannel) begin();
  byId("live-open").textContent = "Connect Sapient and open channel";
  byId("live-note").textContent = "Saved transfer cleared in this browser. You can start live mode again. This does not recover funds already sent to the relay.";
  status("Saved funding transfer discarded locally. The instant simulation is available. If the deposit is still at the relay, use the original refund control before abandoning, or contact the operator.");
}

// A settled or refunded channel holds no deposit, so its saved state can be dropped and live
// mode offered again; the explorer keeps the transactions.
function forgetFinishedLiveChannel() {
  try { sessionStorage.removeItem(LIVE_CHANNEL_KEY); } catch { /* Storage may be restricted. */ }
  try { sessionStorage.removeItem("ghostline-live-payer-secret"); } catch { /* Storage may be restricted. */ }
  liveChannel = undefined;
  liveResult = undefined;
  liveCalls = 0;
  liveAccepted = 0n;
  liveRefundPending = false;
  simulationMode = false;
  byId("live-open").hidden = false;
  byId("live-open").textContent = "Connect Sapient and open channel";
  byId("live-refund").hidden = true;
  byId("live-stop").hidden = true;
  byId("settle").hidden = false;
  byId("service-subtitle").textContent = "Simulated local paywall record · every call and price";
  byId("live-note").textContent = "Your previous live run is finished and was cleared from this tab. Its transactions remain on the explorer. Live mode requires Sapient and your approval for a new 51 µT private testnet transfer and wallet fee.";
}

function reset() {
  const startingOver = Boolean(liveChannel && liveResult);
  if (liveChannel && !liveResult && !simulationMode) {
    status("This live channel is still open. Finish settlement or wait until expiry to request its private refund before resetting.");
    return;
  }
  if (startingOver) forgetFinishedLiveChannel();
  simOverspendTried = false;
  lastRefusal = undefined;
  accepted = 0n;
  begin();
  byId("calls").replaceChildren();
  byId("total").textContent = "0 µT";
  byId("quote").textContent = "Ask for a quote to receive a simulated HTTP 402.";
  hideResult();
  showChainPanelMode();
  settlementShown = false;
  refreshControls();
  status(startingOver
    ? "Ready for a new run. Try the simulation, or connect Sapient below to open a new live channel."
    : "Reset complete. Fresh temporary keys were generated in this page session.");
}

byId("pay").addEventListener("click", payToll);
byId("run-simulation").addEventListener("click", () => (liveChannel && !simulationMode ? simulateFromBanner() : runRemainingSimulation()));
  byId("settle").addEventListener("click", () => {
  if (liveChannel && !simulationMode) void settleLive().catch((error) => {
    if (liveChannel && !simulationMode) returnToSimulation();
    status(`Settlement not completed: ${error instanceof Error ? error.message : String(error)}. Showing the instant simulation; the live channel and recovery receipt remain saved for retry.`);
    byId("live-stop").hidden = false;
    byId("live-stop").textContent = "Continue with simulation";
  });
  else showRecordedEvidence();
});
byId("reset").addEventListener("click", reset);
byId("banner-simulate").addEventListener("click", simulateFromBanner);
byId("overspend").addEventListener("click", tryOverspend);
byId("observer-load").addEventListener("click", () => void loadLiveObserverFields());
byId("copy-tx").addEventListener("click", async () => {
  const transactionId = byId("recorded-tx-id").textContent;
  try {
    await navigator.clipboard.writeText(transactionId);
    status("Transaction ID copied.");
  } catch {
    status(`Copy is unavailable in this browser. Transaction ID: ${transactionId}`);
  }
});
byId("live-open").addEventListener("click", () => void connectAndOpenLive());
byId("refund-pending-funding").addEventListener("click", () => void refundPendingFunding());
byId("abandon-pending-funding").addEventListener("click", abandonPendingFunding);
byId("live-refund").addEventListener("click", () => void requestLiveRefund());
byId("live-stop").addEventListener("click", returnToSimulation);

configureLiveRelay();
{
  const pending = readPendingFunding();
  if (pending) {
    restorePayerSecret(pending);
    showPendingFundingControls(pending);
    status("A previously approved funding transfer is saved in this browser. Retry channel opening, return the deposit to Sapient, or abandon the saved transfer to start over; the wallet will not be charged again.");
  }
}
begin();
refreshControls();
void restoreLiveChannel();
fetch("./evidence.json")
  .then((response) => {
    if (!response.ok) throw new Error("evidence fetch failed");
    return response.json();
  })
  .then((value) => { evidence = value; })
  .catch(() => { evidence = undefined; })
  .finally(() => { evidenceLoaded = true; refreshControls(); });
