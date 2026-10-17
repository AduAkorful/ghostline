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
    pub epochs_until_expiry: Option<i128>,
    pub settle_window: SettleWindow,
    /// Service has accepted a voucher. This is not a chain confirmation that it was settled.
    pub pending_settlement: bool,
    pub has_latest_voucher: bool,
    pub observer: ObserverView,
}

/// Whether `settle-due` should build a settle. At or after expiry the answer is always [`SettleDecision::Expired`]:
/// a live settle then is `OnlyFeeAccepted` and the channel UTXO stays unspent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum SettleDecision {
    NothingOwed,
    MissingVoucher {
        accepted: u64,
    },
    Wait {
        epochs_until_expiry: u64,
    },
    Due {
        epochs_until_expiry: u64,
        amount: u64,
    },
    Expired {
        epochs_overdue: u64,
    },
}

/// Refund is allowed at the expiry epoch (`AfterEpoch` is inclusive). Before that, do not submit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum RefundDecision {
    Wait { epochs_until_expiry: u64 },
    Due { epochs_overdue: u64 },
}

/// What a settlement watcher should do on this poll. A down output is never settled.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum WatchStep {
    Wait {
        epochs_until_expiry: u64,
    },
    Settle {
        epochs_until_expiry: u64,
        amount: u64,
    },
    Refund {
        epochs_overdue: u64,
    },
    OutputSpent,
    NothingOwed,
    MissingVoucher {
        accepted: u64,
    },
}

pub fn watch_step(
    accepted: u64,
    has_voucher: bool,
    current_epoch: u64,
    expiry_epoch: u64,
    margin_epochs: u64,
    force: bool,
    output_down: bool,
) -> WatchStep {
    if output_down {
        return WatchStep::OutputSpent;
    }
    match decide_settle(
        accepted,
        has_voucher,
        current_epoch,
        expiry_epoch,
        margin_epochs,
        force,
    ) {
        SettleDecision::Due {
            epochs_until_expiry,
            amount,
        } => WatchStep::Settle {
            epochs_until_expiry,
            amount,
        },
        SettleDecision::Wait {
            epochs_until_expiry,
        } => WatchStep::Wait {
            epochs_until_expiry,
        },
        SettleDecision::Expired { epochs_overdue } => WatchStep::Refund { epochs_overdue },
        SettleDecision::NothingOwed => WatchStep::NothingOwed,
        SettleDecision::MissingVoucher { accepted } => WatchStep::MissingVoucher { accepted },
    }
}

pub fn decide_refund(current_epoch: u64, expiry_epoch: u64) -> RefundDecision {
    if current_epoch < expiry_epoch {
        RefundDecision::Wait {
            epochs_until_expiry: expiry_epoch - current_epoch,
        }
    } else {
        RefundDecision::Due {
            epochs_overdue: current_epoch - expiry_epoch,
        }
    }
}

pub fn decide_settle(
    accepted: u64,
    has_voucher: bool,
    current_epoch: u64,
    expiry_epoch: u64,
    margin_epochs: u64,
    force: bool,
) -> SettleDecision {
    if accepted == 0 {
        return SettleDecision::NothingOwed;
    }
    if current_epoch >= expiry_epoch {
        return SettleDecision::Expired {
            epochs_overdue: current_epoch - expiry_epoch,
        };
    }
    if !has_voucher {
        return SettleDecision::MissingVoucher { accepted };
    }
    let epochs_until_expiry = expiry_epoch - current_epoch;
    if force || epochs_until_expiry <= margin_epochs {
        SettleDecision::Due {
            epochs_until_expiry,
            amount: accepted,
        }
    } else {
        SettleDecision::Wait {
            epochs_until_expiry,
        }
    }
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

/// What a public observer can see. This panel describes the live-tested claim-free settlement path.
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
                "not the channel deposit amount (claim-free Esmeralda settlement)",
                "the output commitments and output count",
                "the channel id bound into the template",
                "the payer and service authorization data",
                "fees and timing",
                "not the exact service amount or payer remainder",
            ],
        }
    }
}

