// SPDX-License-Identifier: MIT

use std::time::Instant;

use ghostline_core::{PaymentSession, SignedVoucher};
use ghostline_policy::{Policy, PolicyEngine};
use tari_crypto::ristretto::RistrettoSecretKey;
use tari_template_lib::types::crypto::RistrettoPublicKeyBytes;

use crate::{AgentError, ChannelStore, PaymentRequired, encode_voucher_header};

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

    pub fn into_engine(self) -> PolicyEngine {
        self.engine
    }

    pub fn pay(
        &mut self,
        quote: &PaymentRequired,
        secret: &RistrettoSecretKey,
    ) -> Result<SignedVoucher, AgentError> {
        self.validate_quote(quote)?;
        self.engine.approve(
            &self.vendor,
            quote.price,
            self.session.cumulative(),
            Instant::now(),
        )?;
        let output_key = quote.service_output_public_key()?.unwrap_or(self.vendor);
        Ok(self.session.sign_next(secret, quote.price, output_key)?)
    }

    /// Operator has already confirmed this quote. Use when the engine was built with `require_human`.
    pub fn pay_attested(
        &mut self,
        quote: &PaymentRequired,
        secret: &RistrettoSecretKey,
    ) -> Result<SignedVoucher, AgentError> {
        self.validate_quote(quote)?;
        self.engine.approve_attested(
            &self.vendor,
            quote.price,
            self.session.cumulative(),
            Instant::now(),
        )?;
        let output_key = quote.service_output_public_key()?.unwrap_or(self.vendor);
        Ok(self.session.sign_next(secret, quote.price, output_key)?)
    }

    fn validate_quote(&self, quote: &PaymentRequired) -> Result<(), AgentError> {
        quote.validate_for(&self.session.params().channel_id(), &self.vendor)?;
        Ok(())
    }

    pub fn pay_price(
        &mut self,
        price: u64,
        channel_id: &[u8; ghostline_core::CHANNEL_ID_LEN],
        secret: &RistrettoSecretKey,
        attested: bool,
    ) -> Result<SignedVoucher, AgentError> {
        let quote = PaymentRequired::new(price, channel_id);
        if attested {
            self.pay_attested(&quote, secret)
        } else {
            self.pay(&quote, secret)
        }
    }

    pub fn voucher_header(
        &mut self,
        quote: &PaymentRequired,
        secret: &RistrettoSecretKey,
    ) -> Result<String, AgentError> {
        Ok(encode_voucher_header(&self.pay(quote, secret)?))
    }
}

/// Approve `price` against the stored cumulative total, sign the next voucher, persist the session.
pub fn gated_sign(
    store: &mut ChannelStore,
    engine: PolicyEngine,
    vendor: RistrettoPublicKeyBytes,
    price: u64,
    secret: &RistrettoSecretKey,
    attested: bool,
) -> Result<(SignedVoucher, PolicyEngine), AgentError> {
    let mut payer = AgentPayer::with_engine(store.session()?, engine, vendor);
    let channel_id = store.channel_id()?;
    let voucher = payer.pay_price(price, &channel_id, secret, attested)?;
    store.save_session(payer.session())?;
    Ok((voucher, payer.into_engine()))
}

/// Sign a quote while keeping policy tied to the channel's service identity and paying the
/// optional one-time output key advertised by that service.
pub fn gated_sign_quote(
    store: &mut ChannelStore,
    engine: PolicyEngine,
    vendor: RistrettoPublicKeyBytes,
    quote: &PaymentRequired,
    secret: &RistrettoSecretKey,
    attested: bool,
) -> Result<(SignedVoucher, PolicyEngine), AgentError> {
    let mut payer = AgentPayer::with_engine(store.session()?, engine, vendor);
    let voucher = if attested {
        payer.pay_attested(quote, secret)?
    } else {
        payer.pay(quote, secret)?
    };
    store.save_session(payer.session())?;
    Ok((voucher, payer.into_engine()))
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

    #[test]
    fn quote_channel_and_recipient_must_match_before_signing() {
        use crate::{PaymentTerms, PaywallError};

        let (secret, public) = create_key_pair_from_seed(11);
        let payer = public.to_byte_type();
        let (_, other_public) = create_key_pair_from_seed(12);
        let other = other_public.to_byte_type();
        let params = ChannelParams {
            network_tag: 1,
            payer,
            deposit: 100,
            payer_root: Hash32::from_array([9u8; 32]),
            nonce: [3u8; NONCE_LEN],
        };
        let mut agent = AgentPayer::new(
            PaymentSession::new(params.clone()),
            Policy::new(100, 10),
            payer,
        );

        let bad_channel = PaymentRequired::new(10, &[0xabu8; 32]);
        assert!(matches!(
            agent.pay(&bad_channel, &secret),
            Err(AgentError::Paywall(PaywallError::InvalidQuote(_)))
        ));

        let wrong_recipient = PaymentRequired::new(10, &params.channel_id())
            .with_terms(&PaymentTerms::new(other, 20, 100).unwrap());
        assert!(matches!(
            agent.pay(&wrong_recipient, &secret),
            Err(AgentError::Paywall(PaywallError::InvalidQuote(_)))
        ));
        assert_eq!(agent.session().cumulative(), 0);
    }

    #[test]
    fn quote_can_route_the_settlement_to_a_distinct_one_time_key() {
        use crate::PaymentTerms;

        let (secret, payer_public) = create_key_pair_from_seed(11);
        let (_, payout_public) = create_key_pair_from_seed(12);
        let payer = payer_public.to_byte_type();
        let vendor = payer;
        let params = ChannelParams {
            network_tag: 1,
            payer,
            deposit: 100,
            payer_root: Hash32::from_array([9u8; 32]),
            nonce: [3u8; NONCE_LEN],
        };
        let mut agent = AgentPayer::new(
            PaymentSession::new(params.clone()),
            Policy::new(100, 10),
            vendor,
        );
        let terms = PaymentTerms::new(vendor, 20, 100)
            .unwrap()
            .with_service_output_key(payout_public.to_byte_type());
        let quote = PaymentRequired::new(10, &params.channel_id()).with_terms(&terms);
        let voucher = agent.pay(&quote, &secret).unwrap();

        assert_eq!(voucher.service_output_key, payout_public.to_byte_type());
        assert_ne!(voucher.service_output_key, vendor);
        voucher.verify(&params).unwrap();
    }

    #[test]
    fn gated_sign_persists_cumulative() {
        use crate::ChannelStore;
        let (secret, public) = create_key_pair_from_seed(11);
        let payer = public.to_byte_type();
        let params = ChannelParams {
            network_tag: 1,
            payer,
            deposit: 100,
            payer_root: Hash32::from_array([9u8; 32]),
            nonce: [3u8; NONCE_LEN],
        };
        let dir = std::env::temp_dir().join(format!("ghostline-gated-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("channel.json");
        let _ = std::fs::remove_file(&path);
        let mut store = ChannelStore::create(&path, params.clone()).unwrap();
        let engine = PolicyEngine::new(Policy::new(100, 10));
        let (first, engine) = gated_sign(&mut store, engine, payer, 10, &secret, false).unwrap();
        assert_eq!(first.amount, 10);
        let (second, _) = gated_sign(&mut store, engine, payer, 10, &secret, false).unwrap();
        assert_eq!(second.amount, 20);
        let loaded = ChannelStore::open(&path).unwrap();
        assert_eq!(loaded.session().unwrap().cumulative(), 20);
    }
}
