// SPDX-License-Identifier: MIT

//! Ghostline payment channel.
//!
//! A channel is a stealth UTXO whose condition tree has two leaves: a settle leaf that runs [`settle`] and a refund
//! leaf made only of native timelock and key atoms. This template only supplies the settle predicate; the tree itself
//! is assembled client-side so the deposit stays an ordinary stealth output.
//!
//! [`settle`]: channel::GhostlineChannel::settle

use tari_template_lib::{intrinsics, prelude::*};

custom_signature_domain!(VoucherDomain, b"ghostline.channel.voucher.v1");

const CHANNEL_ID_DOMAIN: &[u8] = b"ghostline.channel.id.v1";
const NONCE_LEN: usize = 32;
const CHANNEL_ID_LEN: usize = 32;
const AMOUNT_LEN: usize = 8;

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
        /// signed by `payer` for `amount <= deposit` and the spend returns `deposit - amount` to `payer_root`.
        ///
        /// Spend scripts are stateless, so the channel closes on settlement; a second redemption of the same voucher
        /// cannot exist because the deposit UTXO is consumed.
        pub fn settle(
            network_tag: u8,
            payer: RistrettoPublicKeyBytes,
            deposit: u64,
            payer_root: Hash32,
            nonce: Vec<u8>,
            ctx: SpendContext,
        ) {
            assert!(nonce.len() == NONCE_LEN, "channel nonce must be 32 bytes");

            let (amount, signature): (u64, Signature<VoucherDomain>) =
                tari_bor::decode(&ctx.data()).expect("voucher witness malformed");
            assert!(amount <= deposit, "voucher exceeds deposit");

            // The id commits to every parameter the payer fixed at open time, so a voucher is only valid for the
            // channel it was signed for, on the network it was signed for.
            let channel_id = intrinsics::hash_blake2b_parts(&[
                CHANNEL_ID_DOMAIN,
                &[network_tag],
                payer.as_bytes(),
                &deposit.to_le_bytes(),
                payer_root.as_slice(),
                &nonce,
            ]);

            // Fixed-length fields only, so the message has exactly one parse.
            let mut message = [0u8; CHANNEL_ID_LEN + AMOUNT_LEN];
            message[..CHANNEL_ID_LEN].copy_from_slice(channel_id.as_slice());
            message[CHANNEL_ID_LEN..].copy_from_slice(&amount.to_le_bytes());

            assert!(signature.verify(&PublicKey::from(payer), &message), "invalid voucher signature");

            ctx.require_output_to(&payer_root, deposit - amount);
        }
    }
}
