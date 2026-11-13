# What Ghostline hides, and what it does not

Ghostline lets a payer fund a channel once, sign cumulative vouchers for each service call off-chain, and settle the accepted total once. This hides the call-by-call payment pattern, the deposit and the payout amount from public chain observers. It does not hide that a channel exists, what its transactions paid in network fees, or links you create yourself, such as funding both sides from one wallet. The service receiving payment still sees every request, price and voucher, and knows the channel's declared deposit limit. The deposit's commitment mask stays with the payer in ordinary individual flows.

Ghostline is unaudited and does not promise anonymity or unlinkable funding.

## At a glance

| | What |
|---|---|
| **Hidden** | The deposit and payout amounts, the protocol fee and payer change amounts, each call and its price, how many calls were made, the payer's account, which one-time payout key belongs to the service |
| **Public** | Network fees and who paid them, that an open and a settle exist and when, the executed branch, the opaque channel id, authorization keys, output commitments and the number of outputs |
| **Linked** | The open and the settle (they share the channel output), the service's fee account (named by the settle), every Ghostline settlement (the protocol fee goes to the treasury key), and any funding link you create |

## Mainnet observer test (13 Nov 2026)

One channel run with separate wallets. The payer and the service each had their own walletd and seed. Both were funded from a third wallet (A), so the funding graph links them; this test shows what each Ghostline transaction reveals, not unlinkability from the funder. Everything below was read from the public mainnet indexer (`GET /transactions/{id}`), with no keys.

| Transaction | What a public observer can read |
|---|---|
| Fund payer `6182b000…` (A → payer, 60,000 µT private) | It spends one of A's outputs (`e89d70c7…`) and creates two key-locked outputs, one of which is `80ff41d4…`. Fee 15,919. No amount and no account is named. |
| Fund service `a9a114d9…` (A → service, 40,000 µT revealed) | It spends A's change and creates the service account component `c095ae8e…` with a **public** 40,000 µT revealed vault. Fee 11,065. |
| Open `6d6545b0…` (payer wallet) | It spends output `80ff41d4…` (from the funding transaction) and creates the channel output `8e13565b…` (script-locked) plus private change. Fee 15,919, paid inside the transfer from the private input. **No account component is referenced. The deposit amount is not shown.** |
| Settle `7f61a7bc…` (service wallet) | It spends the channel output `8e13565b…`, reads fee config `37df6af5…`, and pays the fee from **service account `c095ae8e…`** (28,808). Its outputs are payer change (script-locked), a payout on the one-time key `b8037587…`, and the protocol fee on the treasury key `728870ae…`. **The payout, fee and change amounts are commitments only.** The call count and prices do not appear. |

**What this shows:**
- **Hidden:** the deposit (20,201 µT), the payout (5,000 µT), the protocol fee and payer change, the five calls and their prices. The payer's account is never named.
- **Linked by design:** the open and the settle share the channel output, so anyone can pair them. The settle names the service's fee account. The fee output names the treasury key, so every Ghostline settlement is countable.
- **Linked by this test's funding:** the open spends an output created by A's funding transaction, and the service account was created by A's second transaction. With independent funding (separate burns or unrelated sources) and a fee account not tied to the service's public identity, these links disappear. That has not been tested.
- **Not inferable from chain data:** which one-time payout key belongs to the service. walletd found it only because it holds the service's view key.

## What observers can still see

| Transaction | Public information |
|---|---|
| Every transaction | Fee and its funding source, transaction existence and time, and the public transaction and substate graph |
| Open | A script-locked channel output was created, plus private change. The deposit amount is a commitment. The open names no account component. Funding can still identify the payer. |
| Settle | The executed branch, opaque channel id, payer public key and condition root passed to the template, the channel authorization key, the one-time payout key, the fee config it reads, and the fee account that paid the network fee. Its three outputs (service payout, payer change, protocol fee) are commitments only. |
| Refund after expiry | The refund branch, the payer's authorization data and the spend of the channel output. The mainnet refund finalized and the deposit reached the payer's wallet; the observer test was run on a settle, not on the refund. |
| Treasury key holder | The protocol fee output is encrypted to the treasury key, so its holder can recover each fee amount and infer the service payout at the fixed 1% rate. |

The service receives the amount and can open its output. Fees, timing, graph shape and authorization metadata can still correlate activity.

