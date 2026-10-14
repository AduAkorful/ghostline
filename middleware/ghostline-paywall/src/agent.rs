// SPDX-License-Identifier: MIT

use ghostline_core::{PaymentSession, SignedVoucher};
use ghostline_policy::Policy;
use tari_crypto::ristretto::RistrettoSecretKey;
use tari_template_lib::types::crypto::RistrettoPublicKeyBytes;

use crate::{AgentError, PaymentRequired, encode_voucher_header};

/// Signs the next voucher for a 402 quote, after the policy engine approves it.
pub struct AgentPayer {
    session: PaymentSession,
    policy: Policy,
    vendor: RistrettoPublicKeyBytes,
}

impl AgentPayer {
    pub fn new(session: PaymentSession, policy: Policy, vendor: RistrettoPublicKeyBytes) -> Self {
        Self {
            session,
            policy,
            vendor,
        }
    }

    pub fn session(&self) -> &PaymentSession {
        &self.session
    }

    pub fn pay(
        &mut self,
        quote: &PaymentRequired,
        secret: &RistrettoSecretKey,
    ) -> Result<SignedVoucher, AgentError> {
        self.policy
            .approve(&self.vendor, quote.price, self.session.cumulative())?;
        Ok(self.session.sign_next(secret, quote.price)?)
    }

    pub fn voucher_header(
        &mut self,
        quote: &PaymentRequired,
        secret: &RistrettoSecretKey,
    ) -> Result<String, AgentError> {
        Ok(encode_voucher_header(&self.pay(quote, secret)?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ghostline_core::{ChannelParams, NONCE_LEN};
    use ootle_byte_type::ToByteType;
    use tari_ootle_common_types::crypto::create_key_pair_from_seed;
    use tari_template_lib::types::Hash32;

    #[test]
    fn policy_blocks_signing_above_budget() {
        let (secret, public) = create_key_pair_from_seed(11);
        let payer = public.to_byte_type();
        let params = ChannelParams {
            network_tag: 1,
            payer,
            deposit: 100,
            payer_root: Hash32::from_array([9u8; 32]),
            nonce: [3u8; NONCE_LEN],
        };
        let session = PaymentSession::new(params.clone());
        let policy = Policy::new(15, 10);
        let mut agent = AgentPayer::new(session, policy, payer);
        let quote = PaymentRequired::new(10, &params.channel_id());
        agent.pay(&quote, &secret).unwrap();
        let err = agent.pay(&quote, &secret).unwrap_err();
        assert!(matches!(err, AgentError::Policy(_)));
    }
}
