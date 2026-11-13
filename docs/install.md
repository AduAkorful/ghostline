# Install Ghostline

Ghostline's executables are `ghostline` (the command-line tool), `ghostline-mcp` (the MCP server) and `ghostline-paid-api` (the reference paywalled API). They run on your computer; installing them does not create or fund a wallet.

**For mainnet, use Ghostline 0.5.0 or later.** Install the latest release below, or build from source.

## Build from source

The verified development platform is Linux. Prebuilt downloads for macOS, Windows, ARM, and musl-based Linux are not provided; this guide does not establish source-build support on those platforms.

Install Rust through `rustup`, Git, a C/C++ toolchain, Clang with libclang, CMake, and the protobuf compiler (`protoc`). Use the Rust version the source pins.

```sh
git clone https://github.com/AduAkorful/ghostline.git
cd ghostline
cargo build --locked --release -p ghostline-cli -p ghostline-mcp -p ghostline-paid-api
mkdir -p "$HOME/.local/bin"
install -m 0755 target/release/ghostline target/release/ghostline-mcp target/release/ghostline-paid-api "$HOME/.local/bin/"
```

These commands build the host executables from the current source. Building the channel WASM and publishing a template are separate operator tasks; a template hash must come from an actual deployment.

## Download the executables

Releases from 0.5.0 include everything the mainnet quickstart needs. Prebuilt downloads support **glibc-based Linux x86-64**. The installer requires `curl`, `tar`, `sha256sum`, and `awk`. It downloads the latest release, checks its SHA-256 checksum, and installs without root:

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

To install a specific version, for example `v0.5.0`:

```sh
curl -fsSL https://raw.githubusercontent.com/AduAkorful/ghostline/main/scripts/install-ghostline.sh | GHOSTLINE_VERSION=v0.5.0 sh
```

After installation, run the bounded local demo. It uses the real HTTP 402 quote, voucher signing, and paid API code on loopback, then exits without wallet access or chain transactions:

```sh
ghostline demo
```

Set `GHOSTLINE_INSTALL_DIR` on the `sh` process to choose a different directory. To update, rerun the installer. It replaces the three executables; keep your wallet profiles and channel state in their own directories. The checksum checks the archive against the release's checksum file; it is not an independent publisher signature.

## Before making a live payment

Prepare the following for the [mainnet quickstart](mainnet-quickstart.md):

| Requirement | Where it comes from |
|---|---|
| Tari Ootle wallet daemon 0.53.3 | Your own [Tari Ootle setup](https://github.com/tari-project/tari-ootle). Run it with `--cookie-auth true`; see [start the wallet](mainnet-quickstart.md#1-start-the-wallet-with-cookie-login) |
| TARI in that wallet | Burn XTM on L1 and claim it; Tari Universe does this |
| A fee account with revealed TARI | Settle and refund fees are paid from an account's revealed balance; see [fees](fees.md) |
| Deployment parameters | The [mainnet deployment](deployment.md): template, fee config and indexers |
| Payer key and claim secret | Your private key storage; see [export the claim secret](mainnet-quickstart.md#3-export-the-claim-secret) |
| Service signer reference | A KeyId JSON file for the service signing key held in the service wallet |
| Private state directory | Channel record, payer session, service ledger and optional policy audit log |

**Never run a funded wallet with `--authentication none`.** In that mode any program on the computer can spend. Keep the wallet's listener on `127.0.0.1`. A separately funded fee wallet can reduce direct funding linkage, but fees and funding transfers remain public.

Confirm the TARI is visible in the wallet before opening a channel, then use the open dry run to check available inputs and fees. A successful installation does not establish wallet or network compatibility or guarantee funding discovery. Preserve the wallet profile if funding is missing; see [troubleshooting](troubleshooting.md).

Ghostline does not supply a hosted paid service, shared wallet, or default deployed template. Get the public channel terms from the provider you intend to pay. Operators running the reference API need their own deployment record and service signer setup. Never substitute an invented template or account identifier.

Continue with the [mainnet quickstart](mainnet-quickstart.md).
