// SPDX-License-Identifier: MIT

//! Fixed, per-request, and metered prices. The voucher always pays the total in `price`.

use crate::PaywallError;

/// How a 402 total was chosen. The agent signs `price`, which is already the amount due.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PriceKind {
    /// One price for the resource, independent of request size.
    Fixed { price: u64 },
    /// One price for this HTTP request.
    PerRequest { price: u64 },
    /// `unit_price * units`. Both factors must be non-zero.
    Metered { unit_price: u64, units: u64 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Quote {
    pub price: u64,
    pub basis: &'static str,
    pub unit_price: Option<u64>,
    pub units: Option<u64>,
}

pub fn quote(kind: PriceKind) -> Result<Quote, PaywallError> {
    match kind {
        PriceKind::Fixed { price } => positive(price, "fixed price").map(|price| Quote {
            price,
            basis: "fixed",
            unit_price: None,
            units: None,
        }),
        PriceKind::PerRequest { price } => {
            positive(price, "per-request price").map(|price| Quote {
                price,
                basis: "per_request",
                unit_price: None,
                units: None,
            })
        }
        PriceKind::Metered { unit_price, units } => {
            positive(unit_price, "unit price")?;
            positive(units, "units")?;
            let price = unit_price
                .checked_mul(units)
                .ok_or_else(|| PaywallError::BadPrice("metered price overflow".into()))?;
            Ok(Quote {
                price,
                basis: "metered",
                unit_price: Some(unit_price),
                units: Some(units),
            })
        }
    }
}

fn positive(amount: u64, label: &str) -> Result<u64, PaywallError> {
    if amount == 0 {
        Err(PaywallError::BadPrice(format!("{label} must be non-zero")))
    } else {
        Ok(amount)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_price_is_the_resource_price() {
        let quote = quote(PriceKind::Fixed { price: 10 }).unwrap();
        assert_eq!(quote.price, 10);
        assert_eq!(quote.basis, "fixed");
        assert_eq!(quote.unit_price, None);
        assert_eq!(quote.units, None);
    }

    #[test]
    fn metered_price_is_the_product() {
        let quote = quote(PriceKind::Metered {
            unit_price: 10,
            units: 3,
        })
        .unwrap();
        assert_eq!(quote.price, 30);
        assert_eq!(quote.basis, "metered");
        assert_eq!(quote.unit_price, Some(10));
        assert_eq!(quote.units, Some(3));
    }

    #[test]
    fn metered_rejects_zero_and_overflow() {
        assert!(
            quote(PriceKind::Metered {
                unit_price: 10,
                units: 0
            })
            .is_err()
        );
        assert!(
            quote(PriceKind::Metered {
                unit_price: u64::MAX,
                units: 2
            })
            .is_err()
        );
        assert!(quote(PriceKind::Fixed { price: 0 }).is_err());
        assert!(quote(PriceKind::PerRequest { price: 0 }).is_err());
    }
}
