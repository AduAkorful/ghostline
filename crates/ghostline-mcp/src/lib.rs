// SPDX-License-Identifier: MIT

use ghostline_core::{ChannelParams, SignedVoucher, VoucherError, from_hex, to_hex};
use ghostline_paywall::{PaymentRequired, VOUCHER_HEADER, encode_voucher_header};
use serde::{Deserialize, Serialize};
use tari_crypto::ristretto::RistrettoSecretKey;
use tari_template_lib::types::{Hash32, crypto::RistrettoPublicKeyBytes};
use tari_utilities::ByteArray;

#[derive(Debug, Clone, Deserialize, Serialize, schemars::JsonSchema)]
pub struct ChannelArgs {
    pub network_tag: u8,
    pub payer_hex: String,
    pub deposit: u64,
    pub payer_root_hex: String,
    pub nonce_hex: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, schemars::JsonSchema)]
pub struct SignArgs {
    #[serde(flatten)]
    pub channel: ChannelArgs,
    pub amount: u64,
    pub secret_hex: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, schemars::JsonSchema)]
pub struct VerifyArgs {
    #[serde(flatten)]
    pub channel: ChannelArgs,
    pub voucher_hex: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, schemars::JsonSchema)]
pub struct CallPaidArgs {
    pub url: String,
    pub voucher_hex: Option<String>,
}

pub fn parse_params(args: &ChannelArgs) -> Result<ChannelParams, String> {
    let payer = RistrettoPublicKeyBytes::from_hex(&args.payer_hex).map_err(|e| e.to_string())?;
    let payer_root = Hash32::from_hex(&args.payer_root_hex).map_err(|e| e.to_string())?;
    let nonce_bytes = from_hex(&args.nonce_hex).map_err(|e| e.to_string())?;
    let nonce = nonce_bytes
        .try_into()
        .map_err(|_| "nonce must be 32 bytes".to_string())?;
    Ok(ChannelParams {
        network_tag: args.network_tag,
        payer,
        deposit: args.deposit,
        payer_root,
        nonce,
    })
}

pub fn channel_id_hex(args: &ChannelArgs) -> Result<String, String> {
    Ok(to_hex(&parse_params(args)?.channel_id()))
}

pub fn sign_voucher_hex(args: &SignArgs) -> Result<String, String> {
    let params = parse_params(&args.channel)?;
    let secret_bytes = from_hex(&args.secret_hex).map_err(|e| e.to_string())?;
    let secret =
        RistrettoSecretKey::from_canonical_bytes(&secret_bytes).map_err(|e| e.to_string())?;
    let voucher = SignedVoucher::sign(&secret, &params, args.amount)
        .map_err(|e: VoucherError| e.to_string())?;
    Ok(encode_voucher_header(&voucher))
}

pub fn verify_voucher(args: &VerifyArgs) -> Result<(), String> {
    let params = parse_params(&args.channel)?;
    let bytes = from_hex(&args.voucher_hex).map_err(|e| e.to_string())?;
    let voucher = SignedVoucher::from_bytes(&bytes).map_err(|e| e.to_string())?;
    voucher.verify(&params).map_err(|e| e.to_string())
}

#[derive(Debug, Clone, Serialize)]
pub struct PaidCall {
    pub status: u16,
    pub body: String,
    pub payment_required: Option<PaymentRequired>,
}

pub async fn call_paid(args: &CallPaidArgs) -> Result<PaidCall, String> {
    let client = reqwest::Client::new();
    let mut req = client.get(&args.url);
    if let Some(voucher) = &args.voucher_hex {
        req = req.header(VOUCHER_HEADER, voucher);
    }
    let response = req.send().await.map_err(|e| e.to_string())?;
    let status = response.status().as_u16();
    let body = response.text().await.map_err(|e| e.to_string())?;
    let payment_required = if status == 402 {
        serde_json::from_str(&body).ok()
    } else {
        None
    };
    Ok(PaidCall {
        status,
        body,
        payment_required,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use ghostline_core::NONCE_LEN;
    use ootle_byte_type::ToByteType;
    use tari_ootle_common_types::crypto::create_key_pair_from_seed;
    use tari_template_lib::types::Hash32;
    use tari_utilities::ByteArray;

    #[test]
    fn sign_and_verify_round_trip() {
        let (secret, public) = create_key_pair_from_seed(11);
        let args = ChannelArgs {
            network_tag: 1,
            payer_hex: to_hex(public.to_byte_type().as_bytes()),
            deposit: 100,
            payer_root_hex: to_hex(Hash32::from_array([9u8; 32]).as_slice()),
            nonce_hex: to_hex(&[3u8; NONCE_LEN]),
        };
        let voucher_hex = sign_voucher_hex(&SignArgs {
            channel: args.clone(),
            amount: 30,
            secret_hex: to_hex(secret.as_bytes()),
        })
        .unwrap();
        verify_voucher(&VerifyArgs {
            channel: args,
            voucher_hex,
        })
        .unwrap();
    }
}
