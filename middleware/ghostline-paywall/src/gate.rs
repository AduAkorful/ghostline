// SPDX-License-Identifier: MIT

use ghostline_core::{CHANNEL_ID_LEN, ServiceLedger};

use crate::{PaywallError, decode_voucher_header};

/// Checks a request against the channel ledger.
///
/// The paid delta must equal `price`, except an idempotent replay of the latest voucher (delta 0).
pub fn charge(
    ledger: &mut ServiceLedger,
    price: u64,
    voucher_hex: Option<&str>,
) -> Result<u64, PaywallError> {
    let Some(hex) = voucher_hex.filter(|s| !s.is_empty()) else {
        return Err(PaywallError::MissingVoucher);
    };
    let voucher = decode_voucher_header(hex)?;
    voucher.verify(ledger.params())?;
    if voucher.amount < ledger.accepted() {
        return Err(PaywallError::from(
            ghostline_core::VoucherError::NotIncreased {
                amount: voucher.amount,
                accepted: ledger.accepted(),
            },
        ));
    }
    if voucher.amount == ledger.accepted() {
        return Ok(0);
    }
    let delta = voucher.amount - ledger.accepted();
    if delta != price {
        return Err(PaywallError::WrongPrice { paid: delta, price });
    }
    Ok(ledger.accept(&voucher)?)
}

/// Charge only if the signed voucher pays the payout key the service advertised.
pub fn charge_to_output_key(
    ledger: &mut ServiceLedger,
    price: u64,
    voucher_hex: Option<&str>,
    expected_output_key: &tari_template_lib::types::crypto::RistrettoPublicKeyBytes,
) -> Result<u64, PaywallError> {
    validate_output_key(voucher_hex, expected_output_key)?;
    charge(ledger, price, voucher_hex)
}

pub(crate) fn validate_output_key(
    voucher_hex: Option<&str>,
    expected_output_key: &tari_template_lib::types::crypto::RistrettoPublicKeyBytes,
) -> Result<(), PaywallError> {
    let Some(hex) = voucher_hex.filter(|value| !value.is_empty()) else {
        return Err(PaywallError::MissingVoucher);
    };
    let voucher = decode_voucher_header(hex)?;
    if &voucher.service_output_key != expected_output_key {
        return Err(PaywallError::WrongServiceOutputKey);
    }
    Ok(())
}

pub fn channel_id_hex(ledger: &ServiceLedger) -> String {
    ghostline_core::to_hex(&ledger_channel_id(ledger))
}

pub fn ledger_channel_id(ledger: &ServiceLedger) -> [u8; CHANNEL_ID_LEN] {
    ledger.params().channel_id()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::encode_voucher_header;
    use ghostline_core::{NONCE_LEN, PaymentSession, ServiceLedger};
    use ootle_byte_type::ToByteType;
    use tari_ootle_common_types::crypto::create_key_pair_from_seed;
    use tari_template_lib::types::Hash32;

    fn setup() -> (
        PaymentSession,
        ServiceLedger,
        tari_crypto::ristretto::RistrettoSecretKey,
        tari_template_lib::types::crypto::RistrettoPublicKeyBytes,
    ) {
        let (secret, public) = create_key_pair_from_seed(11);
        let (_, service_public) = create_key_pair_from_seed(13);
        let params = ghostline_core::ChannelParams {
            network_tag: 1,
            payer: public.to_byte_type(),
            deposit: 100,
            payer_root: Hash32::from_array([9u8; 32]),
            nonce: [3u8; NONCE_LEN],
        };
        (
            PaymentSession::new(params.clone()),
            ServiceLedger::new(params),
            secret,
            service_public.to_byte_type(),
        )
    }

    #[test]
    fn missing_voucher_is_payment_required() {
        let (_payer, mut ledger, _secret, _service) = setup();
        assert_eq!(
            charge(&mut ledger, 10, None),
            Err(PaywallError::MissingVoucher)
        );
    }

    #[test]
    fn exact_price_is_accepted() {
        let (mut payer, mut ledger, secret, service_key) = setup();
        let voucher = payer.sign_next(&secret, 10, service_key).unwrap();
        let hex = encode_voucher_header(&voucher);
        assert_eq!(charge(&mut ledger, 10, Some(&hex)).unwrap(), 10);
        assert_eq!(charge(&mut ledger, 10, Some(&hex)).unwrap(), 0);
    }

    #[test]
    fn service_rejects_voucher_directed_to_an_unquoted_output_key() {
        let (mut payer, mut ledger, secret, service_key) = setup();
        let voucher = payer.sign_next(&secret, 10, service_key).unwrap();
        let hex = encode_voucher_header(&voucher);
        let (_, other_public) = create_key_pair_from_seed(99);
        let other_key = other_public.to_byte_type();

        assert_eq!(
            charge_to_output_key(&mut ledger, 10, Some(&hex), &other_key),
            Err(PaywallError::WrongServiceOutputKey)
        );
        assert_eq!(ledger.accepted(), 0);
        assert_eq!(
            charge_to_output_key(&mut ledger, 10, Some(&hex), &service_key).unwrap(),
            10
        );
    }

    #[test]
    fn overpaying_the_call_is_rejected() {
        let (mut payer, mut ledger, secret, service_key) = setup();
        let voucher = payer.sign_next(&secret, 20, service_key).unwrap();
        let hex = encode_voucher_header(&voucher);
        assert_eq!(
            charge(&mut ledger, 10, Some(&hex)),
            Err(PaywallError::WrongPrice {
                paid: 20,
                price: 10
            })
        );
        assert_eq!(ledger.accepted(), 0);
    }
}
