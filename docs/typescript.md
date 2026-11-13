# TypeScript SDK

The TypeScript SDK handles channel IDs, voucher encoding and verification, signing, and fee calculations. Use the CLI or MCP server for wallet transactions and policy-managed payments.

## Install

The package is `@ghostline/sdk`. Install it from a local copy of the Ghostline source. Use Node 22 or newer. From your application's directory, run `npm install /absolute/path/to/ghostline/packages/ghostline` with the real path to your copy. The package exports TypeScript source. Your runtime or bundler must support it.

## Calculate fees and sign a voucher

```ts
import { maxSettleable, settlementFee, signSettleableVoucher, toHex, voucherToBytes } from "@ghostline/sdk";

const deposit = 20_201n;
const maximum = maxSettleable(deposit); // 20_000n
const fee = settlementFee(1_000n); // 10n

// Supply channel parameters, a local signing secret, and the quoted output key.
const voucher = signSettleableVoucher(payerSecret, channelParams, 1_000n, serviceOutputKey);
const voucherHeader = toHex(voucherToBytes(voucher));
```

The amount is cumulative across accepted calls. Keep a durable per-channel total and pass each quote through your spending policy before signing. `signSettleableVoucher` checks the settlement grid and deposit ceiling. Use `verifyVoucher` to validate a received voucher under the same channel parameters.

## Check Rust parity

The Rust and TypeScript implementations share test vectors, and both test suites check them. Run the package's tests (`npm ci`, then `npm test`) to check parity. The vectors cover the channel and voucher domains and the integer [fee rules](fees.md).
