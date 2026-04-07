use crate::save::tables::*;
use serde::{Deserialize, Serialize};

/// Structured evidence for a multiway cash validation gate.
///
/// Each seat-count gate (3-max, 6-max, 10-max) records explicit PASS/FAIL
/// evidence for the profile's clustering, training, and serving readiness.
/// Benchmark fields are required for non-pending verdicts so promotion
/// decisions cite measured data, not just code-path existence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GateRecord {
    pub profile_id: String,
    pub abstraction_version: String,
    pub engine_version: String,
    pub info_version: String,
    pub seat_count: usize,
    pub clustering_status: GateStatus,
    pub training_status: GateStatus,
    pub serving_status: GateStatus,
    pub benchmarks: GateBenchmarks,
    pub verdict: GateVerdict,
    pub notes: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum GateVerdict {
    Pass,
    Fail,
    Pending,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum GateStatus {
    Pass,
    Fail,
    Untested,
}

/// Benchmark measurements required for promotion decisions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GateBenchmarks {
    pub memory_mb: Option<f64>,
    pub db_size_mb: Option<f64>,
    pub clustering_runtime_secs: Option<f64>,
    pub training_runtime_secs: Option<f64>,
    pub query_latency_ms: Option<f64>,
}

impl GateBenchmarks {
    pub fn has_any(&self) -> bool {
        self.memory_mb.is_some()
            || self.db_size_mb.is_some()
            || self.clustering_runtime_secs.is_some()
            || self.training_runtime_secs.is_some()
            || self.query_latency_ms.is_some()
    }
}

impl GateRecord {
    /// Validate that the gate record is internally consistent.
    /// Non-pending verdicts require at least one benchmark measurement.
    pub fn validate(&self) -> Result<(), String> {
        if self.profile_id.is_empty() {
            return Err("profile_id is required".to_string());
        }
        if self.abstraction_version.is_empty() {
            return Err("abstraction_version is required".to_string());
        }
        if self.engine_version.is_empty() {
            return Err("engine_version is required".to_string());
        }
        if self.info_version.is_empty() {
            return Err("info_version is required".to_string());
        }
        if !(2..=10).contains(&self.seat_count) {
            return Err(format!("seat_count must be 2-10 (got {})", self.seat_count));
        }
        if self.verdict != GateVerdict::Pending && !self.benchmarks.has_any() {
            return Err(
                "non-pending verdict requires at least one benchmark measurement".to_string(),
            );
        }
        Ok(())
    }
}

