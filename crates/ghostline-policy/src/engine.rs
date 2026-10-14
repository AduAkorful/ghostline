// SPDX-License-Identifier: MIT

use std::collections::VecDeque;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use tari_template_lib::types::crypto::RistrettoPublicKeyBytes;

use crate::{Policy, PolicyError};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AuditEvent {
    pub unix_ms: u128,
    pub approved: bool,
    pub price: u64,
    pub already_owed: u64,
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
    events: VecDeque<(Instant, u64)>,
    log: Vec<AuditEvent>,
}

impl PolicyEngine {
    pub fn new(policy: Policy) -> Self {
        Self {
            policy,
            rate: None,
            human: false,
            events: VecDeque::new(),
            log: Vec::new(),
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
                if self.human && !attested {
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
            price,
            already_owed,
        });
        if result.is_ok() {
            self.events.push_back((now, price));
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
        let spent: u64 = self.events.iter().map(|(_, v)| *v).sum();
        let next = spent.checked_add(price).ok_or(PolicyError::RateLimited)?;
        if next > max_value {
            return Err(PolicyError::RateLimited);
        }
        Ok(())
    }
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
}
