# Deposit confidentiality implementation

**Status:** Claim-free settlement and expiry refund are verified on Esmeralda.

## Change

The SDK assembles the close transfer statement from Tari's output-statement and global balance-proof helpers, leaving `covenant_claims` empty. It retains the script-path witness. Settlement still verifies the payer-signed output commitment, service authorization key, payer remainder path, output count, output promises, and absence of revealed-value inputs/outputs. Refund still requires the payer's script path and native `AfterEpoch` condition, and pays the full channel value to the payer's key path. The global stealth balance proof enforces input/output conservation.

The generic wallet builder adds a balance claim for script-path inputs. The engine requires that claim only when a spend condition uses `Covenant::BalancePreserved`; a separate adversarial test confirms such a covenant rejects a missing claim. Ghostline's settle and refund condition trees do not use that predicate, so omitting the unused claim preserves the channel's authorization and expiry rules.

## Esmeralda settlement proof

A fresh run opened a 10,000 µT channel, accepted one 10 µT HTTP `/fortune` payment, and finalized a claim-free settlement. The serialized transaction had no covenant claims, no plaintext input or output amounts, and two stealth outputs with one-unit minimum-value promises. The open and settlement are both `Finalized`; links and sanitized details are recorded in [`demo/ui/evidence.json`](../demo/ui/evidence.json). The service call and amount come from its local ledger, not a chain proof.

A separate fresh channel was refunded by the payer at expiry epoch 12403. Its finalized transaction had zero covenant claims, zero revealed input amount, one stealth output, and no revealed output. The transaction and its opening are linked in the same evidence record. The first attempt exposed a stale transaction `max_epoch` in the local file. The CLI now fails early when a due close's max epoch is below the current epoch; after correcting the file and using a reviewed fee cap, the retry finalized.

## Local engine evidence

`crates/contract-tests/tests/private_close.rs` passes three in-process engine tests:

- Claim-free settlement preserves voucher signature, channel/network binding, service commitment, output authorization, value conservation, and expiry rules, while rejecting malformed outputs, extra inputs, wrong signers, expiry settlement, and replay.
- Claim-free refund fails before expiry and for a non-payer, then succeeds with the payer after expiry.
- A real `Covenant::BalancePreserved(0)` rejects a missing claim and succeeds with its valid claim.

`crates/contract-tests/tests/hidden_settlement.rs` also confirms both SDK-built close statements have empty claim lists. The previous FROST/key-path prototype is superseded; no custom cryptography is needed. The key-plus-script condition-root workaround remains invalid (OBS-021 in `plans/03-bughunt-log.md`).

## Verified limits

Fees, timing, branch, authorization data, output commitments, and output count remain public. The CLI now checks that a due close's transaction `max_epoch` is not behind the current indexer epoch; a dry-run alone may not catch an expired max-epoch bound at indexer validation.

No template redeployment is expected because script predicates and ABI are unchanged.
