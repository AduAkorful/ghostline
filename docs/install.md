# Install Ghostline

Ghostline `v0.3.2` is the source version prepared for the guided Esmeralda runner. Its prebuilt download will be available from [GitHub Releases](https://github.com/AduAkorful/ghostline/releases/tag/v0.3.2) after the matching tag is pushed and CI publishes it. The download includes `ghostline`, `ghostline-paid-api`, and `ghostline-mcp`. These run on your computer; installation does not create or fund a wallet.

## Download the executables

Prebuilt downloads support **glibc-based Linux x86-64**. The installer requires `curl`, `tar`, `sha256sum`, and `awk`. It downloads the latest release, checks its SHA-256 checksum, and installs without root:

```sh
curl -fsSL https://raw.githubusercontent.com/AduAkorful/ghostline/main/scripts/install-ghostline.sh | sh
```

The default directory is `~/.local/bin`. If your shell cannot find the commands, add it to your PATH:

```sh
export PATH="$HOME/.local/bin:$PATH"
ghostline --help
ghostline-paid-api --help
```

Add that PATH line to your shell's startup file to keep it after reopening the terminal. Launch `ghostline-mcp` through an MCP client; it communicates over stdio.

After the `v0.3.2` release is published, select it explicitly:

```sh
curl -fsSL https://raw.githubusercontent.com/AduAkorful/ghostline/main/scripts/install-ghostline.sh | GHOSTLINE_VERSION=v0.3.2 sh
```

Set `GHOSTLINE_INSTALL_DIR` on the `sh` process to choose a different directory. To update, rerun the installer. It replaces the three executables; keep your wallet profiles and channel state in their own directories. The checksum checks the archive against the release's checksum file; it is not an independent publisher signature.

## Before making a live payment

The installer supplies Ghostline's executables. Prepare the following for the [Esmeralda quickstart](quickstart.md):

| Requirement | Where it comes from |
|---|---|
| Compatible Ootle wallet daemon connected to Esmeralda | Your [Tari Ootle setup](https://github.com/tari-project/tari-ootle); wallet installation and upgrades are separate |
| Funded opener account | Your test wallet; it needs the channel deposit plus opening fees |
| Funded service fee account | The service's wallet; it pays settlement fees separately from the channel deposit |
| Deployment parameters | The provider/deployment record: template hash, indexer URL, network tag and service public key |
| Payer and claim secrets | Your private key storage; the Ghostline payer key signs vouchers, while the wallet view key unblinds the channel output |
| Service signer reference | A KeyId JSON file for the service signing key held in the fee wallet |
| Private state directory | Channel record, payer session, service ledger and optional policy audit log |

The CLI currently authenticates to a local wallet daemon using its `None` authentication flow. Keep that endpoint on loopback; it grants wallet administration access. A separately funded fee wallet can reduce direct funding linkage, but fees and funding transfers remain public.

Confirm incoming tTARI is visible in the wallet before opening a channel, then use the open dry run to check available inputs and fees. A successful Ghostline installation does not establish wallet/network compatibility or guarantee funding discovery. Preserve the wallet profile if funding is missing; see [troubleshooting](quickstart.md#troubleshooting).

The repository does not supply a hosted paid service, shared wallet, or default deployed template. Get the public channel terms from the provider you intend to pay. Operators running the reference API need their own deployment record and service signer setup. Never substitute an invented template or account identifier.

## Build from source

The verified development platform is Linux. Prebuilt downloads for macOS, Windows, ARM, and musl-based Linux are not provided; this guide does not establish source-build support on those platforms.

Install Rust through `rustup`, Git, a C/C++ toolchain, Clang with libclang, CMake, and the protobuf compiler (`protoc`). The repository pins Rust in `rust-toolchain.toml`.

```sh
git clone https://github.com/AduAkorful/ghostline.git
cd ghostline
git checkout v0.3.2
cargo build --locked --release -p ghostline-cli -p ghostline-mcp -p ghostline-paid-api
mkdir -p "$HOME/.local/bin"
install -m 0755 target/release/ghostline target/release/ghostline-mcp target/release/ghostline-paid-api "$HOME/.local/bin/"
```

These commands build the published version's host executables. To build current development code, select the intended commit before building. Building the channel WASM and publishing a template are separate operator tasks; a template hash must come from an actual deployment.

Continue with the [quickstart](quickstart.md). Developers can find workspace checks in [Development](../README.md#development).
