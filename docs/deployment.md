# Mainnet deployment

These are the Ghostline contracts on Tari Ootle mainnet, published on 13 Nov 2026. Check each one against the indexer yourself: `GET /substates/<address>` should return HTTP 200.

## Addresses

| | Value |
|---|---|
| Channel template | [`ae13cead3c34052116c49fc09e4645f3a62edf55857c0e2e45226dd7093cef9c`](https://indexer-a.ootle.tari.com/substates/template_ae13cead3c34052116c49fc09e4645f3a62edf55857c0e2e45226dd7093cef9c) |
| Fee-config template | [`b3628ed057f88c73b88f3c5653b494a10258067c43cf0c667d1dcf17e85e9f45`](https://indexer-a.ootle.tari.com/substates/template_b3628ed057f88c73b88f3c5653b494a10258067c43cf0c667d1dcf17e85e9f45) |
| Fee config | [`component_37df6af59007b9c02ead46df7e320af4255978cf8fefb164fb8bfdc4183d0bb5`](https://indexer-a.ootle.tari.com/substates/component_37df6af59007b9c02ead46df7e320af4255978cf8fefb164fb8bfdc4183d0bb5) |
| Network tag | `0` |
| Indexers | `https://indexer-a.ootle.tari.com`, `https://indexer-b.ootle.tari.com` |

The channel template pins the fee config. At every settlement Ghostline reads the treasury key from the fee config on chain, so you never configure it. The fee config's owner is the only key that can change the treasury.

## Publish transactions

| Transaction | Hash |
|---|---|
| Publish the fee-config template | [`651aff4202a1dd6675c6d82903ebb036e6673075f06428dea8844d4d31a13983`](https://indexer-a.ootle.tari.com/transactions/651aff4202a1dd6675c6d82903ebb036e6673075f06428dea8844d4d31a13983) |
| Create the fee config | [`1fdbb90905399f5427ad2dc8894d84c43c44af4d6da83ce97f5f0f5b93211fd9`](https://indexer-a.ootle.tari.com/transactions/1fdbb90905399f5427ad2dc8894d84c43c44af4d6da83ce97f5f0f5b93211fd9) |
| Publish the channel template | [`3ff0500cf6deef1c66dce31cce01e68105658b0ac9ae7043b7962649f17b2eec`](https://indexer-a.ootle.tari.com/transactions/3ff0500cf6deef1c66dce31cce01e68105658b0ac9ae7043b7962649f17b2eec) |

Compare against the template substate on the indexer, not against a local build. walletd optimises the WASM before publishing, so the on-chain binary differs from the local one.

## Protocol parameters

| Parameter | Value |
|---|---|
| Protocol fee | 1% of the service payout, paid once at settle |
| Price step | Prices and cumulative totals are multiples of 100 µT |
| Reserve | At least 1 µT of payer change on every settlement |
| Maximum settleable payout | `floor((D − 1) / 101) × 100` µT for a deposit `D` |
| Outputs per settlement | Three: service payout, payer change and the treasury fee |

Network fees are not parameters of the template; see [fees](fees.md) for what they cost.

## See also

The [mainnet proof page](https://ghostline-ui.vercel.app/proof/) lists every recorded run, and the [mainnet quickstart](mainnet-quickstart.md) shows how to use these addresses.
