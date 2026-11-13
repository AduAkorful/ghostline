// SPDX-License-Identifier: MIT

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";
import { fromHex } from "./channel.ts";
import {
  FEE_DIVISOR,
  SETTLEMENT_RESERVE,
  checkSettleable,
  maxSettleable,
  settlementFee,
  signSettleableVoucher,
} from "./fee.ts";
import { publicKeyFromSecret, verifyVoucher } from "./voucher.ts";

type FeeVectors = {
  fee_divisor: number;
  settlement_reserve: number;
  settlement_fee: { amount: number; fee: number | null }[];
  max_settleable: { deposit: number; max: number }[];
  check_settleable: { deposit: number; amount: number; ok: boolean }[];
};

const vectors = JSON.parse(
  readFileSync(join(dirname(fileURLToPath(import.meta.url)), "../test-vectors/fee.json"), "utf8"),
) as FeeVectors;

test("constants match the Rust core", () => {
  assert.equal(FEE_DIVISOR, BigInt(vectors.fee_divisor));
  assert.equal(SETTLEMENT_RESERVE, BigInt(vectors.settlement_reserve));
});

test("settlement fee matches the shared vectors", () => {
  for (const { amount, fee } of vectors.settlement_fee) {
    if (fee === null) {
      assert.throws(() => settlementFee(BigInt(amount)), RangeError, `amount ${amount}`);
    } else {
      assert.equal(settlementFee(BigInt(amount)), BigInt(fee), `amount ${amount}`);
    }
  }
});

test("settleable limit matches the shared vectors", () => {
  for (const { deposit, max } of vectors.max_settleable) {
    assert.equal(maxSettleable(BigInt(deposit)), BigInt(max), `deposit ${deposit}`);
  }
  for (const { deposit, amount, ok } of vectors.check_settleable) {
    const run = () => checkSettleable(BigInt(deposit), BigInt(amount));
    if (ok) assert.doesNotThrow(run, `deposit ${deposit} amount ${amount}`);
    else assert.throws(run, RangeError, `deposit ${deposit} amount ${amount}`);
  }
});

test("u64 bounds are enforced", () => {
  assert.throws(() => settlementFee(-100n), RangeError);
  assert.throws(() => maxSettleable(1n << 64n), RangeError);
});

test("signSettleableVoucher signs settleable totals and refuses the rest", () => {
  const secret = new Uint8Array(32);
  secret[0] = 7;
  const params = {
    networkTag: 0,
    payer: publicKeyFromSecret(secret),
    deposit: 20_201n,
    payerRoot: fromHex("09".repeat(32)),
    nonce: fromHex("03".repeat(32)),
  };
  const service = publicKeyFromSecret(new Uint8Array(32).fill(1, 0, 1));
  const voucher = signSettleableVoucher(secret, params, 20_000n, service);
  verifyVoucher(voucher, params);
  assert.throws(() => signSettleableVoucher(secret, params, 1_050n, service), /multiple of 100/);
  assert.throws(() => signSettleableVoucher(secret, params, 20_100n, service), /settleable limit of 20000/);
});
