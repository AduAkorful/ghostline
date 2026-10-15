// SPDX-License-Identifier: MIT

use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Default)]
pub(super) struct ServiceMetrics {
    fortune_requests: AtomicU64,
    meter_requests: AtomicU64,
    payment_required: AtomicU64,
    paid_requests: AtomicU64,
    payment_rejections: AtomicU64,
    internal_errors: AtomicU64,
}

impl ServiceMetrics {
    pub(super) fn request(&self, route: &str) {
        increment(match route {
            "fortune" => &self.fortune_requests,
            "meter" => &self.meter_requests,
            _ => return,
        });
    }

    pub(super) fn payment_required(&self) {
        increment(&self.payment_required);
    }

    pub(super) fn paid(&self) {
        increment(&self.paid_requests);
    }

    pub(super) fn rejected(&self) {
        increment(&self.payment_rejections);
    }

    pub(super) fn internal_error(&self) {
        increment(&self.internal_errors);
    }

    pub(super) fn exposition(
        &self,
        configured_channels: usize,
        ledgers_with_accepted_vouchers: usize,
    ) -> String {
        format!(
            "# HELP ghostline_http_requests_total Requests received by route.\n\
             # TYPE ghostline_http_requests_total counter\n\
             ghostline_http_requests_total{{route=\"fortune\"}} {}\n\
             ghostline_http_requests_total{{route=\"meter\"}} {}\n\
             # HELP ghostline_payment_required_total Requests answered with HTTP 402.\n\
             # TYPE ghostline_payment_required_total counter\n\
             ghostline_payment_required_total {}\n\
             # HELP ghostline_paid_requests_total Successfully paid requests served.\n\
             # TYPE ghostline_paid_requests_total counter\n\
             ghostline_paid_requests_total {}\n\
             # HELP ghostline_payment_rejections_total Requests rejected for payment validation.\n\
             # TYPE ghostline_payment_rejections_total counter\n\
             ghostline_payment_rejections_total {}\n\
             # HELP ghostline_internal_errors_total Internal paywall or durable storage errors.\n\
             # TYPE ghostline_internal_errors_total counter\n\
             ghostline_internal_errors_total {}\n\
             # HELP ghostline_channels_configured Current number of configured channel ledgers.\n\
             # TYPE ghostline_channels_configured gauge\n\
             ghostline_channels_configured {}\n\
             # HELP ghostline_ledgers_with_accepted_vouchers Current ledgers with an accepted voucher; this is not chain settlement confirmation.\n\
             # TYPE ghostline_ledgers_with_accepted_vouchers gauge\n\
             ghostline_ledgers_with_accepted_vouchers {}\n",
            self.fortune_requests.load(Ordering::Relaxed),
            self.meter_requests.load(Ordering::Relaxed),
            self.payment_required.load(Ordering::Relaxed),
            self.paid_requests.load(Ordering::Relaxed),
            self.payment_rejections.load(Ordering::Relaxed),
            self.internal_errors.load(Ordering::Relaxed),
            configured_channels,
            ledgers_with_accepted_vouchers,
        )
    }
}

fn increment(counter: &AtomicU64) {
    counter
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
            Some(value.saturating_add(1))
        })
        .expect("saturating metric increments always succeed");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counters_saturate_instead_of_wrapping() {
        let metrics = ServiceMetrics::default();
        metrics.paid_requests.store(u64::MAX - 1, Ordering::Relaxed);
        metrics.paid();
        metrics.paid();
        assert_eq!(metrics.paid_requests.load(Ordering::Relaxed), u64::MAX);
    }
}
