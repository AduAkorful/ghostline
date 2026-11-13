# What is Ghostline

Ghostline lets an AI agent pay for an API call by call without putting each call on chain. The agent locks funds once in a channel on Tari Ootle, signs a cumulative voucher for every paid call, and the service settles the latest voucher in one transaction. A spending policy that runs outside the agent limits what it can sign.

It runs on Tari Ootle mainnet. It is unaudited, so keep deposits small.

## Where to start

- **You run an agent that pays for APIs.** Follow the [mainnet quickstart](mainnet-quickstart.md), then set up the [MCP server](mcp.md) and the [spending policy](policy.md).
- **You sell an API.** Read the [HTTP 402 paywall](paywall.md) and [settling and payouts](settle.md).
- **You use a coding agent to help you.** Read [use with AI agents](ai-agents.md) and install the skill.
- **You want to know what it reveals.** Read [privacy](privacy.md) and the [threat model](threat-model.md).
- **You want the numbers.** Read [fees](fees.md) and [mainnet deployment](deployment.md).
- **A word is unfamiliar.** See the [glossary](glossary.md).

## One channel in four moves

1. **Open.** The payer's wallet locks a deposit in a channel with an expiry. The chain records one transaction and the amount stays hidden.
2. **Pay.** Each paid call carries a signed voucher for the new running total. Vouchers travel between the agent and the API and never touch the chain.
3. **Settle.** Before expiry the service spends the channel with the latest voucher. The payout, the 1% protocol fee and the payer's change are all hidden amounts.
4. **Refund.** If the service never settles, the payer takes the deposit back at or after the expiry epoch.

## The pieces

| Piece | What it is |
|---|---|
| Channel template | The on-chain contract that enforces the rules above. |
| Fee config | An on-chain component that holds the treasury key for the protocol fee. |
| `ghostline` | The command-line tool for keys, channel files, open, settle and refund. |
| `ghostline-mcp` | The local MCP server that gives an agent payment and channel tools. |
| `ghostline-paid-api` and `ghostline-paywall` | The reference HTTP 402 server and the Rust crate for your own routes. |
| `ghostline-policy` | The policy engine: budget, per-call cap, vendor allowlist and human approval. |
| `@ghostline/sdk` | The TypeScript package for voucher signing, verification and fee rules. |

## What is live

On 13 Nov 2026 the channel template and fee config were published on Tari Ootle mainnet. Five runs followed: opens, settles, an expiry refund, an observer test with separate wallets, and an agent run through MCP in which the policy refused the sixth call. Each transaction is listed with its indexer link on the [mainnet proof page](https://ghostline-ui.vercel.app/proof/).

## What it is not

- **Not audited.** No independent review has closed.
- **Not anonymous.** Network fees, the open and close records, and any funding or fee-account link you create are public. See [privacy](privacy.md).
- **Not a wallet.** Your keys stay in your wallet and your environment. There is no hosted relay for mainnet.
- **Not for a single call.** Opening and settling cost about as much as 448 calls at 100 µT. See [fees](fees.md).
- **Not a browser app.** The website explains and proves Ghostline; it does not open or settle channels.
