# Ghostline

Private pay-per-call payment channels for AI agents on [Tari Ootle](https://ootle.tari.com).

An agent locks funds once in a stealth channel, signs cumulative off-chain vouchers per call, and the service
settles once on-chain. Between open and settle, nothing about the agent's spending pattern touches the chain.

> **Status: early development. Unaudited. Testnet only.** Do not use with real funds.

## How it works

1. **Open.** The payer deposits into a stealth output whose condition tree has two leaves.
2. **Pay.** For each call the payer signs a voucher for the cumulative amount owed, bound to the channel id.
   Vouchers travel with the request; no chain interaction.
3. **Settle.** The service submits its latest voucher once. The contract verifies the signature and forces
   the remainder back to the payer in the same transaction.
4. **Refund.** After the expiry epoch the payer can reclaim the deposit. The settle and refund paths are
   mutually exclusive at every epoch.

### What is hidden, and what is not

| | Visible on-chain |
|---|---|
| Number, size and timing of calls | **No** |
| Which endpoints or vendors were used | **No** |
| Deposit and voucher amount | Yes, once, at settlement |
| Channel id, payer key and recipient | Yes, once, at settlement |
| Transaction fees | Always |

Ghostline hides the spending pattern from third parties. It does not hide anything from the service you pay.

## Repository layout

| Path | Contents |
|---|---|
| `contracts/channel` | The channel template (Rust, compiled to WASM) |
| `crates/ghostline-core` | Channel id, voucher signing, verification, payer session and service ledger |
| `crates/ghostline-sdk` | Condition-tree open, settle and refund stealth statements |
| `crates/ghostline-cli` | `ghostline` CLI: `channel-id`, `sign`, `verify` |
| `crates/ghostline-policy` | Budget, per-call ceiling, vendor allowlist, rate limit and audit log |
| `middleware/ghostline-paywall` | HTTP 402 paywall, agent payer, and file-backed channel store |
| `demo/paid-api` | Reference paid endpoint (`/fortune`) |
| `packages/ghostline` | TypeScript channel-id and voucher-message encodings |
| `crates/ghostline-mcp` | MCP stdio server (`ghostline-mcp`) |
| `crates/contract-tests` | Engine-level tests, run against the in-process Ootle engine |

## Running the tests

Requires Rust (the toolchain is pinned in `rust-toolchain.toml`), `protoc`, `cmake` and `clang`.

```bash
cargo test
```

The first run compiles the Ootle engine and takes several minutes.

The HTTP 402 paywall and demo API do not need the engine:

```bash
cargo test -p ghostline-paywall -p ghostline-paid-api
cd packages/ghostline && npm test
```

Offline voucher commands (no network):

```bash
cargo run -p ghostline-cli -- channel-id --network-tag 1 --deposit 100 \
  --payer-hex <32-byte-hex> --payer-root-hex <32-byte-hex> --nonce-hex <32-byte-hex>
```

MCP stdio server (point an MCP client at `cargo run -p ghostline-mcp --quiet`):

- `ghostline_channel_id`
- `ghostline_sign_voucher`
- `ghostline_verify_voucher`
- `ghostline_call_paid`

## License

MIT. See [LICENSE](LICENSE).
