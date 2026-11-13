// SPDX-License-Identifier: MIT

// Protocol fee rules, mirroring `ghostline-core`. The channel template checks the fee as an exact quotient on
// commitments, so a settled payout must be a multiple of FEE_DIVISOR and leave room for its fee and one micro-unit
// of payer change. Shared vectors in test-vectors/fee.json are asserted by both the Rust and TypeScript tests.

import type { ChannelParams } from "./channel.ts";
import { signVoucher, type SignedVoucher } from "./voucher.ts";

export const FEE_DIVISOR = 100n;
/** Payer change kept on every settlement so the transaction shape is fixed. */
export const SETTLEMENT_RESERVE = 1n;
const U64_MAX = (1n << 64n) - 1n;

function assertU64(value: bigint, label: string): void {
  if (value < 0n || value > U64_MAX) {
    throw new RangeError(`${label} out of u64 range`);
  }
}

/** Fee owed to the treasury on a settled payout. Throws for a payout the template cannot settle. */
export function settlementFee(payout: bigint): bigint {
  assertU64(payout, "payout");
  if (payout % FEE_DIVISOR !== 0n) {
    throw new RangeError(`payout ${payout} µT is not a multiple of ${FEE_DIVISOR}`);
  }
  return payout / FEE_DIVISOR;
}

/** Largest cumulative total a channel with this deposit can settle: `payout + payout / 100 < deposit`. */
export function maxSettleable(deposit: bigint): bigint {
  assertU64(deposit, "deposit");
  const maxSpend = deposit > SETTLEMENT_RESERVE ? deposit - SETTLEMENT_RESERVE : 0n;
  return (maxSpend / (FEE_DIVISOR + 1n)) * FEE_DIVISOR;
}

/** Throws unless `amount` is a cumulative total the channel could settle. */
export function checkSettleable(deposit: bigint, amount: bigint): void {
  settlementFee(amount);
  const max = maxSettleable(deposit);
  if (amount > max) {
    throw new RangeError(`cumulative ${amount} µT exceeds the settleable limit of ${max} µT`);
  }
}

/**
 * Signs a cumulative voucher only if the channel could settle it. Use this for payments; `signVoucher` stays
 * unchecked so the shared signature vectors keep their historical amounts.
 */
export function signSettleableVoucher(
  secret: Uint8Array,
  params: ChannelParams,
  amount: bigint,
  serviceOutputKey: Uint8Array,
  maskSecret?: Uint8Array,
  nonceSecret?: Uint8Array,
): SignedVoucher {
  checkSettleable(params.deposit, amount);
  return signVoucher(secret, params, amount, serviceOutputKey, maskSecret, nonceSecret);
}
