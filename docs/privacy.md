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
| Settle | The settle branch, opaque channel id, payer public key and payer condition root passed to the template, service output authorization key, output commitments, and output count. The local claim-free close omits the deposit-revealing covenant claim; Esmeralda verification is pending. |
| Refund after expiry | The refund branch, payer authorization data, output commitments, and output count. The local claim-free refund omits the deposit-revealing covenant claim; Esmeralda verification is pending. |

The current template signs a Pedersen commitment to the service output instead of a plaintext voucher amount. It requires the signed commitment to appear as the service output, disallows revealed-value inputs/outputs and extra inputs, and requires a one-unit minimum-value promise on every output. This version was published to Esmeralda and passed a live open → paid request → settle run. Inspection of the finalized settlement transaction showed two stealth outputs with one-unit minimum-value promises and no plaintext service amount or remainder in the public fields.

The previously recorded live test used a 1,000 µT deposit and a 10 µT service voucher. That transaction's generic wallet builder included a covenant claim that revealed the deposit, while keeping the 10 µT service amount and 990 µT remainder hidden. The current SDK change manually builds the same global value-conserving proof but omits that optional claim. Local engine tests show the settlement and refund execute without a claim, while a channel with an actual `BalancePreserved` covenant still rejects the missing claim. A fresh Esmeralda run is required before claiming the deployed close hides the deposit. Fees, timing, the selected branch, an opaque channel id, payer and service authorization data, output commitments, and output count remain visible. One output indicates no payer change output; two indicate a change output, though neither output's exact value is disclosed by the promise. The service receives the amount and mask off-chain and can open its output. Funding history and transaction correlation can still leak information.

**Privacy target vs current behavior:** Fees remain public. The prior Esmeralda release revealed the deposit; the local claim-free SDK change is not yet confirmed on Esmeralda. Until that live retest passes, treat the deployed product as still revealing the deposit. Transaction existence and timing, graph shape, script-path leaf, public authorization data, output commitments, and output count are also visible. Do not claim that observers see only fees based on local tests alone.

The generic wallet transfer builder creates a balance claim for each script-path input root. That claim is optional at the transfer-statement level: the engine checks it when a spend condition uses `Covenant::BalancePreserved`, and a missing claim makes that covenant fail. Ghostline's close rules do not use that covenant. They instead verify the voucher commitment and output shape (settlement), or require the payer key and `AfterEpoch` (refund), while the global stealth balance proof still enforces value conservation. The SDK now constructs the global proof with an empty covenant-claim list and preserves the script witness. This avoids revealing the deposit without replacing the channel's authorization or expiry rules.

### Reducing correlation with the current design

These practices reduce the chance that public facts identify the same payer or service, but do not hide the public transaction graph and do not change the prior live transaction's deposit disclosure:

- Generate distinct payer and service signing keys for each channel. Do not reuse an account identity key as a channel key. `ghostline keygen` creates a keypair; keep its secret in a private file or wallet and use the public half for that channel only.
- Fund the channel from an unlinkable stealth UTXO, and pay fees from a separately funded account. A separate account created from the same wallet seed or funded by a public transfer is still correlatable.
- Avoid unusually precise deposit amounts and avoid settling immediately after opening when the application can tolerate a delay. Neither practice hides the amount; they only make timing and amount matching less conclusive.
- Batch calls off-chain and settle once. The service still knows each call and amount, and public observers still see the channel open and close.

### Deposit confidentiality implementation status

No key-path escrow or custom cryptography is needed. The local implementation preserves Ghostline's script-path settlement and refund conditions, but constructs the transfer statement without the generic builder's unused covenant-balance claim. The adversarial local integration tests exercise voucher/signature checks, output authorization, value conservation, expiry, payer-only refund, replay, and the behavior of real balance covenants. The code is not yet validated against Esmeralda; keep the live disclosure caveat until a fresh open → settle and open → expiry → refund run confirms the serialized transactions contain no claims. See [`deposit-confidentiality-redesign.md`](deposit-confidentiality-redesign.md) for the test status and next steps.

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
