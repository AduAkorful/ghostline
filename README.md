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
| `crates/ghostline-sdk` | Condition-tree open (`PayTo::Conditions`), settle and refund stealth statements |
| `crates/ghostline-cli` | `ghostline` CLI: vouchers, `spec`, seal open/settle/refund, indexer submit |
| `docs/privacy.md` | Plain-language observer view |
| `crates/ghostline-policy` | Budget, per-call ceiling, vendor allowlist, rate limit and audit log |
| `middleware/ghostline-paywall` | HTTP 402 paywall, agent payer, and file-backed channel store |
| `demo/paid-api` | Reference paid endpoint (`/fortune`) |
| `packages/ghostline` | TypeScript channel-id and voucher-message encodings |
| `crates/ghostline-mcp` | MCP stdio server (`ghostline-mcp`) |
| `crates/ghostline-net` | Indexer REST: wait, epoch stats, submit / dry-run |
| `scripts/localnet.sh` | Clone/init/start `tari_swarm_daemon` outside this repo |
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

Offline 402 loop (two JSON files: service `ledger`, payer `session`):

```bash
cargo run -p ghostline-paid-api -- init \
  --ledger /tmp/ghostline/ledger.json --session /tmp/ghostline/session.json \
  --network-tag 1 --deposit 100 \
  --payer-hex <32-byte-hex> --payer-root-hex <32-byte-hex> --nonce-hex <32-byte-hex>

cargo run -p ghostline-paid-api -- serve --ledger /tmp/ghostline/ledger.json --price 10

GHOSTLINE_PAYER_SECRET=<32-byte-hex> cargo run -p ghostline-paid-api -- call \
  --url http://127.0.0.1:8787/fortune --session /tmp/ghostline/session.json \
  --vendor-hex <service-or-allowlist-pubkey-hex> --budget 100 --max-per-call 10 --calls 10
```

Ten paid calls still settle as one cumulative amount. Do not share one store file between `serve` and `call`.

Offline voucher commands (no network):

```bash
cargo run -p ghostline-cli -- spec --network-tag 1 --deposit 100 \
  --payer-hex <32-byte-hex> --service-hex <32-byte-hex> --expiry-epoch 100 \
  --nonce-hex <32-byte-hex> --template-hex <32-byte-hex>
```

MCP stdio server (point an MCP client at `cargo run -p ghostline-mcp --quiet`):

- `ghostline_channel_id`
- `ghostline_sign_voucher` (secret from `GHOSTLINE_PAYER_SECRET`; session from `GHOSTLINE_SESSION`)
- `ghostline_verify_voucher`
- `ghostline_call_paid`

Localnet (needs tens of GB free; refused below `GHOSTLINE_MIN_FREE_GB`, default 40):

```bash
scripts/localnet.sh clone
# checkout a matching L1 tag, then:
scripts/localnet.sh init
scripts/localnet.sh start
```

Swarm UI: `http://localhost:8080`. Then:

```bash
cargo run -p ghostline-cli -- indexer wait --url http://127.0.0.1:<indexer-port>/
cargo run -p ghostline-cli -- indexer epoch --url http://127.0.0.1:<indexer-port>/
```

Recover value and mask from a stealth output encrypted to `GHOSTLINE_PAYER_SECRET` (localnet resource default):

```bash
cargo run -p ghostline-cli -- unblind \
  --commitment-hex <32-byte-hex> --nonce-hex <32-byte-hex> \
  --encrypted-hex <encrypted-data-hex> --tag <u32>
```

## License

MIT. See [LICENSE](LICENSE).
