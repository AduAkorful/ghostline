# Troubleshooting

## Wallet authentication

Use a local wallet daemon with cookie authentication as shown in the [mainnet quickstart](mainnet-quickstart.md#1-start-the-wallet-with-cookie-login). Confirm `GHOSTLINE_WALLET_COOKIE` points to the cookie for the wallet URL in the channel record, and that the signing process can read it. Use absolute paths in your MCP config.

## Fee cap and fee balance

Re-run the same action as a dry run. For opens, compare `required_fee` with `--max-fee` or `GHOSTLINE_OPEN_MAX_FEE`. For settle or refund, check that the correct fee account has revealed funds to cover the full cap. Recheck the quote if the account or transaction changed.

## No channel or missing session

`needs_channel` means the paid call could not use an open payer session. Check the channel file and whether the open stored its deposit mask. Run `ghostline channel session --file channel.json` for an opened record or call `ghostline_channel_session` with the channel file configured. Do not reset a session to make an old total appear spendable again.

## Refused calls

Check price alignment, channel and recipient terms, the budget, and the per-call ceiling. If human approval is enabled, make sure your MCP host actually shows elicitation to you. Read the private [policy audit log](policy.md#read-the-audit-log) for the reason the signer recorded.

## Spent output or expiry

`output_spent` means the referenced output is down. Check the close transaction and wallet payout or refund before taking another action. An off-chain total can remain in a spent channel's ledger. At the expiry epoch a service settle is too late and the payer refund becomes eligible. If the indexer is unavailable, wait for a reliable epoch and output reading before retrying. See [Settling and payouts](settle.md).
