# Settling and payouts

The service redeems the latest accepted voucher in one transaction before expiry. Preserve the service ledger until you confirm the close and payout. Accepted calls in a ledger are off-chain accounting.

## Settle before expiry

Use `ghostline channel status --file channel.json` to check the expiry window and output presence. Use the normal settlement margin or `--force` to settle earlier. At the expiry epoch the refund path opens and settlement is refused. A spent output also blocks a second close.

```bash
ghostline channel settle --file channel.json --force --submit --dry-run \
  --seal-key-id-file service-key-id.json --max-fee 35000
```

Review the actual dry-run result and the service wallet's revealed balance. Set the cap from the quote with an explicit margin your balance can cover. The wallet holds the full cap during execution and refunds the unused part on the revealed-account fee path. Remove `--dry-run` only when you intend to submit. Check the indexer's final result and look for the payout in your service wallet.

## Use a wallet payout destination

Generate a destination with `ghostline wallet payout-destination --address` followed by your service wallet address. Save the result and set it on the channel record with `--payout-destination` at initialization. Set the API's `--service-output-key-hex` to the result's `owner_key_hex`. This one-time payout key is distinct from the public service authorization key. Mainnet runs verified that walletd finds this payout.

The protocol fee output uses the treasury key from the on-chain config. Walletd does not discover that fixed-key output or the payer's script-locked change through its regular scan. Consult the [mainnet quickstart](mainnet-quickstart.md#7-settle) before relying on those balances.

## Refund an expired channel

At or after expiry, the payer can reclaim the deposit if the channel output is still unspent. Use the payer wallet's signer reference and fee account with `ghostline channel refund`. Follow the [refund walkthrough](mainnet-quickstart.md#8-expiry-refund-if-the-service-never-settles) for the dry run and submission.
