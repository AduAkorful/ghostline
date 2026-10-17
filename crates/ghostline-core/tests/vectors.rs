// SPDX-License-Identifier: MIT

use ghostline_core::{
    CHANNEL_ID_DOMAIN, ChannelParams, NONCE_LEN, SignedVoucher, VOUCHER_DOMAIN, sign_with_nonce,
    to_hex, voucher_message,
};
use ootle_byte_type::ToByteType;
use tari_ootle_common_types::crypto::create_key_pair_from_seed;
use tari_ootle_wallet_crypto::MaskAndValue;
use tari_template_lib::types::{Hash32, crypto::RistrettoPublicKeyBytes};
use tari_utilities::ByteArray;

fn fixture_params() -> ChannelParams {
    ChannelParams {
        network_tag: 1,
        payer: RistrettoPublicKeyBytes::from_bytes(&[7u8; 32]).unwrap(),
        deposit: 100,
        payer_root: Hash32::from_array([9u8; 32]),
        nonce: [3u8; NONCE_LEN],
    }
}

#[test]
fn committed_vectors_match_rust() {
    let params = fixture_params();
    let channel_id = params.channel_id();
    let (_, service_public) = create_key_pair_from_seed(13);
    let service: RistrettoPublicKeyBytes = service_public.to_byte_type();
    let mask = tari_crypto::ristretto::RistrettoSecretKey::from_canonical_bytes(&{
        let mut bytes = [0u8; 32];
        bytes[0] = 5;
        bytes
    })
    .unwrap();
    let commitment = MaskAndValue::new(30, mask).to_commitment().to_byte_type();
    let message = voucher_message(&channel_id, &service, &commitment);
    let json = serde_json::json!({
        "channel_id_domain": String::from_utf8_lossy(CHANNEL_ID_DOMAIN),
        "voucher_domain": String::from_utf8_lossy(VOUCHER_DOMAIN),
        "network_tag": 1,
        "payer_hex": to_hex(params.payer.as_bytes()),
        "deposit": 100,
        "payer_root_hex": to_hex(params.payer_root.as_slice()),
        "nonce_hex": to_hex(&params.nonce),
        "amount": 30,
        "service_output_key_hex": to_hex(service.as_bytes()),
        "output_commitment_hex": to_hex(commitment.as_bytes()),
        "channel_id_hex": to_hex(&channel_id),
        "voucher_message_hex": to_hex(&message),
    });
    let actual = serde_json::to_string_pretty(&json).unwrap() + "\n";
    let expected = include_str!("../../../packages/ghostline/test-vectors/vectors.json");
    assert_eq!(actual, expected);
}

#[test]
fn committed_voucher_signature_matches_rust() {
    let (secret, public) = create_key_pair_from_seed(11);
    let (nonce, _) = create_key_pair_from_seed(17);
    let (_, service_public) = create_key_pair_from_seed(13);
    let service: RistrettoPublicKeyBytes = service_public.to_byte_type();
    let params = ChannelParams {
        network_tag: 1,
        payer: public.to_byte_type(),
        deposit: 100,
        payer_root: Hash32::from_array([9u8; 32]),
        nonce: [3u8; NONCE_LEN],
    };
    let payer_secret_hex = to_hex(secret.as_bytes());
    let sign_nonce_secret_hex = to_hex(nonce.as_bytes());
    let mask = tari_crypto::ristretto::RistrettoSecretKey::from_canonical_bytes(&{
        let mut bytes = [0u8; 32];
        bytes[0] = 5;
        bytes
    })
    .unwrap();
    let mask_hex = to_hex(mask.as_bytes());
    let commitment = MaskAndValue::new(30, mask.clone())
        .to_commitment()
        .to_byte_type();
    let channel_id = params.channel_id();
    let message = voucher_message(&channel_id, &service, &commitment);
    let signature = sign_with_nonce(&secret, nonce.clone(), VOUCHER_DOMAIN, &message);
    let voucher = SignedVoucher {
        channel_id,
        amount: 30,
        mask,
        service_output_key: service,
        output_commitment: commitment,
        signature,
    };
    voucher.verify(&params).unwrap();
    let json = serde_json::json!({
        "payer_secret_hex": payer_secret_hex,
        "payer_public_hex": to_hex(public.to_byte_type().as_bytes()),
        "sign_nonce_secret_hex": sign_nonce_secret_hex,
        "network_tag": 1,
        "deposit": 100,
        "payer_root_hex": to_hex(params.payer_root.as_slice()),
        "channel_nonce_hex": to_hex(&params.nonce),
        "mask_hex": mask_hex,
        "amount": 30,
        "service_output_key_hex": to_hex(service.as_bytes()),
        "output_commitment_hex": to_hex(commitment.as_bytes()),
        "channel_id_hex": to_hex(&voucher.channel_id),
        "voucher_hex": to_hex(&voucher.to_bytes()),
    });
    let actual = serde_json::to_string_pretty(&json).unwrap() + "\n";
    let expected = include_str!("../../../packages/ghostline/test-vectors/voucher.json");
    assert_eq!(actual, expected);
}
