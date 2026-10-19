# What Ghostline hides, and what it does not

Ghostline lets a payer fund a channel once, sign cumulative vouchers for each service call off-chain, and settle the accepted total once. This hides the call-by-call payment pattern from public chain observers. The service receiving payment still sees every request, price, and voucher, and knows the channel's declared deposit limit. The deposit UTXO's commitment mask remains with the payer in ordinary individual flows.

## Verified settlement behavior

A fresh Esmeralda run used the newly published fixed-output template: open a 10,000 µT channel, accept 20 HTTP `/fortune` payments at 10 µT each off-chain, then settle once. The template publish, open, and settlement transactions finalized. The settlement's stealth transfer statement had an empty covenant-claim list, zero revealed input amount, no revealed output, and exactly two stealth outputs with one-unit minimum-value promises. The deposit, service amount, and payer remainder were not exposed in those fields. The open paid 15,200 µT; the settlement paid 17,744 µT against a 17,756 µT cap. The 20 calls and 200 µT accepted total come from the local service ledger, not the chain. Fees remain public. The transaction links are in [`demo/ui/evidence.json`](../demo/ui/evidence.json).

An earlier settlement showed why this change was needed: the generic wallet builder added an optional covenant-balance claim that exposed the deposit. Ghostline's channel does not use `Covenant::BalancePreserved`; its global stealth balance proof still enforces conservation, while the script witness enforces voucher, authorization, output-shape, and expiry rules. The current SDK omits the unused claim. In-process tests also confirm a real balance-preservation covenant still rejects a missing claim.

The claim-free settlement and expiry-refund paths have both been verified live on the latest Esmeralda template. A fresh 10,000 µT channel was refunded by its payer at expiry epoch 12466. The finalized refund had no covenant claims, zero revealed input amount, no revealed output amount, and one stealth output; the wallet and indexer reported the channel output as spent. The public record confirms the close, but we did not locally unblind the recipient output, so this evidence does not independently establish the refunded amount. The transaction link and sanitized fields are in [`demo/ui/evidence.json`](../demo/ui/evidence.json).

The latest template was also exercised with two batch-enabled channels: each local ledger accepted 20 HTTP calls, then one pooled settlement spent both channel outputs and created three stealth outputs. The pooled service output was confirmed locally using its one-time payout key; the amount is omitted from public evidence. This validates execution of the batch entrypoint, not coordinator privacy: the local builder received both deposit masks and values, and observers can see that both channels participated in the same transaction.

## What observers can still see

| Transaction | Public information |
|---|---|
| Every transaction | Fee and its funding source, transaction existence and time, and the public transaction/substate graph |
| Open | A stealth channel output was created, plus public transaction metadata. The output amount remains masked in the tested flow. Funding can still identify the payer. |
| Settle | Executed branch, opaque channel id, payer public key and condition root passed to the template, channel authorization key, payout key, output commitments, and a two-output shape. The latest-template Esmeralda settlement did not reveal deposit, service amount, or payer remainder in public transaction fields. |
| Pooled batch settle | Two channel inputs in the same transaction, batch branch and metadata, one pooled service output, two payer-change outputs, commitments, and output count. The pooled value was not readable in public fields; grouping the channels in one transaction is visible. |
| Refund after expiry | The refund branch, payer authorization data, output commitments, and output count. The tested claim-free refund did not reveal the deposit amount. |

The service receives the amount and can open its output. Settlement keeps two outputs by reserving one µT for payer change, so output count no longer signals whether the channel's full deposit was spent. Fees, timing, graph shape, and authorization metadata can still correlate activity. Ghostline does not promise anonymity or unlinkable funding.

The service knows the declared deposit limit because it is part of the channel parameters used to validate vouchers. That is separate from the channel output's cryptographic opening: in ordinary individual flows, the payer's secret deposit UTXO mask stays in its local channel file/wallet and is not sent to the service. (The service does receive each voucher's separate output mask so it can open the payment.) The current pooled prototype does not preserve this boundary: its local builder is given every participant's deposit opening.

`ghostline channel trace` can combine a call count and price range from the local service ledger with the indexer status of transaction IDs supplied by the operator. It is an operator-assembled summary, not a cryptographic proof of the call log or a chain-wide scan. It does not include channel masks, keys, or voucher hex. Call totals are business-sensitive; share the receipt only when intended.

## Explorer comparison

As a concrete public-transfer example, this [Ethereum USDC transaction](https://etherscan.io/tx/0xd4b176e5bbb45ca3074f7e5bcb178c2347a7ddf26b17ae99e3d87cb8cc0b5fea) exposes the transfer amount and recipient in Etherscan's transaction summary. Ghostline's [recorded Esmeralda settlement](https://explorer.tari.mw/tx/9c8887f32141bbec3e5d7fa4a8b147c8e1a1bab9599e46e1d8704781ae85b668) instead shows confidential output commitments; the claim-free transaction record had no plaintext deposit, service amount, or payer remainder. These are examples of different transaction designs, not matched payments on the same network. Private payment schemes can also be built on EVM chains.

## Reducing correlation

- Generate separate, fresh payer and service signing keys for every channel. `ghostline keygen --role payer --out <private-file>` writes an owner-only 0600 key file, and `ghostline sign --secret-file <private-file>` uses the payer file. Generate the service signer in its wallet with `ghostline wallet key-create`; use its public key for that channel and its wallet `KeyId` for settlement. Since each channel settles once, that key is used for one channel and one settlement. Do not reuse it across channels. This removes repeated-key correlation; it does not hide the two roles inside one transaction or undo funding, timing, and fee links.
- For a further key boundary, configure `ghostline-paid-api serve --service-output-key-hex <fresh-key>` with a different wallet-managed key for that channel. The 402 quote and signed voucher route settlement to this payout key while the channel's service key authorizes the close. This makes later spends of the payout output less linkable to reuse of the channel authorization key. Both keys are public in the settlement transaction, so the transaction still directly associates the channel with its payout.
- Fund the channel from an unlinkable stealth UTXO and pay fees from a separately funded account. Same-seed accounts and public transfers remain correlatable.
- Batch calls off-chain and settle once. This hides individual calls from the chain, not from the service.
- Settlement vouchers may spend at most `deposit - 1 µT`; the reserved micro-unit keeps the settlement output count fixed at two. It is returned as payer change.
- Avoid treating timing or rounded amounts as privacy guarantees; they only reduce some matching clues.
- A settlement reserves 1 µT for payer change and always creates two outputs. This removes the previous one-output/two-output signal for whether the full channel deposit was spent. The fixed shape is covered by both in-process tests and the latest-template Esmeralda transaction linked above.

Pooled batch settlement is research-only. The current SDK prototype requires every channel's deposit value and mask in one process to construct Ootle's aggregate range and balance proofs. A hosted coordinator would therefore learn those openings. No Ghostline hosted flow uses this prototype; the live Esmeralda run only confirms the contract and transaction path. It does not provide coordinator privacy or unlinkability between channels grouped into one batch. Until a reviewed multi-party proof protocol exists, use individual settlement.

The SDK uses the template's original script-path settlement and refund conditions. No key-path escrow or custom cryptography is involved. See [`deposit-confidentiality-redesign.md`](deposit-confidentiality-redesign.md) for the design and test evidence.

Unaudited. Testnet only. Do not use with real funds.
