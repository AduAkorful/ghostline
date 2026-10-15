# Ghostline read-only demo page

This static prototype explains Ghostline and presents one verified historical Esmeralda run. It contains no wallet connection, credentials, transaction submission, or live network reads. The sanitized figures live in [`evidence.json`](evidence.json).

From the repository root, start a local server:

```bash
python3 -m http.server 8000
```

Then open <http://127.0.0.1:8000/demo/ui/>. Serving from the repository root keeps the quickstart and API documentation links available.

The service-side call count is a record from the paywall rehearsal. Transaction IDs, finality, settlement amount, fee, and output status are labeled as indexer/chain evidence. The page intentionally has no explorer links because a verified public transaction URL has not been established.
