# Deposit confidentiality redesign

**Status: local feasibility proof; not implemented in Ghostline.** The current stealth channel's script-path close publishes the full channel deposit in Ootle's covenant balance claim. Changing output minimum-value promises or hiding fields in Ghostline's JSON cannot remove that protocol-generated claim.

## Goal

Keep the channel deposit, service payout, and payer remainder unreadable to public observers at both settlement and refund. Public transaction fees remain visible. The service still sees every call and amount in its own off-chain ledger.

## Why a key path is the likely direction

Ootle's ordinary key-path stealth transfer uses commitments and a balance proof without publishing a script-path covenant claim. The current claim is generated because the channel uses a script path to enforce its settlement and refund branches.

Replacing that script path with a plain key authorization is unsafe: whoever controls that one signing key can move the whole deposit without the voucher rules or expiry. A `KeyAndScript` output does not fix this because its key path can bypass the script.

I also checked whether settlement outputs could carry the original channel condition root alongside their recipient key, so the covenant claim would count them while the recipient could later spend by key. This does not work: the engine counts only exact `SpendAuthorization::Script(root)` outputs as covenant-preserving. It rejects key-plus-script outputs from `output_preserves_condition` and `covenant_balanced` because the key path is an escape; wallet claim generation uses the same exact-auth rule. The upstream engine regression tests `covenant_rejects_output_with_added_key_path` and `builtin_balance_preserved_rejects_added_key_path` cover this behavior. This is intended enforcement, not an engine bug. See observation OBS-021 in `plans/03-bughunt-log.md`.

The candidate design therefore needs a joint signing key controlled by the payer and service. A single key cannot be held by either party: either side could bypass all channel rules and take the full deposit.

The service must hold a fully signed settlement candidate for each accepted voucher. The payer-side signer checks the exact transaction (service amount and key, payer remainder key, channel input, network, expiry, and fee fields) and contributes its threshold-signature share. The service validates the voucher and transaction, completes the signature, durably stores that candidate, and only then returns the paid response. It must not return the completed settlement signature to the payer, or the payer could race the service with an older, lower settlement. All settlement candidates spend the same channel input; the service submits the latest candidate.

At channel opening, both sides must also create a fully signed refund transaction whose `min_epoch` is after expiry, and the payer must receive it before the open is broadcast. The payer can submit that transaction without the service after expiry; any earlier use is rejected by transaction validity. The signing key must be generated and used with a reviewed threshold-signature protocol compatible with Tari's Ristretto Schnorr verification. Ghostline must not implement its own ad hoc signing protocol.

There is a compatibility hurdle: Tari's `RistrettoSchnorr` challenge uses domain-separated Blake2b-512 over `(R, public key, message)`. The standard FROST Ristretto255 ciphersuite uses SHA-512 and its own transcript rules. A stock FROST signature therefore cannot be assumed to satisfy Ootle's transaction-signature verifier. The FROST core is generic, but a Tari-compatible ciphersuite and its security review would be new cryptographic work, not a routine SDK integration.

An isolated spike using `frost-core` / `frost-ristretto255` 0.6.0 produced a 2-of-2 aggregate signature accepted by Tari's `RistrettoSchnorr` verifier. Two simulated participants completed the library's DKG, agreed on the group key, and signed the test message. The stock `Ristretto255Sha512` challenge was replaced with Tari's domain-separated Blake2b challenge; the DKG challenge and other FROST hash functions remained the audited suite's implementations. This proves signature-byte interoperability and happy-path library mechanics in one process. It does **not** validate a real authenticated transport, malicious-party handling, transaction signing, crash/timeout/nonce recovery, or the channel protocol. The FROST project's NCC audit covered v0.6.0 and the stock listed suites, but not this Tari challenge adapter; that adapter and its compatibility boundary still need independent cryptographic review ([FROST audit scope](https://github.com/ZcashFoundation/frost)). Treat the spike as feasibility evidence, not production crypto approval.

