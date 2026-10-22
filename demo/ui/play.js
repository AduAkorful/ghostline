// SPDX-License-Identifier: MIT

import { SimulatedPaywall, PRICE } from "./sim-service.js";
import { createEphemeralChannelKeys } from "./ephemeral-keys.js";
import { channelId, publicKeyFromSecret, scalarFromWide, signVoucher, voucherToBytes } from "./vendor/ghostline.js";

const byId = (id) => document.getElementById(id);
const countLimit = 5;
const deposit = 1_000n;
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
let simulationMode = false;
let liveSetupCancelled = false;

const XTR_RESOURCE = `resource_${"01".repeat(32)}`;
const LIVE_PENDING_KEY = "ghostline-live-pending";
const LIVE_CHANNEL_KEY = "ghostline-live-channel";
const TARI_PROVIDER_READY_EVENT = "tari#initialized";
const TARI_PROVIDER_TIMEOUT_MS = 5_000;
const configuredRelayUrl = String(window.GHOSTLINE_CONFIG?.relayUrl || "").trim();

const status = (message) => { byId("status").textContent = message; };

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

function showPendingFunding(transactionId) {
  const link = byId("pending-funding-link");
  link.href = `https://explorer.tari.mw/tx/${encodeURIComponent(transactionId)}`;
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
  byId("reset").disabled = true;
  byId("pay").disabled = liveCalls >= countLimit || payerSecret.length !== 32;
  byId("quote").textContent = `Restored live channel ${liveChannel.channelId} · ${liveCalls}/${countLimit} calls · ${liveAccepted} µT accepted`;
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
    } else if (current.state === "Refunded") {
      liveRefundPending = false;
      byId("live-refund").hidden = true;
      byId("live-stop").hidden = false;
      status("This live channel was already refunded.");
    } else if (current.state === "RefundPending") {
      liveRefundPending = true;
      status("The expiry refund has started. Waiting for the private transfer to Sapient to finalize.");
    } else {
      status(`Restored open channel ${liveChannel.channelId}. Continue the calls or check for expiry refund.`);
      if (liveCalls === countLimit) await settleLive();
    }
  } catch (error) {
    status(`Live channel ${liveChannel.channelId} is saved in this tab, but the relay could not be reached: ${error instanceof Error ? error.message : String(error)}`);
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

function refreshControls() {
  if (liveChannel && !simulationMode) {
    byId("pay").disabled = liveRefundPending || liveCalls >= countLimit;
    byId("pay").textContent = liveCalls >= countLimit ? "Five live calls complete" : `Send live call ${liveCalls + 1} of ${countLimit} · sign voucher`;
    byId("run-simulation").hidden = true;
    byId("settle").disabled = liveRefundPending || liveCalls !== countLimit;
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
  byId("demo-progress").textContent = `${paywall.calls} of ${countLimit} simulated calls complete · ${accepted.toString()} µT accepted.${paywall.calls === countLimit ? " Recorded settlement evidence is now shown on the right." : " Run the next call or finish the remaining calls in one click."}`;
  byId("settle").disabled = paywall.calls !== countLimit;
  byId("settle").textContent = paywall.calls === countLimit
    ? "Show recorded settlement evidence · separate run"
    : `Complete ${countLimit} calls to unlock recorded evidence`;
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
  status(`Simulation complete: ${paywall.calls} paid calls, ${accepted.toString()} µT cumulative. The recorded Esmeralda evidence is separate from these browser clicks.`);
  byId("demo-progress").textContent = `${paywall.calls} of ${countLimit} simulated calls complete · ${accepted.toString()} µT accepted. The recorded Esmeralda proof is separate from this simulation.`;
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
  return error instanceof Error ? error.message : String(error);
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
  const method = (options.method || "GET").toUpperCase();
  const retryable = method === "GET" || ["/demo/open", "/demo/settle", "/demo/refund", "/demo/recover"].some((prefix) => path.startsWith(prefix));
  let response;
  for (let attempt = 0; ; attempt += 1) {
    try {
      response = await fetch(`${apiBase()}${path}`, {
        ...options,
        headers: { "content-type": "application/json", ...(options.headers || {}) },
      });
      if (response.status < 500 || !retryable || attempt > 0) break;
    } catch (error) {
      if (!retryable || attempt > 0) throw error;
    }
    status("The relay did not answer. Retrying this safe request once in 3 seconds; your wallet transfer will not be repeated.");
    await new Promise((resolve) => window.setTimeout(resolve, 3000));
  }
  const text = await response.text();
  let body;
  try { body = JSON.parse(text); } catch { body = text; }
  if (!response.ok) {
    const detail = typeof body === "string" ? body : body.error || body.message;
    throw new Error(detail || `Relay returned HTTP ${response.status}`);
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
      status(`🕯️ Waking the spirit… (${Math.ceil((Date.now() - (deadline - 75_000)) / 1000)}s). You can switch to the instant simulation at any time.`);
    }, 2000);
    try {
      const response = await fetch(`${url}/healthz`, { mode: "cors", signal: controller.signal });
      if (!response.ok && response.status !== 503) throw new Error(`Relay health returned HTTP ${response.status}.`);
      const health = await response.json();
      if (response.ok && health.ok && health.walletReady) return;
      if (response.status >= 500 && response.status !== 503) throw new Error(`Relay health returned HTTP ${response.status}.`);
      status(`Relay is waking. Check ${attempt}; the instant simulation remains available.`);
    } catch (error) {
      if (error?.name !== "AbortError" && !(error instanceof TypeError)) {
        throw error;
      }
      status(`Relay is waking. Check ${attempt}; the instant simulation remains available.`);
    } finally {
      window.clearTimeout(timeout);
      window.clearTimeout(wakeNotice);
    }
    await new Promise((resolve) => window.setTimeout(resolve, Math.min(5_000, Math.max(0, deadline - Date.now()))));
  }
  throw new Error("The relay did not become ready within 75 seconds. Continue with the instant simulation or try live mode again later.");
}

