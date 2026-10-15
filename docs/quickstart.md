# Esmeralda quickstart

This guide runs Ghostline's MCP paid-call path against a local paid API and an Esmeralda wallet daemon. The open → call → settle sequence has been exercised live on Esmeralda. Wallet and network identifiers below must come from your own deployment and wallet; this repository does not publish a shared demo template or account.

Ghostline is early development, unaudited, and testnet only. Do not use real funds.

## Prerequisites

- The Ghostline executables. On Linux x86-64, install them with the command in [README](../README.md#install-the-command-line-tools). Otherwise, build them from source using the commands and native dependencies in [README](../README.md#install-the-command-line-tools).
- A `tari_walletd` connected to Esmeralda at `http://127.0.0.1:5100` (or set the URL below), with a funded opener account and a fee account. Settlement fees are public. A separately funded fee wallet reduces direct linkage but does not guarantee privacy.
- The real Esmeralda indexer URL, network tag, deployed Ghostline template hash, and wallet account names/components for your setup.
- An MCP client that supports elicitation if you enable the human approval gate. Configure it to launch the installed `ghostline-mcp` executable.

Supply `GHOSTLINE_PAYER_SECRET` from a private secret manager to sign vouchers. Supply `GHOSTLINE_CLAIM_SECRET` from the opener wallet's `view_only_key` at index 0 so Ghostline can unblind the opened output. They are separate keys. Keep both, wallet credentials, and the channel file private. The channel file becomes sensitive after the channel is opened because it contains the deposit mask. Never put secrets in tool arguments, shell history, or a shared MCP server configuration.

## 1. Prepare a channel file

Choose a unique 32-byte nonce and a future expiry epoch with a safety margin. Get the payer and service public keys from your own key setup. Use integer micro-units for `--deposit`; for example, `100000` is 0.1 tTARI when the resource uses 1,000,000 µTARI per tTARI. Set the deployed template hash and network parameters from your deployment records.

The paid API also needs the payer condition-tree root. Compute it from the exact open parameters with `ghostline spec` and save the printed `payer_root`:

```bash
ghostline spec \
  --network-tag <esmeralda-network-tag> \
  --deposit 100000 \
  --payer-hex <payer-public-key-hex> \
  --service-hex <service-public-key-hex> \
  --expiry-epoch <future-epoch> \
  --nonce-hex <unique-32-byte-nonce-hex> \
  --template-hex <deployed-template-hash-hex>
```

Create the channel record. Values in angle brackets are placeholders, not valid network identifiers:

```bash
ghostline channel init \
  --out /secure/path/channel.json \
  --network-tag <esmeralda-network-tag> \
  --deposit 100000 \
  --payer-hex <payer-public-key-hex> \
  --service-hex <service-public-key-hex> \
  --expiry-epoch <future-epoch> \
  --nonce-hex <unique-32-byte-nonce-hex> \
  --template-hex <deployed-template-hash-hex> \
  --indexer-url <esmeralda-indexer-url> \
  --ootle-network esmeralda \
  --max-epoch <maximum-accepted-epoch> \
  --opener-account <opener-wallet-account-name> \
  --opener-wallet-url http://127.0.0.1:5100 \
  --fee-account <service-fee-account-component> \
  --fee-account-name <service-fee-wallet-account-name> \
  --fee-wallet-url http://127.0.0.1:5100 \
  --ledger /secure/path/ledger.json \
  --session /secure/path/session.json
```

The CLI writes a record only; it does not open the channel. Confirm the wallet, indexer, template, fee account, and fee funding before proceeding. The MCP open tool defaults to a dry run. Review its fee quote, then set both `GHOSTLINE_OPEN_SUBMIT=1` and `GHOSTLINE_OPEN_MAX_FEE` in the MCP server environment to broadcast. After open, protect `channel.json` as secret-bearing material.

## 2. Start the paid API

In a separate terminal, initialize the service ledger with the same channel parameters, then serve the local `/fortune` endpoint at 10 µT per call:

```bash
ghostline-paid-api init \
  --ledger /secure/path/ledger.json \
  --session /secure/path/session.json \
  --network-tag <esmeralda-network-tag> \
  --deposit 100000 \
  --payer-hex <payer-public-key-hex> \
  --payer-root-hex <payer-root-from-ghostline-spec> \
  --nonce-hex <unique-32-byte-nonce-hex>

ghostline-paid-api serve \
  --bind 127.0.0.1:8787 \
  --ledger /secure/path/ledger.json \
  --price 10 \
  --service-key-hex <service-public-key-hex> \
  --min-expiry-epoch <minimum-accepted-expiry> \
  --max-expiry-epoch <maximum-accepted-expiry>
```

The service public key must be the key committed to the channel's settle branch. Choose the expiry bounds from the service's operating policy; both are inclusive epochs. The HTTP 402 response includes this key and range alongside the price and channel id, so a payer can check the intended recipient before opening.

Keep the service ledger and payer session at separate paths. The channel record and paid API use the same ledger path; the channel record and MCP process use the same session path. `paid-api init` creates both snapshots; opening the channel through MCP then uses the matching session without resetting its cumulative total. Do not run `init` again over existing files.

## 3. Connect the MCP server and pay

Configure the MCP client to start `ghostline-mcp` with this environment. Set the policy vendor to the service public key. Keep broadcast flags unset until you are ready to transact.

```text
GHOSTLINE_CHANNEL_FILE=/secure/path/channel.json
GHOSTLINE_POLICY_BUDGET=10000
GHOSTLINE_POLICY_MAX_PER_CALL=10
GHOSTLINE_POLICY_VENDOR_HEX=<service-public-key-hex>
GHOSTLINE_POLICY_AUDIT_LOG=/secure/path/ghostline-policy.jsonl
# Optional approval rules:
GHOSTLINE_POLICY_HUMAN_THRESHOLD=100
GHOSTLINE_POLICY_APPROVE_NEW_VENDOR=1
GHOSTLINE_INDEXER_URL=<esmeralda-indexer-url>
```

The optional JSONL audit file records timestamps, vendor public keys, prices, cumulative amounts, and approval or rejection reasons. Treat it as private business data. When configured, an audit-write failure prevents the policy engine from approving the payment.

In the MCP client, call `ghostline_open_channel` with `{}` while `GHOSTLINE_OPEN_SUBMIT` is unset. Review `estimated_fee` and `required_fee` in the dry-run result. Choose a `GHOSTLINE_OPEN_MAX_FEE` cap that covers the estimate, remembering the fee can change before broadcast. Then restart/configure the server with both `GHOSTLINE_OPEN_SUBMIT=1` and `GHOSTLINE_OPEN_MAX_FEE=<reviewed-µT-cap>` and call `ghostline_open_channel` again. A live open is refused if the cap is missing or zero. The cap bounds the opener's fee payment; it is not an exact quote guarantee. Then call `ghostline_call_paid` with:

```json
{"url":"http://127.0.0.1:8787/fortune"}
```

Use `GHOSTLINE_POLICY_REQUIRE_HUMAN=1` to approve every payment, `GHOSTLINE_POLICY_HUMAN_THRESHOLD=<µT>` to gate calls at or above a price, or `GHOSTLINE_POLICY_APPROVE_NEW_VENDOR=1` to gate each vendor's first payment. The audit file preserves first-vendor history across restarts. Use an MCP client whose elicitation prompt is shown to a person; the tool cannot accept a caller-supplied `attested` flag. Approval still depends on the MCP host displaying its elicitation UI honestly.

## 4. Settle and inspect

With `GHOSTLINE_CLOSE_SUBMIT=1`, call `ghostline_close_channel` with `{}` to broadcast close. Set `GHOSTLINE_SETTLE_FORCE=1` only if settling now is intentional and the channel has not reached expiry. Otherwise the close tool follows the configured settlement window and expiry rules. Confirm finality using the indexer/wallet result and inspect the channel with `ghostline_channel_status`.

The recorded live MCP run accepted ten 10 µT calls and finalized one settlement of 100 µT on Esmeralda. Fees were paid separately and were visible. A later fresh-wallet rehearsal used a new isolated wallet profile funded by a 150,000 µT transfer from the operator's main wallet: a 100,000 µT channel open, one local 10 µT `/fortune` call, and a 10 µT settlement all completed. The open finalized with a 15,217 µT fee (18 µT above the dry-run requirement); the settle finalized with a 17,637 µT fee and no overcharge. This is a valid multi-wallet test: the wallet keys are separate and the complete flow works. The transfer is visible on Esmeralda, so an outside observer can link the new wallet's funding to the main wallet. That visibility only matters when testing whether funding provenance is hidden. A spent channel output cannot be settled or refunded again.

The 1,000-call run proves the batching flow, not that a 10 µT price is economical. Its 10,000 µT of payments settled with a 17,640 µT service-side fee; that fee alone equals 1,764 calls at 10 µT. A separate open required 15,199 µT. Fees change, and the payer and service may pay different transactions, so rerun dry-run estimates and set a price and budget for the deployment.

To exercise 1,000 HTTP paywall calls on one channel, run the paid API client with the payer secret already supplied through its private process environment:

```bash
ghostline-paid-api call \
  --url http://127.0.0.1:8787/fortune \
  --session /secure/path/session.json \
  --calls 1000 \
  --vendor-hex <service-public-key-hex> \
  --budget 10000 \
  --max-per-call 10
```

The 1 tTARI example deposit covers 1,000 calls at 10 µT each. The live HTTP run recorded 1,000 accepted calls totaling 10,000 µT, then one Esmeralda settlement finalized. The MCP stdio path is separately live-proven for ten calls and a single settlement. Neither result implies the wallet or keys in your own setup have been tested.

For offline and CLI examples, see [README](../README.md). For privacy limits, read [privacy notes](privacy.md) and the [threat model](threat-model.md).

## CI smoke test

The Esmeralda open/settle steps require deployment-specific wallet credentials and are not broadcast by CI. To exercise the local HTTP 402 quote → validated voucher → paid response path and opt-in aggregate metrics without a network or wallet, run `bash scripts/quickstart-smoke.sh`. CI runs this script; it uses temporary keys and files, binds only to loopback, and removes its state on exit. It does not claim to test a live channel open or settlement.
