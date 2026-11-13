---
name: ghostline
description: Use when an agent needs to pay for an HTTP API per call on Tari Ootle mainnet with Ghostline, open or close a private payment channel, set or explain spending limits, or help a user set up the Ghostline MCP server, CLI or HTTP 402 paywall.
---

# Ghostline

Ghostline is a private pay-per-call rail on Tari Ootle mainnet. The payer locks funds once in a channel, signs a cumulative voucher for each paid HTTP call off-chain, and the service settles once on chain. A spending policy that runs outside the agent decides what may be signed.

Docs index: https://ghostline-ui.vercel.app/llms.txt. Every docs page is also plain Markdown at `https://ghostline-ui.vercel.app/docs/<page>.md`, and the whole set is at https://ghostline-ui.vercel.app/llms-full.txt. Read the page for a task before acting on it; this file is a summary.

## Rules

1. **Real TARI moves.** Ghostline is unaudited. Never open, settle or refund a channel unless the user asked for that exact action. Keep deposits small.
2. **Quote first.** `ghostline_open_channel` and `ghostline_close_channel` only quote or dry-run until the user sets `GHOSTLINE_OPEN_SUBMIT=1` or `GHOSTLINE_CLOSE_SUBMIT=1` in the server environment. Show the user the quoted fee and let them decide. Never set those flags or the fee caps yourself.
3. **Secrets stay out of the conversation.** Tools take no secrets as arguments. Do not ask the user to paste a seed, a payer secret, a claim secret or a wallet cookie into chat, and do not print them.
4. **Policy is not yours to change.** If a call is refused (for example `cumulative 6000 exceeds policy budget 5000`), report the reason to the user. Do not retry with a different price, edit the policy variables, or set `GHOSTLINE_POLICY_UNSAFE`.
5. **Wallets use cookie login.** Run `tari_ootle_walletd` with `--cookie-auth true`, never `--authentication none`, and keep it on `127.0.0.1`.
6. **No false privacy claims.** Amounts and call patterns are hidden. Network fees, the open and close records, and any funding or fee-account link you create are public. Do not call Ghostline anonymous or untraceable.

## Money rules

- Integer micro-TARI (µT); 1 TARI = 1,000,000 µT. Never use floating point.
- Prices and cumulative totals are multiples of 100 µT.
- The protocol fee is 1% of the service payout, taken once at settle.
- A deposit `D` can settle at most `floor((D − 1) / 101) × 100` µT. A 20,201 µT deposit pays at most 20,000 µT.
- Network fees measured on mainnet on 13 Nov 2026: open about 15,900–16,100 µT, settle about 28,800 µT, expiry refund about 10,000 µT. Dry-run before relying on them, because they change.
- At 100 µT per call the open and settle fees equal about 448 calls, so channels suit agents that call one service often.

## MCP tools

Run `ghostline-mcp` over stdio. The tools below take `{}` unless noted.

| Tool | Use it to |
|---|---|
| `ghostline_call_paid` `{"url": "..."}` | Fetch a paid resource. On a 402 it signs a voucher if the policy allows and retries once. |
| `ghostline_channel_status` | Read off-chain totals, the expiry window and whether the channel output is still unspent. |
| `ghostline_open_channel` | Quote the open; broadcast only when the user has enabled it. |
| `ghostline_close_channel` | Service: settle before expiry. Payer: refund at or after expiry. |
| `ghostline_channel_session` | Create or reuse the payer session for the channel file. |
| `ghostline_channel_list`, `ghostline_channel_calls`, `ghostline_channel_file` | Inspect channels and accepted calls without exposing secrets. |
| `ghostline_verify_voucher`, `ghostline_channel_id`, `ghostline_inspect_tx` | Verify and inspect public data. |

Results to look for in `ghostline_call_paid`: `payment_required` (the server wants a voucher) and `needs_channel` (no open payer session; ask the user to open or bind a channel, do not try to open one yourself).

## Tasks

**Pay for an API.** Call `ghostline_call_paid` with the URL. If it succeeds, use the response. If the policy refuses, stop and report why. If it returns `needs_channel`, tell the user a channel is needed.

**Check a channel.** Call `ghostline_channel_status` before and after a batch of calls so the user can see the accepted total against the budget.

**Help set up an agent.** Mainnet needs Ghostline 0.5.0 or later; the v0.4.0 release predates the protocol fee and cannot open a mainnet channel. Point the user at https://ghostline-ui.vercel.app/docs/mainnet-quickstart.md and https://ghostline-ui.vercel.app/docs/mcp.md. The policy settings are in https://ghostline-ui.vercel.app/docs/policy.md: `GHOSTLINE_POLICY_BUDGET`, `GHOSTLINE_POLICY_MAX_PER_CALL`, `GHOSTLINE_POLICY_VENDOR_HEX`, `GHOSTLINE_POLICY_HUMAN_THRESHOLD`, `GHOSTLINE_POLICY_APPROVE_NEW_VENDOR`, `GHOSTLINE_POLICY_REQUIRE_HUMAN` and `GHOSTLINE_POLICY_AUDIT_LOG`.

**Help sell an API.** The HTTP 402 paywall quotes a price, verifies each voucher against a ledger and keeps the latest one for settlement: https://ghostline-ui.vercel.app/docs/paywall.md. The service must settle before the expiry epoch; at the expiry epoch the payer can refund the deposit and settlement is refused: https://ghostline-ui.vercel.app/docs/settle.md.

**Explain what is public.** https://ghostline-ui.vercel.app/docs/privacy.md lists what an observer can and cannot see, from the mainnet observer test.

## Troubleshooting

https://ghostline-ui.vercel.app/docs/troubleshooting.md covers wallet authentication, fee caps and balances, `needs_channel`, refused calls and `output_spent`.

## Proof

Every recorded mainnet transaction, with indexer links, is at https://ghostline-ui.vercel.app/proof/.
