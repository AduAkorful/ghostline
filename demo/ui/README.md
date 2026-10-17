# Ghostline motion demo

This read-only motion demo opens with the many-calls/one-settlement payoff, then follows the CLI from install through payment and close using a scroll-led story. One schematic payment trace changes with each chapter; the command map covers the wider `ghostline` CLI. The page contains no wallet connection, credentials, transaction submission, or live network reads. The motion is illustrative, not a live feed or a replay of per-call data.

From the repository root, start a local server:

```bash
python3 -m http.server 8000
```

Then open <http://127.0.0.1:8000/demo/ui/>. The landing page links to native Ghostline-styled guides at `docs/install.html`, `docs/quickstart.html`, and `docs/interfaces.html`. The guides summarize common user flows; the Markdown in the repository remains the canonical technical documentation, with full references linked from the relevant guide.

The evidence section reads a sanitized live Esmeralda run from [`evidence.json`](evidence.json). The call count, unit price, and service total come from the service ledger; transaction IDs, finality, fee, and output status come from the recorded wallet/indexer check. The exact service amount remains hidden from public transaction fields, while the covenant claim reveals the deposit. The page intentionally has no explorer links because a verified public transaction URL has not been established. See the [terminal demo runbook](../../docs/demo-runbook.md) for more detail.
