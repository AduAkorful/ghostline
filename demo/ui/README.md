# Ghostline motion demo

This read-only motion demo opens with the many-calls/one-settlement payoff, then follows the CLI from install through payment and close using a scroll-led story. One schematic payment trace changes with each chapter; the command map covers the wider `ghostline` CLI. The page contains no wallet connection, credentials, transaction submission, or live network reads. The motion is illustrative, not a live feed or a replay of per-call data.

## Static hosting

The site needs no backend or environment variables. To deploy it through the Vercel dashboard, import the repository and set **Root Directory** to `demo/ui`, **Framework Preset** to `Other`, leave the build command empty, and use `.` as the output directory. The published page serves only these static files; it has no wallet credentials or transaction controls.

Visitors can try the local, no-funds flow from a checkout:

```bash
bash scripts/quickstart-smoke.sh
```

The first Rust build can take several minutes. It creates temporary local keys, simulates three paid HTTP calls against a loopback API, prints a summary, then deletes its temporary state. It does not use Esmeralda or submit transactions. For live testnet use, follow the [Esmeralda quickstart](../../docs/quickstart.md) and use a wallet the visitor controls.

From the repository root, start a local server:

```bash
python3 -m http.server 8000
```

Then open <http://127.0.0.1:8000/demo/ui/>. The landing page links to native Ghostline-styled guides at `docs/install.html`, `docs/quickstart.html`, and `docs/interfaces.html`. The guides summarize common user flows; the Markdown in the repository remains the canonical technical documentation, with full references linked from the relevant guide.

The evidence section reads sanitized Esmeralda proof records from [`evidence.json`](evidence.json), including the template publish, channel open, settlement, and a latest-template expiry refund. The latest settlement run accepted 20 paid calls off-chain, then finalized one settlement. Its transaction had zero covenant claims, zero revealed input amount, no revealed output, and exactly two stealth outputs. The call count and accepted total come from the local service ledger; the chain proves finality, not off-chain call details. The separate refund finalized and spent the channel output, but its recipient output was not locally unblinded to verify the amount received. Fees, timing, branch, channel id, authorization data, and output commitments remain public. See the [terminal demo runbook](../../docs/demo-runbook.md) for more detail.
