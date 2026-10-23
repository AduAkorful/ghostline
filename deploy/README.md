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
   Every push to `main` triggers a Render deploy. The service stores wallet and
   channel state under ephemeral `/tmp`, so avoid pushing while a demo channel
   is open; a deploy can restart the process and lose that state.
2. Before the first deploy, create a dedicated Esmeralda relay wallet. Keep its
   seed words, wallet database password, and opener account's `view_only_key` private.
   Set these as Render secrets:
   - `GHOSTLINE_WALLET_SEED_WORDS`: seed words for this dedicated testnet wallet.
   - `GHOSTLINE_WALLET_PASSWORD`: a new password for the daemon's local database.
   - `GHOSTLINE_CLAIM_SECRET`: 64-character hex for that wallet's
     `view_only_key` at `GHOSTLINE_DEMO_OPENER_KEY_INDEX` (index 2 in the
     Blueprint). This must match the receiver account used by the relay;
     index 0 is incorrect when the opener uses index 2. The relay checks the
     corresponding public view key before offering a funding session.
     Do not use the payer/voucher signing key here.
   - `GHOSTLINE_DEMO_STATE_KEY`: a fresh 32-byte key encoded as 64 hex
     characters. Generate it locally with `openssl rand -hex 32`; enter it
     directly in Render. It encrypts recovery receipts that the browser keeps
     in session storage. Never publish the key or a recovery receipt.
   - `GHOSTLINE_INDEXER_URL`: the Esmeralda indexer URL used by the relay API.
     Walletd uses its network-configured indexer set by default.
   - Optional `GHOSTLINE_WALLET_INDEXER_URLS`: indexer URL(s) passed to
     walletd. The Blueprint defaults this to Esmeralda indexer-a. A hosted
     open failed when walletd used only indexer-b and that endpoint timed
     out in 30s. Set a comma-separated list only if those endpoints are
     verified for the same Esmeralda network.
   - `GHOSTLINE_DEMO_EXPIRY_EPOCHS`: a short positive lifetime, from 1 to 100.
   To obtain the matching view-only key, restore this dedicated seed in the
   [Tari Ootle wallet CLI](https://github.com/tari-project/ootle-wallet-cli)
   on your own machine and use its documented `ootle-wallet show-keys -a
   <account>` command. Copy the opener account's matching `view_only_key` into
   `GHOSTLINE_CLAIM_SECRET`; verify the index and network before saving it.
   This CLI displays secret key material locally, so do not run it in a shared
   terminal or paste its output into chat.
3. Startup prints the public `account_address=` for `ghostline-demo-fees`.
   `wallet account-ensure` only writes the wallet database; it does not publish
   the account component. When that component is missing on the indexer, startup
   claims the Esmeralda faucet once (`accounts.create_free_test_coins`, 1,000
   TARI) and pays the fee from the new account, capped at
   `GHOSTLINE_DEMO_OPEN_MAX_FEE` (100000 µT). That claim is public. A stealth
   transfer to the printed address does not create the component, and a second
   claim on the same key fails. The demo's private deposits arrive at the
   separate `ghostline-demo-opener` account from each user's Sapient wallet.
   These are derived accounts (indices 3 and 2 respectively) in the dedicated
   relay wallet. Each 51 µT user deposit covers five 10 µT calls and the
   required 1 µT change output; transaction fees are additional.
   Startup uses `ghostline wallet account-ensure --name <name> --key-index
   <index>` to reuse those same accounts after seed recovery, including when
   walletd restores them under `recovered-account-*` names. It verifies the
   owner key and changes only the local display name. If another account
   already has the desired name, setup stops for operator review. Keep the
   configured key indexes stable across deployments so the receive addresses
   and fee balances remain the same.
4. Deploy and wait for `GET /healthz` to return `ok: true` and `walletReady:
   true`. The public API binds immediately while walletd recovers; `/healthz`
   stays unready until both demo accounts exist and the wallet RPC responds.
   Render allows up to 15 minutes for a new instance to pass health checks.
   Copy the service's HTTPS URL. The Render wallet daemon listens only on
   loopback inside this container; do not add a public port for it.
5. In the Ghostline site's **Play** page, enter that HTTPS relay URL. Use a
   Sapient wallet on Esmeralda and approve the clearly labelled 51 µT test
   transfer and wallet fee. The first live run is not available until the
   deployed contract/template address and indexer match the selected Esmeralda
   deployment. `GHOSTLINE_TEMPLATE` may be either raw 32-byte hex or an Ootle
   `template_<hex>` address; the relay stores the hex form for `ghostline spec`.
6. Add the Render `/healthz` URL to an uptime monitor at five-minute intervals
   to reduce idle spin-down, then verify the service remains healthy.

After each relay open, settlement, and refund, the service refreshes the dedicated
fee account's wallet balance. It logs a warning when revealed native XTR falls
below 0.1 XTR (100,000 µT), and a separate warning if walletd cannot provide the
balance. Confidential balance is not counted as fee float. The one-time faucet
claim runs at startup. A later low-balance warning needs a transfer of revealed
test XTR to the printed fee address; do not expose walletd outside the container.

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
