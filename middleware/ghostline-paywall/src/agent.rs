// SPDX-License-Identifier: MIT

use std::time::Instant;

use ghostline_core::{PaymentSession, SignedVoucher};
use ghostline_policy::{Policy, PolicyEngine};
use tari_crypto::ristretto::RistrettoSecretKey;
use tari_template_lib::types::crypto::RistrettoPublicKeyBytes;

use crate::{AgentError, PaymentRequired, encode_voucher_header};

/// Signs the next voucher for a 402 quote, after the policy engine approves it.
pub struct AgentPayer {
    session: PaymentSession,
    engine: PolicyEngine,
    vendor: RistrettoPublicKeyBytes,
}

impl AgentPayer {
    pub fn new(session: PaymentSession, policy: Policy, vendor: RistrettoPublicKeyBytes) -> Self {
        Self::with_engine(session, PolicyEngine::new(policy), vendor)
    }

    pub fn with_engine(
        session: PaymentSession,
        engine: PolicyEngine,
        vendor: RistrettoPublicKeyBytes,
    ) -> Self {
        Self {
            session,
            engine,
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
        self.engine.approve(
            &self.vendor,
            quote.price,
            self.session.cumulative(),
            Instant::now(),
        )?;
        Ok(self.session.sign_next(secret, quote.price)?)
    }

    /// Operator has already confirmed this quote. Use when the engine was built with `require_human`.
    pub fn pay_attested(
        &mut self,
        quote: &PaymentRequired,
        secret: &RistrettoSecretKey,
    ) -> Result<SignedVoucher, AgentError> {
        self.engine.approve_attested(
            &self.vendor,
            quote.price,
            self.session.cumulative(),
            Instant::now(),
        )?;
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

    #[test]
    fn human_gate_requires_attestation() {
        use ghostline_policy::PolicyError;
        let (secret, public) = create_key_pair_from_seed(11);
        let payer = public.to_byte_type();
        let params = ChannelParams {
            network_tag: 1,
            payer,
            deposit: 100,
            payer_root: Hash32::from_array([9u8; 32]),
            nonce: [3u8; NONCE_LEN],
        };
        let engine = PolicyEngine::new(Policy::new(100, 10)).require_human();
        let mut agent = AgentPayer::with_engine(PaymentSession::new(params.clone()), engine, payer);
        let quote = PaymentRequired::new(10, &params.channel_id());
        assert!(matches!(
            agent.pay(&quote, &secret),
            Err(AgentError::Policy(PolicyError::NeedsHumanApproval { .. }))
        ));
        agent.pay_attested(&quote, &secret).unwrap();
    }
}
