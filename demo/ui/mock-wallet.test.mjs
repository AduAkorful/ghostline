// SPDX-License-Identifier: MIT
// Local integration harness for play.html. No browser extension, chain, or funds are used.
import assert from "node:assert/strict";
import { test } from "node:test";
import { channelId, publicKeyFromSecret, toHex } from "./vendor/ghostline.js";

const ROOT = new URL("./", import.meta.url);
const RELAY = "https://relay.test.invalid";
const XTR_RESOURCE = `resource_${"01".repeat(32)}`;
const hex = (bytes) => toHex(bytes);
const rpcError = (code, message) => Object.assign(new Error(message), { code });

class Element {
  constructor(id = "") {
    this.id = id;
    this.value = "";
    this.textContent = "";
    this.hidden = false;
    this.disabled = false;
    this.href = "";
    this.className = "";
    this.children = [];
    this.listeners = new Map();
    this.classes = new Set();
    this.scrolledIntoView = false;
    this.classList = {
      add: (name) => this.classes.add(name),
      remove: (name) => this.classes.delete(name),
    };
  }
  addEventListener(type, callback) { this.listeners.set(type, callback); }
  click() { return this.listeners.get("click")?.({ target: this }); }
  append(...items) { this.children.push(...items); }
  prepend(item) { this.children.unshift(item); }
  replaceChildren(...items) { this.children = items; }
  scrollIntoView() { this.scrolledIntoView = true; }
  querySelector(selector) {
    if (selector === ".result-label" || selector === ".fine-print") {
      return this.childrenBySelector?.get(selector) || (this.childrenBySelector ||= new Map()).set(selector, new Element(selector)).get(selector);
    }
    return null;
  }
}

