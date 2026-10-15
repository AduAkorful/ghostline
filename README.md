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
| `crates/ghostline-cli` | `ghostline` CLI: vouchers, `spec`, `pay-to`, `status`/`report`/`watch`, wallet open/submit, seal open/settle/refund, indexer submit |
| `docs/privacy.md` | Plain-language observer view |
| `docs/threat-model.md` | Assets, actors, attacks, residual risk |
| `crates/ghostline-policy` | Budget, per-call ceiling, vendor allowlist, rate limit and audit log |
| `middleware/ghostline-paywall` | HTTP 402 paywall, agent payer, and file-backed channel store |
| `demo/paid-api` | Reference paid endpoint (`/fortune` per request, `/meter?units=` per unit) |
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

Live Esmeralda: 100 `/fortune` calls at 10 µT (policy-gated 402) against a 1 tTARI channel, then one settle of cumulative 1000 µT. Indexer Finalized Commit; fees from the faucet-funded derived fee account (17640 µT). The chain sees one settlement, not 100 transfers.

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
- `ghostline_channel_status` (session from `GHOSTLINE_SESSION`; optional `GHOSTLINE_LEDGER` / `GHOSTLINE_EXPIRY_EPOCH` / `GHOSTLINE_INDEXER_URL`)
- `ghostline_channel_file` (`GHOSTLINE_CHANNEL_FILE`; no arguments; omits the deposit mask, keys, and fee accounts)
- `ghostline_channel_list` (`GHOSTLINE_CHANNEL_DIR`)
- `ghostline_channel_calls` (`GHOSTLINE_CHANNEL_DIR`; price and accepted total, no voucher hex)
- `ghostline_open_channel` (`GHOSTLINE_CHANNEL_FILE`; dry-run unless `GHOSTLINE_OPEN_SUBMIT=1`; refuses a file that already holds a deposit mask)
- `ghostline_channel_session` (`GHOSTLINE_CHANNEL_FILE`; creates the payer session or keeps a matching cumulative total)
- `ghostline_close_channel` (`GHOSTLINE_CHANNEL_FILE`; settle inside the margin, refund at expiry; broadcasts only with `GHOSTLINE_CLOSE_SUBMIT=1`)

Live Esmeralda: an MCP client speaking JSON-RPC stdio (`initialize` 2025-11-25, then `tools/call` `ghostline_call_paid`) paid 10 `/fortune` calls at 10 µT against a 0.1 tTARI channel. Policy came from env (`GHOSTLINE_POLICY_BUDGET`, `GHOSTLINE_POLICY_MAX_PER_CALL`, `GHOSTLINE_POLICY_VENDOR_HEX`), not from tool arguments. One settle of cumulative 100 µT Finalized Commit (17637 µT fee from the second-seed fee wallet). The protocol path is also covered by an in-process duplex test.

`ghostline_channel_file`, `ghostline_channel_list`, and `ghostline_channel_calls` take no arguments. The path comes from `GHOSTLINE_CHANNEL_FILE` or `GHOSTLINE_CHANNEL_DIR`. Live on the spent channel: accepted 20, remainder 99980, `settle_open`, output `down`, and one call-log note because that total predates the log. The tool result did not contain the deposit mask or the fee-account component.

`ghostline_call_paid` pays from `GHOSTLINE_SESSION`, or from the `session` path on `GHOSTLINE_CHANNEL_FILE` when that env var is unset. If the file has a deposit mask and no session yet, the paid call creates the session and retries. A 402 with no session and no deposit mask returns `needs_channel: true` unless `GHOSTLINE_OPEN_SUBMIT=1`, which opens the channel file, stores the mask, and retries. An in-process test paid a local 402 from the channel file's session (cumulative 10) without `GHOSTLINE_SESSION`. Another paid a local 402 from a file that had a deposit mask and no session yet: the call created the session, left the mask in place, and the cumulative total became 10. A file with no mask still returned `needs_channel` and created no session. `channel session` creates that session from the file. A second run keeps the cumulative total. A session whose channel id does not match the file is refused. `ghostline_channel_session` does the same with no arguments. A live `channel bind` on a copy of an already-opened channel stored the mask again and created a session at cumulative 0. The original file was left unchanged. `ghostline_open_channel` dry-runs `accounts.stealth_transfer` unless `GHOSTLINE_OPEN_SUBMIT=1`. The flag broadcasts, waits for Accept, unblinds the channel output with `GHOSTLINE_CLAIM_SECRET` (or `GHOSTLINE_PAYER_SECRET`), stores the deposit mask, and creates the payer session. It refuses a file that already holds a deposit mask. A dry-run does not write a mask. Live Esmeralda: a dry-run against the opener wallet returned `dry_run=true` and left the file without a mask. A live MCP broadcast has not been re-run. `ghostline_close_channel` follows the same windows as `settle-due` and `refund-due`: settle while the epoch is inside the margin (default 10, or `GHOSTLINE_SETTLE_FORCE=1`), refund at or after expiry, and `output_spent` when the channel output is down. It broadcasts only with `GHOSTLINE_CLOSE_SUBMIT=1`. `GHOSTLINE_CLOSE_DRY_RUN=1` prices that transaction and does not broadcast. The tool result does not include the deposit mask.

