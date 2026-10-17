# What Ghostline hides, and what it does not

Ghostline is a **pay-per-call channel**. You lock a deposit once, sign vouchers off-chain for each call, and settle once. The aim is that a **public observer** cannot reconstruct how often you called, what you paid per call, or which HTTP endpoints you used.

It is **not** anonymity against the service you are paying. That service sees every voucher you send it.

## Hidden from third parties (between open and settle)

- Number of calls
- Per-call prices and timing
- Which APIs or vendors you hit

The current commitment-based template keeps the exact service amount and payer remainder out of public transaction fields when settling once. A live Esmeralda test confirmed this for one 10 µT call. An earlier 1,000-call run proved batching, but used the older template and did not prove amount confidentiality; do not use that run as evidence for this privacy property.

`ghostline channel trace` can export a privacy receipt in text or JSON. It combines the call count and price range from the local service ledger with the indexer status of transaction IDs supplied by the operator. The receipt does not contain channel masks, keys, or voucher hex. The off-chain totals in it are still private business data: share the receipt only if you intend to disclose those totals to its recipient. It is an operator-assembled summary, not a cryptographic proof of the call log and not a chain-wide scan.

## Visible on-chain

| When | What an observer can see |
|---|---|
| Every transaction | The fee amount, its public funding source (for Ghostline's component-paid flow, the account component), transaction existence and time, and public transaction/substate graph |
| Open | That a stealth output was created (not your identity, if you funded it from an unlinkable UTXO) |
| Settle | The settle branch, full channel deposit amount (revealed by Esmeralda's covenant claim), opaque channel id, payer public key and payer condition root passed to the template, service output authorization key, output commitments, and output count |
| Refund after expiry | The refund branch, payer authorization data, output commitments, and output count. The refund output value is hidden, but the same covenant claim reveals the input deposit. |

The current template signs a Pedersen commitment to the service output instead of a plaintext voucher amount. It requires the signed commitment to appear as the service output, disallows revealed-value inputs/outputs and extra inputs, and requires a one-unit minimum-value promise on every output. This version was published to Esmeralda and passed a live open → paid request → settle run. Inspection of the finalized settlement transaction showed two stealth outputs with one-unit minimum-value promises and no plaintext service amount or remainder in the public fields.

The live test used a 1,000 µT deposit and a 10 µT service voucher. Esmeralda's serialized transaction revealed the deposit amount in its covenant claim, while keeping the 10 µT service amount and 990 µT remainder hidden from public observers. Fees, timing, the selected branch, an opaque channel id, payer and service authorization data, output commitments, and the number of outputs remain visible. One output indicates no payer change output; two indicate a change output, though neither output's exact value is disclosed by the promise. The service receives the amount and mask off-chain and can open its output. Funding history and transaction correlation can still leak information.

**Privacy target vs current behavior:** Ootle requires transaction fees to be public, but Ghostline also currently reveals the full deposit when the channel is spent. So the fee is not the only public monetary amount today. Transaction existence and timing, graph shape, script-path leaf, public authorization data, output commitments, and output count are also visible. Do not claim that observers see only fees.

The current claim is generated for each script-path input condition root. Its `revealed_amount` is the input value minus outputs that remain locked under that same root. Ghostline's settle outputs use other authorization roots, so no value remains in the channel root and the claim reveals the full deposit. Preserving payer change under that root would instead reveal the net outflow, which can expose the service payout. Hiding both the deposit and payout therefore needs a different channel close/escrow design; changing the output promise or presentation cannot solve it.

### Reducing correlation with the current design

These practices reduce the chance that public facts identify the same payer or service, but they do not hide the deposit amount or the public transaction graph:

- Generate distinct payer and service signing keys for each channel. Do not reuse an account identity key as a channel key. `ghostline keygen` creates a keypair; keep its secret in a private file or wallet and use the public half for that channel only.
- Fund the channel from an unlinkable stealth UTXO, and pay fees from a separately funded account. A separate account created from the same wallet seed or funded by a public transfer is still correlatable.
- Avoid unusually precise deposit amounts and avoid settling immediately after opening when the application can tolerate a delay. Neither practice hides the amount; they only make timing and amount matching less conclusive.
- Batch calls off-chain and settle once. The service still knows each call and amount, and public observers still see the channel open and close.

### Architecture required to hide the deposit

The current lock uses a script-path spend. Ootle's wallet crypto constructs a public covenant-balance claim for each script-path input root; when the channel closes, that claim reveals the deposit. A `KeyAndScript` output does not avoid this claim on a script-path spend, and its key path would let that key spend without the channel rules. The native TARI resource is a stealth UTXO, not a confidential vault balance. Those are protocol constraints of the current approach, not missing CLI flags.

The plausible trust-minimized direction is a key-path escrow controlled by a vetted two-party Schnorr signing protocol, with the payer and service co-signing each settlement and a safe expiry refund path. A test-only prototype now signs an actual Ootle key-path stealth spend using FROST 0.6.0 and Tari's Ristretto challenge; the engine accepts it without a covenant balance claim. The test creates a signed Ghostline voucher and matches its commitment to the service output. This is feasibility evidence only. It uses a custom Tari challenge adapter, and does not prove secure distributed signing, integration with the paywall/session flow, or refund recovery on a network. The FROST project's published audit covers the stock v0.6.0 implementation, not the custom adapter ([scope](https://github.com/ZcashFoundation/frost)). Do not ship this design without independent cryptographic review, a complete channel redesign, and adversarial testing. A custodian or service-controlled key could hide the claim but would let that party steal the full deposit, so it is not an acceptable privacy fix. The detailed candidate and proof status are in [`deposit-confidentiality-redesign.md`](deposit-confidentiality-redesign.md).

This means Ghostline cannot currently satisfy the goal “observers see only fees.” The next safe implementation step depends on an audited Ootle-compatible threshold-signing path; until then, the honest product claim is that service amount and payer remainder are confidential at settlement, while the deposit and fees are public.

The receipt explains this boundary explicitly. It lists only the transaction IDs the operator names; it does not claim to have found every related transaction or report the exact fee amount. CLI report, call-history, and trace files written with `--out` are owner-only on Unix because they contain private spending activity. If output is redirected by the shell, use `umask 077`; the CLI cannot set permissions on a shell-created file.

## Things that can re-identify you anyway

- Paying fees from an account that is already linked to you (including funding a “service” fee account from the opener)
- Settling immediately after a distinctive deposit amount
- The service's own logs
- IP addresses and TLS metadata on the HTTP path (outside Ootle)

## What the software does not yet do

- Listing Ghostline UTXOs from the chain indexer (`wallet utxos` / live `wallet open` read a wallet-stored transaction via `transactions.get` and emit `unblind --utxo-json`; wallet-built opens keep the channel output in `fee_instructions`; `inspect-tx` still needs a sealed hex you already have; `stealth_utxos.list` is not the source of truth)
- TypeScript signing of `ghostline.channel.voucher.v1` is covered by a shared Rust/TS vector; WASM/Ootle JS wallet signing is a separate path
- Hiding the public faucet (or other funding) of the fee-paying account. A distinct same-seed fee component is proven; a second-seed fee wallet is also proven live (opener fee balance unchanged). Same-seed accounts remain correlated to anyone who has that seed.
- Enforcement of spending limits if the agent signs without the policy engine

Unaudited. Testnet only. Do not use with real funds.
