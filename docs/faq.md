# Ghostline FAQ

## Do I need to clone the repository?

No, if you use the glibc-based Linux x86-64 release. The [installer](install.md) downloads the CLI, MCP server, and reference API. Live payments still require a compatible funded Ootle wallet and the provider's channel configuration; the installer does not supply those.

## What does Ghostline hide?

The service sees each request, its price, and the payer's voucher. Those details stay in service-side records rather than appearing as one chain transaction per request. A live Esmeralda test settled a 10 µT service voucher without exposing that amount or the payer remainder in public transaction fields. The channel deposit was visible in a covenant claim; fees, timing, branch, opaque channel id, authorization data, output commitments, and output count also remain visible. Funding and timing can still link transactions. See [Privacy](privacy.md).

## Is the service blind to my spending?

No. The service receives the API requests and verifies vouchers. Ghostline limits what chain observers can infer; it does not hide the activity from the vendor you are paying.

## Does the agent get unrestricted access to my wallet?

The reference payment path signs vouchers from a payer session and applies policy limits in the CLI/MCP process. It does not give the agent's reasoning model a general wallet signing key. Keep the policy process and its configuration outside the agent's control. The policy library does not authenticate its caller by itself, so an application that lets the agent alter policy or invoke an unrestricted signing interface defeats that boundary.

## What happens if a call is replayed or retried?

The voucher is cumulative. Replaying the latest accepted voucher does not add another charge; a lower total is rejected. The paywall persists the latest voucher and accepted total before returning success. Client retries should use the same channel and avoid constructing a new charge until the first result is known.

## What happens if the payer or service restarts?

Resume the payer's channel session and the service's matching channel ledger from their saved files. The session and ledger have different roles and should not be shared between unrelated channels. Back up sensitive channel files securely: they may contain the deposit mask or signing state. Do not restore an old service ledger over a newer one; that can lose the latest redeemable voucher.

## Can the payer refund at any time?

No. The payer's unilateral refund branch is available at or after the configured expiry epoch. Before expiry, settlement remains available to the service; at the boundary the engine's spend-condition semantics decide which branch is valid. Ghostline's tested flow treats `AfterEpoch` as inclusive for refund and refuses a settle at or after expiry. Check the current epoch and output presence before submitting.

## What if the channel output is already spent?

The channel can be settled or refunded only once in this channel variant. A spent output is reported as `down` / `output_spent`; off-chain remainder values are accounting arithmetic, not proof of funds or a wallet balance.

## What happens if the indexer is temporarily unavailable while watching?

Continuous `channel watch` retries epoch and output reads after the configured interval. `channel watch --once` reports the indexer error immediately. If settlement submission itself fails or times out, the command stops; check the transaction result and whether the channel output is still up before restarting, so an uncertain broadcast is not blindly repeated.

## Why are there separate payer and service fee accounts?

The payer funds the open and pays for a refund. The service pays the settle fee. Separate accounts make responsibility clearer and can reduce a direct on-chain link when funded independently, but they do not guarantee anonymity: faucet claims, transfers, transaction timing, and wallet operations can reveal relationships. A second account under the same seed is still correlated operationally.

## Are small calls profitable after fees?

Not necessarily. Batching makes many calls settle in one transaction, but it does not guarantee the call revenue exceeds open and settlement fees. For example, the verified 1,000-call demo accepted 10,000 µT total while the observed settle fee was about 17,640 µT; that demo proves the payment flow and batching, not positive economics at that price. Operators should measure their own fee path and choose prices/deposits accordingly.

## Does Ghostline support TypeScript wallet opening?

No. The TypeScript package currently matches channel-id and voucher encoding/signing/verification. Ootle wallet opening and transaction submission remain in Rust until an official JavaScript wallet path is verified.

## Is Ghostline audited or ready for real funds?

No. It is early development, has not had an independent audit, and is intended for testnet use. Do not use real funds. The two documented self-review rounds are not a substitute for independent security review.

## Does the side-by-side demo exist?

A [read-only prototype](../demo/ui/README.md) presents sanitized Esmeralda evidence for one paid call on the commitment-based settlement template. It contains no wallet connection or transaction controls. Call counts come from service records; the page and CLI privacy receipts are operator summaries, not cryptographic proof of the off-chain call log.
