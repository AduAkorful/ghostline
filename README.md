# Ghostline

👻 **Ghost payments for AI agents:** many signed off-chain whispers, one sealed séance receipt on [Tari Ootle](https://ootle.tari.com).

Ghostline lets an agent pay a service request by request while keeping the individual voucher stream off-chain. The payer funds a channel, signs cumulative vouchers, and the service settles the accepted amount once.

> **Live on Tari Ootle mainnet since 13 Nov 2026. Unaudited: keep deposits small, and expect fees and the open and close records to be public.**

## Try Ghostline

- **[Read the docs](https://ghostline-ui.vercel.app/docs/):** install, the mainnet quickstart, the MCP server, the spending policy, the paywall, fees and privacy. Every page is also plain Markdown: start from [`llms.txt`](https://ghostline-ui.vercel.app/llms.txt).
- **[Run a channel on mainnet](docs/mainnet-quickstart.md)** with your own wallet: open, paid calls, settle, and an expiry refund. This moves real TARI, so start with a small deposit.
- **[Let an agent pay](docs/mcp.md):** run the local MCP server and set a [spending policy](docs/policy.md) that lives outside the agent.
- **[Check the chain](https://ghostline-ui.vercel.app/proof/):** every recorded mainnet transaction with its indexer link, and the [deployed addresses](docs/deployment.md).
- **[See what stays private](https://ghostline-ui.vercel.app/privacy/):** a simulation that puts your ledger beside the chain's view. It needs no wallet.
- **Use a coding agent:** install the [Ghostline skill](skills/ghostline/SKILL.md) or read [Use with AI agents](docs/ai-agents.md).
- **Run the local demo.** It runs the real HTTP 402 → voucher → paid-call path on your machine and does not contact a wallet or submit a transaction. After installing a release for Linux x86-64:

  ```sh
  curl -fsSL https://raw.githubusercontent.com/AduAkorful/ghostline/main/scripts/install-ghostline.sh | sh && ghostline demo
  ```

- **[Esmeralda contest demo](https://ghostline-ui.vercel.app/demo/)** and its [recorded evidence](https://ghostline-ui.vercel.app/evidence.html): the original testnet entry, kept as a historical record. It targets the retired Esmeralda template, not mainnet.

Ghostline is MIT licensed. Work began on October 12, 2026, after the October 1 contest announcement ([first commit](https://github.com/AduAkorful/ghostline/commit/e4701ea48731d117f3eda1620f60c0c6a193edef)). The channel template and fee config were published on Tari Ootle mainnet on 13 Nov 2026; see [mainnet deployment](docs/deployment.md).

## How it works

1. **Open:** The payer deposits funds into a channel with an expiry.
2. **Pay:** The payer signs a cumulative voucher for each service call. Calls do not create individual chain transactions.
3. **Settle:** The service submits the latest voucher. The contract pays the service, sends a 1% protocol fee to the treasury, and returns the remainder to the payer.
4. **Refund:** After expiry, the payer can reclaim the remaining funds.

The service sees every request and voucher, plus the channel's declared deposit limit. A mainnet observer test read the public indexer with no keys: the deposit, the payout, the protocol fee and the payer's change were commitments only, and the call count and prices did not appear. What stays public is the network fees, the open and settle records and their link through the channel output, the service's fee account, the fixed three-output settlement shape (service payout, payer change, treasury fee), and any funding link you create, such as funding both wallets from one source. Call totals come from local ledgers, not chain proof. See the [privacy notes](docs/privacy.md) for the full test and the earlier Esmeralda evidence.

### What each party sees

| View | What is visible |
|---|---|
| Service | Declared deposit limit, every request and price, voucher amount/mask, and accepted total. The deposit UTXO's secret mask stays with the payer in ordinary individual flows. |
| Chain observer | Channel and close activity, public fees and fee payer, timing, executed branch, channel and authorization metadata, output commitments, and a three-output settlement shape. In the mainnet observer test, the deposit, payout, protocol fee and payer change were not readable from transaction fields. |
| Batch coordinator | No production coordinator exists. The opt-in SDK prototype receives all participants' deposit UTXO openings in one process, is not coordinator-private, and has not been run on mainnet. |
| Outside observer | The same public transaction fields and graph. Ghostline does not promise anonymity or unlinkable funding; public funding, timing, or reused keys can correlate activity. |

On a transparent account-based EVM payment path, each call can add another public transfer and reveal its amount and recipient. Ootle's native stealth UTXOs and confidential commitments let Ghostline batch many off-chain vouchers into a channel and hide tested deposit and settlement values without a mixer or a separate zero-knowledge circuit. Other chains can provide privacy through different designs; Ghostline does not claim this is unique to Tari. See [Tari's privacy overview](https://ootle.tari.com/concepts/privacy/) and Ghostline's [privacy notes](docs/privacy.md).

## Install

Mainnet needs Ghostline 0.5.0 or later: the `v0.4.0` release predates the protocol fee and the mainnet template. Install the latest release or build from source with the [installation guide](docs/install.md), then follow the [mainnet quickstart](docs/mainnet-quickstart.md). You also need Tari Ootle wallet daemon 0.53.3 and TARI in that wallet.

On glibc-based Linux x86-64 the installer downloads the latest release without compiling:

```sh
curl -fsSL https://raw.githubusercontent.com/AduAkorful/ghostline/main/scripts/install-ghostline.sh | sh
```

The installer verifies the release checksum and installs to `~/.local/bin`.

## Project components

| Component | Purpose |
|---|---|
| `contracts/channel` | Rust channel template compiled to WASM |
| `contracts/fee-config` | On-chain component that holds the treasury key for the protocol fee |
| `crates/ghostline-cli` | Operator CLI for channels, vouchers, and settlement |
| `crates/ghostline-sdk` | Rust channel and voucher APIs |
| `crates/ghostline-policy` | Spending limits and vendor controls |
| `middleware/ghostline-paywall` | HTTP 402 payment middleware |
| `crates/ghostline-mcp` | MCP stdio server for agent clients |
| `packages/ghostline` | TypeScript encodings and voucher signing |
| `demo/paid-api` | Reference API with paid endpoints |
| `app` | The website: docs generated from `docs/`, the mainnet proof, and the fee calculator |
| `skills/ghostline` | The Ghostline skill for coding agents |

## Documentation

- [What is Ghostline](docs/overview.md): the short version, and where to start
- [Installation](docs/install.md): source builds and prerequisites
- [Mainnet quickstart](docs/mainnet-quickstart.md): open, pay, settle and refund on Tari Ootle mainnet, through the CLI or MCP
- [MCP server](docs/mcp.md), [spending policy](docs/policy.md) and [use with AI agents](docs/ai-agents.md)
- [HTTP 402 paywall](docs/paywall.md) and [settling and payouts](docs/settle.md): for API sellers
- [Fees](docs/fees.md) and [mainnet deployment](docs/deployment.md): costs, addresses and parameters
- [Architecture](docs/architecture.md): components and payment flow
- [Privacy](docs/privacy.md): what payers, services and observers can see
- [Threat model](docs/threat-model.md): security assumptions and residual risks
- [API reference](docs/api-reference.md), [CLI](docs/cli.md), [TypeScript SDK](docs/typescript.md) and [glossary](docs/glossary.md)
- [FAQ](docs/faq.md) and [troubleshooting](docs/troubleshooting.md)
- [Esmeralda quickstart](docs/quickstart.md), [terminal demo](docs/demo-runbook.md) and [motion demo](demo/ui/README.md): the earlier testnet material

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

Ghostline is live on Tari Ootle mainnet, and its next steps are open. A desktop app is planned and has not been started. Pooled settlement is research only. No independent audit has been done.
