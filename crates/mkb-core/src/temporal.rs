//! Temporal types and the temporal gate for MKB.
//!
//! Core invariant: **No information enters the vault without `observed_at`.**

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

use crate::error::TemporalError;

/// Precision level of a temporal observation.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(rename_all = "lowercase")]
pub enum TemporalPrecision {
    Exact,
    Day,
    Week,
    Month,
    Quarter,
    Approximate,
    #[default]
    Inferred,
}

/// Mandatory temporal fields present on every document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemporalFields {
    /// When this information was true/observed. **MANDATORY.**
    pub observed_at: DateTime<Utc>,

    /// When this information expires. **MANDATORY** (computed if not provided).
    pub valid_until: DateTime<Utc>,

    /// How precise the temporal grounding is. **MANDATORY.**
    #[serde(default)]
    pub temporal_precision: TemporalPrecision,

    /// When the described event actually happened (if different from observed_at).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub occurred_at: Option<DateTime<Utc>>,
}

/// Raw temporal input before gate validation.
/// All fields are optional so we can detect what's missing and give actionable errors.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RawTemporalInput {
    pub observed_at: Option<DateTime<Utc>>,
    pub valid_until: Option<DateTime<Utc>>,
    pub temporal_precision: Option<TemporalPrecision>,
    pub occurred_at: Option<DateTime<Utc>>,
}

/// Decay profile for computing `valid_until` when not explicitly provided.
#[derive(Debug, Clone, PartialEq)]
pub struct DecayProfile {
    /// Base half-life duration for this document type.
    pub half_life: Duration,
}

impl DecayProfile {
    /// Create a new decay profile with the given half-life.
    #[must_use]
    pub fn new(half_life: Duration) -> Self {
        Self { half_life }
    }

    /// Default decay profile: 90-day half-life.
    #[must_use]
    pub fn default_profile() -> Self {
        Self {
            half_life: Duration::days(90),
        }
    }

    /// Project status decays in 14 days.
    #[must_use]
    pub fn project_status() -> Self {
        Self {
            half_life: Duration::days(14),
        }
    }

    /// Decisions never decay (very long half-life).
    #[must_use]
    pub fn decision() -> Self {
        Self {
            half_life: Duration::days(365 * 100),
        }
    }

    /// Signals decay in 7 days.
    #[must_use]
    pub fn signal() -> Self {
        Self {
            half_life: Duration::days(7),
        }
    }

    /// Compute valid_until from observed_at using this profile.
    /// Uses 2x half-life as the default validity window.
    #[must_use]
    pub fn compute_valid_until(&self, observed_at: DateTime<Utc>) -> DateTime<Utc> {
        observed_at + self.half_life * 2
    }
}

/// The Temporal Gate — validates all temporal invariants before a document
/// enters the vault.
///
/// Invariants enforced:
/// - T1: `observed_at` is NEVER null
/// - T2: `valid_until` is NEVER null (auto-computed from decay profile)
/// - T3: `temporal_precision` is NEVER null (defaults to Inferred)
/// - T4: `valid_until >= observed_at`
/// - T5: `occurred_at <= observed_at` (warning-level)
pub struct TemporalGate;

