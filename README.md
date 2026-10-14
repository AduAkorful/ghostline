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
| `crates/ghostline-cli` | `ghostline` CLI: vouchers, `spec`, `pay-to`, wallet open/submit, seal open/settle/refund, indexer submit |
| `docs/privacy.md` | Plain-language observer view |
| `docs/threat-model.md` | Assets, actors, attacks, residual risk |
| `crates/ghostline-policy` | Budget, per-call ceiling, vendor allowlist, rate limit and audit log |
| `middleware/ghostline-paywall` | HTTP 402 paywall, agent payer, and file-backed channel store |
| `demo/paid-api` | Reference paid endpoint (`/fortune`) |
| `packages/ghostline` | TypeScript channel-id encodings and voucher signatures |
| `crates/ghostline-mcp` | MCP stdio server (`ghostline-mcp`) |
| `crates/ghostline-net` | Indexer REST and local wallet-daemon JSON-RPC (unsigned submit) |
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
- `ghostline_inspect_tx`
- `ghostline_unblind_tx` (secret from `GHOSTLINE_PAYER_SECRET`)

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

Wallet-daemon open (`accounts.stealth_transfer` with `PayTo::Conditions`) and settle (unsigned JSON) is proven live on Esmeralda as a CLI-only loop. Daemon must be `--authentication none`. Destination defaults to `accounts.get_default` (encrypted to that wallet's `view_only_key`). After an Accepted open, the CLI loads `transactions.get` and prints `unblind --utxo-json` for the channel output (`PayTo::Conditions` is stored as `Script` auth). Wallet-built `accounts.stealth_transfer` places that output in `fee_instructions` (with `PayFeeFromBucket`); unsigned settle places it in body `instructions`. Dry-run opens are not stored. Ghostline-sealed indexer settle is also proven: `indexer resolve-inputs` (pins versions and appends the fee account vault; CBOR tag 132) then `seal-unsigned` then `indexer submit`. A first submit that omitted the vault FinalizedAbort (`OneOrMoreInputsNotFound`). With the vault, `GET /transactions/{id}/result` is Finalized (`Commit`; 50 µT voucher, 17623 µT fee). `GET /substates/{id}` 404s a spent channel UTXO (`(vN) is down`). `stealth_utxos.list` is not used for this dump (it can lag).

```bash
cargo run -p ghostline-cli -- wallet open --account-name <wallet-account> --dry-run \
  --network-tag 1 --deposit <µT> --payer-hex <32-byte-hex> --service-hex <32-byte-hex> \
  --expiry-epoch <epoch> --nonce-hex <32-byte-hex> --template-hex <32-byte-hex>

# after a live Accepted open (or any wallet-stored stealth tx):
cargo run -p ghostline-cli -- wallet utxos --transaction-id <64-hex> \
  --utxo-json-out /tmp/channel.utxo.json
```

Wallet-daemon settle: the sealer is the service key imported into the wallet. `pay_fee` is public. A live settle paid fees from a derived account funded by an independent faucet (not a transfer from the opener). `wallet submit` does not default to derived account index 0 (the usual opener). Same-seed accounts remain correlated.

```bash
cargo run -p ghostline-cli -- wallet account-create --name <service-fee-account> --key-index 1
cargo run -p ghostline-cli -- wallet account-get --name <service-fee-account>

cargo run -p ghostline-cli -- seal-settle --unsigned-json \
  --ootle-network esmeralda --max-epoch <epoch> \
  --fee-account <service-component_…> --max-fee 100000 \
  …open args… --voucher-hex <hex> > /tmp/settle.unsigned.json

cargo run -p ghostline-cli -- wallet submit \
  --unsigned-json /tmp/settle.unsigned.json \
  --seal-key-id-file /path/to/service.key_id.json \
  --fee-account-name <service-fee-account> \
  --dry-run
```

Indexer-sealed settle (after the same unsigned JSON; versions must still be null so resolve can fill them):

```bash
cargo run -p ghostline-cli -- indexer resolve-inputs --url <indexer-url> \
  --unsigned-json /tmp/settle.unsigned.json --out /tmp/settle.resolved.json
cargo run -p ghostline-cli -- seal-unsigned --unsigned-json /tmp/settle.resolved.json \
  > /tmp/settle.sealed.hex
# indexer dry-run needs --dry-run (envelope dry_run: false is rejected on that path)
# cargo run -p ghostline-cli -- seal-unsigned --dry-run --unsigned-json /tmp/settle.resolved.json
cargo run -p ghostline-cli -- indexer submit --url <indexer-url> --tx-hex "$(cat /tmp/settle.sealed.hex)"
cargo run -p ghostline-cli -- indexer result --url <indexer-url> --transaction-id <64-hex>
```

Wallet-built channel outputs are encrypted to the wallet view key. Unblind with `GHOSTLINE_CLAIM_SECRET` (that view key), not the Ghostline payer key. SQLite `tag_byte` may be a signed INTEGER; `--utxo-json` accepts that.

```bash
GHOSTLINE_CLAIM_SECRET=<view-key-hex> cargo run -p ghostline-cli -- unblind \
  --utxo-json /path/to/utxo.json --ootle-network esmeralda
```

Recover value and mask from a stealth output encrypted to the claim secret (localnet resource default):

```bash
cargo run -p ghostline-cli -- unblind \
  --commitment-hex <32-byte-hex> --nonce-hex <32-byte-hex> \
  --encrypted-hex <encrypted-data-hex> --tag <u32>

cargo run -p ghostline-cli -- inspect-tx --tx-hex <seal-open-or-settle-hex>
cargo run -p ghostline-cli -- unblind-tx --tx-hex <seal-open-or-settle-hex>
```

## License

MIT. See [LICENSE](LICENSE).
