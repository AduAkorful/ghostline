# Mainnet quickstart

This guide runs one Ghostline channel on Tari Ootle mainnet with your own wallet: open, paid calls, settle, and (on a second channel) an expiry refund. Every command here was run on mainnet on 13 Nov 2026; the transactions are listed at the end. **This moves real TARI.** Start with small deposits.

Ghostline is unaudited. Fees are public. If one wallet plays every role, as in this guide, an observer can link the open, the settle and the fees; see [Privacy](#privacy) before you rely on it.

## What you need

- **Ghostline built from source** ([install](install.md#build-from-source)) at commit `bd8ad10` or later. Build `ghostline` and `ghostline-paid-api` with `cargo build --release`. The published `v0.4.0` release predates the protocol fee and mainnet, so it does not work.
- **Tari Ootle wallet daemon 0.53.3** (`tari_ootle_walletd`), from the [tari-ootle v0.53.3 release](https://github.com/tari-project/tari-ootle/releases/tag/v0.53.3). Check the published `.sha256` before running it. There is no Intel macOS build.
- **TARI in that wallet.** Burn XTM on L1 and claim it (Tari Universe does this). A claim arrives as a private (stealth) balance.
- `curl`, `python3` and `openssl` for the helper commands below.

## Mainnet deployment

These are the addresses from the [mainnet deployment](deployment.md). Check them against the indexer yourself (`GET /substates/<address>` should return HTTP 200).

| | Value |
|---|---|
| Channel template | [`ae13cead3c34052116c49fc09e4645f3a62edf55857c0e2e45226dd7093cef9c`](https://indexer-a.ootle.tari.com/substates/template_ae13cead3c34052116c49fc09e4645f3a62edf55857c0e2e45226dd7093cef9c) |
| Fee config | [`component_37df6af59007b9c02ead46df7e320af4255978cf8fefb164fb8bfdc4183d0bb5`](https://indexer-a.ootle.tari.com/substates/component_37df6af59007b9c02ead46df7e320af4255978cf8fefb164fb8bfdc4183d0bb5) |
| Indexers | `https://indexer-a.ootle.tari.com`, `https://indexer-b.ootle.tari.com` |
| Network tag / byte | `0` |

Settlements pay 1% of the service payout to the fee config's current treasury. Ghostline reads that key from the chain at settlement; you do not configure it.

## Fees you will pay

Mainnet observations from 13 Nov 2026. Dry-run the actual transaction because fees change.

| Transaction | Paid by | Fee (µT) |
|---|---|---|
| Create your account with revealed TARI (once) | wallet, from private balance | 11,070 |
| Channel open | opener | 15,919 |
| Settle | service fee account | ~28,800 |
| Expiry refund | payer fee account | 10,014 |

A settle costs about 29,000 µT, so a channel only makes sense when the payments it carries are worth well more than that. Prices must be multiples of 100 µT. A deposit `D` can settle at most `floor((D − 1) / 101) × 100` µT: the 1% fee and 1 µT of change are reserved.

## 1. Start the wallet with cookie login

```bash
tari_ootle_walletd-0.53.3 --network mainnet --base-path ~/ootle_mainnet_wallet --indexer-api-url https://indexer-a.ootle.tari.com/,https://indexer-b.ootle.tari.com/ --listen-on 127.0.0.1:5100 --cookie-auth true
```

- `--cookie-auth true` writes an owner-only token to `~/ootle_mainnet_wallet/mainnet/.cookie`. Ghostline uses it when `GHOSTLINE_WALLET_COOKIE` points at it.
- **Never run a funded wallet with `--authentication none`.** In that mode any program on the computer can spend.
- Keep the listener on `127.0.0.1`.

```bash
export GHOSTLINE_WALLET_COOKIE=~/ootle_mainnet_wallet/mainnet/.cookie
```

## 2. Put some TARI where fees can be paid

Claimed TARI is a private output sent to your account address, and the account component may not exist on chain yet. Settle and refund fees are paid from an account's **revealed** balance, so reveal a small amount. One self-transfer creates the account and funds it.

Dry-run first, then submit with `max_fee` equal to the dry-run's `required_fees`. On this path the whole cap is charged, so do not round it up.

```bash
C=$(cat "$GHOSTLINE_WALLET_COOKIE")
ADDRESS=$(ghostline wallet account-get --name <account-name> | sed -n 's/^address=//p')
reveal() { curl -s -H "Authorization: Bearer $C" -H 'content-type: application/json' http://127.0.0.1:5100/json_rpc -d "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"accounts.stealth_transfer\",\"params\":{\"owner_account\":{\"Name\":\"<account-name>\"},\"fee_params\":{\"input_selection\":\"ConfidentialOnly\",\"pay_fee_with_swap\":null},\"input_selection\":\"ConfidentialOnly\",\"resource_address\":\"resource_$(printf '01%.0s' $(seq 32))\",\"transfers\":[{\"destination_address\":\"$ADDRESS\",\"blinded_output_amount\":\"0\",\"revealed_output_amount\":1000000,\"pay_to\":\"StealthPublicKey\"}],\"max_fee\":\"$1\",\"dry_run\":$2}}"; }
reveal 200000 true    # read result.required_fees
reveal <required_fees> false
```

This reveals 1 TARI and makes that amount and your account public. Use this account for fees and service payouts, not as a private payer.

## 3. Export the claim secret

Ghostline needs your account's **view secret key** to unblind the channel output it opens. walletd only exports it to a logged-in browser session (`accounts.export_view_key` refuses the cookie), and its web UI has no export button.

1. Open `http://localhost:5100` and log in.
2. In DevTools → Network, copy the `Authorization: Bearer …` value from any `json_rpc` request.
3. Run this, paste the token, and press Enter. The key goes straight into an owner-only file and is never shown:

```bash
umask 077; mkdir -p ~/.ghostline/mainnet/run && read -rs T && curl -s -H "Authorization: Bearer $T" -H 'content-type: application/json' http://127.0.0.1:5100/json_rpc -d '{"jsonrpc":"2.0","id":1,"method":"accounts.export_view_key","params":{"account":{"Name":"<account-name>"}}}' | python3 -c 'import sys,json,os;d=json.load(sys.stdin);r=d.get("result") or sys.exit("walletd: "+str(d.get("error")));fd=os.open(os.path.expanduser("~/.ghostline/mainnet/run/claim.secret"),os.O_WRONLY|os.O_CREAT|os.O_TRUNC,0o600);os.write(fd,r["view_secret_key"].encode());os.close(fd);print("saved claim.secret")'; unset T
```

This is not your account's public key. Anyone holding it can read your private amounts.

## 4. Create keys and the channel record

Use a fresh directory and fresh keys for every channel.

```bash
R=~/.ghostline/mainnet/run; cd "$R"
ghostline keygen --role payer --out payer.json                       # note public=
ghostline wallet key-create --out service-key-id.json                # service signer
ghostline wallet key-create --out refund-key-id.json                 # expiry refund signer
ghostline wallet payout-destination --address "$ADDRESS" --ootle-network mainnet --out payout-destination.json
openssl rand -hex 32 > nonce.hex
```

- **Refund key:** the refund key must be a wallet key. If the refund branch used the file-based payer key, walletd could not sign the refund.
- **Payout destination:** `payout-destination` prints `owner_key_hex`. That is a one-time key derived from your wallet address, and the service quotes it so the payout lands in your wallet.

Check the current epoch with `curl -s https://indexer-a.ootle.tari.com/info`. Epochs were about 10–20 minutes apart. Then:

```bash
COMMON="--network-tag 0 --deposit 20201 --payer-hex <payer-public> --service-hex <service-public> --refund-hex <refund-public> --expiry-epoch <epoch + 144> --nonce-hex $(cat nonce.hex) --template-hex ae13cead3c34052116c49fc09e4645f3a62edf55857c0e2e45226dd7093cef9c --fee-config component_37df6af59007b9c02ead46df7e320af4255978cf8fefb164fb8bfdc4183d0bb5"
ghostline spec $COMMON                                               # note payer_root=
ACCOUNT=<your account component_…>
ghostline channel init --out channel.json $COMMON \
  --indexer-url https://indexer-a.ootle.tari.com --ootle-network mainnet --max-epoch <epoch + 50> \
  --opener-account <account-name> --opener-wallet-url http://127.0.0.1:5100 \
  --fee-account "$ACCOUNT" --fee-account-name <account-name> --fee-wallet-url http://127.0.0.1:5100 \
  --payer-fee-account "$ACCOUNT" --payer-fee-account-name <account-name> --payer-wallet-url http://127.0.0.1:5100 \
  --ledger ledger.json --session session.json --payout-destination payout-destination.json
ghostline-paid-api init --ledger ledger.json --session session.json --network-tag 0 --deposit 20201 \
  --payer-hex <payer-public> --payer-root-hex <payer_root> --nonce-hex "$(cat nonce.hex)"
```

`--max-epoch` bounds when transactions built from this record are valid. Settle and refund must happen before it; refund also needs it to be after expiry.

## 5. Open the channel

```bash
GHOSTLINE_CLAIM_SECRET=$(cat claim.secret) ghostline channel open --file channel.json --dry-run
GHOSTLINE_CLAIM_SECRET=$(cat claim.secret) ghostline channel open --file channel.json --max-fee 20000
```

- The live open dry-runs again and submits with walletd's `required_fee`; the `--max-fee` value is a hard ceiling.
- `value=20201` confirms the deposit was unblinded. The deposit mask is now stored in `channel.json`, so that file is secret.

## 6. Serve and pay

```bash
ghostline-paid-api serve --bind 127.0.0.1:8787 --ledger ledger.json --price 1000 \
  --service-key-hex <service-public> --service-output-key-hex <owner_key_hex> \
  --min-expiry-epoch <epoch> --max-expiry-epoch <epoch + 200>
```

In another terminal:

```bash
GHOSTLINE_PAYER_SECRET=$(python3 -c "import json;print(json.load(open('payer.json'))['secret_hex'])") \
  ghostline-paid-api call --url http://127.0.0.1:8787/fortune --session session.json \
  --calls 5 --vendor-hex <service-public> --budget 5000 --max-per-call 1000
```

## 7. Settle

```bash
ghostline channel settle --file channel.json --force --submit --dry-run --seal-key-id-file service-key-id.json --max-fee 100000
ghostline channel settle --file channel.json --force --submit --seal-key-id-file service-key-id.json --max-fee <required_fees + margin>
ghostline indexer result --url https://indexer-a.ootle.tari.com --transaction-id <settle-tx>
```

- The fee is paid from the account's revealed balance, and the unused part of the cap is refunded.
- The payout appears in `stealth_utxos.list` (and the wallet UI) as unspent on walletd's next scan.
- **Not visible to walletd:**
  - The **protocol fee output**, which the template locks to the treasury key.
  - The **payer change**, which is locked to the payer's script.
  - Recovering them needs a sweep. It is not built yet.

## 8. Expiry refund (if the service never settles)

At or after the expiry epoch, on an unspent channel:

```bash
ghostline channel refund --file channel.json --submit --dry-run --max-fee 100000 --seal-key-id-file refund-key-id.json
ghostline channel refund --file channel.json --submit --max-fee <required_fees + margin> --seal-key-id-file refund-key-id.json
```

The deposit is refunded to the payer wallet's own address, and walletd lists it on its next scan. Before expiry, refund prints `wait` and does nothing.

## Agent path (MCP)

`ghostline-mcp` runs the same flow as stdio tools for an agent host. Run on mainnet on 13 Nov 2026: open [`83e7510e…85f0`](https://indexer-a.ootle.tari.com/transactions/83e7510e9d504a3e80b6f4e45f17d41894931eca6b1dcaa256eee7526b9985f0), five `ghostline_call_paid` calls, sixth refused by policy, settle [`ca036bf1…b8c0`](https://indexer-a.ootle.tari.com/transactions/ca036bf190cfc29786511feaf91d4a6b0579d4058ce7cc55a6655063d03ab8c0).

**Payer side.** Give the server only the payer wallet's cookie:

```text
GHOSTLINE_CHANNEL_FILE=<run>/channel.json          # created with --claim-key-file
GHOSTLINE_WALLET_COOKIE=<payer wallet>/mainnet/.cookie
GHOSTLINE_PAYER_SECRET=<payer key secret>          # from a private environment, never a tool argument
GHOSTLINE_POLICY_BUDGET=5000
GHOSTLINE_POLICY_MAX_PER_CALL=1000
GHOSTLINE_POLICY_VENDOR_HEX=<service-public>
GHOSTLINE_POLICY_AUDIT_LOG=<run>/policy-audit.jsonl
GHOSTLINE_OPEN_MAX_FEE=20000                       # also needed for the dry-run on a small wallet
```

- **Opening:** call `ghostline_open_channel` with `{}` to get a quote. Add `GHOSTLINE_OPEN_SUBMIT=1`, restart the server and call it again. The live open submits at walletd's quote, never above the cap.
- **Paying:** `ghostline_call_paid {"url": …}` pays per call. A call past the budget is refused and logged.

**Service side.** A separate server gets only the service wallet's cookie:

```text
GHOSTLINE_CHANNEL_FILE=<run>/channel.json          # with payout_destination set
GHOSTLINE_WALLET_COOKIE=<service wallet>/mainnet/.cookie
GHOSTLINE_SEAL_KEY_ID_FILE=<run>/service-key-id.json
GHOSTLINE_SETTLE_FORCE=1
GHOSTLINE_CLOSE_MAX_FEE=35000                      # pay_fee locks the whole cap; keep it below the revealed balance
GHOSTLINE_CLOSE_DRY_RUN=1                          # then GHOSTLINE_CLOSE_SUBMIT=1
```

Call `ghostline_close_channel` with `{}`. The settle pays the treasury read from the fee config and addresses the payout to the service wallet.

## Privacy

- **Hidden from observers:** the deposit, the exact payout and payer change, and the individual calls.
- **Visible to observers:**
  - that a channel was opened and settled or refunded
  - all fees and timing
  - which branch was taken
  - the fee config address and treasury key on every settle
  - links between accounts that share a seed or pay each other's fees
- **What this guide does:** it uses one wallet for every role, which links everything. For real privacy, the payer, the service and each fee account need separate wallets, funded independently.

## Mainnet record (13 Nov 2026)

- Open [`e0abfa2b…5cdd`](https://indexer-a.ootle.tari.com/transactions/e0abfa2b6381a793f592b9396bc5d10635254528a19fb481e33eed1c76365cdd), 10 calls, settle [`b0270a26…ba72`](https://indexer-a.ootle.tari.com/transactions/b0270a268121e67922884423cb5123a7442a5a1833f6c694f74058d81140ba72): the first fee output.
- Open [`2053b611…41e5`](https://indexer-a.ootle.tari.com/transactions/2053b611c4b09741a0d5f2ceb4db0d4b6b38e97bbccf6bba1a82051e999641e5), 5 calls, settle [`f4aa9c61…67c1`](https://indexer-a.ootle.tari.com/transactions/f4aa9c613d5def17c0ca2ac4c50d266febf997293f49c8e70dbe71a90cd667c1): the payout was found by walletd.
- Open [`1664ba30…a0b6`](https://indexer-a.ootle.tari.com/transactions/1664ba301a51fe7de3ad1e44336757b056aa03f368f1c9ded9822ab094a7a0b6), expiry refund [`4cc8c48f…9204`](https://indexer-a.ootle.tari.com/transactions/4cc8c48f3428cfa99ce82e271e9960c1ca5b8f4ca7ccb6f5d8e620e68b469204): the deposit was found by walletd.