/// Canonical 3-max cash profile definition for the first validation gate.
pub fn canonical_3max_cash_profile() -> (TrainingTables, TrainingProfileConfig) {
    let tables = TrainingTables::new("bp_3max_cash", "abs_v4_p3");
    let config = TrainingProfileConfig::from_json(
        &serde_json::json!({
            "player_count": 3,
            "format": "cash",
            "abstraction_version": "abs_v4_p3",
            "blinds": "1/2",
            "ante": 0,
            "stack_bb": 50
        })
        .to_string(),
    )
    .expect("canonical 3-max cash profile config");
    (tables, config)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::save::tables::AbstractionTables;

    // --- RPM-08 spec-required tests ---

    #[test]
    fn test_3max_profile_round_trip_gate() {
        let (tables, config) = canonical_3max_cash_profile();

        assert_eq!(tables.profile.profile_key(), "bp_3max_cash");
        assert_eq!(tables.abstraction.abstraction_version(), "abs_v4_p3");
        assert!(!tables.profile.is_default_hu());
        assert!(!tables.abstraction.is_default_v1());
        assert_eq!(tables.profile.info_version(), InfoVersion::V2);

        let parsed = tables.abstraction.parsed_version().unwrap();
        assert_eq!(parsed.player_count(), 3);
        assert!(parsed.is_v4_or_newer());

        assert_eq!(config.resolved_schedule().len(), 1);
        let level = &config.resolved_schedule()[0];
        assert_eq!(level.sb, 1);
        assert_eq!(level.bb, 2);
        assert_eq!(level.ante, 0);

        let json = serde_json::json!({
            "player_count": 3,
            "format": "cash",
            "abstraction_version": "abs_v4_p3",
            "blinds": "1/2",
            "stack_bb": 50
        });
        let roundtrip = TrainingProfileConfig::from_json(&json.to_string()).unwrap();
        assert_eq!(
            roundtrip.resolved_schedule().len(),
            config.resolved_schedule().len()
        );
    }

    #[test]
    fn test_3max_analysis_reads_profile_native_tables() {
        let (tables, _) = canonical_3max_cash_profile();

        // Analysis must route to profile-scoped tables, not static defaults
        assert_eq!(tables.abstraction.abstraction(), "abstraction_abs_v4_p3");
        assert_eq!(tables.abstraction.isomorphism(), "isomorphism_abs_v4_p3");
        assert_eq!(tables.abstraction.metric(), "metric_abs_v4_p3");
        assert_eq!(tables.abstraction.transitions(), "transitions_abs_v4_p3");
        assert_eq!(tables.profile.blueprint(), "blueprint_bp_3max_cash");

        // Must not collide with default HU tables
        let hu = TrainingTables::default_hu();
        assert_ne!(
            tables.abstraction.abstraction(),
            hu.abstraction.abstraction()
        );
        assert_ne!(tables.profile.blueprint(), hu.profile.blueprint());

        // V2 abstraction requires seat-qualified observations
        assert!(tables.abstraction.uses_exact_seat_lookup());
    }

    #[test]
    fn test_6max_profile_rejects_heads_up_fallback() {
        let tables_6max = TrainingTables::new("bp_6max_cash", "abs_v4_p6");

        // 6-max must not be default HU
        assert!(!tables_6max.profile.is_default_hu());
        assert!(!tables_6max.abstraction.is_default_v1());
        assert_eq!(tables_6max.profile.info_version(), InfoVersion::V2);

        // Parsed version must reflect 6-max
        let parsed = tables_6max.abstraction.parsed_version().unwrap();
        assert_eq!(parsed.player_count(), 6);

        // Table names must be 6-max-specific, not HU defaults
        assert_eq!(
            tables_6max.abstraction.isomorphism(),
            "isomorphism_abs_v4_p6"
        );
        assert_ne!(
            tables_6max.abstraction.isomorphism(),
            AbstractionTables::default_v1().isomorphism()
        );

        // Must not collide with 3-max tables either
        let tables_3max = TrainingTables::new("bp_3max_cash", "abs_v4_p3");
        assert_ne!(
            tables_6max.profile.blueprint(),
            tables_3max.profile.blueprint()
        );
        assert_ne!(
            tables_6max.abstraction.isomorphism(),
            tables_3max.abstraction.isomorphism()
        );
    }

    #[test]
    fn test_gate_record_requires_benchmark_fields() {
        // A PASS verdict without benchmarks must be rejected
        let record = GateRecord {
            profile_id: "bp_3max_cash".to_string(),
            abstraction_version: "abs_v4_p3".to_string(),
            engine_version: "v2".to_string(),
            info_version: "V2".to_string(),
            seat_count: 3,
            clustering_status: GateStatus::Pass,
            training_status: GateStatus::Pass,
            serving_status: GateStatus::Pass,
            benchmarks: GateBenchmarks {
                memory_mb: None,
                db_size_mb: None,
                clustering_runtime_secs: None,
                training_runtime_secs: None,
                query_latency_ms: None,
            },
            verdict: GateVerdict::Pass,
            notes: String::new(),
        };
        let err = record.validate().unwrap_err();
        assert!(
            err.contains("benchmark"),
            "expected benchmark error, got: {}",
            err
        );

        // Pending verdict allows empty benchmarks
        let mut pending = record.clone();
        pending.verdict = GateVerdict::Pending;
        assert!(pending.validate().is_ok());

        // PASS verdict with at least one benchmark is accepted
        let mut with_bench = record;
        with_bench.benchmarks.memory_mb = Some(128.0);
        assert!(with_bench.validate().is_ok());

        // FAIL verdict also requires benchmarks
        let fail_no_bench = GateRecord {
            profile_id: "bp_3max_cash".to_string(),
            abstraction_version: "abs_v4_p3".to_string(),
            engine_version: "v2".to_string(),
            info_version: "V2".to_string(),
            seat_count: 3,
            clustering_status: GateStatus::Fail,
            training_status: GateStatus::Untested,
            serving_status: GateStatus::Untested,
            benchmarks: GateBenchmarks {
                memory_mb: None,
                db_size_mb: None,
                clustering_runtime_secs: None,
                training_runtime_secs: None,
                query_latency_ms: None,
            },
            verdict: GateVerdict::Fail,
            notes: "clustering OOM".to_string(),
        };
        assert!(fail_no_bench.validate().is_err());
    }
}
