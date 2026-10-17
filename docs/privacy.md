# What Ghostline hides, and what it does not

Ghostline lets a payer fund a channel once, sign cumulative vouchers for each service call off-chain, and settle the accepted total once. This hides the call-by-call payment pattern from public chain observers. The service receiving payment still sees every request, price, and voucher.

## Verified settlement behavior

A fresh Esmeralda run used the current claim-free close implementation: open a 10,000 µT channel, accept one 10 µT HTTP `/fortune` payment, then settle. Both open and settlement transactions finalized. The serialized settlement had an empty covenant-claim list, zero plaintext revealed input/output amounts, and two stealth outputs with one-unit minimum-value promises. The deposit, service amount, and payer remainder were not exposed in those transaction fields. The call and accepted total are confirmed by the local service ledger, not by the chain. The public transaction links are in [`demo/ui/evidence.json`](../demo/ui/evidence.json).

An earlier settlement showed why this change was needed: the generic wallet builder added an optional covenant-balance claim that exposed the deposit. Ghostline's channel does not use `Covenant::BalancePreserved`; its global stealth balance proof still enforces conservation, while the script witness enforces voucher, authorization, output-shape, and expiry rules. The current SDK omits the unused claim. In-process tests also confirm a real balance-preservation covenant still rejects a missing claim.

The claim-free settlement and expiry-refund paths have both been verified live on Esmeralda. The refund was a separate channel, refunded by the payer at epoch 12403, and finalized with no covenant claims, zero revealed input amount, and no revealed output amount. Both paths use the same masking approach; they were exercised in separate transactions.

## What observers can still see

| Transaction | Public information |
|---|---|
| Every transaction | Fee and its funding source, transaction existence and time, and the public transaction/substate graph |
| Open | A stealth channel output was created, plus public transaction metadata. The output amount remains masked in the tested flow. Funding can still identify the payer. |
| Settle | Executed branch, opaque channel id, payer public key and condition root passed to the template, service output authorization key, output commitments, and output count. The tested transaction did not reveal deposit, service amount, or payer remainder. |
| Refund after expiry | The refund branch, payer authorization data, output commitments, and output count. The tested claim-free refund did not reveal the deposit amount. |

The service receives the amount and can open its output. Output count may indicate whether payer change exists, but not its exact value. Fees, timing, graph shape, and authorization metadata can still correlate activity. Ghostline does not promise anonymity or unlinkable funding.

`ghostline channel trace` can combine a call count and price range from the local service ledger with the indexer status of transaction IDs supplied by the operator. It is an operator-assembled summary, not a cryptographic proof of the call log or a chain-wide scan. It does not include channel masks, keys, or voucher hex. Call totals are business-sensitive; share the receipt only when intended.

## Reducing correlation

- Use distinct payer and service signing keys for each channel; do not reuse an account identity key.
- Fund the channel from an unlinkable stealth UTXO and pay fees from a separately funded account. Same-seed accounts and public transfers remain correlatable.
- Batch calls off-chain and settle once. This hides individual calls from the chain, not from the service.
- Avoid treating timing or rounded amounts as privacy guarantees; they only reduce some matching clues.

The SDK uses the template's original script-path settlement and refund conditions. No key-path escrow or custom cryptography is involved. See [`deposit-confidentiality-redesign.md`](deposit-confidentiality-redesign.md) for the design and test evidence.

Unaudited. Testnet only. Do not use with real funds.
