// SPDX-License-Identifier: MIT

import assert from "node:assert/strict";
import test from "node:test";
import { numberToBytesLE } from "@noble/curves/utils.js";
import { SimulatedPaywall, PRICE } from "../../../demo/ui/sim-service.js";
import { scalarFromWide, signVoucher, publicKeyFromSecret } from "./voucher.ts";
import type { ChannelParams } from "./channel.ts";

function bytes(size: number, fill: number): Uint8Array {
  return new Uint8Array(size).fill(fill);
}

const secret = numberToBytesLE(scalarFromWide(bytes(64, 7)), 32);
const serviceSecret = numberToBytesLE(scalarFromWide(bytes(64, 9)), 32);
const serviceKey = publicKeyFromSecret(serviceSecret);
const params: ChannelParams = {
  networkTag: 38,
  payer: publicKeyFromSecret(secret),
  deposit: 1_000n,
  payerRoot: bytes(32, 0),
  nonce: bytes(32, 3),
};

test("simulated paywall accepts a valid quote-priced cumulative voucher", () => {
  const paywall = new SimulatedPaywall(params, serviceKey);
  assert.equal(paywall.quote().status, 402);
  const result = paywall.pay(signVoucher(secret, params, PRICE, serviceKey));
  assert.equal(result.status, 200);
  assert.equal(paywall.calls, 1);
});

test("simulated paywall rejects a tampered voucher", () => {
  const paywall = new SimulatedPaywall(params, serviceKey);
  const voucher = signVoucher(secret, params, PRICE, serviceKey);
  voucher.amount += 1n;
  assert.throws(() => paywall.pay(voucher));
  assert.equal(paywall.calls, 0);
});

test("simulated paywall rejects a non-increasing cumulative voucher", () => {
  const paywall = new SimulatedPaywall(params, serviceKey);
  const first = signVoucher(secret, params, PRICE, serviceKey);
  paywall.pay(first);
  const replay = signVoucher(secret, params, PRICE, serviceKey);
  assert.throws(() => paywall.pay(replay), /must increase/);
});

test("simulated paywall rejects a total above the deposit", () => {
  const paywall = new SimulatedPaywall(params, serviceKey);
  const overLimit = signVoucher(secret, params, params.deposit, serviceKey);
  assert.throws(() => paywall.pay(overLimit), /spend limit/);
});
