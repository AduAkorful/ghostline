// SPDX-License-Identifier: MIT

//! One-time stealth destinations that a wallet daemon recognises as its own.
//!
//! walletd addresses its own transfers by encrypting the output data to the recipient's view key and committing a
//! one-time spend key `H(r·V)·G + A` derived from the recipient's address. Its scanner finds such an output with the
//! view secret alone and derives the matching spend secret. An output encrypted to a bare spend key, as Ghostline did
//! before, is never found, so its value is stranded until custom tooling recovers it (seen on mainnet, plan 40).

use std::fmt;
use std::str::FromStr;

use ootle_byte_type::ToByteType;
use ootle_network::Network;
use tari_crypto::{
    keys::{PublicKey as _, SecretKey as _},
    ristretto::{RistrettoPublicKey, RistrettoSecretKey},
};
use tari_ootle_address::OotleAddress;
use tari_ootle_wallet_crypto::StealthCryptoApi;
use tari_template_lib::types::crypto::RistrettoPublicKeyBytes;
use tari_utilities::ByteArray;

use crate::SdkError;

/// A wallet address plus the sender nonce `r` for one output to it.
///
/// The nonce is secret: with the view public key it recomputes the output's encryption key, so anyone holding it can
/// read that output's value and mask. Keep it with the channel's service state.
#[derive(Clone, PartialEq, Eq)]
pub struct WalletDestination {
    network: Network,
    account: RistrettoPublicKey,
    view: RistrettoPublicKey,
    nonce: RistrettoSecretKey,
}

impl WalletDestination {
    pub fn new(
        network: Network,
        account: RistrettoPublicKeyBytes,
        view: RistrettoPublicKeyBytes,
        nonce: RistrettoSecretKey,
    ) -> Result<Self, SdkError> {
        Ok(Self {
            network,
            account: public_key(&account)?,
            view: public_key(&view)?,
            nonce,
        })
    }

    /// Parses an `otl_…` address and draws a fresh nonce. The address's network must match `network`, so a testnet
    /// address can never receive a mainnet payout.
    pub fn random_for_address(address: &str, network: Network) -> Result<Self, SdkError> {
        let parsed =
            OotleAddress::from_str(address.trim()).map_err(|e| SdkError::Address(e.to_string()))?;
        if parsed.network() != network {
            return Err(SdkError::Address(format!(
                "address is for {} but the channel is on {}",
                parsed.network(),
                network
            )));
        }
        Self::new(
            network,
            *parsed.account_public_key(),
            *parsed.view_only_key(),
            RistrettoSecretKey::random(&mut rand::rng()),
        )
    }

    pub fn network(&self) -> Network {
        self.network
    }

    pub fn account_key(&self) -> RistrettoPublicKeyBytes {
        self.account.to_byte_type()
    }

    pub fn view_key(&self) -> RistrettoPublicKeyBytes {
        self.view.to_byte_type()
    }

    pub fn nonce(&self) -> &RistrettoSecretKey {
        &self.nonce
    }

    pub(crate) fn view_public_key(&self) -> &RistrettoPublicKey {
        &self.view
    }

    pub(crate) fn public_nonce(&self) -> RistrettoPublicKey {
        RistrettoPublicKey::from_secret_key(&self.nonce)
    }

    /// The one-time spend key `H(r·V)·G + A` the output must be locked to. On a settle this is the voucher's
    /// `service_output_key`, so the service advertises it in its quote.
    pub fn owner_key(&self) -> RistrettoPublicKeyBytes {
        StealthCryptoApi::new()
            .derive_stealth_owner_public_key(self.network, &self.account, &self.view, &self.nonce)
            .to_byte_type()
    }
}

impl fmt::Debug for WalletDestination {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WalletDestination")
            .field("network", &self.network)
            .field("account", &self.account_key())
            .field("view", &self.view_key())
            .field("nonce", &"<redacted>")
            .finish()
    }
}

fn public_key(bytes: &RistrettoPublicKeyBytes) -> Result<RistrettoPublicKey, SdkError> {
    RistrettoPublicKey::from_canonical_bytes(bytes.as_bytes()).map_err(|_| SdkError::InvalidKey)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keypair(seed: u8) -> (RistrettoSecretKey, RistrettoPublicKey) {
        let secret = RistrettoSecretKey::from_uniform_bytes(&[seed; 64]).unwrap();
        let public = RistrettoPublicKey::from_secret_key(&secret);
        (secret, public)
    }

    #[test]
    fn owner_key_matches_what_the_recipient_derives_from_its_view_secret() {
        let (_, account) = keypair(1);
        let (view_secret, view) = keypair(2);
        let destination = WalletDestination::new(
            Network::MainNet,
            account.to_byte_type(),
            view.to_byte_type(),
            keypair(3).0,
        )
        .unwrap();
        let receiver_side = StealthCryptoApi::new().derive_stealth_owner_public_key_from_view_key(
            Network::MainNet,
            &account,
            &view_secret,
            &destination.public_nonce(),
        );
        assert_eq!(destination.owner_key(), receiver_side.to_byte_type());
    }

    #[test]
    fn address_round_trip_and_network_guard() {
        let (_, account) = keypair(4);
        let (_, view) = keypair(5);
        let address = OotleAddress::new(
            Network::MainNet,
            view.to_byte_type(),
            account.to_byte_type(),
        )
        .to_bech32_string();
        let destination =
            WalletDestination::random_for_address(&address, Network::MainNet).unwrap();
        assert_eq!(destination.account_key(), account.to_byte_type());
        assert_eq!(destination.view_key(), view.to_byte_type());

        let err = WalletDestination::random_for_address(&address, Network::Esmeralda).unwrap_err();
        assert!(err.to_string().contains("mainnet"), "{err}");
    }

    #[test]
    fn debug_never_prints_the_nonce() {
        let (_, account) = keypair(6);
        let (_, view) = keypair(7);
        let nonce = keypair(8).0;
        let destination = WalletDestination::new(
            Network::MainNet,
            account.to_byte_type(),
            view.to_byte_type(),
            nonce.clone(),
        )
        .unwrap();
        let printed = format!("{destination:?}");
        assert!(printed.contains("<redacted>"));
        assert!(!printed.contains(&tari_utilities::hex::Hex::to_hex(&nonce)));
    }
}
