# HTTP 402 paywall

The reference server gates `/fortune` and `/meter`. Prepare a ledger and the channel terms with the [mainnet quickstart](mainnet-quickstart.md#6-serve-and-pay).

## Serve the reference API

```bash
ghostline-paid-api serve --bind 127.0.0.1:8787 --ledger ledger.json --price 1000 \
  --service-key-hex SERVICE_PUBLIC_KEY --service-output-key-hex ONE_TIME_OWNER_KEY \
  --min-expiry-epoch MIN_EPOCH --max-expiry-epoch MAX_EPOCH
```

Replace the uppercase placeholders with your service's public keys and inclusive expiry bounds. Use the authorization key for `--service-key-hex` and the wallet payout destination's `owner_key_hex` for `--service-output-key-hex`. The latter is optional, but use it if walletd should discover the payout. `--ledger` accepts one ledger or a directory. When several channels are configured, identify the channel with the `ghostline-channel` header. `--metrics` enables aggregate metrics at `GET /metrics`.

## Price a request

| Pricing | Meaning |
|---|---|
| Fixed | A single quoted resource price. |
| Per request | One price for the request, as on `/fortune`. |
| Metered | Unit price times positive units, as on `/meter?units=3`. |

Use positive prices in 100 µT steps and checked multiplication. The 402 quote includes the scheme, price, and channel id. The reference server also includes pricing, the service key, optional payout key, and inclusive expiry bounds. The payer retries with `ghostline-voucher`; see the [wire format](api-reference.md#http-payment-protocol).

## Embed the Rust middleware

Use the `ghostline-paywall` crate. Build quotes with `quote(PriceKind)` and serialize them as `PaymentRequired`. Verify a voucher against the selected service ledger with `charge_to_output_key` when quoting a wallet destination. Persist the resulting ledger and voucher with `ChannelStore::commit_charge` before returning success. Serialize concurrent writes for each channel so two requests cannot advance the same accepted total independently.

The reference server (`ghostline-paid-api`) shows this order and returns errors if quote validation or persistence fails. Retain the latest accepted voucher for [settlement](settle.md).
