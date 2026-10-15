# Ghostline API reference

This reference covers the current HTTP payment protocol, Rust/TypeScript entry points, and MCP tools. `ghostline.channel.voucher.v1` is Ghostline's own scheme; it is not the Ethereum `x402` protocol.

## HTTP payment protocol

### Request headers

| Header | Meaning |
|---|---|
| `ghostline-voucher` | Hex encoding of the voucher's Tari BOR bytes. Required when retrying a paid request. |
| `ghostline-channel` | Hex channel id used to select a service ledger when the server is configured with a channel directory. |

Header names are case-insensitive under HTTP. A caller should retry a 402 at most once for the quote it validated; the reference agent does so.

### Payment-required response

An unpaid request returns HTTP `402` with a JSON body shaped like:

```json
{
  "scheme": "ghostline.channel.voucher.v1",
  "price": 30,
  "channel_id": "<64 hex characters>",
  "basis": "metered",
  "unit_price": 10,
  "units": 3,
  "recipient_key": "<64 hex characters>",
  "min_expiry_epoch": 12000,
  "max_expiry_epoch": 13000
}
```

`price` is the total amount due for this request, in micro-units. `basis` is `fixed`, `per_request`, or `metered`; `unit_price` and `units` are present for metered quotes. The reference API requires recipient and inclusive expiry terms at startup and returns them together. Older/shared quote producers may omit those opening terms; clients must accept either all three terms (`recipient_key`, minimum and maximum epoch) or none.

Before signing, the payer checks the scheme and channel id. If opening terms are present, it also checks the recipient key and chooses an expiry inside the inclusive range. The HTTP payer rejects an incomplete terms tuple, wrong recipient, wrong channel id, or unsupported scheme before signing or opening.

### Paid request and errors

The payer signs the **cumulative total owed for the channel**, not merely the current request's `price`, then retries with `ghostline-voucher`. The server verifies the signature against the ledger's channel, enforces that the total increases and stays within the deposit, writes the accepted state, and only then serves the resource. Replaying the latest accepted voucher is idempotent and charges no additional delta. Older totals, a mismatched channel, malformed signatures, or over-deposit amounts are rejected.

Rejected payment responses use JSON `{ "scheme": "ghostline.channel.voucher.v1", "status": <HTTP status>, "reason": "<description>" }`. A zero or overflowing metered price is a bad request. A missing voucher is the 402 quote path; other failures use the status associated with the typed paywall error.

### Reference API routes

| Route | Pricing | Paid response |
|---|---|---|
| `GET /fortune` | Per request, configured by `--price` | `{"fortune":"pay once, call many"}` |
| `GET /meter?units=N` | `unit_price * N`, configured by `--price`; both values must be nonzero and multiplication must fit `u64` | `{"units":N,"charged":<total micro-units>}` |
| `GET /metrics` | Available only when `serve --metrics` is set | Aggregate Prometheus counters and channel-state gauges; no per-channel labels or amounts |

The current reference server exposes the payment routes; applications can use the paywall crate for other routes and pricing policies. The opt-in metrics route reports request totals by fixed route, payment-required, paid, rejected and internal-error counts, configured ledger count, and the number of ledgers with accepted vouchers. The last gauge is off-chain state, not chain settlement confirmation. Metrics are aggregate and do not expose ids, keys, vouchers, or amounts. CLI watch/settle/refund emits structured `channel_operation` tracing events with bounded operation/outcome fields; a log collector can map the `ghostline_channel_operation_outcomes_total` metric field to its counter backend. `wallet_accepted` means the wallet submission returned an accepted result; it does not by itself assert an indexer Finalized Commit. Structured logs are written to stderr and can be filtered with `RUST_LOG`.

## Rust crates