async function waitWalletRequest(requestId) {
  for (let attempt = 0; attempt < 60; attempt += 1) {
    const result = await tariRequest("tari_getTransactionRequest", { requestId });
    if (result.status !== "pending") {
      if (result.status !== "approved") throw new Error(result.error || `Wallet request ${result.status}`);
      return tariRequest("tari_submitTransactionRequest", { requestId });
    }
    await new Promise((resolve) => window.setTimeout(resolve, 2000));
  }
  throw new Error("Wallet approval timed out. Retry from your wallet.");
}

async function waitFinalized(transactionId) {
  for (let attempt = 0; attempt < 60; attempt += 1) {
    const result = await tariRequest("tari_getTransactionResult", { transactionId });
    if (result?.Finalized || result?.result?.Finalized) return;
    if (result?.Rejected || result?.result?.Rejected) throw new Error("The funding transaction was rejected by Ootle.");
    await new Promise((resolve) => window.setTimeout(resolve, 2000));
  }
  throw new Error(`Funding transaction ${transactionId} is still pending. Keep its id and retry after it finalizes.`);
}

async function connectAndOpenLive() {
  let setupStage = "checking relay configuration";
  liveSetupCancelled = false;
  byId("live-open").disabled = true;
  byId("live-open").textContent = "Checking Sapient…";
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
    byId("funding-requirement").textContent = `Switch Sapient to Esmeralda and use its built-in faucet to request test XTR. Wait for the transfer to finalize. This demo sends ${relay.deposit} µT to open the channel; your wallet also needs enough XTR for its transaction fee. If Sapient still shows an old balance after funding, refresh its wallet view and reload this page before retrying.`;
    let pending;
    try { pending = JSON.parse(sessionStorage.getItem(LIVE_PENDING_KEY) || "null"); } catch { pending = undefined; }
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
      let balances;
      setupStage = "checking the testnet balance";
      try {
        balances = await tariRequest("tari_getBalances");
      } catch {
        // Some wallet accounts do not grant balance inspection; the wallet still checks funds on approval.
      }
      const xtr = Array.isArray(balances) ? balances.find((balance) => balance.resourceAddress === XTR_RESOURCE) : undefined;
      const shownBalance = xtr ? BigInt(xtr.amount || "0") + BigInt(xtr.confidentialAmount || "0") : 0n;
      if ((xtr && shownBalance < BigInt(relay.deposit)) || (caps.privateViewGranted === true && !xtr)) {
        const reported = xtr ? `Sapient currently reports ${shownBalance} µT` : "Sapient did not return a spendable XTR balance";
        byId("live-note").textContent = `${reported}; the live channel needs ${relay.deposit} µT plus a wallet fee. If you just funded this wallet, refresh Sapient's wallet view, wait for finality, then reload this page and retry.`;
        throw new Error(`${reported}; ${relay.deposit} µT plus a wallet fee is required. Refresh Sapient after funding and retry.`);
      }
      if (caps.privateViewGranted !== true || !xtr) {
        byId("live-note").textContent = `This wallet does not expose a spendable XTR balance to the page. It will check funds when you approve. You need at least ${relay.deposit} µT plus the wallet fee; if Sapient reports insufficient funds, use its Esmeralda faucet, wait for finality, refresh Sapient, and retry.`;
      }
      const channelKeys = createEphemeralChannelKeys(randomBytes, scalarFromWide, publicKeyFromSecret);
      payerSecret = channelKeys.payerSecret;
      sessionStorage.setItem("ghostline-live-payer-secret", bytesToHex(payerSecret));
      setupStage = "creating a relay session";
      const relaySession = await relayJson("/demo/session", {
        method: "POST",
        body: JSON.stringify({ payerPublicKey: bytesToHex(channelKeys.payerPublic), refundAddress }),
      });
      byId("live-note").textContent = `Approve a private send of ${relaySession.deposit} µT to the relay and pay your wallet’s transaction fee. Five calls spend 50 µT; Ghostline reserves 1 µT for its fixed-shape change output. The relay opens the channel, then can settle or return the full deposit privately if it expires unsettled. This session expires in 10 minutes.`;
      setupStage = "waiting for Sapient approval";
      const operation = await tariRequest("tari_createTransactionRequest", {
        kind: "sendPrivately",
        resourceAddress: XTR_RESOURCE,
        recipientWalletAddress: relaySession.fundingAddress,
        amount: String(relaySession.deposit),
        maxFee: "50000",
        memo: `Ghostline:${relaySession.sessionId}`,
      });
      status("Approve the private testnet transfer in Sapient. The relay will wait for finality before opening your channel.");
      const funding = await waitWalletRequest(operation.requestId);
      if (!funding?.transactionId) throw new Error("Sapient did not return the funding transaction id.");
      pending = {
        sessionId: relaySession.sessionId,
        fundingTxId: funding.transactionId,
        payerPublicKey: bytesToHex(channelKeys.payerPublic),
      };
      sessionStorage.setItem(LIVE_PENDING_KEY, JSON.stringify(pending));
      showPendingFunding(funding.transactionId);
      if (liveSetupCancelled) {
        status("The approved funding transaction is saved in this tab. Continue live setup later to open its channel without sending again.");
        return;
      }
    } else {
      payerSecret = Uint8Array.from((sessionStorage.getItem("ghostline-live-payer-secret") || "").match(/../g) || [], (byte) => Number.parseInt(byte, 16));
      if (payerSecret.length !== 32) throw new Error("This tab lost its local voucher key. Keep the pending transfer id and contact the relay before retrying.");
      status("Retrying the relay open for your existing finalized funding transaction; no second wallet transfer will be requested.");
      showPendingFunding(pending.fundingTxId);
    }
    setupStage = "waiting for funding finality";
    await waitFinalized(pending.fundingTxId);
    if (liveSetupCancelled) {
      status(`Funding transaction ${pending.fundingTxId} is finalized and saved in this tab. Continue live setup later to open its channel without paying again.`);
      return;
    }
    setupStage = "opening the funded channel through the relay";
    const opened = await relayJson("/demo/open", {
      method: "POST",
      body: JSON.stringify({
        fundingTxId: pending.fundingTxId,
        sessionId: pending.sessionId,
      }),
    });
    sessionStorage.removeItem(LIVE_PENDING_KEY);
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
    saveLiveChannel();
    serviceKey = hexToBytes(opened.servicePublicKey);
    byId("service-subtitle").textContent = "Live Esmeralda relay · each accepted call has a real signed voucher";
    byId("quote").textContent = `Channel ${opened.channelId} · expires at epoch ${opened.expiryEpoch}`;
    byId("live-note").textContent = `Channel open through expiry epoch ${opened.expiryEpoch}. Custodial testnet flow: five calls can settle 50 µT; 1 µT remains in Ghostline’s fixed-shape change output. If left unsettled, the relay can return the full deposit privately after expiry. Your voucher key stays in this tab.`;
    byId("live-stop").hidden = false;
    byId("live-stop").textContent = "Continue with simulation";
    byId("live-refund").hidden = false;
    byId("reset").disabled = true;
    refreshControls();
    status("Channel finalized. Each click now sends a real signed voucher to the relay.");
  } catch (error) {
    const pending = sessionStorage.getItem(LIVE_PENDING_KEY);
    const cancelled = Number(error?.code) === 4001;
    byId("live-stop").hidden = false;
    byId("live-stop").textContent = "Continue with simulation";
    if (cancelled) {
      byId("live-stop").hidden = true;
      byId("live-open").textContent = "Connect Sapient and open channel";
      byId("live-note").textContent = "Wallet approval was cancelled. No funding transaction was submitted; you can retry whenever you're ready.";
      status(walletError(error));
    } else if (!pending && isSapientUnavailable(error)) {
      byId("live-open").textContent = "Retry Sapient connection";
      byId("live-stop").textContent = "Start instant simulation";
      byId("live-note").textContent = "Sapient wasn't detected in this tab. Reload this page in the Chrome profile where Sapient is enabled, or open it in Tari Universe, then retry. No wallet transfer was requested.";
      status("No Tari wallet became available. Reload this page in the Chrome profile where Sapient is enabled, then retry.");
    } else {
      byId("live-open").textContent = "Retry live setup";
      const detail = walletError(error);
      byId("live-note").textContent = pending
        ? `Live setup stopped while ${setupStage}: ${detail}. Your approved funding transaction remains saved in this tab; retrying will not request another transfer.`
        : `Live setup stopped while ${setupStage}: ${detail}. No channel was opened; the instant simulation remains available.`;
      status(`Live setup stopped while ${setupStage}: ${detail}.${pending ? " Your funding transaction is saved in this tab; retry without sending again." : " No channel was opened."}`);
    }
  } finally {
    byId("live-open").disabled = false;
    if (byId("live-open").textContent === "Checking Sapient…") {
      byId("live-open").textContent = "Connect Sapient and open channel";
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
    if (liveCalls === countLimit) await settleLive();
  } catch (error) {
    if (liveChannel && !simulationMode) returnToSimulation();
    byId("live-stop").hidden = false;
    byId("live-stop").textContent = "Continue with simulation";
    status(`Live call failed: ${walletError(error)} Showing the instant simulation. The open live channel, recovery receipt and voucher key remain saved for retry or expiry refund.`);
  } finally {
    refreshControls();
  }
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
    const link = byId("explorer-link");
    link.href = result.explorerUrl;
    link.textContent = "Inspect the private refund transfer ↗";
    byId("recorded-tx-id").textContent = result.refundTransactionId;
    byId("chain-empty").hidden = true;
    byId("chain-result").hidden = false;
    byId("record-summary").textContent = `The relay refunded the unspent ${liveChannel.deposit} µT channel privately to the connected Sapient wallet. Refund transaction ${result.refundTransactionId}.`;
    byId("chain-result").querySelector(".result-label").textContent = "LIVE TESTNET REFUND · FINALIZED";
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

function showLiveEvidence(url, transactionId) {
  liveRefundPending = false;
  byId("chain-panel").classList.remove("is-settling");
  byId("chain-empty").hidden = true;
  byId("chain-result").hidden = false;
  byId("record-summary").textContent = `Live run: ${liveCalls} paid calls · ${liveAccepted} µT cumulative · channel ${liveChannel.channelId} · settlement ${transactionId}.`;
  const link = byId("explorer-link");
  link.href = url;
  byId("recorded-tx-id").textContent = transactionId;
  link.textContent = "Inspect this live Esmeralda settlement ↗";
  byId("chain-result").querySelector(".result-label").textContent = "LIVE TESTNET SETTLEMENT · FINALIZED";
  byId("chain-result").querySelector(".fine-print").textContent = "This run created these calls and this settlement. The service knows the requests, prices, and vouchers; observers can still see transaction metadata and fees. The on-chain record does not publish the off-chain call count.";
  void verifySapientFinality(transactionId);
  byId("settle").hidden = true;
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
    if (!(result?.Finalized || result?.result?.Finalized)) {
      byId("chain-result").querySelector(".fine-print").textContent += " Sapient has not independently reported finality yet; the relay indexer reports Finalized.";
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
  if (simulationMode) {
    begin();
    paywall = new SimulatedPaywall(params, serviceKey);
    accepted = 0n;
    byId("calls").replaceChildren();
    byId("total").textContent = "0 µT";
    byId("service-subtitle").textContent = "Separate browser simulation · the saved live channel was not changed";
    byId("live-stop").textContent = "Return to live session";
    byId("live-stop").hidden = false;
    status("Simulation is separate. Your live channel, recovery receipt, and local voucher key are still saved; settle it or claim the expiry refund later.");
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
    status("Back in the live session. Its channel and local voucher key remain saved.");
  }
  refreshControls();
  byId("demo-title").scrollIntoView?.({ behavior: "smooth", block: "start" });
}

function showRecordedEvidence() {
  if (!evidence || settlementShown) {
    if (settlementShown) return;
    status("Could not load the static Esmeralda evidence record. Refresh and try again.");
    return;
  }
  settlementShown = true;
  const chain = evidence.chainRecord;
  byId("record-summary").textContent = "This is a separate recorded transaction from an earlier Esmeralda run. The browser clicks did not create or submit it.";
  const link = byId("explorer-link");
  link.href = chain.settlementTransactionUrl;
  byId("recorded-tx-id").textContent = chain.settlementTransactionId;
  link.setAttribute("aria-label", `Try historical Esmeralda transaction ${chain.settlementTransactionId} in Tari explorer`);
  byId("chain-panel").classList.add("is-settling");
  byId("chain-empty").hidden = true;
  byId("chain-result").hidden = false;
  status(`Separate recorded evidence: ${chain.settlementFinality}. The simulated run did not submit it.`);
  if (window.matchMedia("(max-width: 760px)").matches) {
    byId("chain-panel").scrollIntoView({ behavior: window.matchMedia("(prefers-reduced-motion: reduce)").matches ? "auto" : "smooth", block: "center" });
  }
  window.setTimeout(() => byId("chain-panel").classList.remove("is-settling"), 420);
  byId("settle").disabled = true;
  byId("settle").textContent = "Recorded evidence shown";
}

function reset() {
  if (liveChannel && !simulationMode) {
    status("This live channel is still open. Finish settlement or wait until expiry to request its private refund before resetting.");
    return;
  }
  accepted = 0n;
  begin();
  byId("calls").replaceChildren();
  byId("total").textContent = "0 µT";
  byId("quote").textContent = "Ask for a quote to receive a simulated HTTP 402.";
  byId("chain-empty").hidden = false;
  byId("chain-result").hidden = true;
  byId("chain-panel").classList.remove("is-settling");
  settlementShown = false;
  byId("settle").textContent = "Show recorded settlement evidence";
  refreshControls();
  status("Reset complete. Fresh temporary keys were generated in this page session.");
}

byId("pay").addEventListener("click", payToll);
byId("run-simulation").addEventListener("click", runRemainingSimulation);
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
byId("live-open").addEventListener("click", () => void connectAndOpenLive());
byId("live-refund").addEventListener("click", () => void requestLiveRefund());
byId("live-stop").addEventListener("click", returnToSimulation);

configureLiveRelay();
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
