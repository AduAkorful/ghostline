# Ghostline terminal demo

This runbook gives a short, reviewable demonstration of Ghostline without a web interface. It separates the repeatable local HTTP flow from previously recorded Esmeralda evidence: the local flow shows quote validation and cumulative voucher accounting, while the indexer can be used to check the historical on-chain settlement.

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

## 2. Show the MCP agent interface

Ghostline exposes the same paid-call flow through its MCP stdio server. For a real MCP-host demonstration, prepare a channel and local paid API using the [Esmeralda quickstart](quickstart.md), then configure the host to launch `ghostline-mcp` with that channel file, payer session, and policy environment. Keep secrets in the server process's private environment; do not paste them into a tool call or shared host configuration.

In the MCP host, show these calls:

1. `ghostline_channel_status` with `{}` to display the channel's off-chain totals and observer status.
2. `ghostline_call_paid` with `{"url":"http://127.0.0.1:8787/fortune"}` to make a paid request through HTTP 402.
3. `ghostline_channel_status` again to show the updated cumulative total.

If the policy requires human approval, the MCP host must support elicitation and visibly show the approval prompt. MCP status is off-chain reporting; it is not proof that a UTXO remains unspent or that settlement finalized. Do not reuse a channel that has already been settled.

This MCP walkthrough needs a correctly configured wallet, deployment, and unspent channel. It is not part of the self-contained local smoke script, and it should not be presented as a fresh live run unless those steps were actually performed during the demo.

## 3. Verify the recorded Esmeralda settlement

The repository includes sanitized evidence for a historical run in [`demo/ui/evidence.json`](../demo/ui/evidence.json). That run recorded 1,000 HTTP `/fortune` calls at 10 µT each in the service ledger, then one 10,000 µT settlement. The indexer reported both open and settlement transactions as `Finalized`; the settlement fee was 17,640 µT, paid by a service-side fee account. The 1,000 calls were not individual chain transactions.

With a current Esmeralda indexer URL, verify the historical settlement result from the CLI:

```bash
ghostline indexer result \
  --url "$ESMERALDA_INDEXER_URL" \
  --transaction-id d2857abcf189b228c173552f284b8926f5da922c4a0cfd57d283c0847bc82400
```

The recorded result is `Finalized`. This command performs a read only. The evidence file is a historical record, not a live network read; current indexer availability and retention can affect whether an old transaction remains queryable.

## Suggested narration

“The first run is reproducible locally: the service returns a 402 quote, Ghostline validates it, then three paid calls accumulate to 30 micro-Tari. No transaction is sent in that local run. The separate Esmeralda record shows the same pay-per-call accounting pattern settled once on-chain: 1,000 calls, 10,000 micro-Tari total, and one finalized settlement. MCP exposes the paid call to an agent host, with policy and optional human approval enforced by the Ghostline process.”

When describing privacy, say that call count, prices, and timing remain in service-side records in this demo; public fees remain visible, and settlement reveals the deposited amount, cumulative settlement, channel identity, and executed branch. See the [privacy notes](privacy.md) for the full limits.
