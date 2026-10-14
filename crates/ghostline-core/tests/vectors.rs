// SPDX-License-Identifier: MIT

use ghostline_core::{
    CHANNEL_ID_DOMAIN, ChannelParams, NONCE_LEN, VOUCHER_DOMAIN, to_hex, voucher_message,
};
use tari_template_lib::types::{Hash32, crypto::RistrettoPublicKeyBytes};

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
    let message = voucher_message(&channel_id, 30);
    let json = serde_json::json!({
        "channel_id_domain": String::from_utf8_lossy(CHANNEL_ID_DOMAIN),
        "voucher_domain": String::from_utf8_lossy(VOUCHER_DOMAIN),
        "network_tag": 1,
        "payer_hex": to_hex(params.payer.as_bytes()),
        "deposit": 100,
        "payer_root_hex": to_hex(params.payer_root.as_slice()),
        "nonce_hex": to_hex(&params.nonce),
        "amount": 30,
        "channel_id_hex": to_hex(&channel_id),
        "voucher_message_hex": to_hex(&message),
    });
    let actual = serde_json::to_string_pretty(&json).unwrap() + "\n";
    let expected = include_str!("../../../packages/ghostline/test-vectors/vectors.json");
    assert_eq!(actual, expected);
}
