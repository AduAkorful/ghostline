// SPDX-License-Identifier: MIT

//! Ghostline payment channel.
//!
//! A channel is a stealth UTXO whose condition tree has two leaves: a settle leaf that runs [`settle`] and a refund
//! leaf made only of native timelock and key atoms. This template only supplies the settle predicate; the tree itself
//! is assembled client-side so the deposit stays an ordinary stealth output.
//!
//! [`settle`]: channel::GhostlineChannel::settle

use tari_template_lib::prelude::*;

custom_signature_domain!(VoucherDomain, b"ghostline.channel.voucher.v1");

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
            ) =
                tari_bor::decode(&ctx.data()).expect("voucher witness malformed");

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
            assert!(inputs.len() == 1, "settle must spend only the channel input");
            assert!(
                ctx.revealed_input_amount() == Amount::zero(),
                "settle cannot use revealed input value"
            );
            assert!(
                ctx.revealed_output_amount() == Amount::zero(),
                "settle cannot create revealed output value"
            );

            let outputs = ctx.outputs();
            assert!(!outputs.is_empty() && outputs.len() <= 2, "invalid settle output count");
            let mut service_output_found = false;
            let mut payer_change_found = false;
            for output in outputs {
                assert!(output.minimum_value_promise == 1, "settle output must use a non-disclosing value promise");
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
        }
    }
}
