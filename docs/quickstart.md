# Esmeralda quickstart

Run a local paid API, open an Esmeralda channel, pay through the CLI or MCP, and settle once. Install the executables using the [installation guide](install.md); no repository clone is needed for the downloaded commands.

Wallet and network identifiers must come from your deployment and wallet. This is an operator setup guide with placeholders, not a hosted demo or a preconfigured wallet.

Ghostline is early development, unaudited, and testnet only. Do not use real funds.

## Prerequisites

- The installed `ghostline`, `ghostline-paid-api`, and, for the MCP path, `ghostline-mcp` executables.
- A compatible Ootle wallet daemon connected to Esmeralda at `http://127.0.0.1:5100` (or your own loopback URL), with funded opener and service fee accounts. Ghostline uses the daemon's `None` authentication flow; do not expose this endpoint publicly. A separately funded fee wallet can reduce direct linkage but does not guarantee privacy.
- The real Esmeralda indexer URL, network tag, deployed Ghostline template hash, and wallet account names/components for your setup.
- A service signing key available in the fee wallet, matching the channel's service public key. Obtain its KeyId JSON reference from the wallet operator and save that JSON value in an owner-only file. This is a wallet-local key reference, not the service secret and not the whole RPC response. The fee account owner key is a separate signer unless the deployment deliberately uses the same key for both roles.
- An MCP client that supports elicitation if you choose MCP with a human approval gate.

Supply `GHOSTLINE_PAYER_SECRET` through the signing process's private environment to sign vouchers. Supply `GHOSTLINE_CLAIM_SECRET` from the opener wallet's `view_only_key` at index 0 so Ghostline can unblind the opened output. They are separate keys. The examples assume the opener's default receiving address uses that key; confirm your wallet's key index if you change the destination. Keep wallet credentials and channel state private. The channel file becomes sensitive after opening because it contains the deposit mask. Never put secrets in tool arguments, shell history, or shared MCP client configuration.