Operator view (off-chain store, not a UTXO balance). `status` / `report` print the same JSON: totals, remainder *if* you settled at those totals, settle vs refund window, and the observer panel. `--include-params` adds keys; omit it from anything you publish. `watch` is a one-shot epoch heartbeat against an indexer.

```bash
cargo run -p ghostline-cli -- status \
  --session /tmp/ghostline/session.json --ledger /tmp/ghostline/ledger.json \
  --expiry-epoch <epoch> --indexer-url <indexer-url>
cargo run -p ghostline-cli -- watch \
  --expiry-epoch <epoch> --indexer-url <indexer-url> --margin-epochs 100
```

Live Esmeralda: `status` on the MCP channel's session+ledger (accepted 100, cumulative 100) with indexer epoch showed `settle_open` and the observer panel. That channel had already settled on-chain; remainder in the JSON is hypothetical at those off-chain totals.

Pass `--utxo-json` (or `--commitment-hex`) with `--indexer-url` and `status` also reports `channel_output`: `up` or `down` from `GET /substates/utxo_{resource}_{commitment}`. A down output means the channel UTXO is spent; the remainder fields stay the off-chain arithmetic and are not a balance. Without a commitment, `channel_output` is `not_checked`. `settle-due` and `refund-due` with the same flag return `output_spent` and do not submit.

`refund-due` submits only when `current_epoch >= expiry_epoch` (`AfterEpoch` is inclusive). Before that it prints `wait` and does not broadcast, including with `--submit`. Fees come from the payer account. A live refund after expiry was already proven with `seal-refund` (Finalized Commit). `refund-due` itself was checked live on a channel 5000 epochs early (`wait`, not submitted) and on that channel's spent output (`output_spent`).

`channel init` writes one JSON record (open parameters, indexer, fee accounts, ledger path, optional payer session path). `channel session` creates that payer session when it is missing, or keeps it when the channel id already matches, and stores the path on the file. It does not reset a cumulative total. `channel bind` and a live `channel open` do the same after they store the deposit mask. `channel bind` unblinds an existing UTXO dump into that file. `channel open` is the wallet open plus that unblind. `channel status`, `channel settle`, and `channel refund` read the file instead of repeating the open arguments. Once the deposit mask is stored, treat the file as sensitive. `channel open --dry-run` does not write the mask.

```bash
cargo run -p ghostline-cli -- channel init --out /tmp/channel.json \
  …open args… --indexer-url <indexer-url> --ootle-network esmeralda \
  --max-epoch <epoch> --ledger /tmp/ghostline/ledger.json \
  --fee-account <service-component_…> --fee-account-name <service-fee-account>
cargo run -p ghostline-cli -- channel bind --file /tmp/channel.json \
  --utxo-json /tmp/channel.utxo.json
cargo run -p ghostline-cli -- channel status --file /tmp/channel.json
```

Live Esmeralda, using an already-opened channel (no new open): `channel status` showed accepted 20, remainder 99980, `settle_open`, and the channel output `down`. `channel settle --submit` and `channel refund --submit` both returned `output_spent` and did not broadcast. `channel open --dry-run` reached the opener wallet and left the file without a mask.

`channel list --dir` prints one row per channel file in that directory (other JSON is skipped). The row has the off-chain totals, expiry window, and output presence. It does not include the deposit mask, keys, or account components. `pending_settlement` means the ledger has accepted a voucher; a `down` output means that UTXO is already spent.

```bash
cargo run -p ghostline-cli -- channel list --dir /tmp/channels
```

Live Esmeralda: the directory that holds the spent channel file listed one row (accepted 20, remainder 99980, `settle_open`, output `down`, `pending_settlement` true).

`channel report --dir` writes that directory as CSV (or `--format json`) for accounting. The default columns are the file name, deposit, accepted total, remainder, expiry, settle window, pending settlement, and output presence. They omit the channel id, keys, fee-account components, and the deposit mask. `--disclose channel_id` (also `cumulative`, `has_latest_voucher`, `epochs_until_expiry`, `current_epoch`) adds a column. Any other name is an error. The top-level `report` command is still the full status JSON, including the observer panel.

