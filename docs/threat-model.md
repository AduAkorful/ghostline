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

**Fee linkage.** `pay_fee` always names an account component. Using the opener for settle fees publishes that the same account opened and settled. `wallet submit` requires `--fee-account-name` or `--other-signer` so that path is not implied. A distinct service-owned fee account is the intended design (the service key is already on the settle leaf). Funding that account from the opener, or creating it in a wallet whose other spends are already linked, still links them. Independent funding of a service wallet is not proven live. That is a privacy leak, not a theft path.

**Indexer-sealed envelopes.** Unsigned settles leave input `version` null and omit the fee account vault. Walletd `detect_inputs` fills both. The indexer REST submit does not. A sealed hex with null versions can return an id and stay Pending. Version-only resolve (UTXO + component, no vault) FinalizedAbort with `OneOrMoreInputsNotFound` / vault not found. `indexer resolve-inputs` pins versions, appends vault ids from account component state (CBOR tag 132), then `seal-unsigned`. A live unspent-channel settle on that path Finalized `Commit`. `GET /substates/{id}` 404s spent UTXOs as `(vN) is down`. Indexer `POST /transactions/dry-run` rejects envelopes with `dry_run: false` (use `/transactions` or build a dry-run transaction).

**List lag.** After an Accepted settle, `stealth_utxos.list` has shown the spent script UTXO as Unspent. Execution Accept is the source of truth, not the list. Channel deposit fields for unblind come from `transactions.get` of the open transaction, not from that list. Wallet-built opens store that Script output in `fee_instructions`.

## Residual risk

No independent review round has closed. Property tests cover accounting invariants in-process; they do not replace a review of the template against the current engine. Do not use with real funds.
