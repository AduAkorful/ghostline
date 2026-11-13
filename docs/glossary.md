# Glossary

Terms used across the Ghostline docs.

## Channel and payments

**Channel.** A deposit locked on chain with an expiry. A service can be paid from it up to the total the payer has signed for.

**Deposit.** The amount locked when a channel opens. On chain it is a commitment, so the amount is hidden.

**Voucher.** A signed statement of the cumulative total owed on one channel. Each voucher replaces the last, so only the latest is settled.

**Cumulative total.** The running sum of accepted calls. A voucher carries this total, not the price of one call.

**Channel id.** A 32-byte hash that commits to the channel's network, payer key, deposit, spend conditions and a nonce. A voucher for one channel is useless on another.

**HTTP 402.** The "Payment Required" status. A Ghostline server answers an unpaid request with 402 and a quote.

**Quote.** The body of a 402 response: the scheme, the price, the channel id and the terms the payer should check before signing.

**Settle.** The service spends the channel with the latest voucher before expiry.

**Refund.** The payer takes back the deposit at or after the expiry epoch, if the channel output is still unspent.

**Expiry epoch.** The epoch at which settlement ends and refund begins.

**Epoch.** The time unit Tari Ootle uses for expiry.

## Keys and privacy

**Stealth output.** A Tari Ootle output whose amount and owner are hidden from the public.

**Commitment.** A value that hides an amount while letting the chain check that amounts add up.

**Mask.** The secret blinding value that, with the amount, opens a commitment.

**Unblind.** Recover an output's amount and mask using the key it was encrypted to.

**Claim secret.** The secret that lets Ghostline read a deposit's amount and mask so it can sign and settle. It is either your wallet's view secret, as in the [mainnet quickstart](mainnet-quickstart.md#3-export-the-claim-secret), or a separate claim key made with `ghostline keygen --role claim` and passed to `channel init --claim-key-file`.

**Service key.** The public key that authorizes the service's settlement.

**Payout destination.** A one-time key derived from the service wallet's address. The service quotes it so the payout lands in its own wallet. See [settling and payouts](settle.md).

**Observer.** Anyone reading public chain data. See [privacy](privacy.md) for what an observer can and cannot see.

## Fees

**Network fee.** The fee a transaction pays to Tari validators. It is public. See [fees](fees.md).

**Protocol fee.** Ghostline's fee: 1% of the service payout, taken once at settle.

**Treasury.** The key that receives protocol fees.

**Fee config.** The on-chain component that holds the treasury key. Its owner can change the treasury.

**Revealed balance.** The public balance in an account. Settle and refund network fees are paid from it.

**µT.** Micro-TARI. One TARI is 1,000,000 µT. Ghostline uses integer µT everywhere.

## Tools

**Policy.** The limits that run outside the agent: budget, per-call cap, vendor allowlist and human approval. See [spending policy](policy.md).

**MCP.** The Model Context Protocol. `ghostline-mcp` is a local server that gives an agent tools for paying and managing channels. See [MCP server](mcp.md).

**Indexer.** A public service that reads chain state. Ghostline uses `indexer-a.ootle.tari.com` and `indexer-b.ootle.tari.com`.

**walletd.** The Tari Ootle wallet daemon, which Ghostline uses to open, settle and refund.