| Crate | Public surface |
|---|---|
| `ghostline-core` | `ChannelParams::channel_id`, `SignedVoucher::{sign, verify, to_bytes, from_bytes}`, `PaymentSession`, `ServiceLedger`, hex helpers, and `write_private_file` |
| `ghostline-sdk` | `Channel::open`, `fund_open`, `settle`, `refund`, `unblind_output`, `unblind_transaction`, and transaction inspection/sealing helpers |
| `ghostline-policy` | `Policy`, `PolicyEngine`, `RateLimit`, and durable `AuditEvent` support |
| `ghostline-paywall` | `quote`, `PaymentRequired`, `PaymentTerms`, voucher header helpers, `ChannelStore`, `ChannelDirectory`, `AgentPayer`, and settlement/refund status decisions |
| `ghostline-net` | Indexer client and wallet-daemon JSON-RPC types; writes are made through the configured wallet daemon |

The CLI is the operator interface for channel files, wallet opens/submits, offline signing/verification, settlement/refund, reports, and status checks. Run `cargo run -p ghostline-cli -- --help` and the relevant subcommand's `--help` for the exact installed flags. Live opens require an explicit reviewed fee cap; dry-run first and account for fees being public.

## TypeScript package

Package: `@ghostline/sdk` (`packages/ghostline`). It implements the shared voucher/channel-id surface, not wallet operations. Current exports are:

- Channel: `CHANNEL_ID_DOMAIN`, `CHANNEL_ID_LEN`, `NONCE_LEN`, `ChannelParams`, `channelId`, `fromHex`, `toHex`, `voucherMessage`.
- Voucher: `VOUCHER_DOMAIN`, `SignedVoucher`, `computeChallenge`, `publicKeyFromSecret`, `signVoucher`, `verifyVoucher`, `voucherToBytes`.

The package requires Node.js 22 or newer. Rust/TypeScript shared vectors live in the package and Rust tests; use `npm test --prefix packages/ghostline` to check parity.

## MCP stdio server

Run `cargo run -p ghostline-mcp --quiet` and configure an MCP client to launch it over stdio. Payer and claim secrets are read from the server process environment; there is no tool argument for a private key or seed.

| Tool | Arguments / environment | Purpose |
|---|---|---|
| `ghostline_channel_id` | `network_tag`, `payer_hex`, `deposit`, `payer_root_hex`, `nonce_hex` | Compute a channel id. |
| `ghostline_sign_voucher` | Channel fields plus per-call `price`; secret/session/policy from environment | Sign the next cumulative voucher under policy. |
| `ghostline_verify_voucher` | Channel fields plus `voucher_hex` | Verify an encoded voucher. |
| `ghostline_call_paid` | `url`, optional `voucher_hex`; payer session/channel file and policy from environment | Call an HTTP resource and handle one validated 402 retry. |
| `ghostline_inspect_tx` | `tx_hex` | Inspect public stealth-output metadata in a sealed transaction. |
| `ghostline_unblind_tx` | `tx_hex`; claim secret from environment | Unblind outputs addressed to the configured claim secret. |
| `ghostline_channel_status` | Optional `expiry_epoch`, `indexer_url`; session/ledger from environment | Show off-chain totals and observer/status information. It is not a UTXO balance or settlement proof. |
| `ghostline_channel_file` | `GHOSTLINE_CHANNEL_FILE` | Read channel-file status without returning its mask, keys, or fee accounts. |
| `ghostline_channel_list` | `GHOSTLINE_CHANNEL_DIR` | List channel-file summaries. |
| `ghostline_channel_calls` | `GHOSTLINE_CHANNEL_DIR` | Export accepted prices/totals without voucher hex. |
| `ghostline_open_channel` | `GHOSTLINE_CHANNEL_FILE`; submit only with `GHOSTLINE_OPEN_SUBMIT=1` and explicit `GHOSTLINE_OPEN_MAX_FEE` | Dry-run or open the configured file's channel. |
| `ghostline_channel_session` | `GHOSTLINE_CHANNEL_FILE` | Create or resume a matching payer session. |
| `ghostline_close_channel` | `GHOSTLINE_CHANNEL_FILE`; submit only with `GHOSTLINE_CLOSE_SUBMIT=1` | Settle inside the expiry margin or refund at/after expiry. |

Human approval is requested through MCP elicitation when the configured policy requires it. If the MCP client cannot provide consent, the payment does not proceed. For all process environment variables and the tested open/call/close sequence, see [Quickstart](quickstart.md).
