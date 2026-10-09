//! Platform-wide merchant verification (KYC/KYB) tiers, thresholds and the
//! one-year unregistered-business grace period.
//!
//! This is the pure decision core only. It models *when* a merchant should be
//! restricted and *why*; it deliberately does not mutate state, send email, or
//! touch the payment path. Callers map their stored merchant record onto
//! [`MerchantVerification`] and feed in the volume collected over the relevant
//! window; they are responsible for acting on the returned decision.
//!
//! Two rules from the product spec are modelled here:
//!
//! * The general amount threshold: a merchant on baseline individual KYC may
//!   transact freely until the volume collected within a rolling window reaches
//!   [`VerificationPolicy::general_threshold_minor`], at which point full KYB is
//!   required.
//! * The unregistered-business grace period: a merchant with no business
//!   registration on file at all gets the complete, uncapped experience for
//!   exactly one year from account creation, regardless of volume. The deadline
//!   is intentionally invisible to the merchant — only the resulting
//!   [`VerificationDecision`] is meant to cross this boundary.

use time::OffsetDateTime;

/// Length of the unregistered-business grace period, in days.
///
/// Exactly one year is modelled as 365 days; this ignores leap-day drift, which
/// is immaterial for a deadline that must stay invisible rather than calendar
/// exact.
pub const GRACE_PERIOD_DAYS: i64 = 365;

/// How verified a merchant is, which decides which restriction rule applies.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VerificationTier {
    /// No business registration on file at all. Gets the one-year grace period.
    Unregistered,
    /// An individual who has completed baseline (personal) KYC, but no business
    /// registration. Subject to the general amount threshold.
    IndividualKyc,
    /// Full business verification (registration, beneficial ownership). Never
    /// restricted by this model.
    BusinessKyb,
}

/// Tunable thresholds for the general (non-grace-period) rule.
///
/// These are placeholders: the spec leaves the amount and period to product
/// input. They are configuration, not derived, and the default is a deliberately
/// conservative example rather than a launch value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VerificationPolicy {
    /// Cumulative collected volume, in minor units of the account currency, that
    /// a baseline-KYC merchant may reach within the window before unrestricted
    /// business verification is required.
    pub general_threshold_minor: i128,
    /// Length of the rolling window, in days, over which volume is measured.
    pub general_threshold_window_days: i64,
}

impl Default for VerificationPolicy {
    fn default() -> Self {
        Self {
            general_threshold_minor: 5_000_000,
            general_threshold_window_days: 90,
        }
    }
}

/// Why a merchant is restricted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RestrictionReason {
    /// The unregistered-business grace period has ended; KYB is now required.
    GracePeriodExpired,
    /// The general amount threshold was reached within the window.
    ThresholdExceeded,
}

/// The outcome of evaluating a merchant against the verification model.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VerificationDecision {
    /// The merchant may transact normally.
    Allowed,
    /// The merchant must complete the required verification to continue.
    Restricted {
        /// Why the restriction applies.
        reason: RestrictionReason,
    },
}

/// A merchant's verification state, reduced to what the decision needs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MerchantVerification {
    /// The merchant's verification tier.
    pub tier: VerificationTier,
    /// When the merchant account was created; the grace-period clock start.
    pub account_created_at: OffsetDateTime,
}

impl MerchantVerification {
    /// Builds the verification state for a merchant.
    pub fn new(tier: VerificationTier, account_created_at: OffsetDateTime) -> Self {
        Self {
            tier,
            account_created_at,
        }
    }

    /// The instant at which an unregistered merchant's grace period ends, or
    /// `None` if the deadline is beyond the representable range (treated as
    /// never expiring rather than panicking).
    pub fn grace_period_end(&self) -> Option<OffsetDateTime> {
        self.account_created_at
            .checked_add(time::Duration::days(GRACE_PERIOD_DAYS))
    }

