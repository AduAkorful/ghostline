// SPDX-License-Identifier: MIT

use std::collections::{BTreeSet, VecDeque};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tari_template_lib::types::crypto::RistrettoPublicKeyBytes;

use crate::{Policy, PolicyError};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditEvent {
    pub unix_ms: u128,
    pub approved: bool,
    pub vendor_hex: String,
    pub price: u64,
    pub already_owed: u64,
    pub reason: Option<String>,
}

#[derive(Debug, Clone)]
pub struct RateLimit {
    pub max_calls: u32,
    pub max_value: u64,
    pub window: Duration,
}

#[derive(Debug, Clone)]
pub struct PolicyEngine {
    policy: Policy,
    rate: Option<RateLimit>,
    human: bool,
    human_threshold: Option<u64>,
    first_time_vendor: bool,
    seen_vendors: BTreeSet<String>,
    events: VecDeque<(Instant, u64)>,
    log: Vec<AuditEvent>,
    audit_path: Option<PathBuf>,
}

impl PolicyEngine {
    pub fn new(policy: Policy) -> Self {
        Self {
            policy,
            rate: None,
            human: false,
            human_threshold: None,
            first_time_vendor: false,
            seen_vendors: BTreeSet::new(),
            events: VecDeque::new(),
            log: Vec::new(),
            audit_path: None,
        }
    }

    /// After the numeric limits pass, `approve` still fails until [`approve_attested`] is used.
    pub fn require_human(mut self) -> Self {
        self.human = true;
        self
    }

    pub fn with_rate_limit(mut self, rate: RateLimit) -> Self {
        self.rate = Some(rate);
        self
    }

    /// Persist every decision as a synced JSONL row before allowing the caller to continue.
    pub fn with_audit_file(mut self, path: impl Into<PathBuf>) -> Result<Self, PolicyError> {
        let path = path.into();
        self.seen_vendors = load_approved_vendors(&path)?;
        self.audit_path = Some(path);
        Ok(self)
    }

    /// Use `GHOSTLINE_POLICY_AUDIT_LOG` when set to a non-empty path.
    pub fn with_audit_file_from_env(self) -> Result<Self, PolicyError> {
        match std::env::var("GHOSTLINE_POLICY_AUDIT_LOG") {
            Ok(path) if !path.trim().is_empty() => {
                let path = PathBuf::from(path.trim());
                let seen_vendors = load_approved_vendors(&path)?;
                let mut engine = self;
                engine.seen_vendors = seen_vendors;
                engine.audit_path = Some(path);
                Ok(engine)
            }
            Ok(_) | Err(std::env::VarError::NotPresent) => Ok(self),
            Err(std::env::VarError::NotUnicode(_)) => Err(PolicyError::AuditLogFailed),
        }
    }

    /// Require a human when a call costs at least `threshold` micro-units.
    pub fn with_human_threshold(mut self, threshold: u64) -> Result<Self, PolicyError> {
        if threshold == 0 {
            return Err(PolicyError::InvalidConfiguration);
        }
        self.human_threshold = Some(threshold);
        Ok(self)
    }

    /// Require human approval for the first allowed payment to each vendor.
    pub fn require_first_time_vendor_approval(mut self) -> Self {
        self.first_time_vendor = true;
        self
    }

    /// Configure the all-payments flag, threshold, and first-vendor flag from environment.
    pub fn with_human_rules_from_env(mut self) -> Result<Self, PolicyError> {
        if crate::env_flag("GHOSTLINE_POLICY_REQUIRE_HUMAN") {
            self.human = true;
        }
        if let Ok(value) = std::env::var("GHOSTLINE_POLICY_HUMAN_THRESHOLD") {
            let threshold = value
                .trim()
                .parse::<u64>()
                .map_err(|_| PolicyError::InvalidConfiguration)?;
            if threshold == 0 {
                return Err(PolicyError::InvalidConfiguration);
            }
            self.human_threshold = Some(threshold);
        }
        if crate::env_flag("GHOSTLINE_POLICY_APPROVE_NEW_VENDOR") {
            self.first_time_vendor = true;
        }
        Ok(self)
    }

    pub fn requires_human_approval(&self, vendor: &RistrettoPublicKeyBytes, price: u64) -> bool {
        self.human
            || self
                .human_threshold
                .is_some_and(|threshold| price >= threshold)
            || (self.first_time_vendor && !self.seen_vendors.contains(&hex(vendor.as_bytes())))
    }

    pub fn policy(&self) -> &Policy {
        &self.policy
    }

    pub fn audit_log(&self) -> &[AuditEvent] {
        &self.log
    }

    pub fn approve(
        &mut self,
        vendor: &RistrettoPublicKeyBytes,
        price: u64,
        already_owed: u64,
        now: Instant,
    ) -> Result<(), PolicyError> {
        self.decide(vendor, price, already_owed, now, false)
    }