function makeHarness({ wallet = true, injectWalletOnLoad = wallet, walletOverrides = {}, failHealth = false, coldHealth = false, fastTime = false } = {}) {
  const elements = new Map();
  const session = new Map();
  const local = new Map();
  const calls = [];
  const walletCalls = [];
  const windowListeners = new Map();
  let fundingTxId = "funding-test-tx";
  let payerPublicKey;
  let opened;
  let paidCalls = 0;
  const originalDateNow = Date.now;
  let virtualNow = originalDateNow();
  if (fastTime) Date.now = () => virtualNow;

  const document = {
    getElementById(id) {
      if (!elements.has(id)) elements.set(id, new Element(id));
      return elements.get(id);
    },
    createElement(tag) { return new Element(tag); },
    head: { append(script) { queueMicrotask(() => script.onload?.()); } },
  };

  const walletMethods = {
    tari_getNetwork: "esmeralda",
    tari_requestAccounts: ["otl_esm_1mockwallet"],
    tari_getCapabilities: { privateSpend: true, transactionRequests: true, walletAddress: true, transactionResultLookup: true, privateViewGranted: true },
    tari_getWalletAddress: "otl_esm_1mockwallet",
    tari_getBalances: [{ resourceAddress: XTR_RESOURCE, amount: "100000", confidentialAmount: "0" }],
    tari_createTransactionRequest: { requestId: "funding-request" },
    tari_getTransactionRequest: { status: "approved" },
    tari_submitTransactionRequest: { transactionId: fundingTxId },
    tari_getTransactionResult: { Finalized: {} },
    ...walletOverrides,
  };
  const tari = wallet ? {
    async request({ method, params }) {
      walletCalls.push({ method, params });
      const value = walletMethods[method];
      if (value instanceof Error) throw value;
      if (typeof value === "function") return value(params);
      return value;
    },
  } : undefined;

  const json = (data, status = 200, headers = {}) => new Response(JSON.stringify(data), { status, headers });
  const fetch = async (input, options = {}) => {
    const url = new URL(String(input), "https://site.test.invalid");
    if (url.pathname === "/evidence.json") return json({ chainRecord: { settlementTransactionUrl: "https://explorer.tari.mw/tx/recorded", settlementFinality: "Finalized" } });
    calls.push({ url: url.href, method: (options.method || "GET").toUpperCase(), headers: new Headers(options.headers), body: options.body || "" });
    if (url.origin !== RELAY) throw new Error(`Unexpected outbound origin: ${url.origin}`);
    if (url.pathname === "/healthz") return failHealth ? json({ error: "missing" }, 404) : coldHealth ? json({ ok: false, walletReady: false }, 503) : json({ ok: true, walletReady: true });
    if (url.pathname === "/demo/config") return json({ enabled: true, deposit: 51, price: 10, network: "esmeralda" });
    if (url.pathname === "/demo/session") {
      const body = JSON.parse(options.body);
      payerPublicKey = body.payerPublicKey;
      return json({ sessionId: "session-test", fundingAddress: "otl_esm_1relay", deposit: 51, price: 10 }, 201);
    }
    if (url.pathname === "/demo/open") {
      const payerRoot = new Uint8Array(32).fill(7);
      const nonce = new Uint8Array(32).fill(8);
      const serviceSecret = new Uint8Array(32).fill(9);
      const servicePublicKey = publicKeyFromSecret(serviceSecret);
      const params = { networkTag: 38, payer: Uint8Array.from(payerPublicKey.match(/../g), (b) => Number.parseInt(b, 16)), deposit: 51n, payerRoot, nonce };
      opened = {
        channelId: hex(channelId(params)), payerRoot: hex(payerRoot), nonce: hex(nonce), networkTag: 38,
        deposit: "51", expiryEpoch: 999, servicePublicKey: hex(servicePublicKey), serviceOutputPublicKey: hex(servicePublicKey),
        fundingTransactionId: fundingTxId, openTransactionId: "open-test-tx", recoveryReceipt: "opaque-recovery-receipt",
      };
      return json(opened);
    }
    if (url.pathname === "/fortune") {
      const headers = new Headers(options.headers);
      if (!headers.has("ghostline-voucher")) return json({ scheme: "ghostline.channel.voucher.v1", channel_id: opened.channelId, price: "10", service_output_key: opened.serviceOutputPublicKey }, 402);
      paidCalls += 1;
      return json({ fortune: `Mock paid response ${paidCalls}` }, 200, { "ghostline-recovery": `opaque-recovery-${paidCalls}` });
    }
    if (url.pathname === "/demo/settle") return json({ state: "Finalized", settlementTransactionId: "settle-test-tx", explorerUrl: "https://explorer.tari.mw/tx/settle-test-tx", recoveryReceipt: "opaque-recovery-final" });
    if (url.pathname === "/demo/status") return json({ state: "Finalized", settlementTransactionId: "settle-test-tx", explorerUrl: "https://explorer.tari.mw/tx/settle-test-tx" });
    throw new Error(`Unexpected relay request: ${options.method || "GET"} ${url.pathname}`);
  };

  const window = {
    ...(tari && injectWalletOnLoad ? { tari } : {}),
    GHOSTLINE_CONFIG: { relayUrl: RELAY },
    addEventListener(type, callback) {
      const listeners = windowListeners.get(type) || new Set();
      listeners.add(callback);
      windowListeners.set(type, listeners);
    },
    removeEventListener(type, callback) { windowListeners.get(type)?.delete(callback); },
    dispatchEvent(event) {
      for (const callback of [...(windowListeners.get(event.type) || [])]) callback(event);
    },
    setTimeout: fastTime ? ((callback, delay = 0) => setTimeout(() => { virtualNow += Number(delay); callback?.(); }, 0)) : setTimeout,
    clearTimeout,
  };
  Object.assign(globalThis, {
    document,
    window,
    fetch,
    sessionStorage: {
      getItem: (key) => session.get(key) ?? null,
      setItem: (key, value) => session.set(key, String(value)),
      removeItem: (key) => session.delete(key),
    },
    localStorage: {
      getItem: (key) => local.get(key) ?? null,
      setItem: (key, value) => local.set(key, String(value)),
      removeItem: (key) => local.delete(key),
    },
  });
  elements.set("relay-config-note", new Element("relay-config-note"));

  return {
    calls, walletCalls, elements, session,
    injectWallet() {
      if (!tari) throw new Error("No mock wallet was configured");
      window.tari = tari;
      window.dispatchEvent({ type: "tari#initialized" });
    },
    async load() {
      await import(new URL(`play.js?mock=${Math.random()}`, ROOT));
      await new Promise((resolve) => setImmediate(resolve));
    },
    async waitFor(predicate) {
      const deadline = originalDateNow() + 7_500;
      while (originalDateNow() < deadline) {
        if (predicate()) return;
        await new Promise((resolve) => setTimeout(resolve, 10));
      }
      assert.fail("Timed out waiting for mock UI flow to finish");
    },
    restoreClock() { Date.now = originalDateNow; },
    payerSecret: () => session.get("ghostline-live-payer-secret"),
    paidCalls: () => paidCalls,
  };
}

