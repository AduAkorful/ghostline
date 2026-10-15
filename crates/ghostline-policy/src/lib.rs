// SPDX-License-Identifier: MIT

//! Spending limits the agent process cannot relax. The contract still caps payout at the deposit;
//! this crate stops the agent signing for vendors or prices the operator did not allow.

use std::collections::BTreeSet;
use std::fmt;

use tari_template_lib::types::crypto::RistrettoPublicKeyBytes;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PolicyError {
    PriceAboveLimit { price: u64, max_per_call: u64 },
    BudgetExceeded { would_owe: u64, budget: u64 },
    VendorDenied,
    RateLimited,
    NeedsHumanApproval { price: u64, already_owed: u64 },
    AuditLogFailed,
    InvalidConfiguration,
}

impl fmt::Display for PolicyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PriceAboveLimit {
                price,
                max_per_call,
            } => {
                write!(
                    f,
                    "call price {price} exceeds per-call limit {max_per_call}"
                )
            }
            Self::BudgetExceeded { would_owe, budget } => {
                write!(f, "cumulative {would_owe} exceeds policy budget {budget}")
            }
            Self::VendorDenied => write!(f, "vendor is not on the allowlist"),
            Self::RateLimited => write!(f, "rate limit exceeded"),
            Self::NeedsHumanApproval {
                price,
                already_owed,
            } => write!(
                f,
                "human approval required for price {price} with {already_owed} already owed"
            ),
            Self::AuditLogFailed => write!(f, "could not persist policy decision audit event"),
            Self::InvalidConfiguration => write!(f, "invalid human approval policy configuration"),
        }
    }
}

impl std::error::Error for PolicyError {}

mod engine;
pub use engine::{AuditEvent, PolicyEngine, RateLimit};

/// Operator-set limits. Fields are private so agent-supplied text cannot widen them after construction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Policy {
    budget: u64,
    max_per_call: u64,
    vendors: Option<BTreeSet<RistrettoPublicKeyBytes>>,
}

impl Policy {
    pub fn new(budget: u64, max_per_call: u64) -> Self {
        Self {
            budget,
            max_per_call,
            vendors: None,
        }
    }

    pub fn allow_vendor(mut self, vendor: RistrettoPublicKeyBytes) -> Self {
        self.vendors
            .get_or_insert_with(BTreeSet::new)
            .insert(vendor);
        self
    }

    pub fn budget(&self) -> u64 {
        self.budget
    }

    pub fn max_per_call(&self) -> u64 {
        self.max_per_call
    }

    /// `GHOSTLINE_POLICY_BUDGET` and `GHOSTLINE_POLICY_MAX_PER_CALL`. Optional
    /// `GHOSTLINE_POLICY_VENDOR_HEX` becomes a one-vendor allowlist.
    pub fn from_env() -> Result<Option<Self>, String> {
        let budget = env_u64("GHOSTLINE_POLICY_BUDGET")?;
        let max_per_call = env_u64("GHOSTLINE_POLICY_MAX_PER_CALL")?;
        match (budget, max_per_call) {
            (None, None) => Ok(None),
            (Some(budget), Some(max_per_call)) => {
                let mut policy = Policy::new(budget, max_per_call);
                if let Ok(hex) = std::env::var("GHOSTLINE_POLICY_VENDOR_HEX") {
                    let hex = hex.trim();
                    if !hex.is_empty() {
                        let vendor = RistrettoPublicKeyBytes::from_hex(hex)
                            .map_err(|e| format!("GHOSTLINE_POLICY_VENDOR_HEX: {e}"))?;
                        policy = policy.allow_vendor(vendor);
                    }
                }
                Ok(Some(policy))
            }
            _ => Err(
                "set both GHOSTLINE_POLICY_BUDGET and GHOSTLINE_POLICY_MAX_PER_CALL, or neither"
                    .to_string(),
            ),
        }
    }

    /// Approves paying `price` to `vendor` when the session already owes `already_owed`.
    pub fn approve(
        &self,
        vendor: &RistrettoPublicKeyBytes,
        price: u64,
        already_owed: u64,
    ) -> Result<(), PolicyError> {
        if let Some(vendors) = &self.vendors
            && !vendors.contains(vendor)
        {
            return Err(PolicyError::VendorDenied);
        }
        if price == 0 || price > self.max_per_call {
            return Err(PolicyError::PriceAboveLimit {
                price,
                max_per_call: self.max_per_call,
            });
        }
        let would_owe = already_owed
            .checked_add(price)
            .ok_or(PolicyError::BudgetExceeded {
                would_owe: u64::MAX,
                budget: self.budget,
            })?;
        if would_owe > self.budget {
            return Err(PolicyError::BudgetExceeded {
                would_owe,
                budget: self.budget,
            });
        }
        Ok(())
    }
}

fn env_u64(name: &str) -> Result<Option<u64>, String> {
    match std::env::var(name) {
        Err(_) => Ok(None),
        Ok(s) if s.trim().is_empty() => Ok(None),
        Ok(s) => s
            .trim()
            .parse()
            .map(Some)
            .map_err(|_| format!("{name} must be an integer")),
    }
}

pub fn env_flag(name: &str) -> bool {
    std::env::var(name).is_ok_and(|s| {
        let s = s.trim();
        s == "1" || s.eq_ignore_ascii_case("true") || s.eq_ignore_ascii_case("yes")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vendor(byte: u8) -> RistrettoPublicKeyBytes {
        RistrettoPublicKeyBytes::from_bytes(&[byte; 32]).unwrap()
    }

    #[test]
    fn allows_a_call_inside_limits() {
        let v = vendor(1);
        Policy::new(100, 10).approve(&v, 10, 0).unwrap();
    }

    #[test]
    fn rejects_price_above_per_call_limit() {
        let v = vendor(1);
        assert_eq!(
            Policy::new(100, 10).approve(&v, 11, 0),
            Err(PolicyError::PriceAboveLimit {
                price: 11,
                max_per_call: 10
            })
        );
    }

    #[test]
    fn rejects_when_budget_would_break() {
        let v = vendor(1);
        assert_eq!(
            Policy::new(100, 50).approve(&v, 50, 60),
            Err(PolicyError::BudgetExceeded {
                would_owe: 110,
                budget: 100
            })
        );
    }

    #[test]
    fn rejects_vendor_off_the_allowlist() {
        let policy = Policy::new(100, 10).allow_vendor(vendor(1));
        assert_eq!(
            policy.approve(&vendor(2), 10, 0),
            Err(PolicyError::VendorDenied)
        );
    }
}