After funding, confirm native tTARI appears as unspent in the opener wallet. `ghostline wallet account-create` associates the resource and requests an asynchronous scan. Missing funding needs wallet/indexer investigation; neither installation nor waiting alone guarantees recovery. See [troubleshooting](#troubleshooting).

Use an owner-only working directory and keep service and payer state in separate files. Replace `/secure/path` throughout the examples with your actual private directory. To create a new Ghostline payer key, save its output directly to a private file:

```sh
umask 077
mkdir -p /secure/path
ghostline keygen > /secure/path/payer-key.txt
```

The file contains `secret=` and `public=` lines. Use only its public value in channel parameters and provide its secret through your private environment. Generate a unique 32-byte nonce for each channel; `openssl rand -hex 32` can produce one if OpenSSL is installed. These are separate from the wallet seed and provider's service key.

## 1. Prepare a channel file

Choose a future expiry epoch with a safety margin. Read the current epoch with `ghostline indexer epoch --url <esmeralda-indexer-url>`. The `--max-epoch` transaction bound must also remain in the future for the intended submission; it is distinct from channel expiry. Get the service public key and template hash from the provider/deployment record. Use integer micro-units: `100000` is 0.1 tTARI at 1,000,000 µTARI per tTARI. Opening and settlement fees are paid in addition to the deposit.

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

The CLI writes a record only; it does not open the channel. Confirm the wallet, indexer, template, service signer, fee account, and fee funding before proceeding. Use an opener profile/account whose default receiving address matches the supplied claim key. After opening, protect `channel.json` as secret-bearing material.

For expiry refunds, also include `--payer-fee-account <payer-fee-account-component>`, `--payer-fee-account-name <payer-fee-account-name>`, and `--payer-wallet-url <payer-loopback-wallet-url>` when initializing. The payer wallet must hold a signer matching the channel's payer public key, distinct from the claim/view key.

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

Keep the service ledger and payer session at separate paths. The channel record and paid API use the same ledger path; the channel record and signing process use the same session path. `paid-api init` creates both snapshots; opening the channel uses the matching session without resetting its cumulative total. Do not run `init` again over existing files.

## 3. Open and pay

Choose the CLI path or the MCP path for this channel. Both use the same channel record and payer session; do not open it twice or use separate signing processes concurrently against one session.

### CLI path

With both secrets supplied privately to the process, estimate the opening fee first:

```sh
ghostline channel open --file /secure/path/channel.json --dry-run
```

Review `estimated_fee` and `required_fee`, confirm the deposit plus cap is available, then open with an explicit positive cap in integer µT:

```sh
ghostline channel open --file /secure/path/channel.json --max-fee <reviewed-opening-fee-cap>
ghostline channel status --file /secure/path/channel.json
```

A live open stores the output's deposit mask and creates/resumes the matching session. The fee cap can be charged on the opening path even if the dry-run requirement was lower; avoid a needlessly large cap. If submission times out, reconcile the transaction before retrying (see [troubleshooting](#troubleshooting)).

Pay for ten calls at 10 µT each:

```sh
ghostline-paid-api call \
  --url http://127.0.0.1:8787/fortune \
  --session /secure/path/session.json \
  --calls 10 \
  --vendor-hex <service-public-key-hex> \
  --budget 100 \
  --max-per-call 10

ghostline channel status --file /secure/path/channel.json
```

The service's `accepted` total should now be 100 µT. Calls do not submit individual on-chain transactions. The policy budget limits voucher spending; it does not include wallet transaction fees.

### MCP path

Configure the MCP client to start `ghostline-mcp` with this environment. Supply the payer and claim secrets privately to that server process as described above; the MCP client's environment is not necessarily inherited automatically. Set the policy vendor to the service public key. Keep broadcast flags unset until you are ready to transact.

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

Stop paying on the channel before settlement. This channel variant is spent once; use a new channel for later calls.

### CLI settlement

Before expiry, use `settle-due` to choose a fee cap for this transaction. `--force` means settle now instead of waiting for the usual settlement window; it cannot override expiry. Set `GHOSTLINE_DEPOSIT_MASK` in the wallet operator's private environment from the channel file's stored mask. For a local shell with `jq`, for example:

```sh
export GHOSTLINE_DEPOSIT_MASK="$(jq -er '.deposit_mask_hex' /secure/path/channel.json)"
```

The mask stays out of command arguments and shell history. Keep the shell private and clear the variable when settlement is complete.

```sh
ghostline settle-due \
  --ledger /secure/path/ledger.json \
  --network-tag <esmeralda-network-tag> \
  --deposit 100000 \
  --payer-hex <payer-public-key-hex> \
  --service-hex <service-public-key-hex> \
  --expiry-epoch <channel-expiry-epoch> \
  --nonce-hex <channel-nonce-hex> \
  --template-hex <deployed-template-hash-hex> \
  --indexer-url <esmeralda-indexer-url> \
  --utxo-json /secure/path/channel.utxo.json \
  --max-epoch <maximum-accepted-epoch> \
  --ootle-network esmeralda \
  --fee-account <service-fee-account-component> \
  --max-fee <reviewed-settlement-fee-cap> \
  --force --submit --dry-run \
  --wallet-url http://127.0.0.1:5100 \
  --seal-key-id-file /secure/path/service-key-id.json \
  --fee-account-name <service-fee-wallet-account-name>
```

Review `required_fees` and confirm the channel is still unspent. The service fee account needs enough funds for the selected cap. Use the same command to submit after removing `--dry-run`; keep `--submit` and the reviewed cap. The channel-file shortcut `ghostline channel settle` has a fixed maximum fee of 100000 µT, so use it only if that cap is acceptable for your deployment.

```sh
ghostline indexer result \
  --url <esmeralda-indexer-url> \
  --transaction-id <settlement-transaction-id>
```

Confirm `Finalized Commit` from the result. Then `ghostline channel status --file /secure/path/channel.json` should show its output as `down`. Local `accepted` totals are service accounting, not settlement proof or wallet balances.

### MCP settlement

Set `GHOSTLINE_SEAL_KEY_ID_FILE=/secure/path/service-key-id.json` in the MCP server's environment. To settle before the normal window, set `GHOSTLINE_SETTLE_FORCE=1` while the channel is still before expiry. First set `GHOSTLINE_CLOSE_DRY_RUN=1`, leave `GHOSTLINE_CLOSE_SUBMIT` unset, and call `ghostline_close_channel` with `{}`. Review acceptance and the fee requirement through the fee wallet's `transactions.get` RPC using the returned dry-run id; dry runs are not submitted to the indexer. Close uses the same fixed **100000 µT** maximum fee; there is no configurable close-fee environment variable.

To broadcast, unset `GHOSTLINE_CLOSE_DRY_RUN`, set `GHOSTLINE_CLOSE_SUBMIT=1`, reload the server environment, and call `ghostline_close_channel` again. The tool otherwise follows the settlement window and expiry rules. Confirm finality with the indexer/wallet result and inspect through `ghostline_channel_status`.

The recorded live MCP run accepted ten 10 µT calls and finalized one settlement of 100 µT on Esmeralda. Fees were paid separately and were visible. Separate-wallet rehearsals also completed open → paid call → settle. Operator-funded transfers are valid test funding, but their public history can link wallets. A spent channel cannot be settled or refunded again.

### Refund after expiry

For an unspent channel at or after expiry, use `ghostline channel refund --file /secure/path/channel.json --submit --dry-run --seal-key-id-file /secure/path/payer-key-id.json` before submitting without `--dry-run`. This needs the payer fee fields described in step 1, a funded payer fee account, and the payer signing key in that wallet. The channel-file refund helper also uses a fixed 100000 µT fee cap. For MCP, switch `GHOSTLINE_SEAL_KEY_ID_FILE` to the payer signer reference before closing an expired channel; the service signer cannot substitute for it.

## Larger batches

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

The 0.1 tTARI example deposit covers 1,000 calls at 10 µT each. Use this batch on a fresh, unspent channel with a matching session and sufficient policy budget. The historical live HTTP run recorded 1,000 accepted calls totaling 10,000 µT, then one finalized settlement. The MCP stdio path is separately live-proven for ten calls and a single settlement. These results do not validate your wallet build or configuration.

For more commands, see the [API reference](api-reference.md). For privacy limits, read [privacy notes](privacy.md) and the [threat model](threat-model.md).

## Troubleshooting

| Symptom | What to check |
|---|---|
| `ghostline: command not found` | Add your install directory to PATH; see [installation](install.md#download-the-executables). |
| Incoming funding is finalized but absent in the wallet | Confirm the receiving wallet/account and resource association, preserve the profile and transaction id, and inspect wallet/indexer state. Do not delete wallet data or resend money merely because it is missing from the local view. |
| Open dry run reports insufficient funds | Confirm spendable inputs cover deposit plus fee cap. This message alone cannot distinguish a shortfall from undiscovered funding. |
| Open or settlement times out | Check the transaction's final result and output presence before retrying. A timeout does not prove the transaction failed. |
| Open committed but the channel file lacks its mask | Recover the channel UTXO with `ghostline wallet utxos --transaction-id <open-transaction-id> --utxo-json-out /secure/path/channel.utxo.json --url <opener-wallet-url>`, then use `ghostline channel bind --file /secure/path/channel.json --utxo-json /secure/path/channel.utxo.json` with the correct claim secret. Do not open again. |
| Close reports `wait` | Check the current epoch and settlement margin; force settlement only intentionally and before expiry. |
| Close reports `output_spent` | Confirm the previous settlement/refund result; this channel cannot be spent again. |
| Signing or close cannot find a key | Confirm the KeyId file references the intended signer in the wallet endpoint used for this operation. Key references are wallet-local. |
| MCP cannot obtain human approval | Use a client supporting elicitation and confirm it displays the approval prompt to a person. |

## CI smoke test

The Esmeralda open/settle steps require deployment-specific wallet credentials and are not broadcast by CI. To exercise the local HTTP 402 quote → validated voucher → paid response path and opt-in aggregate metrics without a network or wallet, run `bash scripts/quickstart-smoke.sh`. CI runs this script; it uses temporary keys and files, binds only to loopback, and removes its state on exit. It does not claim to test a live channel open or settlement.
