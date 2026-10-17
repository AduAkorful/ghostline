// SPDX-License-Identifier: MIT

use ghostline_core::{ChannelParams, NONCE_LEN, PaymentSession, ServiceLedger};
use ootle_byte_type::ToByteType;
use proptest::prelude::*;
use tari_ootle_common_types::crypto::create_key_pair_from_seed;
use tari_template_lib::types::{Hash32, crypto::RistrettoPublicKeyBytes};

fn params(deposit: u64) -> ChannelParams {
    let (_, public) = create_key_pair_from_seed(11);
    ChannelParams {
        network_tag: 1,
        payer: public.to_byte_type(),
        deposit,
        payer_root: Hash32::from_array([9u8; 32]),
        nonce: [3u8; NONCE_LEN],
    }
}

proptest! {
    #[test]
    fn ledger_never_exceeds_deposit_and_sums_with_remaining(
        deposit in 1u64..500,
        increments in prop::collection::vec(1u64..40, 1..12),
    ) {
        let (secret, _) = create_key_pair_from_seed(11);
        let p = params(deposit);
        let (_, service_public) = create_key_pair_from_seed(13);
        let service_key: RistrettoPublicKeyBytes = service_public.to_byte_type();
        let mut payer = PaymentSession::new(p.clone());
        let mut service = ServiceLedger::new(p);
        let mut last_ok = 0u64;
        for inc in increments {
            match payer.sign_next(&secret, inc, service_key) {
                Ok(voucher) => {
                    let delta = service.accept(&voucher).unwrap();
                    prop_assert!(service.accepted() <= deposit);
                    prop_assert_eq!(service.accepted() + service.remaining(), deposit);
                    prop_assert_eq!(delta, voucher.amount - last_ok);
                    last_ok = voucher.amount;
                    prop_assert_eq!(service.accept(&voucher).unwrap(), 0);
                }
                Err(_) => {
                    prop_assert!(payer.cumulative().saturating_add(inc) > deposit);
                    prop_assert_eq!(payer.cumulative(), last_ok);
                    prop_assert_eq!(service.accepted(), last_ok);
                }
            }
        }
    }

    #[test]
    fn older_totals_do_not_move_accepted(
        first in 2u64..50,
        second in 1u64..50,
    ) {
        prop_assume!(first + second <= 100);
        let (secret, _) = create_key_pair_from_seed(11);
        let (_, service_public) = create_key_pair_from_seed(13);
        let service_key: RistrettoPublicKeyBytes = service_public.to_byte_type();
        let mut payer = PaymentSession::new(params(100));
        let mut service = ServiceLedger::new(params(100));
        let older = payer.sign_next(&secret, first, service_key).unwrap();
        let newer = payer.sign_next(&secret, second, service_key).unwrap();
        service.accept(&newer).unwrap();
        prop_assert!(service.accept(&older).is_err());
        prop_assert_eq!(service.accepted(), first + second);
    }
}
