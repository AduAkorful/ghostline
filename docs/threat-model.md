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
| Policy engine | Runs outside the agent. Enforces limits only if the agent actually signs through it; the human gate depends on a separate CLI operator or MCP host UI |
| Service | Sees every voucher. Can settle any valid cumulative amount it holds, once |
| Public observer | Sees fees, condition branches, and the deposit amount revealed by Esmeralda's covenant claim; the exact service settlement amount and remainder were hidden in the live test |
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
- Hiding fees, timing, output count, opaque channel id, and authorization data revealed by the selected spend leaf
- Stopping an agent that signs with `--unsafe-no-policy` or a raw secret
- Hiding the faucet (or other public funding) of the fee-paying account
- Anonymity on the HTTP path (IP, TLS, logs)

## Main attacks

**Replay / stale voucher.** Cumulative amounts only increase. Redeeming pays the delta over what was already redeemed. A lower voucher after a higher one must fail.

**Settle after expiry / refund before.** The condition tree is two leaves: settle (`BeforeEpoch` + service key + template `settle`) and refund (`AfterEpoch` + payer key). In-process tests cover the epoch cut. Live Esmeralda: a refund before `AfterEpoch` does not spend the channel UTXO (`OnlyFeeAccepted` / `AcceptFeeRejectRest`). A refund at/after expiry Finalized Commit (stealth transfer of the deposit; opener paid the public fee). A live settle at the expiry epoch is `OnlyFeeAccepted`: `BeforeEpoch` fails, the channel UTXO stays unspent, public fees are still taken. A wallet dry-run settle at that epoch can still print Accept; that is not a live spend.

**Wrong-channel voucher.** Vouchers are bound to `channel_id` (network tag, payer, deposit, payer root, nonce). A voucher for another channel must fail verification.

**Settlement amount confidentiality.** The payer signs the service output's Pedersen commitment, and the template requires that exact commitment to be paid to the signed service key. The witness contains no amount or commitment opening, outputs use a one-unit minimum-value promise, and the template rejects revealed values or extra inputs. A 10 µT live Esmeralda settlement finalized and its serialized transaction showed no plaintext service amount or remainder. The same transaction revealed the 1,000 µT channel deposit in its covenant claim. Output count, branch, opaque channel id, authorization data, timing, and fees remain visible; this does not hide activity from the service.

**Policy bypass.** If the agent can reach the payer secret, policy is advisory. The on-chain limit is the deposit. Human rules can gate every payment, calls at or above a configured µT threshold, or a vendor's first payment. The CLI asks for terminal confirmation and MCP requests client elicitation; the MCP host must actually present the prompt to a person. The low-level policy API trusts its caller's attestation boolean. Optional JSONL audit logging is append-only and fails closed on write errors, but it does not protect against an operator who can edit or replace the file.

**Wallet-built deposit encrypted to the view key.** `accounts.stealth_transfer` with a wallet `destination_address` encrypts the output to the wallet view key, not the Ghostline payer key. Unblinding with the payer secret fails. Settlement needs the view-key mask (or a wallet that already holds it).

**Fee linkage.** `pay_fee` always names an account component. Using the opener for settle fees publishes that the same account opened and settled. `wallet submit` requires `--fee-account-name` or `--other-signer` so that path is not implied. A live settle paid fees from a derived account (index 1) that was funded by a community faucet, not by a transfer from the opener. The public fee component is therefore distinct. Those two accounts still share the wallet seed, so they are not unlinkable to someone who has that seed or who maps both addresses to one wallet.

A later live settle paid fees from a second wallet seed (separate walletd, separate faucet claim). Indexer Finalized Commit; the first seed's fee account balance did not move. An observer who only sees the open and settle components cannot derive one from the other. The faucet claim that funded the fee wallet is still public, and timing correlation remains possible. That is a privacy leak, not a theft path.

New derived accounts are not scanned until `keys.create` has produced that index on the `view_only_key` and `account` branches; `accounts.create` alone left the faucet stealth UTXO invisible. `wallet account-create` now derives those keys and associates stealth tTARI. `accounts.create` also does not publish the component; the first spend from that key does.

**Indexer-sealed envelopes.** Unsigned settles leave input `version` null and omit the fee account vault. Walletd `detect_inputs` fills both. The indexer REST submit does not. A sealed hex with null versions can return an id and stay Pending. Version-only resolve (UTXO + component, no vault) FinalizedAbort with `OneOrMoreInputsNotFound` / vault not found. `indexer resolve-inputs` pins versions, appends vault ids from account component state (CBOR tag 132), then `seal-unsigned`. A live unspent-channel settle on that path Finalized `Commit`. `GET /substates/{id}` 404s spent UTXOs as `(vN) is down`. Indexer `POST /transactions/dry-run` rejects envelopes with `dry_run: false` (`Non-dry-run transactions must be submitted to the /transactions endpoint`). `seal-unsigned --dry-run` sets the flag; a replay of a spent settle then 400s because inputs are down, not because of that check.

**List lag.** After an Accepted settle, `stealth_utxos.list` has shown the spent script UTXO as Unspent. Execution Accept is the source of truth, not the list. Channel deposit fields for unblind come from `transactions.get` of the open transaction, not from that list. Wallet-built opens store that Script output in `fee_instructions`.

## Residual risk

No independent review round has closed. Property tests cover accounting invariants in-process; they do not replace a review of the template against the current engine. Do not use with real funds.