    /// Same checks as [`approve`], but satisfies a configured human gate. The caller must have
    /// confirmed the spend out of band; this crate does not present a UI.
    pub fn approve_attested(
        &mut self,
        vendor: &RistrettoPublicKeyBytes,
        price: u64,
        already_owed: u64,
        now: Instant,
    ) -> Result<(), PolicyError> {
        self.decide(vendor, price, already_owed, now, true)
    }

    fn decide(
        &mut self,
        vendor: &RistrettoPublicKeyBytes,
        price: u64,
        already_owed: u64,
        now: Instant,
        attested: bool,
    ) -> Result<(), PolicyError> {
        let result = self
            .policy
            .approve(vendor, price, already_owed)
            .and_then(|_| self.check_rate(now, price))
            .and_then(|_| {
                if self.requires_human_approval(vendor, price) && !attested {
                    Err(PolicyError::NeedsHumanApproval {
                        price,
                        already_owed,
                    })
                } else {
                    Ok(())
                }
            });
        self.log.push(AuditEvent {
            unix_ms: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_millis())
                .unwrap_or(0),
            approved: result.is_ok(),
            vendor_hex: hex(vendor.as_bytes()),
            price,
            already_owed,
            reason: result.as_ref().err().map(ToString::to_string),
        });
        if let Some(path) = &self.audit_path {
            let written = self
                .log
                .last()
                .is_some_and(|event| append_audit(path, event).is_ok());
            if !written {
                if let Some(event) = self.log.last_mut() {
                    event.approved = false;
                    event.reason = Some(PolicyError::AuditLogFailed.to_string());
                }
                return Err(PolicyError::AuditLogFailed);
            }
        }
        if result.is_ok() {
            self.events.push_back((now, price));
            self.seen_vendors.insert(hex(vendor.as_bytes()));
        }
        result
    }

    fn check_rate(&mut self, now: Instant, price: u64) -> Result<(), PolicyError> {
        let Some(rate) = &self.rate else {
            return Ok(());
        };
        let window = rate.window;
        let max_calls = rate.max_calls;
        let max_value = rate.max_value;
        while self
            .events
            .front()
            .is_some_and(|(t, _)| now.checked_duration_since(*t).is_some_and(|d| d >= window))
        {
            self.events.pop_front();
        }
        if self.events.len() as u32 >= max_calls {
            return Err(PolicyError::RateLimited);
        }
        let spent = self
            .events
            .iter()
            .try_fold(0u64, |total, (_, value)| total.checked_add(*value))
            .ok_or(PolicyError::RateLimited)?;
        let next = spent.checked_add(price).ok_or(PolicyError::RateLimited)?;
        if next > max_value {
            return Err(PolicyError::RateLimited);
        }
        Ok(())
    }
}

fn append_audit(path: &std::path::Path, event: &AuditEvent) -> std::io::Result<()> {
    let mut options = OpenOptions::new();
    options.create(true).append(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        options.mode(0o600);
        let mut file = options.open(path)?;
        file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
        append_audit_row(&mut file, event)
    }
    #[cfg(not(unix))]
    {
        let mut file = options.open(path)?;
        append_audit_row(&mut file, event)
    }
}

fn append_audit_row(file: &mut std::fs::File, event: &AuditEvent) -> std::io::Result<()> {
    let mut row = serde_json::to_vec(event).map_err(std::io::Error::other)?;
    row.push(b'\n');
    file.write_all(&row)?;
    file.sync_data()
}

fn load_approved_vendors(path: &std::path::Path) -> Result<BTreeSet<String>, PolicyError> {
    let contents = match std::fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(BTreeSet::new()),
        Err(_) => return Err(PolicyError::AuditLogFailed),
    };
    contents
        .lines()
        .map(|line| {
            serde_json::from_str::<AuditEvent>(line).map_err(|_| PolicyError::AuditLogFailed)
        })
        .collect::<Result<Vec<_>, _>>()
        .map(|events| {
            events
                .into_iter()
                .filter(|event| event.approved)
                .map(|event| event.vendor_hex)
                .collect()
        })
}

