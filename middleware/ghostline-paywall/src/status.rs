// SPDX-License-Identifier: MIT

use ghostline_core::to_hex;
use serde::Serialize;

use crate::{ChannelStore, PaywallError};

/// Off-chain view of a channel store. Not a chain confirmation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ChannelStatus {
    /// Always `off_chain_store`. Remainder is hypothetical at these totals, not a UTXO balance.
    pub source: &'static str,
    pub channel_id: String,
    pub deposit: u64,
    pub accepted: u64,
    pub cumulative: u64,
    pub remainder_at_accepted: u64,
    pub remainder_at_cumulative: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expiry_epoch: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_epoch: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub epochs_until_expiry: Option<i64>,
    pub settle_window: SettleWindow,
    pub observer: ObserverView,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SettleWindow {
    Unknown,
    /// Current epoch is strictly before expiry (`BeforeEpoch` still holds).
    SettleOpen,
    /// Current epoch is at or after expiry (`AfterEpoch` holds; live settle is `OnlyFeeAccepted`).
    RefundOpen,
}

/// What a public observer can see. Fees are always public; call pattern is not, if you settle once.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ObserverView {
    pub now: Vec<&'static str>,
    pub at_settle: Vec<&'static str>,
}

impl ObserverView {
    pub fn default_panel() -> Self {
        Self {
            now: vec![
                "transaction fees and the account component that paid them",
                "that a stealth output exists (open), if you can find it",
            ],
            at_settle: vec![
                "the settle branch",
                "the voucher amount",
                "the remainder (which reveals the deposit)",
                "the channel id bound into the template",
                "the service key on that leaf",
            ],
        }
    }
}

impl ChannelStatus {
    pub fn from_store(
        store: &ChannelStore,
        expiry_epoch: Option<u64>,
        current_epoch: Option<u64>,
    ) -> Result<Self, PaywallError> {
        Self::from_totals(
            to_hex(&store.channel_id()?),
            store.params()?.deposit,
            store.accepted(),
            store.cumulative(),
            expiry_epoch,
            current_epoch,
        )
    }

    /// Service ledger supplies `accepted`; payer session supplies `cumulative`.
    pub fn from_session_and_ledger(
        session: &ChannelStore,
        ledger: &ChannelStore,
        expiry_epoch: Option<u64>,
        current_epoch: Option<u64>,
    ) -> Result<Self, PaywallError> {
        if session.channel_id()? != ledger.channel_id()? {
            return Err(PaywallError::Voucher(
                ghostline_core::VoucherError::ChannelMismatch,
            ));
        }
        Self::from_totals(
            to_hex(&session.channel_id()?),
            session.params()?.deposit,
            ledger.accepted(),
            session.cumulative(),
            expiry_epoch,
            current_epoch,
        )
    }

    fn from_totals(
        channel_id: String,
        deposit: u64,
        accepted: u64,
        cumulative: u64,
        expiry_epoch: Option<u64>,
        current_epoch: Option<u64>,
    ) -> Result<Self, PaywallError> {
        let remainder_at_accepted = deposit.checked_sub(accepted).ok_or(PaywallError::Voucher(
            ghostline_core::VoucherError::ExceedsDeposit {
                amount: accepted,
                deposit,
            },
        ))?;
        let remainder_at_cumulative =
            deposit
                .checked_sub(cumulative)
                .ok_or(PaywallError::Voucher(
                    ghostline_core::VoucherError::ExceedsDeposit {
                        amount: cumulative,
                        deposit,
                    },
                ))?;
        let epochs_until_expiry = match (expiry_epoch, current_epoch) {
            (Some(exp), Some(cur)) => Some(exp as i64 - cur as i64),
            _ => None,
        };
        let settle_window = match (expiry_epoch, current_epoch) {
            (Some(exp), Some(cur)) if cur < exp => SettleWindow::SettleOpen,
            (Some(exp), Some(cur)) if cur >= exp => SettleWindow::RefundOpen,
            _ => SettleWindow::Unknown,
        };
        Ok(Self {
            source: "off_chain_store",
            channel_id,
            deposit,
            accepted,
            cumulative,
            remainder_at_accepted,
            remainder_at_cumulative,
            expiry_epoch,
            current_epoch,
            epochs_until_expiry,
            settle_window,
            observer: ObserverView::default_panel(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ghostline_core::{ChannelParams, NONCE_LEN};
    use tari_template_lib::types::{Hash32, crypto::RistrettoPublicKeyBytes};

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
    fn remainder_and_window() {
        let dir = std::env::temp_dir().join(format!("ghostline-status-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("channel.json");
        let _ = std::fs::remove_file(&path);
        let mut store = ChannelStore::create(&path, params()).unwrap();
        store.set_totals(40, 40).unwrap();
        let view = ChannelStatus::from_store(&store, Some(50), Some(40)).unwrap();
        assert_eq!(view.remainder_at_accepted, 60);
        assert_eq!(view.settle_window, SettleWindow::SettleOpen);
        let late = ChannelStatus::from_store(&store, Some(50), Some(50)).unwrap();
        assert_eq!(late.settle_window, SettleWindow::RefundOpen);
        assert_eq!(late.epochs_until_expiry, Some(0));
    }
}