The test-only `keypath_threshold` integration proof opens a candidate escrow by moving a payer-owned input into a two-party key-path output, then spends it into two encrypted outputs with one-unit minimum-value promises. Both opening and settlement execute in the Ootle engine harness, and the settlement has no covenant balance claims. It creates and verifies a real Ghostline voucher and checks that the service output commitment matches the voucher. Engine execution rejects attempts by either party's ordinary key to spend the joint-key UTXO; FROST aggregation fails with only one participant share; and replaying the finalized settlement fails after its input is spent. This confirms the key-path mechanism can avoid the current deposit-revealing claim at the engine layer; it does **not** yet run the Ghostline paywall/session flow, use a distributed signing transport, authorize a fee account, or execute an on-network refund. `TemplateTest` executes instructions without applying the transaction-validation stage: it accepted the pre-signed future-`min_epoch` refund candidate when we initially tried to exercise it. The upstream `transaction_validation::EpochRangeValidator` unit tests passed for rejecting a transaction before `min_epoch`, after `max_epoch`, and when `max_epoch` exceeds the configured 2,160-epoch validity ceiling. This confirms the validator rules in isolation, but an integrated consensus/local-network refund is still required before relying on it for recovery.

## Refund recovery is a release blocker

The payer must be able to reclaim funds after expiry even if the service is offline or refuses to cooperate. A payer-only key path would allow an early theft; a service co-signature required at refund would make the funds depend on the service staying available.

Ootle transactions have signed `min_epoch` and `max_epoch` validity bounds. The current upstream development checkout sets `max_transaction_validity_epochs` to 2,160; the network rejects a transaction whose `max_epoch` is too far ahead of the current epoch. A pre-signed payer refund can therefore have a future `min_epoch`, but its final usable epoch is still bounded from when it was signed. A refund that expires before the payer can recover is not acceptable. This makes channel lifetime and recovery window explicit protocol limits; the supported Esmeralda parameters and worst-case wallet outage window still need a local-network proof before implementation. The rule was confirmed by upstream unit tests only; no new Esmeralda transaction was submitted.

## Required proofs before implementation

1. Independently review the Tari challenge adapter and its use with the audited FROST v0.6.0 implementation. The local spike proves compatibility only; it does not satisfy this review requirement.
2. Demonstrate that no single party can produce a valid key-path spend, including during key setup, retries, participant aborts, and malformed messages.
3. Demonstrate that a payer cannot complete a spend or race the service with an older settlement candidate; the service must hold the final candidate before it serves each paid response.
4. Demonstrate a pre-authorized refund that cannot execute before expiry and remains usable after expiry without service cooperation, within the network's transaction-validity limits. The current development source sets that maximum transaction horizon to 2,160 epochs; a channel expiry near that horizon leaves a correspondingly short post-expiry refund window if the transaction was signed at open.
5. Extend the test-only engine proof into Ghostline settlement and refund flows with no disclosed deposit, payout, or remainder, preserving voucher authorization, value conservation, expiry behavior, and replay safety. The current proof covers only the UTXO spend authorization and no-claim property.
6. Run adversarial tests for early refund, payer theft, service theft, stale settlement races, replay, changed payout key, altered transaction, participant abort, nonce reuse, and validity-window boundaries.
7. Repeat the full open → paid calls → settle and open → expiry → refund flows on Esmeralda with a fresh channel and verify finality and observer-visible fields.

## Rejected shortcuts

- Adding a key path controlled by either party alone: that party can bypass the channel rules and take all funds.
- Keeping the remainder under the same covenant root: the covenant claim would reveal the amount leaving the channel, exposing the settlement amount and defeating the existing amount privacy.
- Requiring the service to co-sign a refund after expiry: service failure could lock the payer's deposit.
- Claiming confidentiality from output commitments alone: the covenant balance claim is a separate public field and must be included in observer analysis.

Until every release blocker above passes, the script-path channel remains the safe implementation and its deposit disclosure must stay explicit in the privacy docs and UI.
