// SPDX-License-Identifier: MIT

use std::fs;
use std::path::{Path, PathBuf};

use ghostline_core::{
    CHANNEL_ID_LEN, ChannelParams, NONCE_LEN, PaymentSession, ServiceLedger, VoucherError,
    from_hex, to_hex,
};
use serde::{Deserialize, Serialize};
use tari_template_lib::types::{Hash32, crypto::RistrettoPublicKeyBytes};

use crate::PaywallError;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct ChannelSnapshot {
    network_tag: u8,
    payer_hex: String,
    deposit: u64,
    payer_root_hex: String,
    nonce_hex: String,
    accepted: u64,
    cumulative: u64,
}

impl ChannelSnapshot {
    fn from_params(params: &ChannelParams, accepted: u64, cumulative: u64) -> Self {
        Self {
            network_tag: params.network_tag,
            payer_hex: to_hex(params.payer.as_bytes()),
            deposit: params.deposit,
            payer_root_hex: to_hex(params.payer_root.as_slice()),
            nonce_hex: to_hex(&params.nonce),
            accepted,
            cumulative,
        }
    }

    fn params(&self) -> Result<ChannelParams, PaywallError> {
        let payer = RistrettoPublicKeyBytes::from_hex(&self.payer_hex)
            .map_err(|_| PaywallError::Voucher(VoucherError::Malformed))?;
        let payer_root = Hash32::from_hex(&self.payer_root_hex)
            .map_err(|_| PaywallError::Voucher(VoucherError::Malformed))?;
        let nonce_bytes = from_hex(&self.nonce_hex)
            .map_err(|_| PaywallError::Voucher(VoucherError::Malformed))?;
        let nonce: [u8; NONCE_LEN] = nonce_bytes
            .try_into()
            .map_err(|_| PaywallError::Voucher(VoucherError::Malformed))?;
        Ok(ChannelParams {
            network_tag: self.network_tag,
            payer,
            deposit: self.deposit,
            payer_root,
            nonce,
        })
    }
}

/// JSON file holding one channel's params plus the service accepted total and payer cumulative.
pub struct ChannelStore {
    path: PathBuf,
    snapshot: ChannelSnapshot,
}

impl ChannelStore {
    pub fn create(path: impl Into<PathBuf>, params: ChannelParams) -> Result<Self, PaywallError> {
        let path = path.into();
        let snapshot = ChannelSnapshot::from_params(&params, 0, 0);
        let store = Self { path, snapshot };
        store.flush()?;
        Ok(store)
    }

    pub fn open(path: impl AsRef<Path>) -> Result<Self, PaywallError> {
        let path = path.as_ref().to_path_buf();
        let bytes = fs::read(&path).map_err(|e| PaywallError::Storage(e.to_string()))?;
        let snapshot: ChannelSnapshot = serde_json::from_slice(&bytes)
            .map_err(|_| PaywallError::Voucher(VoucherError::Malformed))?;
        let params = snapshot.params()?;
        ServiceLedger::resume(params.clone(), snapshot.accepted)?;
        PaymentSession::resume(params, snapshot.cumulative)?;
        Ok(Self { path, snapshot })
    }

    pub fn params(&self) -> Result<ChannelParams, PaywallError> {
        self.snapshot.params()
    }

    pub fn channel_id(&self) -> Result<[u8; CHANNEL_ID_LEN], PaywallError> {
        Ok(self.params()?.channel_id())
    }

    pub fn ledger(&self) -> Result<ServiceLedger, PaywallError> {
        ServiceLedger::resume(self.params()?, self.snapshot.accepted).map_err(PaywallError::from)
    }

    pub fn session(&self) -> Result<PaymentSession, PaywallError> {
        PaymentSession::resume(self.params()?, self.snapshot.cumulative).map_err(PaywallError::from)
    }

    pub fn save_ledger(&mut self, ledger: &ServiceLedger) -> Result<(), PaywallError> {
        if ledger.params() != &self.params()? {
            return Err(PaywallError::Voucher(VoucherError::ChannelMismatch));
        }
        self.snapshot.accepted = ledger.accepted();
        self.flush()
    }

    pub fn save_session(&mut self, session: &PaymentSession) -> Result<(), PaywallError> {
        if session.params() != &self.params()? {
            return Err(PaywallError::Voucher(VoucherError::ChannelMismatch));
        }
        self.snapshot.cumulative = session.cumulative();
        self.flush()
    }

    fn flush(&self) -> Result<(), PaywallError> {
        let json = serde_json::to_vec_pretty(&self.snapshot)
            .map_err(|_| PaywallError::Voucher(VoucherError::Malformed))?;
        let tmp = self.path.with_extension("json.tmp");
        fs::write(&tmp, json).map_err(|e| PaywallError::Storage(e.to_string()))?;
        fs::rename(&tmp, &self.path).map_err(|e| PaywallError::Storage(e.to_string()))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ghostline_core::NONCE_LEN;
    use tari_template_lib::types::crypto::RistrettoPublicKeyBytes;

    fn params() -> ChannelParams {
        ChannelParams {
            network_tag: 1,
            payer: RistrettoPublicKeyBytes::from_bytes(&[7u8; 32]).unwrap(),
            deposit: 100,
            payer_root: Hash32::from_array([9u8; 32]),
            nonce: [3u8; NONCE_LEN],
        }
    }

    #[test]
    fn round_trips_accepted_and_cumulative() {
        let dir = std::env::temp_dir().join(format!("ghostline-store-{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        let path = dir.join("channel.json");
        let _ = fs::remove_file(&path);
        let mut store = ChannelStore::create(&path, params()).unwrap();
        store.snapshot.accepted = 30;
        store.snapshot.cumulative = 30;
        store.flush().unwrap();
        let loaded = ChannelStore::open(&path).unwrap();
        assert_eq!(loaded.ledger().unwrap().accepted(), 30);
        assert_eq!(loaded.session().unwrap().cumulative(), 30);
    }
}