    /// Evaluates the merchant at `now`, given the payments collected for the
    /// account.
    ///
    /// `collected` is every `(timestamp, amount_minor)` the caller holds; the
    /// window filtering happens here so the boundary is defined in one place.
    /// Amounts are expected in minor units and non-negative.
    pub fn evaluate(
        &self,
        policy: &VerificationPolicy,
        now: OffsetDateTime,
        collected: &[(OffsetDateTime, i128)],
    ) -> VerificationDecision {
        match self.tier {
            VerificationTier::BusinessKyb => VerificationDecision::Allowed,
            VerificationTier::Unregistered => match self.grace_period_end() {
                // A deadline we cannot represent never arrives.
                Some(end) if now >= end => VerificationDecision::Restricted {
                    reason: RestrictionReason::GracePeriodExpired,
                },
                _ => VerificationDecision::Allowed,
            },
            VerificationTier::IndividualKyc => {
                let collected_in_window = self.collected_in_window(policy, now, collected);
                if collected_in_window >= policy.general_threshold_minor {
                    VerificationDecision::Restricted {
                        reason: RestrictionReason::ThresholdExceeded,
                    }
                } else {
                    VerificationDecision::Allowed
                }
            }
        }
    }

    fn collected_in_window(
        &self,
        policy: &VerificationPolicy,
        now: OffsetDateTime,
        collected: &[(OffsetDateTime, i128)],
    ) -> i128 {
        // A window that cannot be represented reaches back forever, so nothing
        // is excluded on the low end.
        let window_start =
            now.checked_sub(time::Duration::days(policy.general_threshold_window_days));
        collected
            .iter()
            .filter(|(timestamp, _)| {
                *timestamp <= now && window_start.is_none_or(|start| *timestamp > start)
            })
            .map(|(_, amount_minor)| *amount_minor)
            .sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(y: i32, m: u8, d: u8) -> OffsetDateTime {
        OffsetDateTime::from_unix_timestamp(
            time::Date::from_calendar_date(y, time::Month::try_from(m).unwrap(), d)
                .unwrap()
                .midnight()
                .assume_utc()
                .unix_timestamp(),
        )
        .unwrap()
    }

    #[test]
    fn business_kyb_is_never_restricted() {
        let merchant = MerchantVerification::new(VerificationTier::BusinessKyb, t(2020, 1, 1));
        let decision = merchant.evaluate(
            &VerificationPolicy::default(),
            t(2030, 1, 1),
            &[(t(2029, 12, 1), i128::MAX)],
        );
        assert_eq!(decision, VerificationDecision::Allowed);
    }

    #[test]
    fn unregistered_is_allowed_within_grace_period_even_when_uncapped() {
        let merchant = MerchantVerification::new(VerificationTier::Unregistered, t(2026, 1, 1));
        let decision = merchant.evaluate(
            &VerificationPolicy::default(),
            t(2026, 12, 31),
            &[(t(2026, 12, 15), i128::MAX)],
        );
        assert_eq!(decision, VerificationDecision::Allowed);
    }

    #[test]
    fn unregistered_is_restricted_at_the_exact_deadline() {
        let merchant = MerchantVerification::new(VerificationTier::Unregistered, t(2026, 1, 1));
        let end = merchant.grace_period_end().unwrap();
        assert_eq!(
            merchant.evaluate(&VerificationPolicy::default(), end, &[]),
            VerificationDecision::Restricted {
                reason: RestrictionReason::GracePeriodExpired
            }
        );
        // One second before the deadline is still allowed.
        let just_before = end - time::Duration::seconds(1);
        assert_eq!(
            merchant.evaluate(&VerificationPolicy::default(), just_before, &[]),
            VerificationDecision::Allowed
        );
    }

    #[test]
    fn individual_kyc_crosses_the_threshold_only_within_the_window() {
        let merchant = MerchantVerification::new(VerificationTier::IndividualKyc, t(2026, 1, 1));
        let policy = VerificationPolicy {
            general_threshold_minor: 1_000,
            general_threshold_window_days: 30,
        };
        let now = t(2026, 6, 1);

        // Old volume outside the window does not count.
        assert_eq!(
            merchant.evaluate(&policy, now, &[(t(2026, 1, 15), 10_000)]),
            VerificationDecision::Allowed
        );

        // In-window volume exactly at the threshold triggers it.
        assert_eq!(
            merchant.evaluate(&policy, now, &[(t(2026, 5, 20), 1_000)]),
            VerificationDecision::Restricted {
                reason: RestrictionReason::ThresholdExceeded
            }
        );

        // Just below the threshold stays allowed.
        assert_eq!(
            merchant.evaluate(&policy, now, &[(t(2026, 5, 20), 999)]),
            VerificationDecision::Allowed
        );
    }
}
