// SPDX-License-Identifier: MIT

use tari_crypto::ristretto::RistrettoSecretKey;
use tari_template_lib::types::crypto::RistrettoPublicKeyBytes;

use crate::{ChannelParams, SignedVoucher, VoucherError};

/// Payer-side running total. Crash-safe persistence of `cumulative` is the caller's job.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaymentSession {
    params: ChannelParams,
    cumulative: u64,
}

impl PaymentSession {
    pub fn new(params: ChannelParams) -> Self {
        Self {
            params,
            cumulative: 0,
        }
    }

    /// Restores a session after a crash. `cumulative` must be the last signed total.
    pub fn resume(params: ChannelParams, cumulative: u64) -> Result<Self, VoucherError> {
        if cumulative > params.deposit {
            return Err(VoucherError::ExceedsDeposit {
                amount: cumulative,
                deposit: params.deposit,
            });
        }
        Ok(Self { params, cumulative })
    }

    pub fn params(&self) -> &ChannelParams {
        &self.params
    }

    pub fn cumulative(&self) -> u64 {
        self.cumulative
    }

    /// Signs a voucher for `cumulative + increment` and advances the session.
    pub fn sign_next(
        &mut self,
        secret: &RistrettoSecretKey,
        increment: u64,
        service_output_key: RistrettoPublicKeyBytes,
    ) -> Result<SignedVoucher, VoucherError> {
        let amount = self
            .cumulative
            .checked_add(increment)
            .ok_or(VoucherError::Overflow)?;
        let voucher = SignedVoucher::sign(secret, &self.params, amount, service_output_key)?;
        self.cumulative = amount;
        Ok(voucher)
    }
}

/// Service-side highest accepted cumulative total.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceLedger {
    params: ChannelParams,
    accepted: u64,
}

impl ServiceLedger {
    pub fn new(params: ChannelParams) -> Self {
        Self {
            params,
            accepted: 0,
        }
    }

    pub fn resume(params: ChannelParams, accepted: u64) -> Result<Self, VoucherError> {
        if accepted > params.deposit {
            return Err(VoucherError::ExceedsDeposit {
                amount: accepted,
                deposit: params.deposit,
            });
        }
        Ok(Self { params, accepted })
    }

    pub fn params(&self) -> &ChannelParams {
        &self.params
    }

    pub fn accepted(&self) -> u64 {
        self.accepted
    }

    pub fn remaining(&self) -> u64 {
        self.params.deposit - self.accepted
    }

    /// Verifies the voucher and records it.
    ///
    /// A strictly larger amount returns the delta. The same amount as already accepted is an idempotent
    /// replay of the latest voucher (HTTP retries) and returns `0`. A smaller amount is rejected.
    pub fn accept(&mut self, voucher: &SignedVoucher) -> Result<u64, VoucherError> {
        voucher.verify(&self.params)?;
        if voucher.amount < self.accepted {
            return Err(VoucherError::NotIncreased {
                amount: voucher.amount,
                accepted: self.accepted,
            });
        }
        if voucher.amount == self.accepted {
            return Ok(0);
        }
        let delta = voucher.amount - self.accepted;
        self.accepted = voucher.amount;
        Ok(delta)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::NONCE_LEN;
    use ootle_byte_type::ToByteType;
    use tari_ootle_common_types::crypto::create_key_pair_from_seed;
    use tari_template_lib::types::Hash32;

    fn setup() -> (
        RistrettoSecretKey,
        PaymentSession,
        ServiceLedger,
        RistrettoPublicKeyBytes,
    ) {
        let (secret, public) = create_key_pair_from_seed(11);
        let (_, service_public) = create_key_pair_from_seed(13);
        let params = ChannelParams {
            network_tag: 1,
            payer: public.to_byte_type(),
            deposit: 100,
            payer_root: Hash32::from_array([9u8; 32]),
            nonce: [3u8; NONCE_LEN],
        };
        (
            secret,
            PaymentSession::new(params.clone()),
            ServiceLedger::new(params),
            service_public.to_byte_type(),
        )
    }

    #[test]
    fn two_payments_settle_the_delta() {
        let (secret, mut payer, mut service, service_key) = setup();
        let first = payer.sign_next(&secret, 10, service_key).unwrap();
        assert_eq!(service.accept(&first).unwrap(), 10);
        let second = payer.sign_next(&secret, 25, service_key).unwrap();
        assert_eq!(service.accept(&second).unwrap(), 25);
        assert_eq!(service.accepted(), 35);
        assert_eq!(service.remaining(), 65);
    }

    #[test]
    fn replay_of_older_total_is_rejected() {
        let (secret, mut payer, mut service, service_key) = setup();
        let first = payer.sign_next(&secret, 10, service_key).unwrap();
        let second = payer.sign_next(&secret, 5, service_key).unwrap();
        service.accept(&second).unwrap();
        assert_eq!(
            service.accept(&first),
            Err(VoucherError::NotIncreased {
                amount: 10,
                accepted: 15
            })
        );
        assert_eq!(service.accepted(), 15);
    }

    #[test]
    fn cannot_exceed_deposit() {
        let (secret, mut payer, _service, service_key) = setup();
        assert!(matches!(
            payer.sign_next(&secret, 101, service_key),
            Err(VoucherError::ExceedsDeposit { .. })
        ));
        assert_eq!(payer.cumulative(), 0);
    }

    #[test]
    fn latest_voucher_replay_is_idempotent() {
        let (secret, mut payer, mut service, service_key) = setup();
        let first = payer.sign_next(&secret, 10, service_key).unwrap();
        assert_eq!(service.accept(&first).unwrap(), 10);
        assert_eq!(service.accept(&first).unwrap(), 0);
        assert_eq!(service.accepted(), 10);
    }

    #[test]
    fn thousand_calls_are_one_cumulative_total() {
        let (secret, public) = create_key_pair_from_seed(11);
        let (_, service_public) = create_key_pair_from_seed(13);
        let params = ChannelParams {
            network_tag: 1,
            payer: public.to_byte_type(),
            deposit: 10_000,
            payer_root: Hash32::from_array([9u8; 32]),
            nonce: [3u8; NONCE_LEN],
        };
        let mut payer = PaymentSession::new(params.clone());
        let mut service = ServiceLedger::new(params);
        let mut last = None;
        for _ in 0..1000 {
            let voucher = payer
                .sign_next(&secret, 10, service_public.to_byte_type())
                .unwrap();
            assert_eq!(service.accept(&voucher).unwrap(), 10);
            last = Some(voucher);
        }
        assert_eq!(service.accepted(), 10_000);
        assert_eq!(service.remaining(), 0);
        assert_eq!(payer.cumulative(), 10_000);
        assert_eq!(last.unwrap().amount, 10_000);
    }
}
