# Ghostline threat model

Ghostline has finalized opens, settlements, and an expiry refund on Tari Ootle mainnet. No independent review has closed. The model below describes the mainnet deployment and identifies historical Esmeralda evidence where it applies. See the [mainnet quickstart](mainnet-quickstart.md) and the [mainnet observer test](privacy.md#mainnet-observer-test-13-nov-2026).

## Assets

- Channel deposit (stealth UTXO)
- Payer signing key
- Service sealing key
- Ephemeral browser signing key in the frozen Esmeralda demo
- Wallet view key and account key (when the deposit is wallet-built)
- Off-chain vouchers and the HTTP path they travel on
- Policy limits (budget, per-call ceiling, vendor allowlist)

## Actors

| Actor | Trust |
|---|---|
| Payer / agent | Holds the voucher key. Assumed to want to spend, not to protect the operator from itself |
| Policy engine | Runs outside the agent. Enforces limits only if the agent actually signs through it; the human gate depends on a separate CLI operator or MCP host UI |
| Service | Knows the declared channel deposit limit, sees every request and voucher (including voucher amount/mask), and can settle any valid cumulative amount it holds, once. It does not receive the deposit UTXO mask in ordinary individual flows. |
| Batch coordinator | No production coordinator exists. The current SDK batch prototype receives every channel's deposit value and mask in one process, so that process can learn every opening. It is not suitable as a hosted multi-user coordinator. |
| Public observer | Sees the open and close, their UTXO link, public network fees, branch and authorization data, output commitments, and timing. Mainnet run 4 disclosed no plaintext deposit or settlement amounts. |
| Treasury key holder | Can recover each protocol fee amount and infer the corresponding service payout at the fixed 1% rate. The fee-config owner can update the treasury key. |
| Wallet daemon | If used for open/settle, it sees amounts, masks, and keys it holds |
| Ootle validators | See the same public fields any observer sees, plus whatever the protocol leaks in proofs and fees |
| Frozen Esmeralda demo relay | Temporarily controls the test deposit and the wallet profile that opens the channel. This custody and funding linkage are specific to the contest demo. |

## Goals we try to meet

1. A third party watching the chain cannot reconstruct how many calls were made or at what prices, if the service settles once.
2. The service cannot take more than the voucher authorises, or more than the deposit.
3. The payer cannot reclaim the deposit on the settle path before expiry without the service's key.
4. After expiry, settle and refund are mutually exclusive at the same epoch (tested in-process).
5. An agent that only signs through the policy engine cannot exceed the configured budget or per-call ceiling.
6. A future batch coordinator must not learn participants' deposit openings; this property is **not implemented** by the local batch prototype.

## What we do not try to meet

- Hiding the relationship from the service you pay
- Hiding the declared deposit limit or voucher details from the service; the service needs them to validate and serve payments
- Hiding deposit openings from a coordinator in the current batch prototype; production flows do not use that prototype
- Hiding transaction fees or the account that paid them
- Hiding fees, timing, the three-output settlement shape, opaque channel id, and authorization data revealed by the selected spend leaf
- Hiding the payer/service association within a settlement, even when the service uses separate channel-authorization and payout keys
- Stopping an agent that signs with `--unsafe-no-policy` or a raw secret
- Hiding the faucet (or other public funding) of the fee-paying account
- Anonymity on the HTTP path (IP, TLS, logs)

## Main attacks

**Replay / stale voucher.** The service accepts only increasing cumulative amounts. The on-chain template settles once and spends the channel output, so a second redemption cannot spend the same output.

**Settle after expiry / refund before.** The condition tree is two leaves: settle (`BeforeEpoch` + service key + template `settle`) and refund (`AfterEpoch` + payer key). In-process tests cover the epoch cut. Live Esmeralda: a refund before `AfterEpoch` does not spend the channel UTXO (`OnlyFeeAccepted` / `AcceptFeeRejectRest`). A refund at/after expiry Finalized Commit (stealth transfer of the deposit; opener paid the public fee). A live settle at the expiry epoch is `OnlyFeeAccepted`: `BeforeEpoch` fails, the channel UTXO stays unspent, public fees are still taken. A wallet dry-run settle at that epoch can still print Accept; that is not a live spend.

**Separate relay refund key (new SDK path, not yet used by the hosted demo).** `Channel::open_with_refund_key` leaves the voucher payer and settlement authorization unchanged, but puts the supplied key on the `AfterEpoch` refund branch. Contract-engine regression coverage confirms that the relay key cannot forge a voucher accepted by `settle`, the payer key cannot claim that refund leaf, and the refund key can claim after expiry. This limits the relay's ability to take the channel early; it does not remove custody, because the relay could withhold the remaining funds after expiry.

**Wrong-channel voucher.** Vouchers are bound to `channel_id` (network tag, payer, deposit, payer root, nonce). A voucher for another channel must fail verification.

**Settlement amount confidentiality.** The payer signs the service output's Pedersen commitment, and the template requires that exact commitment to be paid to the signed service key. The witness contains no amount or commitment opening, outputs use a one-unit minimum-value promise, and the template rejects revealed values or extra inputs. Fresh Esmeralda settlement and expiry-refund transactions finalized with empty covenant-claim lists and no plaintext amounts. Branch, opaque channel id, authorization data, timing, and fees remain visible; this does not hide activity from the service. Mainnet settles have three stealth outputs for service payout, payer change, and the 1% treasury fee. Cumulative amounts must be multiples of 100 µT and leave room for the protocol fee and at least 1 µT of change.

**Policy bypass.** If the agent can reach the payer secret, policy is advisory. The on-chain limit is the deposit. Human rules can gate every payment, calls at or above a configured µT threshold, or a vendor's first payment. The CLI asks for terminal confirmation and MCP requests client elicitation; the MCP host must actually present the prompt to a person. The low-level policy API trusts its caller's attestation boolean. Optional JSONL audit logging is append-only and fails closed on write errors, but it does not protect against an operator who can edit or replace the file.

**Browser-session key exposure (Esmeralda demo).** A compromised tab could authorize vouchers up to that test channel’s deposit limit. The mainnet site simulates channels in the browser. Mainnet signing stays in the operator’s local tools.

**Wallet-built deposit encrypted to the view key.** `accounts.stealth_transfer` with a wallet `destination_address` encrypts the output to the wallet view key, not the Ghostline payer key. Unblinding with the payer secret fails. Settlement needs the view-key mask (or a wallet that already holds it).

**Fee linkage.** `pay_fee` always names an account component. Using the opener for settle fees publishes that the same account opened and settled. `wallet submit` requires `--fee-account-name` or `--other-signer` so that path is not implied. A live settle paid fees from a derived account (index 1) that was funded by a community faucet, not by a transfer from the opener. The public fee component is therefore distinct. Those two accounts still share the wallet seed, so they are not unlinkable to someone who has that seed or who maps both addresses to one wallet.

A later live settle paid fees from a second wallet seed (separate walletd, separate faucet claim). Indexer Finalized Commit; the first seed's fee account balance did not move. An observer who only sees the open and settle components cannot derive one from the other. The faucet claim that funded the fee wallet is still public, and timing correlation remains possible. That is a privacy leak, not a theft path.

New derived accounts are not scanned until `keys.create` has produced that index on the `view_only_key` and `account` branches; `accounts.create` alone left the faucet stealth UTXO invisible. `wallet account-create` now derives those keys and associates stealth tTARI. `accounts.create` and `accounts.create_or_get` do not publish the component. The hosted relay publishes its fee account with walletd `accounts.create_free_test_coins` when the indexer says that component was never created. If the Esmeralda faucet is empty, startup publishes it with a revealed `accounts.transfer` from the opener instead. The faucet claim, that transfer, and their fees are public. A stealth transfer to the account address does not create the component.

**Indexer-sealed envelopes.** Unsigned settles leave input `version` null and omit the fee account vault. Walletd `detect_inputs` fills both. The indexer REST submit does not. A sealed hex with null versions can return an id and stay Pending. Version-only resolve (UTXO + component, no vault) FinalizedAbort with `OneOrMoreInputsNotFound` / vault not found. `indexer resolve-inputs` pins versions, appends vault ids from account component state (CBOR tag 132), then `seal-unsigned`. A live unspent-channel settle on that path Finalized `Commit`. `GET /substates/{id}` 404s spent UTXOs as `(vN) is down`. Indexer `POST /transactions/dry-run` rejects envelopes with `dry_run: false` (`Non-dry-run transactions must be submitted to the /transactions endpoint`). `seal-unsigned --dry-run` sets the flag; a replay of a spent settle then 400s because inputs are down, not because of that check.

**List lag.** After an Accepted settle, `stealth_utxos.list` has shown the spent script UTXO as Unspent. Execution Accept is the source of truth, not the list. Channel deposit fields for unblind come from `transactions.get` of the open transaction, not from that list. Wallet-built opens store that Script output in `fee_instructions`.

## Residual risk

No independent review round has closed. Property tests cover accounting invariants in-process; they do not replace a review of the template against the current engine. Keep mainnet exposure small and account for the public fee and funding links. See [Privacy](privacy.md) for the measured disclosures.
