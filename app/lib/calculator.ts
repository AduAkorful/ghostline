import { FEE_DIVISOR, maxSettleable, settlementFee } from "@ghostline/sdk/fee";

const U64_MAX = (1n << 64n) - 1n;

export function parseAmount(input: string): bigint | null {
  if (!/^\d{1,20}$/.test(input)) return null;
  const value = BigInt(input);
  return value <= U64_MAX ? value : null;
}

export function calculateFees(deposit: bigint, price: bigint, calls: bigint, network: bigint) {
  const cap = maxSettleable(deposit);
  const validPrice = price > 0n && price % FEE_DIVISOR === 0n;
  const capCalls = validPrice ? cap / price : 0n;
  const usedCalls = calls < capCalls ? calls : capCalls;
  const payout = usedCalls * price;
  const protocol = settlementFee(payout);
  const change = deposit - payout - protocol;
  return {
    cap, capCalls, usedCalls, payout, protocol, change,
    perCall: usedCalls ? (payout + protocol + network + usedCalls - 1n) / usedCalls : null,
    breakEven: validPrice ? (network + price - 1n) / price : null,
    capped: validPrice && calls > capCalls,
  };
}

export function tari(deposit: bigint) {
  const whole = (deposit / 1_000_000n).toLocaleString("en-US");
  const fraction = (deposit % 1_000_000n).toString().padStart(6, "0").replace(/0+$/, "");
  return fraction ? `${whole}.${fraction}` : whole;
}
