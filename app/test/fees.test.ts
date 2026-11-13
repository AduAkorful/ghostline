import assert from "node:assert/strict";
import { test } from "node:test";
import { settlementFee } from "@ghostline/sdk/fee";
import vectors from "../../packages/ghostline/test-vectors/fee.json";
import { calculateFees, parseAmount, tari } from "../lib/calculator.ts";

test("calculator capacity and settlement fees match shared Rust vectors", () => {
  for (const { deposit, max } of vectors.max_settleable) {
    assert.equal(calculateFees(BigInt(deposit), 100n, 0n, 0n).cap, BigInt(max));
  }
  for (const { amount, fee } of vectors.settlement_fee) {
    if (fee === null) assert.throws(() => settlementFee(BigInt(amount)), RangeError);
    else assert.equal(settlementFee(BigInt(amount)), BigInt(fee));
  }
  for (const { deposit, amount, ok } of vectors.check_settleable) {
    const result = calculateFees(BigInt(deposit), BigInt(amount), 1n, 0n);
    assert.equal(amount === 0 || result.payout === BigInt(amount), ok);
  }
});

test("caps the payout, preserves change and rounds all-in cost up", () => {
  assert.deepEqual(calculateFees(100000n, 100n, 1000n, 44727n), {
    cap: 99000n, capCalls: 990n, usedCalls: 990n, payout: 99000n, protocol: 990n,
    change: 10n, perCall: 147n, breakEven: 448n, capped: true,
  });
  assert.equal(calculateFees(100000n, 101n, 500n, 44727n).perCall, null);
});

test("keeps u64 inputs exact and rejects ambiguous values", () => {
  assert.equal(parseAmount("18446744073709551615"), 18446744073709551615n);
  for (const input of ["", "-10", "1.5", "1e3", "18446744073709551616"]) {
    assert.equal(parseAmount(input), null);
  }
  assert.equal(tari(1000010n), "1.00001");
});
