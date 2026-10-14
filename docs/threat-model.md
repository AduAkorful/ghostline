# Ghostline threat model

Unaudited. Testnet only. This is a working model of who can do what, not a claim that the implementation is safe.

## Assets

- Channel deposit (stealth UTXO)
- Payer signing key
- Service sealing key
- Wallet view key and account key (when the deposit is wallet-built)
- Off-chain vouchers and the HTTP path they travel on
- Policy limits (budget, per-call ceiling, vendor allowlist)

## Actors

| Actor | Trust |
|---|---|
| Payer / agent | Holds the voucher key. Assumed to want to spend, not to protect the operator from itself |
| Policy engine | Runs outside the agent. Enforces limits only if the agent actually signs through it |
| Service | Sees every voucher. Can settle any valid cumulative amount it holds, once |
| Public observer | Sees fees always; sees settle/refund amounts and condition branches when those transactions land |
| Wallet daemon | If used for open/settle, it sees amounts, masks, and keys it holds |
| Ootle validators | See the same public fields any observer sees, plus whatever the protocol leaks in proofs and fees |

## Goals we try to meet

1. A third party watching the chain cannot reconstruct how many calls were made or at what prices, if the service settles once.
2. The service cannot take more than the voucher authorises, or more than the deposit.
3. The payer cannot reclaim the deposit on the settle path before expiry without the service's key.
4. After expiry, settle and refund are mutually exclusive at the same epoch (tested in-process).
5. An agent that only signs through the policy engine cannot exceed the configured budget or per-call ceiling.

## What we do not try to meet

- Hiding the relationship from the service you pay
- Hiding transaction fees or the account that paid them
- Hiding the settled amount, remainder, channel id, or keys revealed in the settle leaf
- Stopping an agent that signs with `--unsafe-no-policy` or a raw secret
- Unlinkable fees from a named wallet account (not proven)
- Anonymity on the HTTP path (IP, TLS, logs)

## Main attacks

**Replay / stale voucher.** Cumulative amounts only increase. Redeeming pays the delta over what was already redeemed. A lower voucher after a higher one must fail.

**Settle after expiry / refund before.** The condition tree is two leaves: settle (`BeforeEpoch` + service key + template `settle`) and refund (`AfterEpoch` + payer key). In-process tests cover the epoch cut. Live races on Esmeralda are not separately proven.

**Wrong-channel voucher.** Vouchers are bound to `channel_id` (network tag, payer, deposit, payer root, nonce). A voucher for another channel must fail verification.

**Policy bypass.** If the agent can reach the payer secret, policy is advisory. The on-chain limit is the deposit.

**Wallet-built deposit encrypted to the view key.** `accounts.stealth_transfer` with a wallet `destination_address` encrypts the output to the wallet view key, not the Ghostline payer key. Unblinding with the payer secret fails. Settlement needs the view-key mask (or a wallet that already holds it).

**Fee linkage.** Paying `pay_fee` from a reused account component publishes that the same account funded the settle. That is a privacy leak, not a theft path.

**Indexer-sealed envelopes.** Ghostline-sealed 0.45 envelopes submitted to the public indexer REST API have returned an id without finalizing. Live settles that worked went through the wallet daemon (`transactions.submit` of unsigned JSON, service sealer, account co-signer, `detect_inputs`). Until that indexer path is reproduced, do not treat sealed-hex submit as a working money path.

**List lag.** After an Accepted settle, `stealth_utxos.list` has shown the spent script UTXO as Unspent. Execution Accept is the source of truth, not the list.

## Residual risk

No independent review round has closed. Property tests cover accounting invariants in-process; they do not replace a review of the template against the current engine. Do not use with real funds.
