# Ghostline terminal demo

This runbook gives a short, reviewable demonstration of Ghostline without a web interface. It separates the repeatable local HTTP flow from Esmeralda evidence: the local flow shows quote validation and cumulative voucher accounting, while the indexer links show finalized on-chain settlement transactions.

## 1. Reproduce the local HTTP 402 flow

From a checkout of Ghostline, run:

```bash
bash scripts/quickstart-smoke.sh
```

The script builds the CLI and reference API, creates temporary test keys and state, starts the API on loopback, validates its HTTP 402 quote, and pays for three `/fortune` requests. It also checks that the aggregate metrics omit channel and payment details. Expected final output:

```text
quickstart smoke passed: quote validated, 3 paid calls, cumulative 30 µT
```

This demo uses local test data only. It does not connect to a wallet, open a channel, or submit a transaction. The script removes its temporary files when it exits. It requires the Rust build prerequisites listed in the [installation guide](install.md).

## 1a. One-command Esmeralda batch demonstration

For a live demonstration, an operator can configure one fresh, funded channel, a running paid API, its payer session, and the service wallet's one-channel settlement KeyId. Once those prerequisites are in place, the wrapper runs 20 paid HTTP requests, submits exactly one settlement, checks the indexer result, and prints the explorer link. Substitute real values for the illustrative paths and key placeholders before executing; never put the payer secret in a public site or shell history:

```bash
export GHOSTLINE_DEMO_CHANNEL_FILE=/secure/demo/channel.json
export GHOSTLINE_DEMO_SESSION_FILE=/secure/demo/session.json
export GHOSTLINE_DEMO_API_URL=https://demo.example/fortune
export GHOSTLINE_DEMO_SERVICE_PUBLIC_KEY=your-service-public-key-hex
export GHOSTLINE_DEMO_SERVICE_KEY_ID_FILE=/secure/demo/service-key-id.json
read -rsp 'Payer secret hex: ' GHOSTLINE_PAYER_SECRET; echo
export GHOSTLINE_PAYER_SECRET
bash scripts/esmeralda-20-call-demo.sh
unset GHOSTLINE_PAYER_SECRET
```

The API must use the same service public key and channel; the channel file must reference an Esmeralda indexer, fee account, wallet, and current deployed template. The service ledger must be empty, and the script refuses to reuse a ledger with accepted payments. A completed settlement consumes that channel. Enter the payer secret at the hidden prompt so it does not appear in shell history; it is supplied only to the local process and must not be put in a hosted page, public environment, or video. This wrapper is an operator tool; it does not create a public pre-funded service or open/fund a channel. The latest fixed two-output rule has been exercised with the current Esmeralda template; see the evidence file for its transaction links.

## 2. Show the MCP agent interface

Ghostline exposes the same paid-call flow through its MCP stdio server. For a real MCP-host demonstration, prepare a channel and local paid API using the [Esmeralda quickstart](quickstart.md), then configure the host to launch `ghostline-mcp` with that channel file, payer session, and policy environment. Keep secrets in the server process's private environment; do not paste them into a tool call or shared host configuration.

In the MCP host, show these calls:

1. `ghostline_channel_status` with `{}` to display the channel's off-chain totals and observer status.
2. `ghostline_call_paid` with `{"url":"http://127.0.0.1:8787/fortune"}` to make a paid request through HTTP 402.
3. `ghostline_channel_status` again to show the updated cumulative total.

If the policy requires human approval, the MCP host must support elicitation and visibly show the approval prompt. MCP status is off-chain reporting; it is not proof that a UTXO remains unspent or that settlement finalized. Do not reuse a channel that has already been settled.

This MCP walkthrough needs a correctly configured wallet, deployment, and unspent channel. It is not part of the self-contained local smoke script, and it should not be presented as a fresh live run unless those steps were actually performed during the demo.

## 3. Verify the recorded Esmeralda settlement

The repository includes sanitized evidence for the latest Esmeralda standard, batch, and expiry-refund runs in [`demo/ui/evidence.json`](../demo/ui/evidence.json). The updated template was published and finalized; a fresh 10,000 µT channel opened, the service ledger accepted 20 HTTP `/fortune` calls at 10 µT each, and one settlement finalized. The settlement's stealth transfer statement has no covenant claims, zero revealed input amount, no revealed output, and exactly two stealth outputs. The deposit, service amount, and payer remainder were not revealed in those fields. Fee instructions and fees remain public. The service ledger, not the chain, is the source for the call count and 200 µT accepted total.

The batch run opened two additional channels with the latest template, accepted 20 calls on each local service ledger, and finalized one pooled settlement. The transaction had two channel inputs and three stealth outputs. A local unblind confirmed the pooled output; its amount is omitted from public evidence. This is a trusted-local prototype: the batch builder received both deposit values and masks, and observers can see that the channels were grouped in one transaction. A fresh payer-initiated expiry refund also finalized against the latest template at epoch 12466. Its transaction has no revealed input amount or output amount and one stealth output. The wallet confirmed the channel output was spent, but the recipient output was not locally unblinded, so the public evidence does not claim a verified refunded amount.

With a current Esmeralda indexer URL, verify the historical settlement result from the CLI:

```bash
ghostline indexer result \
  --url "$ESMERALDA_INDEXER_URL" \
  --transaction-id 6a44670272ee5cb5528c8b7bfac3ce89170b6885afb6cfdf1ae1cd0952424e17
```

The recorded result is `Finalized`. To verify the pooled settlement, use transaction ID `0e192d7b81581be024ffe5348b5944b8af010edac5cf52628024f21cf78e2e2f`. These commands perform read-only checks. The evidence file is a static record, not a live network read; current indexer availability and retention can affect whether an old transaction remains queryable.

## Suggested narration

"The local demo shows HTTP 402 quote validation and cumulative voucher accounting without sending a transaction. In the live Esmeralda run, the service ledger accepted 20 calls at 10 micro-Tari each; one claim-free settlement finalized using the latest fixed-output template. It reveals neither the deposit nor the exact service amount or payer remainder in public transaction fields, and it creates exactly two stealth outputs. A separate prototype run pooled two channels into one three-output settlement; it was built locally with both deposit openings, and the chain shows the two channels in the same transaction. Fees remain public. MCP exposes the paid call to an agent host, with policy and optional human approval enforced by the Ghostline process."

When describing privacy, say that call count and prices remain in service-side records, and that the latest-template standard settlement hid the deposit, exact service amount, and payer remainder in transaction fields. Fees, timing, branch, opaque channel id, authorization data, output commitments, and output count remain visible. The latest-template expiry refund finalized with confidential output fields; the payer's wallet showed the channel output spent, but the recipient output was not unblinded to verify its amount. The batch builder learns participant deposits and the chain shows which channels were grouped together. See the [privacy notes](privacy.md) for the full limits.
