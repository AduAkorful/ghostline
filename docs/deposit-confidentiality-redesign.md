# Deposit confidentiality implementation

**Status: claim-free settlement and refund pass local engine tests; Esmeralda validation pending.** A prior live close used the generic wallet transfer builder, which added an optional covenant-balance claim and disclosed the full deposit. Ghostline's actual channel rules do not use a balance-preservation covenant, so the claim is not needed to enforce them.

## Change

The Ghostline SDK now assembles its close transfer statement from Tari's output-statement and global balance-proof helpers, while leaving `covenant_claims` empty. The input still contains the proven script-path witness. Settlement still verifies the payer-signed output commitment, the service output authorization key, the payer remainder path, output count, output promises, and absence of revealed-value inputs/outputs. Refund still requires the payer's script path and native `AfterEpoch` condition, and pays the full channel value to the payer's key path. The global stealth balance proof still enforces input/output conservation.

This works because a covenant claim is optional in `StealthTransferStatement`. The engine rejects malformed or duplicate claims, but does not require a claim merely because an input uses a script path. `Covenant::BalancePreserved` explicitly looks up its claim and rejects when it is missing. Ghostline's settle and refund condition trees contain no `BalancePreserved` predicate, so omitting this otherwise unused disclosure does not remove any rule the channel relies on.

## Local evidence

`crates/contract-tests/tests/private_close.rs` passes three in-process engine tests:

- Claim-free settlement preserves the Ghostline template's voucher signature, channel/network binding, exact service commitment, output authorization, output-count and value-promise checks; rejects extra inputs/outputs, revealed outputs, imbalance, wrong signers, expiry settlement, and replay; and executes the valid spend with no claim.
- Claim-free refund fails before expiry and for a non-payer, then succeeds with the payer alone at epoch 10,001 for a channel expiring at epoch 10. Because the payer constructs the transaction after expiry, refund recovery is not bounded by a pre-signed transaction's validity window.
- A separate output with a real `Covenant::BalancePreserved(0)` rejects a missing claim and succeeds with its valid claim. This confirms the omission only applies to Ghostline's non-balance covenant rules.

`crates/contract-tests/tests/hidden_settlement.rs` also passes using the SDK's settle and refund builders and confirms both statements have empty claim lists.

The previous FROST/key-path prototype is not part of this design. It is unnecessary for hiding the deposit and adds a custom cryptographic adapter and different trust assumptions. The condition-root-on-key-plus-script workaround is also rejected by upstream engine tests because the key path bypasses covenant enforcement; see OBS-021 in `plans/03-bughunt-log.md`.

## Remaining verification

1. Run a fresh Esmeralda open → paid request(s) → settlement using the updated CLI/SDK, and inspect the finalized serialized transaction to confirm `covenant_claims` is empty, output amounts are still hidden, and the service received the voucher total.
2. Open a second fresh channel, wait until expiry, submit a payer-only refund with the updated SDK, and confirm finality plus an empty claim list.
3. Keep transaction fees, timing, script branch, authorization data, output commitments and count documented as observable. The service continues to see its full call/voucher history.
4. Update the sanitized evidence and public UI privacy copy only after both Esmeralda paths pass.

No contract redeployment is expected because the script predicates and template ABI are unchanged. Do not state that the deployed product hides the deposit until the live serialized-transaction checks pass.
