# What Ghostline hides, and what it does not

Ghostline is a **pay-per-call channel**. You lock a deposit once, sign vouchers off-chain for each call, and settle once. The aim is that a **public observer** cannot reconstruct how often you called, what you paid per call, or which HTTP endpoints you used.

It is **not** anonymity against the service you are paying. That service sees every voucher you send it.

## Hidden from third parties (between open and settle)

- Number of calls
- Per-call prices and timing
- Which APIs or vendors you hit

Those facts never go on-chain if you settle once at the end.

## Visible on-chain

| When | What an observer can see |
|---|---|
| Every transaction | Fees, and which account component paid them |
| Open | That a stealth output was created (not your identity, if you funded it from an unlinkable UTXO) |
| Settle | The settle branch, the voucher amount, the remainder, the channel id bound into the template, the service key on that leaf |
| Refund after expiry | The refund branch, the full deposit returning to the payer key |

Because remainder + claimed = deposit, **settlement reveals the deposit**.

## Things that can re-identify you anyway

- Paying fees from an account that is already linked to you (including funding a “service” fee account from the opener)
- Settling immediately after a distinctive deposit amount
- The service's own logs
- IP addresses and TLS metadata on the HTTP path (outside Ootle)

## What the software does not yet do

- Listing Ghostline UTXOs from the chain indexer (`wallet utxos` / live `wallet open` read a wallet-stored transaction via `transactions.get` and emit `unblind --utxo-json`; wallet-built opens keep the channel output in `fee_instructions`; `inspect-tx` still needs a sealed hex you already have; `stealth_utxos.list` is not the source of truth)
- TypeScript signing of `ghostline.channel.voucher.v1` is covered by a shared Rust/TS vector; WASM/Ootle JS wallet signing is a separate path
- Cryptographic unlinkability of settle fees from the opener (a distinct, faucet-funded fee component is proven; the accounts still share one wallet seed)
- Enforcement of spending limits if the agent signs without the policy engine

Unaudited. Testnet only. Do not use with real funds.