The service knows the declared deposit limit because it is part of the channel parameters used to validate vouchers. That is separate from the channel output's cryptographic opening: in ordinary individual flows, the payer's secret deposit mask stays in its local channel file or wallet and is not sent to the service. (The service does receive each voucher's separate output mask so it can open the payment.)

`ghostline channel trace` can combine a call count and price range from the local service ledger with the indexer status of transaction IDs supplied by the operator. It is an operator-assembled summary, not a cryptographic proof of the call log or a chain-wide scan. It does not include channel masks, keys, or voucher hex. Call totals are business-sensitive; share the receipt only when intended.

## Explorer comparison

As a concrete public-transfer example, this [Ethereum USDC transaction](https://etherscan.io/tx/0xd4b176e5bbb45ca3074f7e5bcb178c2347a7ddf26b17ae99e3d87cb8cc0b5fea) exposes the transfer amount and recipient in Etherscan's transaction summary. Ghostline's [mainnet settle from the observer test](https://indexer-a.ootle.tari.com/transactions/7f61a7bccf9c139284dc15701a850a56933321e4e3d9ed3109ff595d4f3401a7) instead shows confidential output commitments. These are examples of different transaction designs, not matched payments on the same network. Private payment schemes can also be built on EVM chains.

## Reducing correlation

- Generate separate, fresh payer and service signing keys for every channel. `ghostline keygen --role payer --out <private-file>` writes an owner-only 0600 key file, and `ghostline sign --secret-file <private-file>` uses the payer file. Generate the service signer in its wallet with `ghostline wallet key-create`; use its public key for that channel and its wallet `KeyId` for settlement. Since each channel settles once, that key is used for one channel and one settlement. Do not reuse it across channels. This removes repeated-key correlation; it does not hide the two roles inside one transaction or undo funding, timing, and fee links.
- Give the payer and the service their own wallets. The mainnet observer test did, and the transactions did not name the payer's account.
- Fund the payer and the service from unrelated sources. The observer test funded both from one wallet, which linked them; independent funding has not been tested.
- Pay settle fees from an account that is not tied to the service's public identity. The settle names its fee account.
- Address the payout to a wallet destination (`ghostline wallet payout-destination`, see [settling and payouts](settle.md)). The one-time payout key does not identify its owner in public fields.
- Batch calls off-chain and settle once. This hides individual calls from the chain, not from the service.
- Settlement vouchers may spend at most `floor((deposit - 1) / 101) * 100` µT: the protocol fee and 1 µT of payer change are reserved. That keeps every settlement at three outputs, so the output count does not reveal whether the whole deposit was spent.
- Avoid treating timing or rounded amounts as privacy guarantees; they only reduce some matching clues.

## Earlier Esmeralda evidence

The rest of this page records what was verified on the Esmeralda testnet with the template published on 7 Nov 2026. That template predates the protocol fee: its settlement had two stealth outputs, where mainnet settles have three.

A fresh Esmeralda run used the newly published fixed-output template: open a 10,000 µT channel, accept 20 HTTP `/fortune` payments at 10 µT each off-chain, then settle once. The template publish, open, and settlement transactions finalized. The settlement's stealth transfer statement had an empty covenant-claim list, zero revealed input amount, no revealed output, and exactly two stealth outputs with one-unit minimum-value promises. The deposit, service amount, and payer remainder were not exposed in those fields. The open paid 15,200 µT; the settlement paid 17,744 µT against a 17,756 µT cap. The 20 calls and 200 µT accepted total come from the local service ledger, not the chain. Fees remain public. The transaction links are in [recorded evidence](https://ghostline-ui.vercel.app/evidence.html).

An earlier settlement showed why this change was needed: the generic wallet builder added an optional covenant-balance claim that exposed the deposit. Ghostline's channel does not use `Covenant::BalancePreserved`; its global stealth balance proof still enforces conservation, while the script witness enforces voucher, authorization, output-shape, and expiry rules. The current SDK omits the unused claim. In-process tests also confirm a real balance-preservation covenant still rejects a missing claim.

The claim-free settlement and expiry-refund paths have both been verified live on the latest Esmeralda template. A fresh 10,000 µT channel was refunded by its payer at expiry epoch 12466. The finalized refund had no covenant claims, zero revealed input amount, no revealed output amount, and one stealth output; the wallet and indexer reported the channel output as spent. The public record confirms the close, but we did not locally unblind the recipient output, so this evidence does not independently establish the refunded amount. The transaction link and sanitized fields are in [recorded evidence](https://ghostline-ui.vercel.app/evidence.html).

The latest template was also exercised with two batch-enabled channels: each local ledger accepted 20 HTTP calls, then one pooled settlement spent both channel outputs and created three stealth outputs. The pooled service output was confirmed locally using its one-time payout key; the amount is omitted from public evidence. This validates execution of the batch entrypoint, not coordinator privacy: the local builder received both deposit masks and values, and observers can see that both channels participated in the same transaction.

Pooled batch settlement is research-only. The current SDK prototype requires every channel's deposit value and mask in one process to construct Ootle's aggregate range and balance proofs. A hosted coordinator would therefore learn those openings. No Ghostline hosted flow uses this prototype, and it has not been run on mainnet. It does not provide coordinator privacy or unlinkability between channels grouped into one batch. Until a reviewed multi-party proof protocol exists, use individual settlement.

The SDK uses the template's original script-path settlement and refund conditions. No key-path escrow or custom cryptography is involved.

Unaudited. Keep mainnet deposits small. Fees and the open and close records are public.
