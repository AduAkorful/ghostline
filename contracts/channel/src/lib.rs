// SPDX-License-Identifier: MIT

//! Ghostline payment channel.
//!
//! A standard channel is a stealth UTXO with a settle leaf that runs [`settle`] and a refund leaf made only of native
//! timelock and key atoms. Batch-enabled channels additionally include [`settle_batch`] and retain the individual
//! settle leaf as an offline-participant fallback. The condition tree is assembled client-side so the deposit stays
//! an ordinary stealth output.
//!
//! [`settle`]: channel::GhostlineChannel::settle

use tari_template_lib::prelude::*;
use curve25519_dalek::{ristretto::{CompressedRistretto, RistrettoPoint}, traits::Identity};

custom_signature_domain!(VoucherDomain, b"ghostline.channel.voucher.v1");
custom_signature_domain!(BatchBalanceDomain, b"ghostline.channel.batch-balance.v1");

const CHANNEL_ID_LEN: usize = 32;

#[template]
mod channel {
    use super::*;

    pub struct GhostlineChannel {}

    impl GhostlineChannel {
        pub fn new() -> Component<Self> {
            Component::new(Self {})
                .with_access_rules(AccessRules::allow_all())
                .create()
        }

        /// Spend predicate for the settle leaf. Authorises a one-shot settlement when the witness carries a voucher
        /// signed by `payer` for a service-output commitment and the spend returns any remainder to `payer_root`.
        ///
        /// Spend scripts are stateless, so the channel closes on settlement; a second redemption of the same voucher
        /// cannot exist because the deposit UTXO is consumed. The transfer proof enforces conservation and range.
        pub fn settle(
            channel_id: Hash32,
            payer: RistrettoPublicKeyBytes,
            payer_root: Hash32,
            ctx: SpendContext,
        ) {
            let (output_commitment, service_output_key, signature): (
                PedersenCommitmentBytes,
                RistrettoPublicKeyBytes,
                Signature<VoucherDomain>,
            ) = tari_bor::decode(&ctx.data()).expect("voucher witness malformed");

            // The payer authorizes a Pedersen commitment rather than a plaintext amount. The off-chain service
            // receives the opening with the voucher; the ledger only needs to verify that this exact commitment is
            // paid to the authorized one-time service key.
            let mut message = [0u8; CHANNEL_ID_LEN + 64];
            message[..CHANNEL_ID_LEN].copy_from_slice(channel_id.as_slice());
            message[CHANNEL_ID_LEN..CHANNEL_ID_LEN + 32]
                .copy_from_slice(service_output_key.as_bytes());
            message[CHANNEL_ID_LEN + 32..].copy_from_slice(output_commitment.as_bytes());

            assert!(
                signature.verify(&PublicKey::from(payer), &message),
                "invalid voucher signature"
            );

            // SpendContext exposes only commitments for stealth values. Constrain the transfer shape so the channel
            // cannot be supplemented with another input or an output that bypasses the payer/service split. The
            // engine's transfer balance proof then enforces that the hidden output values sum to the channel input.
            let inputs = ctx.inputs();
            assert!(
                inputs.len() == 1,
                "settle must spend only the channel input"
            );
            assert!(
                ctx.revealed_input_amount() == Amount::zero(),
                "settle cannot use revealed input value"
            );
            assert!(
                ctx.revealed_output_amount() == Amount::zero(),
                "settle cannot create revealed output value"
            );

            let outputs = ctx.outputs();
            // A payer-change output is always required. This keeps the settlement's output count fixed even when
            // the cumulative voucher is at the channel's spend limit.
            assert!(outputs.len() == 2, "settle must have exactly two outputs");
            let mut service_output_found = false;
            let mut payer_change_found = false;
            for output in outputs {
                assert!(
                    output.minimum_value_promise == 1,
                    "settle output must use a non-disclosing value promise"
                );
                if output.commitment == output_commitment
                    && output.auth == SpendAuthorization::Key(service_output_key)
                {
                    assert!(!service_output_found, "duplicate service output");
                    service_output_found = true;
                } else if output.auth == SpendAuthorization::Script(payer_root) {
                    assert!(!payer_change_found, "duplicate payer change output");
                    payer_change_found = true;
                } else {
                    panic!("settle contains an unauthorized output");
                }
            }
            assert!(service_output_found, "signed service output is missing");
            assert!(
                payer_change_found,
                "reserved payer change output is missing"
            );
        }

