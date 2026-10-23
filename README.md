# Ghostline

👻 **Ghost payments for AI agents:** many signed off-chain whispers, one sealed séance receipt on [Tari Ootle](https://ootle.tari.com).

Ghostline lets an agent pay a service request by request while keeping the individual voucher stream off-chain. The payer funds a channel, signs cumulative vouchers, and the service settles the accepted amount once.

> **Early development. Unaudited. Esmeralda testnet only. Do not use real funds.**

## Try Ghostline

- **[Try the browser demo](https://ghostline-ui.vercel.app/play.html):** the simulation needs no install, wallet, or backend; optional live mode uses Sapient and the configured Esmeralda relay. The page also links to separate recorded transaction evidence.
- **Run the prebuilt local demo** after installing Ghostline on Linux x86-64:

  ```sh
  curl -fsSL https://raw.githubusercontent.com/AduAkorful/ghostline/main/scripts/install-ghostline.sh | sh && ghostline demo
  ```

  This runs the real local HTTP 402 → voucher → paid-call path. It does not contact a wallet or submit a transaction.

- **Install the CLI** on Linux x86-64:

  ```sh
  curl -fsSL https://raw.githubusercontent.com/AduAkorful/ghostline/main/scripts/install-ghostline.sh | sh
  ```

- **[Follow the CLI demo](docs/demo-runbook.md):** run the local quote → voucher → paid-call flow, then inspect the recorded Esmeralda close and its explorer links. The local flow is safe and repeatable; it does not submit a transaction.
- **Guided live run:** install the current `v0.4.0` release, set up your local Esmeralda wallet and test funds, then use the [guided runner](docs/quickstart.md#guided-esmeralda-demo). It prepares a local service, asks before each transaction, and prints explorer links after finality.
- **[Operator batch run](docs/demo-runbook.md):** configure an existing funded channel and paid API, then run `bash scripts/esmeralda-20-call-demo.sh` for 20 paid calls, one settlement, and an explorer link.
- **[Explore Ghostline](https://ghostline-ui.vercel.app/):** the landing page explains the flow and links to the browser demo, CLI guide, and on-chain evidence.
- **[Inspect verified Esmeralda evidence](demo/ui/evidence.json):** explorer links show the finalized template publish, channel open, and settlement; the local service ledger separately records 20 paid calls.

Ghostline is MIT licensed. Work began on October 12, 2026, after the October 1 contest announcement ([first commit](https://github.com/AduAkorful/ghostline/commit/e4701ea48731d117f3eda1620f60c0c6a193edef)). Ootle's announced November 11 mainnet launch is a target for a future Ghostline template release; it is not live or guaranteed.

## How it works

1. **Open:** The payer deposits funds into a channel with an expiry.
2. **Pay:** The payer signs a cumulative voucher for each service call. Calls do not create individual chain transactions.
3. **Settle:** The service submits the latest voucher. The contract pays the amount owed and returns the remainder.
4. **Refund:** After expiry, the payer can reclaim the remaining funds.

The service sees every request and voucher, plus the channel's declared deposit limit. Fresh Esmeralda runs on the latest template finalized a claim-free settlement and an expiry refund without revealing the channel deposit or output amounts in the inspected public transaction fields. The standard settlement had two stealth outputs; the refund had one. The local service ledger accepted 20 paid calls before the standard settlement. A separate two-channel batch prototype also finalized, but its local builder handled both deposits and the chain shows both channels in one transaction; it is not coordinator-private. See the [transaction evidence and explorer links](demo/ui/evidence.json). Fees, timing, branch, channel id, authorization data, output commitments, and output count remain visible. Funding patterns and timing may still link activity; call totals come from local ledgers, not chain proof.

### What each party sees

| View | What is visible |
|---|---|
| Service | Declared deposit limit, every request and price, voucher amount/mask, and accepted total. The deposit UTXO's secret mask stays with the payer in ordinary individual flows. |
| Chain observer | Channel and close activity, public fees and fee payer, timing, executed branch, channel and authorization metadata, output commitments, and a two-output settlement shape. In the tested claim-free close, the exact deposit, service amount, and payer remainder were not readable from transaction fields. |
| Batch coordinator | No production coordinator exists. The opt-in SDK prototype receives all participants' deposit UTXO openings in one process and is not coordinator-private. |
| Outside observer | The same public transaction fields and graph. Ghostline does not promise anonymity or unlinkable funding; public funding, timing, or reused keys can correlate activity. |

On a transparent account-based EVM payment path, each call can add another public transfer and reveal its amount and recipient. Ootle's native stealth UTXOs and confidential commitments let Ghostline batch many off-chain vouchers into a channel and hide tested deposit and settlement values without a mixer or a separate zero-knowledge circuit. Other chains can provide privacy through different designs; Ghostline does not claim this is unique to Tari. See [Tari's privacy overview](https://ootle.tari.com/concepts/privacy/) and Ghostline's [privacy notes](docs/privacy.md).

## Install

Install the CLI, MCP server, and reference API on glibc-based Linux x86-64 without cloning or compiling:

```sh
curl -fsSL https://raw.githubusercontent.com/AduAkorful/ghostline/main/scripts/install-ghostline.sh | sh
```

The installer downloads a [published release](https://github.com/AduAkorful/ghostline/releases), verifies its checksum, and installs to `~/.local/bin`. Start with the [installation guide](docs/install.md), then the [Esmeralda quickstart](docs/quickstart.md). A funded Ootle wallet and service configuration are required for live payments. Other operating systems can use the browser demo or build from source.

## Project components

| Component | Purpose |
|---|---|
| `contracts/channel` | Rust channel template compiled to WASM |
| `crates/ghostline-cli` | Operator CLI for channels, vouchers, and settlement |
| `crates/ghostline-sdk` | Rust channel and voucher APIs |
| `crates/ghostline-policy` | Spending limits and vendor controls |
| `middleware/ghostline-paywall` | HTTP 402 payment middleware |
| `crates/ghostline-mcp` | MCP stdio server for agent clients |
| `packages/ghostline` | TypeScript encodings and voucher signing |
| `demo/paid-api` | Reference API with paid endpoints |

## Documentation

- [Installation](docs/install.md): downloads, PATH, source builds, and prerequisites
- [Quickstart](docs/quickstart.md): open, pay, and settle through CLI or MCP
- [Terminal demo](docs/demo-runbook.md): reproduce the local flow and inspect recorded Esmeralda evidence
- [Motion demo](demo/ui/README.md): step through the CLI and browse its command map
- [Architecture](docs/architecture.md): components and payment flow
- [Privacy](docs/privacy.md): what payers, services, and observers can see
- [Threat model](docs/threat-model.md): security assumptions and residual risks
- [API reference](docs/api-reference.md): CLI, HTTP, SDK, and MCP surfaces
- [FAQ](docs/faq.md): fees, recovery, privacy, and readiness

## Development

Install the pinned Rust toolchain, `protoc`, CMake, and Clang. Rust workspace tests build the Ootle engine on the first run.

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
npm ci --prefix packages/ghostline
npm test --prefix packages/ghostline
```

AI tools assisted with research, implementation, tests, and documentation. The project owner reviews the code and validates releases.

## License

MIT. See [LICENSE](LICENSE).

Ghostline development began on October 12, 2026, after the contest's October 1 announcement.

## Roadmap

Ghostline is being built for Esmeralda today, with a mainnet-readiness goal aligned with the announced Ootle launch on November 11, 2026. Mainnet deployment is not yet live or guaranteed.
