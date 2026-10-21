# Sapient integration spike

**Checked:** 2026-10-21 against Tari's [integration reference](https://universe.tari.mw/integration/llms-full.txt). The browser simulation stays local by default; the Sapient connector loads only after a user starts the optional live flow.

## Verified wallet surface

- `window.tari.request({ method: "tari_getNetwork" })` reads the network without connecting. Network values must be compared case-insensitively with `esmeralda`.
- `tari_requestAccounts` prompts for an account; `tari_getCapabilities` must be used for feature detection; `tari_getBalances` returns native units with a resource divisibility field.
- `tari_createTransactionRequest` with `kind: "sendPrivately"` takes `resourceAddress`, `recipientWalletAddress`, and `amount`, plus optional fee/memo/proof fields. It has no arbitrary spend-condition or script-path argument. It cannot directly create Ghostline's script-locked channel output.
- `withdrawStealthAndExecute` exposes the selected amount as a `Bucket` and runs caller-supplied follow-up instructions. A prototype can pass that bucket to a custom template method and turn it into Ghostline's script-locked output, but it does **not** preserve deposit privacy: the stealth transfer statement must set its public `revealed_amount` to the amount being withdrawn. For a channel open, that reveals the exact deposit. This route is therefore rejected for Ghostline's privacy-preserving flow, even though the template call itself is technically possible.
- `Bucket::stealth_transfer` in the installed template library only supplies the input bucket when `revealed_amount` is nonzero, and the value is included in the transfer statement. A passing template test would not change that public disclosure. Do not describe this as a masked-deposit open or expose it as a live demo path.
- The documented `scriptPathSpend` capability is limited to supported HTLC claim/refund flows; the current integration reference does not document arbitrary script-path spending for Ghostline's custom payer-expiry refund condition. Even a privacy-preserving open would need a supported way for the payer to reclaim the channel output after expiry.
- Tari deliberately exposes no arbitrary-message signing method. The wallet ownership challenge is signed under Tari's fixed domain and cannot authorize Ghostline vouchers. Voucher signatures therefore use an ephemeral browser key in the signed Ghostline channel parameters. Keep that private key in `sessionStorage`; it must never be sent to the API.
- User rejection is code `4001`; not-connected is `4100`; unsupported method is `4200`; network rejection is `-32603`. Treat these separately and keep the simulated demo available.

## Relay-path assessment

The contest plan's relay path C is technically distinct from the unsupported direct-open path: Sapient can privately transfer a bounded Esmeralda test amount to a service wallet, and the relay can use its wallet daemon to create the script-locked channel. This makes the relay a custodian of the test deposit and gives its operator the ability to observe the funding transfer and channel state. It is not equivalent to the user's wallet opening its own Ghostline channel.

The SDK and CLI support a separate expiry-refund key (`--refund-hex` on channel setup). The browser retains the voucher-signing key, while a relay key appears only on the `AfterEpoch` refund branch. The existing settlement template verifies vouchers against the browser's payer public key, so possession of the refund key alone cannot create an early settlement voucher. SDK and contract-engine regression tests confirm the separation. The relay creates channels, registers per-channel quote, authorization, and payout keys, and issues authenticated encrypted recovery receipts after opening and each accepted call. The browser retains the receipt in tab session storage and can present it to `/demo/recover` or with the next signed paid call to restore ledger and signer files after process state is lost. The receipt is a bearer recovery credential, encrypted with `GHOSTLINE_DEMO_STATE_KEY`; it is sent only to the configured relay over HTTPS. The payer signing secret, seed phrase and Sapient credentials stay out of relay requests.

The paid API now has an opt-in relay API for `/demo/session`, `/demo/open`, `/demo/settle`, `/demo/refund`, and `/demo/status`, plus a `ghostline-paid-api relay` startup mode for a dynamic channel directory. Its open path checks that Sapient funding finalized and that exactly one matching private XTR output belongs to the relay. The browser keeps its voucher-signing key local, retries the same funding transaction without asking for a second transfer, submits real signed vouchers, and exposes an expiry-refund action. Settlement finality is checked through the indexer. These are implementation paths, not a live deployment: no Render credentials or wallet daemon are configured in this workspace, and no Sapient/Esmeralda relay run has verified them.

The default live demo amount is 51 µT: five 10 µT calls can settle 50 µT, leaving only the template's required 1 µT fixed-shape change output. That final micro-unit remains encrypted to the ephemeral payer key and is not returned to Sapient after settlement. An unsettled channel can still take the expiry branch and return the full 51 µT privately. Do not increase `GHOSTLINE_DEMO_DEPOSIT` without implementing and verifying payer-change recovery.

Render packaging is prepared in `deploy/` with a pinned Docker build, loopback-only walletd startup, deterministic opener/fee accounts, Blueprint, and operator instructions. Startup emits only the public fee-account address so its testnet fee float can be funded. Shell syntax and Blueprint YAML parse locally; the Docker image could not be built because the local Docker daemon is unavailable. No Render deployment or Sapient run has occurred. The wallet seed, database password, and matching index-0 view-only claim key must be entered as Render secrets; the claim key is separate from the seed phrase and voucher-signing key.

The relay now writes its session map, channel-to-key/path map, per-channel quote keys, funding idempotency records, and submitted settle/refund transaction IDs atomically to `GHOSTLINE_LEDGER_PATH/relay-state.json` with owner-only permissions. Startup restores those records and per-channel payment terms from the same state directory; a process-restart unit test verifies the saved session, channel, and funding retry can be recovered. The channel ledgers and channel files remain in `GHOSTLINE_LEDGER_PATH/channels`.

Recovery receipts remove the relay-ledger dependency on Render's ephemeral filesystem, but channel signing still depends on walletd being able to recreate its seed-derived key references after replacement. That behavior has not been verified on a deployed Render instance. An uptime monitor reduces free-tier idle sleeps; it cannot prevent deploys, crashes, or host restarts. Do not describe the hosted path as restart-proof until walletd recovery is tested with the actual deployment. Do not deploy real-money credentials or expose a wallet daemon outside loopback.

## Required live-mode checklist

- [ ] For a direct, non-custodial open, wait for Sapient support for custom-condition stealth output creation that keeps the input amount hidden; do not use `withdrawStealthAndExecute` with a nonzero `revealed_amount` for Ghostline deposits.
- [ ] For the custodial relay alternative, prove private Sapient funding → walletd channel open → paid calls → settlement → expiry refund returned privately to the funder.
- [ ] Demonstrate recovery after an unexpected Render restart, including the channel mask, latest accepted voucher, and any refund still due.
- [ ] Persist the channel mask and wallet signer references in a protected location, with explicit restart recovery; Render's free ephemeral filesystem alone is insufficient.
- [ ] Demonstrate voucher acceptance, service settlement, finalized transaction lookup, and payer remainder recovery after expiry.
- [ ] Verify per-channel service authorization and payout keys are fresh and not reused.
- [ ] Exercise connector absent, wrong network, low balance, rejected popup, unsupported method, chain rejection, cold backend, and pending transaction fallbacks.
- [ ] Record exact tested Sapient version and new transaction identifiers in `demo/ui/evidence.json`.

## Transaction request guidance

The live flow uses the persistent request trio so page reloads can resume a wallet approval:

1. `tari_createTransactionRequest` from an explicit user action.
2. Poll `tari_getTransactionRequest({ requestId })` until its status is not `pending`.
3. Call `tari_submitTransactionRequest({ requestId })` only after approval.

Do not probe a public Tari node adversarially. A failed transaction request is a product integration failure, not evidence of an Ootle vulnerability.
