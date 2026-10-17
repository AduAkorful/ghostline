# Ghostline architecture

Ghostline separates per-call accounting from settlement. The payer signs a cumulative voucher for each accepted request; the service keeps the latest voucher in its local ledger and later submits one settlement transaction to Tari Ootle.

## Components

| Component | Responsibility | Main location |
|---|---|---|
| Channel template | Verify the payer signature and channel identity, enforce the deposit ceiling, and direct settlement or expiry refund outputs | `contracts/channel` |
| Core | Derive channel ids, encode/sign/verify vouchers, and track payer and service cumulative totals | `crates/ghostline-core` |
| Transaction SDK | Build stealth condition trees, settle/refund statements, and unblind stealth outputs | `crates/ghostline-sdk` |
| Network adapter | Read epoch and substate status from an indexer; call a local wallet daemon for fee quotes and submission | `crates/ghostline-net` |
| Policy engine | Enforce operator budgets, per-call caps, vendor allowlists, rate limits, and configured human approval; optionally persist decision records | `crates/ghostline-policy` |
| HTTP paywall | Quote a request, verify a voucher, and persist accepted service-side state | `middleware/ghostline-paywall` |
| Reference payer/API | Demonstrate HTTP 402 negotiation and paid `/fortune` and `/meter` routes | `demo/paid-api` |
| MCP server | Expose payment, channel-file, inspection, and close operations over MCP stdio | `crates/ghostline-mcp` |
| TypeScript package | Match Rust channel-id and voucher encodings, signing, and verification; it does not open or submit Ootle transactions | `packages/ghostline` |

## Payment sequence

```mermaid
sequenceDiagram
    participant P as Payer / agent
    participant W as Wallet daemon
    participant A as Paid API
    participant L as Service ledger
    participant I as Ootle indexer
    P->>W: Open stealth channel (operator-reviewed fee cap)
    W-->>P: Accepted open and unblinded channel output
    P->>A: Request without voucher
    A-->>P: HTTP 402 quote (scheme, price, channel id, opening terms)
    P->>P: Validate quote, policy, and human approval; sign cumulative voucher
    P->>A: Retry with ghostline-voucher header
    A->>L: Verify and atomically persist accepted voucher/charge
    A-->>P: Paid response
    Note over P,L: Repeats off-chain; ledger stores only service-side activity
    L->>I: Submit latest voucher once before expiry
    I-->>L: Settlement result / output presence
```

The service ledger and payer session are separate files/process state. The service must retain its latest accepted voucher for settlement. The payer session retains the cumulative signing state and must not be shared by unrelated channels. The service sees requests and call-level prices; the privacy property concerns what third parties can infer from the chain, not what the service knows.

## Channel identity and voucher

`ChannelParams` binds the network tag, payer public key, deposit in integer micro-units, payer spend-condition root, and a 32-byte nonce into a 32-byte channel id using the versioned channel-id domain and Blake2b-256. The recipient key and expiry are enforced by the spend-condition tree. The channel id is a hash commitment to those parameters, rather than a serialization of them.

The payer creates a fresh-mask Pedersen commitment to the cumulative amount and signs the 32-byte channel id, service output authorization key, and 32-byte commitment (96 bytes total) under the versioned voucher domain. The off-chain voucher also carries the amount and mask so the service can verify the opening and later scan its output. The on-chain witness carries only the commitment, service key, and signature. The template requires the matching service output and an optional payer-condition change output, with no revealed values or additional inputs. Ootle's transfer proofs enforce range and conservation. The Ghostline channel variant settles and closes in one spend; a full-deposit voucher omits a zero-valued payer output.

Amounts are integer micro-units throughout. Metered pricing uses checked multiplication. Rust and TypeScript maintain shared vectors for the supported channel-id and voucher format.

## Policy and persistence boundaries

- The policy process must be outside the agent's authority to modify. MCP and the CLI read policy settings from their process environment; the Rust policy library itself does not authenticate its caller.
- Optional JSONL policy audit records include vendor and private spending details. When configured, an audit write failure prevents the payment decision from succeeding.
- Channel/session files can contain deposit masks or signing state. Treat them as secrets. CLI `--out` files use owner-only permissions on Unix; shell redirection requires a restrictive `umask`.
- The paywall persists its latest voucher and accepted totals before returning success. A failed persistence operation must not advance in-memory accounting.
- The wallet daemon signs/submits transactions and pays public fees. Indexer responses inform status but are not a substitute for checking the final transaction result.

## What the chain reveals

An unopened stealth output hides its amount and owner. Live Esmeralda validation shows the settle witness and output minimum-value promises do not reveal the exact service amount or payer remainder. An earlier transaction's generic covenant claim did reveal the channel deposit. The SDK now omits this claim while retaining the global balance proof and script witness; local engine tests pass, but fresh Esmeralda settlement and refund checks are pending. Until those pass, treat deployed behavior as still revealing the deposit. The branch, opaque channel id, payer and service authorization data, output commitments/count, fees, and timing are also visible. Funding history, fee payer, and later wallet behavior may correlate activity. See [Privacy and observer view](privacy.md) and [Threat model](threat-model.md) for the limits.

## Network coverage

The open, paid-call, settle, and refund paths have live Esmeralda evidence recorded in the product spec and spike notes. A continuous `channel watch` retries failed epoch/output reads at its configured interval; `--once` returns the read error immediately. A failed settlement submission stops the command so the operator can check output presence and transaction status before restarting the watcher. A fresh local swarm run is still pending: the host must meet `scripts/localnet.sh`'s 40 GiB free-space guard before compiling the upstream network. Do not lower the guard to force a build.

The reference API can expose aggregate Prometheus metrics with `serve --metrics`. It reports HTTP outcomes and counts of configured ledgers and ledgers with accepted vouchers; the latter is off-chain state, not chain settlement confirmation. Metrics contain no per-channel labels or amounts. The endpoint is opt-in because a deployment should choose its own access boundary. The CLI emits structured operation/outcome events for watch, settle, and refund; logs omit channel identifiers, payment amounts, secrets, and paths.

The product's side-by-side visual is intentionally excluded from this non-UI work; no claim is made that the UI requirement is complete.