test("waits for Sapient's late provider initialization event", async () => {
  const h = makeHarness({ injectWalletOnLoad: false });
  await h.load();
  const opening = h.elements.get("live-open").click();
  setTimeout(() => h.injectWallet(), 0);
  await opening;
  await h.waitFor(() => h.elements.get("status").textContent.includes("Channel finalized") || h.elements.get("status").textContent.startsWith("Live setup stopped:"));
  assert.match(h.elements.get("status").textContent, /Channel finalized/);
  assert.ok(h.walletCalls.some((call) => call.method === "tari_getNetwork"));
});

test("mock wallet completes connect → private funding → 5 vouchers → finalized settlement without leaking the signing key", async () => {
  const h = makeHarness();
  await h.load();
  await h.elements.get("live-open").click();
  await h.waitFor(() => h.elements.get("status").textContent.includes("Channel finalized") || h.elements.get("status").textContent.startsWith("Live setup stopped:"));
  assert.match(h.elements.get("status").textContent, /Channel finalized/);
  assert.equal(h.walletCalls.find((c) => c.method === "tari_createTransactionRequest").params.amount, "51");

  for (let i = 0; i < 5; i += 1) {
    await h.elements.get("pay").click();
    await h.waitFor(() => (h.elements.get("calls")?.children.length || 0) === i + 1 || h.elements.get("status").textContent.startsWith("Live call failed:"));
  }

  assert.equal(h.paidCalls(), 5);
  assert.match(h.elements.get("record-summary").textContent, /5 paid calls · 50 µT cumulative/);
  assert.equal(h.elements.get("explorer-link").href, "https://explorer.tari.mw/tx/settle-test-tx");
  assert.match(h.elements.get("status").textContent, /Live settlement finalized/);
  assert.equal(h.calls.every((call) => call.url.startsWith(`${RELAY}/`)), true);

  const secretHex = h.payerSecret();
  assert.ok(secretHex?.length === 64, "voucher signing key is kept in sessionStorage");
  for (const call of h.calls) {
    assert.equal(call.body.includes(secretHex), false, "voucher signing key never appears in request bodies");
    assert.equal([...call.headers.values()].some((value) => value.includes(secretHex)), false, "voucher signing key never appears in request headers");
  }
  const fundingRequest = h.walletCalls.find((call) => call.method === "tari_createTransactionRequest");
  assert.equal(fundingRequest.params.kind, "sendPrivately");
  assert.equal(fundingRequest.params.amount, "51");
  assert.equal(h.calls.filter((call) => call.url.includes("/demo/session")).length, 1);

  await h.elements.get("live-stop").click();
  assert.match(h.elements.get("service-subtitle").textContent, /Separate browser simulation/);
  assert.equal(h.elements.get("live-stop").textContent, "Return to live session");
  assert.equal(h.elements.get("demo-title").scrolledIntoView, true);
  await h.elements.get("pay").click();
  assert.equal(h.elements.get("calls").children.length, 1, "continuing with simulation switches from live vouchers to usable mock calls");
});

test("relay health failure leaves the user in a usable simulated mode without calling the wallet", async () => {
  const h = makeHarness({ failHealth: true });
  await h.load();
  await h.elements.get("live-open").click();
  await h.waitFor(() => h.elements.get("status").textContent.startsWith("Live setup stopped:"));
  assert.match(h.elements.get("status").textContent, /HTTP 404/);
  assert.equal(h.walletCalls.length, 0);
  assert.equal(h.elements.get("pay").disabled, false);
});

test("wrong network falls back without requesting funds", async () => {
  const h = makeHarness({ walletOverrides: { tari_getNetwork: "localnet" } });
  await h.load();
  await h.elements.get("live-open").click();
  await h.waitFor(() => h.elements.get("status").textContent.startsWith("Live setup stopped:"));
  assert.match(h.elements.get("status").textContent, /Switch your Tari wallet to Esmeralda/);
  assert.equal(h.walletCalls.some((call) => call.method === "tari_createTransactionRequest"), false);
  assert.equal(h.elements.get("pay").disabled, false);
});