        /// Spend predicate for a bounded batch close. Every channel input carries the same manifest, with its own
        /// input index. The manifest's voucher commitments sum to one pooled service output; each channel separately
        /// proves its own deposit is split between its voucher and its payer change.
        pub fn settle_batch(
            channel_id: Hash32,
            payer: RistrettoPublicKeyBytes,
            payer_root: Hash32,
            ctx: SpendContext,
        ) {
            // Entry: channel id, input commitment, payer key, payer change root, voucher commitment, signature.
            let (batch_index, service_output_key, entries): (
                u32,
                RistrettoPublicKeyBytes,
                Vec<(
                    Hash32,
                    PedersenCommitmentBytes,
                    RistrettoPublicKeyBytes,
                    Hash32,
                    PedersenCommitmentBytes,
                    Signature<VoucherDomain>,
                    Signature<BatchBalanceDomain>,
                )>,
            ) = tari_bor::decode(&ctx.data()).expect("batch voucher witness malformed");

            let inputs = ctx.inputs();
            assert!(
                entries.len() >= 2 && entries.len() <= 12,
                "batch size must be between 2 and 12"
            );
            assert!(inputs.len() == entries.len(), "batch input count mismatch");
            assert!(
                ctx.revealed_input_amount() == Amount::zero(),
                "batch cannot use revealed input value"
            );
            assert!(
                ctx.revealed_output_amount() == Amount::zero(),
                "batch cannot create revealed output value"
            );

            let current = ctx.current_input();
            assert!(current.index == batch_index, "batch index mismatch");
            assert!((batch_index as usize) < entries.len(), "batch index out of range");

            let outputs = ctx.outputs();
            assert!(outputs.len() == entries.len() + 1, "batch output count mismatch");
            let mut pooled_commitment = RistrettoPoint::identity();
            for (index, entry) in entries.iter().enumerate() {
                let (
                    entry_channel_id,
                    input_commitment,
                    entry_payer,
                    entry_payer_root,
                    voucher_commitment,
                    signature,
                    balance_proof,
                ) = entry;

                assert!(inputs[index].commitment == *input_commitment, "batch input order mismatch");
                for earlier in entries.iter().take(index) {
                    assert!(earlier.0 != *entry_channel_id, "duplicate batch channel");
                    assert!(earlier.1 != *input_commitment, "duplicate batch input");
                    assert!(earlier.3 != *entry_payer_root, "duplicate payer change root");
                }

                let mut message = [0u8; CHANNEL_ID_LEN + 64];
                message[..CHANNEL_ID_LEN].copy_from_slice(entry_channel_id.as_slice());
                message[CHANNEL_ID_LEN..CHANNEL_ID_LEN + 32]
                    .copy_from_slice(service_output_key.as_bytes());
                message[CHANNEL_ID_LEN + 32..].copy_from_slice(voucher_commitment.as_bytes());
                assert!(
                    signature.verify(&PublicKey::from(*entry_payer), &message),
                    "invalid batch voucher signature"
                );

                pooled_commitment += decode_commitment(voucher_commitment);

                let change_commitment = outputs
                    .iter()
                    .find(|output| output.auth == SpendAuthorization::Script(*entry_payer_root))
                    .expect("batch payer change output is missing")
                    .commitment;
                let excess_point = decode_commitment(input_commitment)
                    - decode_commitment(voucher_commitment)
                    - decode_commitment(&change_commitment);
                let excess_key = RistrettoPublicKeyBytes::from_bytes(
                    &excess_point.compress().to_bytes(),
                )
                .expect("batch mask-excess point encoding is valid");
                let mut proof_message = [0u8; CHANNEL_ID_LEN + 160];
                proof_message[..32].copy_from_slice(entry_channel_id.as_slice());
                proof_message[32..64].copy_from_slice(input_commitment.as_bytes());
                proof_message[64..96].copy_from_slice(entry_payer_root.as_slice());
                proof_message[96..128].copy_from_slice(service_output_key.as_bytes());
                proof_message[128..160].copy_from_slice(voucher_commitment.as_bytes());
                proof_message[160..].copy_from_slice(change_commitment.as_bytes());
                assert!(
                    balance_proof.verify(&PublicKey::from(excess_key), &proof_message),
                    "invalid per-channel balance proof"
                );
            }

            let own = &entries[batch_index as usize];
            assert!(own.0 == channel_id, "batch channel id mismatch");
            assert!(own.2 == payer, "batch payer mismatch");
            assert!(own.3 == payer_root, "batch payer root mismatch");
            assert!(own.1 == current.commitment, "batch current input mismatch");

            let pooled_commitment = PedersenCommitmentBytes::from_array(pooled_commitment.compress().to_bytes());
            let mut pooled_output_found = false;
            let mut own_change = None;
            let mut change_roots_found: Vec<Hash32> = Vec::with_capacity(entries.len());
            for output in outputs {
                assert!(
                    output.minimum_value_promise == 1,
                    "batch output must use a non-disclosing value promise"
                );
                if output.auth == SpendAuthorization::Key(service_output_key) {
                    assert!(!pooled_output_found, "duplicate pooled service output");
                    assert!(output.commitment == pooled_commitment, "pooled service commitment mismatch");
                    pooled_output_found = true;
                } else if let SpendAuthorization::Script(root) = output.auth {
                    assert!(
                        entries.iter().any(|entry| entry.3 == root),
                        "batch contains an unauthorized change output"
                    );
                    assert!(
                        !change_roots_found.contains(&root),
                        "duplicate payer change output"
                    );
                    change_roots_found.push(root);
                    if root == payer_root {
                        own_change = Some(output.commitment);
                    }
                } else {
                    panic!("batch contains an unauthorized output");
                }
            }
            assert!(pooled_output_found, "pooled service output is missing");
            assert!(
                change_roots_found.len() == entries.len(),
                "batch payer change outputs are incomplete"
            );

            let _ = own_change.expect("payer change output is missing");
        }
    }
}

fn decode_commitment(commitment: &PedersenCommitmentBytes) -> RistrettoPoint {
    CompressedRistretto(**commitment)
        .decompress()
        .expect("invalid Pedersen commitment point")
}
