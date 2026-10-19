# Deposit confidentiality implementation

**Status:** Claim-free settlement and expiry refund are verified on Esmeralda.

## Change

The SDK assembles the close transfer statement from Tari's output-statement and global balance-proof helpers, leaving `covenant_claims` empty. It retains the script-path witness. Settlement still verifies the payer-signed output commitment, service authorization key, payer remainder path, output count, output promises, and absence of revealed-value inputs/outputs. Refund still requires the payer's script path and native `AfterEpoch` condition, and pays the full channel value to the payer's key path. The global stealth balance proof enforces input/output conservation.

The generic wallet builder adds a balance claim for script-path inputs. The engine requires that claim only when a spend condition uses `Covenant::BalancePreserved`; a separate adversarial test confirms such a covenant rejects a missing claim. Ghostline's settle and refund condition trees do not use that predicate, so omitting the unused claim preserves the channel's authorization and expiry rules.

## Esmeralda settlement proof

On 2026-10-18, the updated template finalized at [`59fc0b…8c9741`](https://explorer.tari.mw/tx/59fc0b3c52b36b6e0a8c7f3e44c8219e1d4777950dd6d9d4cd862aa1498c9741) with address `template_61bad5eb1b599ba6351e1573958fb84c4aeb4dd47415e60c0b44b1c50d7157a1`. Using this deployment, a fresh 10,000 µT channel opened and finalized at [`1ea836…36ffca9`](https://explorer.tari.mw/tx/1ea836af8636a655abda0ee6e32a7880551624960c1cb6d12a3eac0a936ffca9). The local HTTP `/fortune` paywall accepted 20 calls at 10 µT each (200 µT cumulative). One claim-free settlement finalized at [`6a4467…52424e17`](https://explorer.tari.mw/tx/6a44670272ee5cb5528c8b7bfac3ce89170b6885afb6cfdf1ae1cd0952424e17).

The finalized settlement's stealth transfer statement has one channel input, two stealth outputs, zero covenant claims, a zero revealed input amount, and no revealed output. Both outputs carry the one-unit minimum-value promise. The deposit and service amount do not appear as plaintext in the transaction. The public fee cap was 17,756 µT; the wallet receipt reports an actual settlement fee of 17,744 µT. The opening fee was 15,200 µT. The 20-call count and 200 µT cumulative amount are from the local service ledger, not a chain proof. The indexer reports the channel output as `down`. Sanitized details are in [`demo/ui/evidence.json`](../demo/ui/evidence.json).

The same deployment also finalized a two-channel pooled settlement at [`0e192d…cf78e2f`](https://explorer.tari.mw/tx/0e192d7b81581be024ffe5348b5944b8af010edac5cf52628024f21cf78e2e2f). Each channel had 20 locally accepted calls. The transaction spent two channel inputs and created three stealth outputs; the pooled payout was confirmed locally with the one-time payout key, while its exact amount is omitted from public evidence. Both input UTXOs are down. The public transaction groups both channels together, and the local batch builder received both deposit masks and values. This is execution evidence for a trusted-local prototype, not proof of coordinator privacy or unlinkability.

A fresh channel using the latest published template was refunded by the payer at expiry epoch 12466. Its open finalized at [`341145…e6bdf3`](https://explorer.tari.mw/tx/3411459ed95199a041fb289df5a24037acff0557769a280c607f3f5c87e6bdf3), and the refund finalized at [`050dc1…71cd2a2`](https://explorer.tari.mw/tx/050dc1cb38803875deb5bcbba7f124086c62f40a5ae82e902a780f3c171cd2a2). The refund transaction had zero covenant claims, zero revealed input amount, one stealth output, and no revealed output. The wallet/indexer confirmed the channel output was spent; we did not locally unblind the recipient output, so we do not claim an independently verified refund amount. The dry-run estimated a 9,510 µT fee and the finalized transaction paid 9,498 µT from the payer-side fee account. An earlier attempt exposed a stale transaction `max_epoch` in a local file; the CLI now rejects a due close when that bound is behind the current epoch.

## Local engine evidence

`crates/contract-tests/tests/private_close.rs` passes three in-process engine tests:

- Claim-free settlement preserves voucher signature, channel/network binding, service commitment, output authorization, value conservation, and expiry rules, while rejecting malformed outputs, extra inputs, wrong signers, expiry settlement, and replay.
- Claim-free refund fails before expiry and for a non-payer, then succeeds with the payer after expiry.
- A real `Covenant::BalancePreserved(0)` rejects a missing claim and succeeds with its valid claim.

`crates/contract-tests/tests/hidden_settlement.rs` also confirms both SDK-built close statements have empty claim lists. The previous FROST/key-path prototype is superseded; no custom cryptography is needed. The key-plus-script condition-root workaround remains invalid (OBS-021 in `plans/03-bughunt-log.md`).

## Verified limits

Fees, timing, branch, authorization data, output commitments, and output count remain public. Settlement now reserves 1 µT for payer change and requires exactly two outputs, so output count does not disclose whether the voucher consumed the entire channel deposit. The service can be paid at most `deposit - 1 µT`; the reserve is returned to the payer. The CLI also checks that a due close's transaction `max_epoch` is not behind the current indexer epoch; a dry-run alone may not catch an expired max-epoch bound at indexer validation.

The updated contract source changes the settle predicate (it now requires exactly two outputs) and adds the batch-settlement entrypoint. The settlement, batch, and refund paths have now been exercised on the newly published Esmeralda template. The refund ABI is unchanged. The public transaction confirms a confidential output and that the channel was spent; without local unblinding, it does not prove the recipient received a particular amount.