impl TemporalGate {
    /// Validate raw temporal input and produce validated [`TemporalFields`].
    ///
    /// If `valid_until` is not provided, it is computed from the decay profile.
    /// If `temporal_precision` is not provided, it defaults to `Inferred`.
    ///
    /// # Errors
    ///
    /// Returns [`TemporalError::MissingObservedAt`] if `observed_at` is `None`.
    /// Returns [`TemporalError::ValidUntilBeforeObservedAt`] if `valid_until < observed_at`.
    /// Returns [`TemporalError::OccurredAtAfterObservedAt`] if `occurred_at > observed_at`.
    pub fn validate(
        input: &RawTemporalInput,
        decay_profile: &DecayProfile,
    ) -> Result<TemporalFields, TemporalError> {
        // T1: observed_at is NEVER null
        let observed_at = input.observed_at.ok_or(TemporalError::MissingObservedAt)?;

        // T2: valid_until is NEVER null (compute from decay profile if missing)
        let valid_until = input
            .valid_until
            .unwrap_or_else(|| decay_profile.compute_valid_until(observed_at));

        // T3: temporal_precision defaults to Inferred
        let temporal_precision = input
            .temporal_precision
            .unwrap_or(TemporalPrecision::Inferred);

        // T4: valid_until >= observed_at
        if valid_until < observed_at {
            return Err(TemporalError::ValidUntilBeforeObservedAt {
                observed_at: observed_at.to_rfc3339(),
                valid_until: valid_until.to_rfc3339(),
            });
        }

        // T5: occurred_at <= observed_at
        if let Some(occurred_at) = input.occurred_at {
            if occurred_at > observed_at {
                return Err(TemporalError::OccurredAtAfterObservedAt {
                    observed_at: observed_at.to_rfc3339(),
                    occurred_at: occurred_at.to_rfc3339(),
                });
            }
        }

        Ok(TemporalFields {
            observed_at,
            valid_until,
            temporal_precision,
            occurred_at: input.occurred_at,
        })
    }