#[cfg(test)]
mod observer_tests {
    use super::ObserverView;

    #[test]
    fn panel_reports_live_settlement_disclosures() {
        let panel = ObserverView::default_panel();
        assert!(panel.at_settle.contains(&"not the channel deposit amount (claim-free Esmeralda settlement)"));
        assert!(
            panel
                .at_settle
                .contains(&"not the exact service amount or payer remainder")
        );
        assert!(!panel.at_settle.iter().any(|item| item.contains("revealed in the covenant claim")));
        assert!(!panel.at_settle.contains(&"the voucher amount"));
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
            store.latest_voucher_hex().is_some(),
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
            ledger.latest_voucher_hex().is_some(),
            expiry_epoch,
            current_epoch,
        )
    }

    fn from_totals(
        channel_id: String,
        deposit: u64,
        accepted: u64,
        cumulative: u64,
        has_latest_voucher: bool,
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
            (Some(exp), Some(cur)) => Some(i128::from(exp) - i128::from(cur)),
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
            pending_settlement: accepted > 0,
            has_latest_voucher,
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
        assert!(view.pending_settlement);
        assert!(!view.has_latest_voucher);
    }

    #[test]
    fn expiry_distance_preserves_full_u64_epoch_range() {
        let status = ChannelStatus::from_totals(
            "channel".to_string(),
            100,
            0,
            0,
            false,
            Some(u64::MAX),
            Some(0),
        )
        .unwrap();
        assert_eq!(status.epochs_until_expiry, Some(i128::from(u64::MAX)));
    }

    #[test]
    fn settle_due_refuses_the_expiry_epoch() {
        assert_eq!(
            decide_settle(40, true, 40, 50, 9, false),
            SettleDecision::Wait {
                epochs_until_expiry: 10
            }
        );
        assert!(matches!(
            decide_settle(40, true, 40, 50, 10, true),
            SettleDecision::Due { amount: 40, .. }
        ));
        assert_eq!(
            decide_settle(40, true, 49, 50, 10, false),
            SettleDecision::Due {
                epochs_until_expiry: 1,
                amount: 40
            }
        );
        assert_eq!(
            decide_settle(40, true, 50, 50, 10, true),
            SettleDecision::Expired { epochs_overdue: 0 }
        );
        assert_eq!(
            decide_settle(0, false, 40, 50, 10, true),
            SettleDecision::NothingOwed
        );
        assert_eq!(
            decide_settle(40, false, 40, 50, 10, true),
            SettleDecision::MissingVoucher { accepted: 40 }
        );
    }

    #[test]
    fn watcher_settles_only_inside_the_open_window() {
        assert_eq!(
            watch_step(20, true, 10, 50, 10, false, true),
            WatchStep::OutputSpent
        );
        assert_eq!(
            watch_step(20, true, 10, 50, 10, false, false),
            WatchStep::Wait {
                epochs_until_expiry: 40
            }
        );
        assert_eq!(
            watch_step(20, true, 40, 50, 10, false, false),
            WatchStep::Settle {
                epochs_until_expiry: 10,
                amount: 20
            }
        );
        assert_eq!(
            watch_step(20, true, 50, 50, 10, true, false),
            WatchStep::Refund { epochs_overdue: 0 }
        );
        assert_eq!(
            watch_step(0, false, 40, 50, 10, true, false),
            WatchStep::NothingOwed
        );
    }

    #[test]
    fn refund_due_starts_at_the_expiry_epoch() {
        assert_eq!(
            decide_refund(49, 50),
            RefundDecision::Wait {
                epochs_until_expiry: 1
            }
        );
        assert_eq!(
            decide_refund(50, 50),
            RefundDecision::Due { epochs_overdue: 0 }
        );
        assert_eq!(
            decide_refund(51, 50),
            RefundDecision::Due { epochs_overdue: 1 }
        );
    }
}