fn hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Policy, PolicyError};

    fn vendor() -> RistrettoPublicKeyBytes {
        RistrettoPublicKeyBytes::from_bytes(&[1u8; 32]).unwrap()
    }

    #[test]
    fn logs_rejections() {
        let mut engine = PolicyEngine::new(Policy::new(100, 10));
        let now = Instant::now();
        assert!(engine.approve(&vendor(), 11, 0, now).is_err());
        assert_eq!(engine.audit_log().len(), 1);
        assert!(!engine.audit_log()[0].approved);
        assert_eq!(engine.audit_log()[0].vendor_hex, hex(vendor().as_bytes()));
        assert!(engine.audit_log()[0].reason.is_some());
    }

    #[test]
    fn audit_file_appends_approved_and_rejected_decisions() {
        let path = std::env::temp_dir().join(format!(
            "ghostline-policy-audit-{}-{}.jsonl",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut engine = PolicyEngine::new(Policy::new(100, 10))
            .with_audit_file(path.clone())
            .unwrap();
        let now = Instant::now();
        assert!(engine.approve(&vendor(), 11, 0, now).is_err());
        engine.approve(&vendor(), 10, 0, now).unwrap();

        let contents = std::fs::read_to_string(&path).unwrap();
        let events: Vec<serde_json::Value> = contents
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0]["approved"], false);
        assert!(events[0]["reason"].as_str().unwrap().contains("exceeds"));
        assert_eq!(events[1]["approved"], true);
        assert_eq!(events[1]["vendor_hex"], hex(vendor().as_bytes()));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn audit_file_failure_blocks_approval() {
        let parent = std::env::temp_dir().join(format!(
            "ghostline-policy-missing-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let path = parent.join("audit.jsonl");
        let mut engine = PolicyEngine::new(Policy::new(100, 10))
            .with_audit_file(path)
            .unwrap();
        assert_eq!(
            engine.approve(&vendor(), 10, 0, Instant::now()),
            Err(PolicyError::AuditLogFailed)
        );
        assert!(!engine.audit_log()[0].approved);
    }

    #[test]
    fn rate_limit_caps_calls_in_the_window() {
        let mut engine = PolicyEngine::new(Policy::new(100, 10)).with_rate_limit(RateLimit {
            max_calls: 2,
            max_value: 100,
            window: Duration::from_secs(60),
        });
        let now = Instant::now();
        engine.approve(&vendor(), 10, 0, now).unwrap();
        engine.approve(&vendor(), 10, 10, now).unwrap();
        assert_eq!(
            engine.approve(&vendor(), 10, 20, now),
            Err(PolicyError::RateLimited)
        );
        assert_eq!(engine.audit_log().iter().filter(|e| e.approved).count(), 2);
        assert_eq!(engine.audit_log().iter().filter(|e| !e.approved).count(), 1);
    }

    #[test]
    fn human_gate_blocks_until_attested() {
        let mut engine = PolicyEngine::new(Policy::new(100, 10)).require_human();
        let now = Instant::now();
        assert_eq!(
            engine.approve(&vendor(), 10, 0, now),
            Err(PolicyError::NeedsHumanApproval {
                price: 10,
                already_owed: 0
            })
        );
        engine.approve_attested(&vendor(), 10, 0, now).unwrap();
        assert_eq!(engine.audit_log().iter().filter(|e| e.approved).count(), 1);
    }

    #[test]
    fn human_threshold_applies_at_and_above_the_configured_price() {
        let mut engine = PolicyEngine::new(Policy::new(100, 50))
            .with_human_threshold(20)
            .unwrap();
        let now = Instant::now();
        assert!(!engine.requires_human_approval(&vendor(), 19));
        assert!(engine.requires_human_approval(&vendor(), 20));
        engine.approve(&vendor(), 19, 0, now).unwrap();
        assert_eq!(
            engine.approve(&vendor(), 20, 19, now),
            Err(PolicyError::NeedsHumanApproval {
                price: 20,
                already_owed: 19
            })
        );
        engine.approve_attested(&vendor(), 20, 19, now).unwrap();
        assert!(engine.with_human_threshold(0).is_err());
    }

    #[test]
    fn first_time_vendor_uses_prior_durable_approval() {
        let path = std::env::temp_dir().join(format!(
            "ghostline-policy-vendor-{}-{}.jsonl",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let now = Instant::now();
        let mut first = PolicyEngine::new(Policy::new(100, 10))
            .with_audit_file(path.clone())
            .unwrap()
            .require_first_time_vendor_approval();
        assert!(first.requires_human_approval(&vendor(), 10));
        first.approve_attested(&vendor(), 10, 0, now).unwrap();

        let second = PolicyEngine::new(Policy::new(100, 10))
            .with_audit_file(path.clone())
            .unwrap()
            .require_first_time_vendor_approval();
        assert!(!second.requires_human_approval(&vendor(), 10));
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn overflowing_rate_window_total_is_rejected() {
        let mut engine =
            PolicyEngine::new(Policy::new(u64::MAX, u64::MAX)).with_rate_limit(RateLimit {
                max_calls: 3,
                max_value: u64::MAX,
                window: Duration::from_secs(60),
            });
        let now = Instant::now();
        engine.approve(&vendor(), u64::MAX, 0, now).unwrap();
        assert_eq!(
            engine.approve(&vendor(), u64::MAX, 0, now),
            Err(PolicyError::RateLimited)
        );
    }
}
