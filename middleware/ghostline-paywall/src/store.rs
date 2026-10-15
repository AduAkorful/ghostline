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
    /// Latest accepted voucher hex. Absent on stores written before this field existed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    latest_voucher_hex: Option<String>,
    /// Each new charge. Empty on stores written before this field existed; those totals are not backfilled.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    calls: Vec<AcceptedCall>,
}

/// One accepted charge. The price is the delta; `accepted` is the cumulative total after it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AcceptedCall {
    pub price: u64,
    pub accepted: u64,
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
            latest_voucher_hex: None,
            calls: Vec::new(),
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

    pub fn open_or_create(
        path: impl Into<PathBuf>,
        params: ChannelParams,
    ) -> Result<Self, PaywallError> {
        let path = path.into();
        if path.exists() {
            let existing = Self::open(&path)?;
            if existing.params()? != params {
                return Err(PaywallError::Voucher(VoucherError::ChannelMismatch));
            }
            Ok(existing)
        } else {
            if let Some(parent) = path.parent()
                && !parent.as_os_str().is_empty()
            {
                fs::create_dir_all(parent).map_err(|e| PaywallError::Storage(e.to_string()))?;
            }
            Self::create(path, params)
        }
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

    pub fn accepted(&self) -> u64 {
        self.snapshot.accepted
    }

    pub fn cumulative(&self) -> u64 {
        self.snapshot.cumulative
    }

    pub fn latest_voucher_hex(&self) -> Option<&str> {
        self.snapshot.latest_voucher_hex.as_deref()
    }

    pub fn calls(&self) -> &[AcceptedCall] {
        &self.snapshot.calls
    }

    /// Append one new charge. `price` is the delta from [`ServiceLedger::accept`] (a replay is `0` and is ignored).
    ///
    /// When the log is empty, a price at or below the current accepted total is stored as this call only.
    /// Earlier charges on an older ledger are not invented.
    pub fn record_call(&mut self, price: u64) -> Result<(), PaywallError> {
        if price == 0 {
            return Ok(());
        }
        if let Some(last) = self.snapshot.calls.last() {
            let next = last
                .accepted
                .checked_add(price)
                .ok_or_else(|| PaywallError::Storage("call price overflow".into()))?;
            if next != self.snapshot.accepted {
                return Err(PaywallError::Storage(format!(
                    "call price {price} does not advance accepted from {} to {}",
                    last.accepted, self.snapshot.accepted
                )));
            }
        } else if price > self.snapshot.accepted {
            return Err(PaywallError::Storage(format!(
                "call price {price} exceeds accepted {}",
                self.snapshot.accepted
            )));
        }
        self.snapshot.calls.push(AcceptedCall {
            price,
            accepted: self.snapshot.accepted,
        });
        self.flush()
    }

    /// Record the voucher that matches `accepted`. The hex is a bearer credential; do not print it.
    pub fn remember_voucher(&mut self, voucher_hex: &str) -> Result<(), PaywallError> {
        let voucher = crate::decode_voucher_header(voucher_hex)?;
        let params = self.params()?;
        voucher.verify(&params)?;
        if voucher.amount != self.snapshot.accepted {
            return Err(PaywallError::Storage(format!(
                "voucher amount {} does not match accepted {}",
                voucher.amount, self.snapshot.accepted
            )));
        }
        self.snapshot.latest_voucher_hex = Some(voucher_hex.trim().to_string());
        self.flush()
    }

    #[cfg(test)]
    pub(crate) fn set_totals(
        &mut self,
        accepted: u64,
        cumulative: u64,
    ) -> Result<(), PaywallError> {
        self.snapshot.accepted = accepted;
        self.snapshot.cumulative = cumulative;
        self.flush()
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

    pub fn path(&self) -> &Path {
        &self.path
    }
}

/// Ledgers for more than one channel. A voucher is charged on the ledger with the same channel id.
pub struct ChannelDirectory {
    stores: Vec<ChannelStore>,
}

impl ChannelDirectory {
    pub fn open(dir: impl AsRef<Path>) -> Result<Self, PaywallError> {
        let dir = dir.as_ref();
        let mut paths: Vec<PathBuf> = fs::read_dir(dir)
            .map_err(|e| PaywallError::Storage(e.to_string()))?
            .filter_map(|entry| entry.ok().map(|e| e.path()))
            .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("json"))
            .collect();
        paths.sort();
        let mut stores = Vec::new();
        for path in paths {
            let Ok(store) = ChannelStore::open(&path) else {
                continue;
            };
            let id = store.channel_id()?;
            if stores
                .iter()
                .any(|existing: &ChannelStore| existing.channel_id().ok().as_ref() == Some(&id))
            {
                return Err(PaywallError::Storage(format!(
                    "duplicate channel {}",
                    to_hex(&id)
                )));
            }
            stores.push(store);
        }
        Ok(Self { stores })
    }

    pub fn len(&self) -> usize {
        self.stores.len()
    }

    pub fn is_empty(&self) -> bool {
        self.stores.is_empty()
    }

    pub fn first_params(&self) -> Result<ChannelParams, PaywallError> {
        self.stores
            .first()
            .ok_or_else(|| PaywallError::Storage("no channel ledgers".into()))?
            .params()
    }

    /// The channel a 402 should name. One ledger is implied. Several require `requested` (hex channel id).
    pub fn channel_for_quote(
        &self,
        requested: Option<&str>,
    ) -> Result<[u8; CHANNEL_ID_LEN], PaywallError> {
        if let Some(hex) = requested.filter(|value| !value.is_empty()) {
            let bytes =
                from_hex(hex).map_err(|_| PaywallError::Voucher(VoucherError::Malformed))?;
            let id: [u8; CHANNEL_ID_LEN] = bytes
                .try_into()
                .map_err(|_| PaywallError::Voucher(VoucherError::Malformed))?;
            if self.store_index(&id).is_none() {
                return Err(PaywallError::UnknownChannel);
            }
            return Ok(id);
        }
        match self.stores.len() {
            0 => Err(PaywallError::Storage("no channel ledgers".into())),
            1 => self.stores[0].channel_id(),
            _ => Err(PaywallError::AmbiguousChannel),
        }
    }

    pub fn path_for_channel(&self, channel_id_hex: &str) -> Result<&Path, PaywallError> {
        let id = self.channel_for_quote(Some(channel_id_hex))?;
        let index = self.store_index(&id).ok_or(PaywallError::UnknownChannel)?;
        Ok(self.stores[index].path())
    }

    pub fn accepted(&self, channel_id: &[u8; CHANNEL_ID_LEN]) -> Result<u64, PaywallError> {
        let index = self
            .store_index(channel_id)
            .ok_or(PaywallError::UnknownChannel)?;
        Ok(self.stores[index].accepted())
    }

    /// Charge `price` on the ledger named by the voucher. Other ledgers stay unchanged.
    pub fn charge(&mut self, price: u64, voucher_hex: Option<&str>) -> Result<u64, PaywallError> {
        let Some(hex) = voucher_hex.filter(|value| !value.is_empty()) else {
            return Err(PaywallError::MissingVoucher);
        };
        let voucher = crate::decode_voucher_header(hex)?;
        let index = self
            .store_index(&voucher.channel_id)
            .ok_or(PaywallError::UnknownChannel)?;
        let mut ledger = self.stores[index].ledger()?;
        let delta = crate::charge(&mut ledger, price, Some(hex))?;
        self.stores[index].save_ledger(&ledger)?;
        self.stores[index].record_call(delta)?;
        if delta > 0 {
            self.stores[index].remember_voucher(hex)?;
        }
        Ok(delta)
    }

    fn store_index(&self, channel_id: &[u8; CHANNEL_ID_LEN]) -> Option<usize> {
        self.stores
            .iter()
            .position(|store| store.channel_id().ok().as_ref() == Some(channel_id))
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
        assert!(loaded.latest_voucher_hex().is_none());
    }

    #[test]
    fn old_snapshot_without_voucher_still_opens() {
        let dir = std::env::temp_dir().join(format!("ghostline-store-old-{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        let path = dir.join("old.json");
        let body = format!(
            r#"{{"network_tag":1,"payer_hex":"{}","deposit":100,"payer_root_hex":"{}","nonce_hex":"{}","accepted":0,"cumulative":0}}"#,
            to_hex(&[7u8; 32]),
            to_hex(&[9u8; 32]),
            to_hex(&[3u8; NONCE_LEN])
        );
        fs::write(&path, body).unwrap();
        let loaded = ChannelStore::open(&path).unwrap();
        assert_eq!(loaded.accepted(), 0);
        assert!(loaded.latest_voucher_hex().is_none());
        assert!(loaded.calls().is_empty());
    }

    #[test]
    fn record_call_appends_delta_and_ignores_replay() {
        let dir = std::env::temp_dir().join(format!("ghostline-calls-{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        let path = dir.join("calls.json");
        let _ = fs::remove_file(&path);
        let mut store = ChannelStore::create(&path, params()).unwrap();
        store.set_totals(10, 10).unwrap();
        store.record_call(10).unwrap();
        store.record_call(0).unwrap();
        store.set_totals(30, 30).unwrap();
        store.record_call(20).unwrap();
        let loaded = ChannelStore::open(&path).unwrap();
        assert_eq!(
            loaded.calls(),
            &[
                AcceptedCall {
                    price: 10,
                    accepted: 10
                },
                AcceptedCall {
                    price: 20,
                    accepted: 30
                }
            ]
        );
        store.record_call(5).unwrap_err();
    }

    #[test]
    fn directory_charges_only_the_matching_channel() {
        use ghostline_core::SignedVoucher;
        use ootle_byte_type::ToByteType;
        use tari_ootle_common_types::crypto::create_key_pair_from_seed;

        let (secret, public) = create_key_pair_from_seed(11);
        let payer = public.to_byte_type();
        let mut first = params();
        first.payer = payer;
        let mut second = first.clone();
        second.nonce = [4u8; NONCE_LEN];
        let dir = std::env::temp_dir().join(format!(
            "ghostline-dir-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        ChannelStore::create(dir.join("a.json"), first.clone()).unwrap();
        ChannelStore::create(dir.join("b.json"), second.clone()).unwrap();
        fs::write(dir.join("notes.json"), b"{\"opener_account\":\"skip\"}").unwrap();
        let mut directory = ChannelDirectory::open(&dir).unwrap();
        assert_eq!(directory.len(), 2);
        assert!(directory.channel_for_quote(None).is_err());
        let quoted = directory
            .channel_for_quote(Some(&to_hex(&second.channel_id())))
            .unwrap();
        assert_eq!(quoted, second.channel_id());
        let voucher = SignedVoucher::sign(&secret, &second, 10).unwrap();
        let hex = crate::encode_voucher_header(&voucher);
        assert_eq!(directory.charge(10, Some(&hex)).unwrap(), 10);
        assert_eq!(directory.accepted(&first.channel_id()).unwrap(), 0);
        assert_eq!(directory.accepted(&second.channel_id()).unwrap(), 10);
        let mut unknown = first.clone();
        unknown.nonce = [5u8; NONCE_LEN];
        let foreign = SignedVoucher::sign(&secret, &unknown, 10).unwrap();
        let err = directory
            .charge(10, Some(&crate::encode_voucher_header(&foreign)))
            .unwrap_err();
        assert!(matches!(err, PaywallError::UnknownChannel));
        let voucher = SignedVoucher::sign(&secret, &first, 10).unwrap();
        assert_eq!(
            directory
                .charge(10, Some(&crate::encode_voucher_header(&voucher)))
                .unwrap(),
            10
        );
        assert_eq!(directory.accepted(&second.channel_id()).unwrap(), 10);
    }
}