```bash
cargo run -p ghostline-cli -- channel report --dir /tmp/channels
cargo run -p ghostline-cli -- channel report --dir /tmp/channels --disclose channel_id --out /tmp/spend.csv
```

Live Esmeralda: the default CSV for that directory was one data row (accepted 20, remainder 99980, `settle_open`, output `down`). `--disclose deposit_mask` was rejected.

`channel calls --dir` exports each accepted charge as `file,price,accepted`. The price is the delta for that call. The voucher hex is not stored in the call log. A ledger written before the log existed is not backfilled: if it already has an accepted total and no calls, the CSV has one note row instead of invented prices.

```bash
cargo run -p ghostline-cli -- channel calls --dir /tmp/channels
```

Live Esmeralda: that spent channel's ledger has accepted 20 and no call rows, so the export was the note that the total predates the log.

`settle-due` reads the ledger's latest voucher (the paywall stores it on each accepted call; the hex is not printed). It settles when the remaining epochs are within `--margin-epochs` (default 10) and the current epoch is still strictly before expiry. At or after expiry it refuses, including with `--force` (a live settle there is `OnlyFeeAccepted`). Without `--submit` it only prints the decision. `--force` settles early while the window is still open. `channel watch` polls that same decision. It sleeps until the settle is due, then runs `channel settle`. It stops without settling when the epoch reaches expiry, the output is down, or nothing is owed. `--once` prints one decision and returns. `--submit` is what broadcasts.

```bash
cargo run -p ghostline-cli -- settle-due \
  --ledger /tmp/ghostline/ledger.json \
  …open args… --indexer-url <indexer-url> --margin-epochs 10 \
  --max-epoch <epoch> --ootle-network esmeralda \
  --fee-account <service-component_…> --out /tmp/settle.unsigned.json
# add --submit --wallet-url <walletd> --seal-key-id-file <service.key_id.json> \
#   --fee-account-name <service-fee-account> to broadcast
```

Live Esmeralda: two `/fortune` calls (10 µT) left voucher amount 20 on the ledger. `settle-due` without `--force` printed `wait` (5000 epochs left). `--force --submit` Finalized Commit (17636 µT fee from the second-seed fee wallet).

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

Wallet-daemon settle: the sealer is the service key imported into the wallet. `pay_fee` is public. A live settle paid fees from a derived account funded by an independent faucet (not a transfer from the opener). `wallet submit` does not default to derived account index 0 (the usual opener). Same-seed accounts remain correlated: anyone who has that seed, or who maps both components to one wallet, can still link them.

A live settle also paid fees from a **second wallet seed** (separate walletd data dir and faucet claim). Indexer Finalized Commit; the opener fee account's revealed balance did not move. Import the service key into that fee wallet and pass `--url` so submit talks to it. The faucet transaction that funded the fee wallet is still public.

```bash
cargo run -p ghostline-cli -- wallet account-create --name <service-fee-account> --key-index 1
# also derives view_only_key/account at that index and associates stealth tTARI
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

# second-seed fee wallet (after importing the service key into that daemon):
# cargo run -p ghostline-cli -- wallet submit --url http://127.0.0.1:<fee-walletd-port> \
#   --unsigned-json /tmp/settle.unsigned.json \
#   --seal-key-id-file /path/to/fee-wallet.service.key_id.json \
#   --fee-account-name <service-fee-account>
```

Refund after expiry uses the same unsigned + `wallet submit` path. The sealer is the Ghostline payer key (import it as `GeneralPurpose`). Fees come from the payer's account, not the service fee account. Live Esmeralda: 0.1 tTARI channel, refund before `AfterEpoch` left the UTXO unspent; at expiry a stealth-transfer refund Finalized Commit (9502 µT fee from the opener). A live settle at the expiry epoch is `OnlyFeeAccepted` (`BeforeEpoch` fails; channel UTXO unspent; 16921 µT fee still taken).

```bash
cargo run -p ghostline-cli -- seal-refund --unsigned-json \
  --ootle-network esmeralda --max-epoch <epoch> \
  --fee-account <payer-component_…> --max-fee 100000 \
  …open args… > /tmp/refund.unsigned.json

cargo run -p ghostline-cli -- wallet submit \
  --unsigned-json /tmp/refund.unsigned.json \
  --seal-key-id-file /path/to/payer.key_id.json \
  --fee-account-name <payer-account> \
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
