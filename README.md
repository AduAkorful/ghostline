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

Linux x86-64 users can install the CLI, MCP server, and reference API from a [GitHub release](https://github.com/AduAkorful/ghostline/releases) once one is available:

```sh
curl -fsSL https://raw.githubusercontent.com/AduAkorful/ghostline/main/scripts/install-ghostline.sh | sh
```

The installer verifies the archive checksum and places the commands in `~/.local/bin`. Prebuilt binaries currently target glibc-based Linux x86-64. See the [quickstart](docs/quickstart.md) for setup and Esmeralda instructions.

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

- [Quickstart](docs/quickstart.md): build, configure, and run Ghostline on Esmeralda
- [Architecture](docs/architecture.md): components and payment flow
- [Privacy](docs/privacy.md): what payers, services, and observers can see
- [Threat model](docs/threat-model.md): security assumptions and residual risks
- [API reference](docs/api-reference.md): CLI, HTTP, SDK, and MCP surfaces
- [FAQ](docs/faq.md): fees, recovery, privacy, and readiness
- [Evidence demo](demo/ui/README.md): static summary of a historical Esmeralda run

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
