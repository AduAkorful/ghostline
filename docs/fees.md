# Ghostline fees

All values here use integer micro-TARI (µT). One TARI is 1,000,000 µT.

## Network fees

| Transaction | Fee payer | Mainnet evidence, 13 Nov 2026 |
|---|---|---:|
| Open | Payer | 15,919–16,072 µT paid |
| Settle | Service | 28,808–28,810 µT paid in runs with recorded final fees |
| Expiry refund | Payer | 10,014 µT paid |

Network fees are public. They vary with the transaction and engine. Dry-run the actual transaction before submitting. For the private-input open tested on mainnet, the wallet charges the entire submitted cap. The Ghostline open helper quotes first and submits at the required fee within your configured ceiling. Settle and refund use a revealed fee balance. The account must cover the cap up front.

## Protocol fee and capacity

Settlement adds a treasury output of 1% of the service payout. Prices and cumulative voucher totals must be multiples of 100 µT. The deposit also reserves at least 1 µT in payer change. For deposit `D`, the maximum settleable payout is `floor((D − 1) / 101) × 100` µT for a positive deposit. A 20,201 µT deposit supports a 20,000 µT payout, a 200 µT protocol fee, and 1 µT change.

The protocol fee applies once at settle. Refunds pay the network fee. Paid calls run off-chain.

## Break-even

Use the open fee plus the settle fee to estimate the number of calls at which service revenue covers that network overhead. At 100 µT per call, 15,919 µT to open plus 28,808 µT to settle gives 448 calls after rounding the count up. This compares call revenue with combined network fees. Your margin also depends on the protocol fee, who pays each network fee, and your service costs.

Use the [fee calculator](https://ghostline-ui.vercel.app/fees/) to see what fits in a deposit and the [mainnet quickstart](mainnet-quickstart.md#fees-you-will-pay) to quote a transaction.
