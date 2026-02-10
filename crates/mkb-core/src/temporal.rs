//! Temporal types and the temporal gate for MKB.
//!
//! Core invariant: **No information enters the vault without `observed_at`.**

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn temporal_precision_default_is_inferred() {
        assert_eq!(TemporalPrecision::default(), TemporalPrecision::Inferred);
    }

    #[test]
    fn temporal_precision_ordering() {
        assert!(TemporalPrecision::Exact < TemporalPrecision::Day);
        assert!(TemporalPrecision::Day < TemporalPrecision::Inferred);
    }
}