test("missing extension, rejected approval, unsupported methods, chain errors, and low balance keep simulation usable", async (t) => {
  const cases = [
    { name: "missing extension", options: { wallet: false }, message: /No Tari wallet became available/ },
    { name: "rejected approval", options: { walletOverrides: { tari_getNetwork: rpcError(4001, "User rejected") } }, message: /Wallet approval was cancelled/ },
    { name: "unsupported wallet method", options: { walletOverrides: { tari_getCapabilities: rpcError(4200, "Unsupported") } }, message: /does not support a required feature/ },
    { name: "chain rejection", options: { walletOverrides: { tari_getCapabilities: rpcError(-32603, "Ootle rejected wallet request") } }, message: /Ootle rejected wallet request/ },
    { name: "low balance", options: { walletOverrides: { tari_getBalances: [{ resourceAddress: XTR_RESOURCE, amount: "0", confidentialAmount: "0" }] } }, message: /Low Esmeralda XTR balance/ },
  ];
  for (const scenario of cases) {
    await t.test(scenario.name, async () => {
      const h = makeHarness(scenario.options);
      await h.load();
      await h.elements.get("live-open").click();
      await h.waitFor(() => h.elements.get("status").textContent.startsWith("Live setup stopped:") || scenario.message.test(h.elements.get("status").textContent));
      assert.match(h.elements.get("status").textContent, scenario.message);
      if (scenario.name === "missing extension") {
        assert.equal(h.elements.get("live-open").textContent, "Retry Sapient connection");
        assert.equal(h.elements.get("live-stop").textContent, "Start instant simulation");
        assert.match(h.elements.get("live-note").textContent, /Chrome profile where Sapient is enabled.*No wallet transfer was requested/);
        await h.elements.get("live-stop").click();
        assert.equal(h.elements.get("live-stop").hidden, true);
        assert.match(h.elements.get("service-subtitle").textContent, /Instant browser simulation/);
        assert.match(h.elements.get("status").textContent, /will not contact Sapient or send a transaction/);
        assert.equal(h.elements.get("pay").disabled, false);
        await h.elements.get("pay").click();
        assert.equal(h.elements.get("calls").children.length, 1, "the simulation button leads to a usable mock call");
      }
      assert.equal(h.elements.get("pay").disabled, false);
      assert.equal(h.walletCalls.some((call) => call.method === "tari_createTransactionRequest"), false);
    });
  }
});

test("disconnected wallet is asked to reconnect and the request is retried", async () => {
  let capabilityCalls = 0;
  const h = makeHarness({ walletOverrides: {
    tari_getCapabilities: () => {
      capabilityCalls += 1;
      if (capabilityCalls === 1) throw rpcError(4100, "Disconnected");
      return { privateSpend: true, transactionRequests: true, walletAddress: true, transactionResultLookup: true, privateViewGranted: true };
    },
  } });
  await h.load();
  await h.elements.get("live-open").click();
  await h.waitFor(() => h.walletCalls.some((call) => call.method === "tari_createTransactionRequest") || h.elements.get("status").textContent.startsWith("Live setup stopped:"));
  assert.equal(capabilityCalls, 2);
  assert.equal(h.walletCalls.filter((call) => call.method === "tari_requestAccounts").length, 2);
});

test("cold relay retries to the deadline and then leaves simulated mode usable", async () => {
  const h = makeHarness({ coldHealth: true, fastTime: true });
  try {
    await h.load();
    await h.elements.get("live-open").click();
    await h.waitFor(() => h.elements.get("status").textContent.startsWith("Live setup stopped:"));
    assert.match(h.elements.get("status").textContent, /within 75 seconds/);
    assert.equal(h.elements.get("pay").disabled, false);
    assert.equal(h.walletCalls.length, 0);
  } finally { h.restoreClock(); }
});

test("a funding transaction that stays pending is reported without opening a channel", async () => {
  const h = makeHarness({ fastTime: true, walletOverrides: { tari_getTransactionResult: {} } });
  try {
    await h.load();
    await h.elements.get("live-open").click();
    await h.waitFor(() => h.elements.get("status").textContent.startsWith("Live setup stopped:"));
    assert.match(h.elements.get("status").textContent, /still pending/);
    assert.ok(h.session.has("ghostline-live-pending"), "the funding transaction id is retained for a safe retry");
    assert.equal(h.elements.get("pending-funding-link").href, "https://explorer.tari.mw/tx/funding-test-tx");
    assert.equal(h.elements.get("pending-funding-link").hidden, false);
    assert.equal(h.calls.some((call) => call.url.includes("/demo/open")), false);
    assert.equal(h.elements.get("pay").disabled, false);
  } finally { h.restoreClock(); }
});