    /// Validate already-constructed [`TemporalFields`] (e.g., from deserialization).
    ///
    /// Checks T4 and T5 invariants. T1-T3 are guaranteed by the type.
    ///
    /// # Errors
    ///
    /// Returns [`TemporalError::ValidUntilBeforeObservedAt`] if `valid_until < observed_at`.
    /// Returns [`TemporalError::OccurredAtAfterObservedAt`] if `occurred_at > observed_at`.
    pub fn validate_fields(fields: &TemporalFields) -> Result<(), TemporalError> {
        // T4: valid_until >= observed_at
        if fields.valid_until < fields.observed_at {
            return Err(TemporalError::ValidUntilBeforeObservedAt {
                observed_at: fields.observed_at.to_rfc3339(),
                valid_until: fields.valid_until.to_rfc3339(),
            });
        }

        // T5: occurred_at <= observed_at
        if let Some(occurred_at) = fields.occurred_at {
            if occurred_at > fields.observed_at {
                return Err(TemporalError::OccurredAtAfterObservedAt {
                    observed_at: fields.observed_at.to_rfc3339(),
                    occurred_at: occurred_at.to_rfc3339(),
                });
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn utc(y: i32, m: u32, d: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(y, m, d, 0, 0, 0).unwrap()
    }

    // === TemporalPrecision tests ===

    #[test]
    fn temporal_precision_default_is_inferred() {
        assert_eq!(TemporalPrecision::default(), TemporalPrecision::Inferred);
    }

    #[test]
    fn temporal_precision_ordering() {
        assert!(TemporalPrecision::Exact < TemporalPrecision::Day);
        assert!(TemporalPrecision::Day < TemporalPrecision::Inferred);
    }

    // === TemporalGate tests (T-100.2) ===

    #[test]
    fn gate_rejects_null_observed_at() {
        let input = RawTemporalInput::default();
        let profile = DecayProfile::default_profile();

        let result = TemporalGate::validate(&input, &profile);
        assert!(result.is_err());

        let err = result.unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("observed_at"),
            "Error should mention observed_at: {msg}"
        );
    }

    #[test]
    fn gate_accepts_complete_temporal_fields() {
        let input = RawTemporalInput {
            observed_at: Some(utc(2025, 2, 10)),
            valid_until: Some(utc(2025, 8, 10)),
            temporal_precision: Some(TemporalPrecision::Day),
            occurred_at: None,
        };
        let profile = DecayProfile::default_profile();

        let result = TemporalGate::validate(&input, &profile);
        assert!(result.is_ok());

        let fields = result.unwrap();
        assert_eq!(fields.observed_at, utc(2025, 2, 10));
        assert_eq!(fields.valid_until, utc(2025, 8, 10));
        assert_eq!(fields.temporal_precision, TemporalPrecision::Day);
    }

    #[test]
    fn gate_computes_valid_until_from_decay_profile() {
        let input = RawTemporalInput {
            observed_at: Some(utc(2025, 1, 1)),
            valid_until: None, // not provided — gate should compute
            temporal_precision: None,
            occurred_at: None,
        };
        let profile = DecayProfile::default_profile(); // 90-day half-life, 180-day validity

        let result = TemporalGate::validate(&input, &profile);
        assert!(result.is_ok());

        let fields = result.unwrap();
        // Default profile: 90-day half-life, 2x = 180 days validity
        let expected_valid_until = utc(2025, 1, 1) + Duration::days(180);
        assert_eq!(fields.valid_until, expected_valid_until);
    }

    #[test]
    fn gate_defaults_precision_to_inferred() {
        let input = RawTemporalInput {
            observed_at: Some(utc(2025, 2, 10)),
            valid_until: None,
            temporal_precision: None, // not provided
            occurred_at: None,
        };
        let profile = DecayProfile::default_profile();

        let fields = TemporalGate::validate(&input, &profile).unwrap();
        assert_eq!(fields.temporal_precision, TemporalPrecision::Inferred);
    }

    #[test]
    fn gate_returns_rejection_with_actionable_suggestion() {
        let input = RawTemporalInput::default();
        let profile = DecayProfile::default_profile();

        let err = TemporalGate::validate(&input, &profile).unwrap_err();
        let msg = err.to_string();

        // Must contain actionable information
        assert!(
            msg.contains("REJECTED"),
            "Error should be marked as REJECTED"
        );
        assert!(
            msg.contains("observed_at"),
            "Error should mention the missing field"
        );
    }

    #[test]
    fn gate_rejects_valid_until_before_observed_at() {
        let input = RawTemporalInput {
            observed_at: Some(utc(2025, 6, 1)),
            valid_until: Some(utc(2025, 1, 1)), // before observed_at!
            temporal_precision: Some(TemporalPrecision::Day),
            occurred_at: None,
        };
        let profile = DecayProfile::default_profile();

        let err = TemporalGate::validate(&input, &profile).unwrap_err();
        assert!(matches!(
            err,
            TemporalError::ValidUntilBeforeObservedAt { .. }
        ));
    }

    #[test]
    fn gate_rejects_occurred_at_after_observed_at() {
        let input = RawTemporalInput {
            observed_at: Some(utc(2025, 1, 1)),
            valid_until: None,
            temporal_precision: None,
            occurred_at: Some(utc(2025, 6, 1)), // after observed_at!
        };
        let profile = DecayProfile::default_profile();

        let err = TemporalGate::validate(&input, &profile).unwrap_err();
        assert!(matches!(
            err,
            TemporalError::OccurredAtAfterObservedAt { .. }
        ));
    }

    #[test]
    fn gate_accepts_occurred_at_before_observed_at() {
        let input = RawTemporalInput {
            observed_at: Some(utc(2025, 6, 1)),
            valid_until: None,
            temporal_precision: None,
            occurred_at: Some(utc(2025, 1, 1)), // before observed_at, valid
        };
        let profile = DecayProfile::default_profile();

        let result = TemporalGate::validate(&input, &profile);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().occurred_at, Some(utc(2025, 1, 1)));
    }

    #[test]
    fn validate_fields_checks_existing_temporal_fields() {
        let fields = TemporalFields {
            observed_at: utc(2025, 6, 1),
            valid_until: utc(2025, 1, 1), // invalid!
            temporal_precision: TemporalPrecision::Day,
            occurred_at: None,
        };

        let result = TemporalGate::validate_fields(&fields);
        assert!(result.is_err());
    }

    // === DecayProfile tests ===

    #[test]
    fn project_status_decays_in_14_days() {
        let profile = DecayProfile::project_status();
        assert_eq!(profile.half_life, Duration::days(14));

        let observed = utc(2025, 1, 1);
        let valid_until = profile.compute_valid_until(observed);
        assert_eq!(valid_until, utc(2025, 1, 1) + Duration::days(28));
    }

    #[test]
    fn decision_never_decays() {
        let profile = DecayProfile::decision();
        assert!(profile.half_life > Duration::days(365 * 50));
    }

    #[test]
    fn signal_decays_in_7_days() {
        let profile = DecayProfile::signal();
        assert_eq!(profile.half_life, Duration::days(7));
    }
}
