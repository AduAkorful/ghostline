# Hosted Esmeralda relay

This deploys the opt-in custodial Sapient demo relay. It is testnet-only. The
relay receives each user's small private test transfer, opens the channel, and
controls the service settlement and expiry-refund key. A user keeps the
voucher-signing key in their browser. The relay cannot forge an early voucher,
but it can delay or withhold an expiry refund. Do not use real funds or reuse a
wallet that holds valuable assets.

## Render setup

1. In Render, create a Blueprint from this repository and select `main`. Review
   [`render.yaml`](../render.yaml); the service is named `ghostline-paywall`.
   Automatic deploys are disabled so pushing unrelated commits does not restart
   the wallet. Deploy manually only when no channel is open.
2. Before the first deploy, create a dedicated Esmeralda relay wallet. Keep its
   seed words, wallet database password, and index-0 `view_only_key` private.
   Set these as Render secrets:
   - `GHOSTLINE_WALLET_SEED_WORDS`: seed words for this dedicated testnet wallet.
   - `GHOSTLINE_WALLET_PASSWORD`: a new password for the daemon's local database.
   - `GHOSTLINE_CLAIM_SECRET`: 64-character hex for that wallet's
     `view_only_key` at index 0. This must match the receiver identity used by
     the wallet. Do not use the payer/voucher signing key here.
   - `GHOSTLINE_DEMO_STATE_KEY`: a fresh 32-byte key encoded as 64 hex
     characters. Generate it locally with `openssl rand -hex 32`; enter it
     directly in Render. It encrypts recovery receipts that the browser keeps
     in session storage. Never publish the key or a recovery receipt.
   - `GHOSTLINE_INDEXER_URL`: the Esmeralda indexer URL for the selected network.
   - `GHOSTLINE_DEMO_EXPIRY_EPOCHS`: a short positive lifetime, from 1 to 100.
   To obtain the matching view-only key, restore this dedicated seed in the
   [Tari Ootle wallet CLI](https://github.com/tari-project/ootle-wallet-cli)
   on your own machine and use its documented `ootle-wallet show-keys -a
   <account>` command. Copy the `view_only_key` for index 0 into
   `GHOSTLINE_CLAIM_SECRET`; verify the index and network before saving it.
   This CLI displays secret key material locally, so do not run it in a shared
   terminal or paste its output into chat.
3. Copy the `account_address=` value for `ghostline-demo-fees` from the Render
   startup logs and fund that account with Esmeralda test XTR for open,
   settlement, and refund fees. The demo's private deposits arrive at the
   separate `ghostline-demo-opener` account from each user's Sapient wallet.
   These are derived accounts (indices 2 and 1 respectively) in the dedicated
   relay wallet. Each 51 µT user deposit covers five 10 µT calls and the
   required 1 µT change output; transaction fees are additional.
4. Deploy and wait for `GET /healthz` to return `ok: true` and `walletReady:
   true`. Copy the service's HTTPS URL. The Render wallet daemon listens only
   on loopback inside this container; do not add a public port for it.
5. In the Ghostline site's **Play** page, enter that HTTPS relay URL. Use a
   Sapient wallet on Esmeralda and approve the clearly labelled 51 µT test
   transfer and wallet fee. The first live run is not available until the
   deployed contract/template address and indexer match the selected Esmeralda
   deployment.
6. Add the Render `/healthz` URL to an uptime monitor at five-minute intervals
   to reduce idle spin-down, then verify the service remains healthy.

After each relay open, settlement, and refund, the service refreshes the dedicated
fee account's wallet balance. It logs a warning when revealed native XTR falls
below 0.1 XTR (100,000 µT), and a separate warning if walletd cannot provide the
balance. Confidential balance is not counted as fee float. Top up the fee account
from the Esmeralda faucet when the warning appears; do not expose walletd outside
the container.

Never paste seed words or private keys into the site, a browser console, a
GitHub file, or a shared terminal. Enter secret values directly in Render's
secret fields. Keep `GHOSTLINE_CORS_ORIGINS` restricted to the deployed site.

## Storage and restart boundary

The free service stores wallet and relay state under `/tmp`. This is enough for
the normal process lifetime, and an uptime monitor can reduce idle sleep. It is
not durable storage: a replacement instance, redeploy, crash, or host restart
can lose the channel mask, ledgers, and wallet database. A seed can recreate
deterministic wallet keys and accounts, but it cannot reconstruct the relay's
channel ledger by itself. Do not redeploy or restart while a demo channel is
open. If the relay becomes unhealthy with open channels, stop accepting new
sessions and recover/close them before replacing the instance.

After channel open and each accepted call, the relay returns an authenticated,
encrypted recovery receipt to the browser. On a page reload, the browser can
present that receipt to rebuild the relay's channel file, call ledger, signer
references, and transaction state. The receipt is a bearer recovery credential;
protect it like a password. This path still depends on walletd being able to
recreate the seed-derived key references after a container replacement. That
behavior must be verified on the deployed walletd before claiming restart
recovery. The free-tier monitor reduces idle sleep; it does not prevent other
restarts.

## Build locally

From the repository root:

```sh
docker build -f deploy/Dockerfile -t ghostline-paywall .
```

Do not pass secret values as Docker build arguments. Provide them only at
runtime through Render's secret environment fields.
