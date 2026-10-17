# Ghostline

Private pay-per-call channels for AI agents on [Tari Ootle](https://ootle.tari.com). A payer locks funds once, signs cumulative vouchers for service calls, and settles the total on-chain in one transaction.

> **Early development. Unaudited. Esmeralda testnet only. Do not use real funds.**

## How it works

1. **Open:** The payer deposits funds into a channel with an expiry.
2. **Pay:** The payer signs a cumulative voucher for each service call. Calls do not create individual chain transactions.
3. **Settle:** The service submits the latest voucher. The contract pays the amount owed and returns the remainder.
4. **Refund:** After expiry, the payer can reclaim the remaining funds.

The service sees each request and voucher. Chain observers see channel transactions, settlement details, and fees. Funding patterns and timing may still create links between activity.

## Install

Install the CLI, MCP server, and reference API on glibc-based Linux x86-64 without cloning or compiling:

```sh
curl -fsSL https://raw.githubusercontent.com/AduAkorful/ghostline/main/scripts/install-ghostline.sh | sh
```

The installer downloads a [published release](https://github.com/AduAkorful/ghostline/releases), verifies its checksum, and installs to `~/.local/bin`. Start with the [installation guide](docs/install.md), then the [Esmeralda quickstart](docs/quickstart.md). A funded Ootle wallet and service configuration are required for live payments.

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
