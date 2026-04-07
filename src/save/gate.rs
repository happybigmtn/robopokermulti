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

/// A single row in the benchmark matrix, recording performance measurements
/// for one training configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkEntry {
    pub seat_count: usize,
    pub profile_id: String,
    pub abstraction_version: String,
    pub batch_size: Option<usize>,
    pub tree_count: Option<usize>,
    pub clustering_runtime_secs: Option<f64>,
    pub training_throughput_iters_per_sec: Option<f64>,
    pub memory_pressure_mb: Option<f64>,
    pub db_footprint_mb: Option<f64>,
    pub strategy_lookup_latency_ms: Option<f64>,
}

impl BenchmarkEntry {
    pub fn validate(&self) -> Result<(), String> {
        if !(2..=10).contains(&self.seat_count) {
            return Err(format!("seat_count must be 2-10 (got {})", self.seat_count));
        }
        if self.profile_id.is_empty() {
            return Err("profile_id is required".to_string());
        }
        if self.abstraction_version.is_empty() {
            return Err("abstraction_version is required".to_string());
        }
        let has_any = self.clustering_runtime_secs.is_some()
            || self.training_throughput_iters_per_sec.is_some()
            || self.memory_pressure_mb.is_some()
            || self.db_footprint_mb.is_some()
            || self.strategy_lookup_latency_ms.is_some();
        if !has_any {
            return Err("benchmark entry must include at least one measurement".to_string());
        }
        Ok(())
    }
}

/// Quality evidence required for blueprint promotion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualityGate {
    pub seat_count: usize,
    pub profile_id: String,
    pub self_play_stability: Option<QualitySignal>,
    pub exploitability_proxy: Option<QualitySignal>,
    pub policy_smoothness: Option<QualitySignal>,
    pub restart_determinism: Option<QualitySignal>,
}

/// A single quality measurement with a value and pass/fail evaluation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualitySignal {
    pub value: f64,
    pub description: String,
    pub pass: bool,
}

impl QualityGate {
    pub fn validate(&self) -> Result<(), String> {
        if !(2..=10).contains(&self.seat_count) {
            return Err(format!("seat_count must be 2-10 (got {})", self.seat_count));
        }
        if self.profile_id.is_empty() {
            return Err("profile_id is required".to_string());
        }
        if self.restart_determinism.is_none() {
            return Err("restart_determinism is required for quality gates".to_string());
        }
        if self.policy_smoothness.is_none() {
            return Err("policy_smoothness is required for quality gates".to_string());
        }
        Ok(())
    }
}

/// Validates that a 10-max gate attempt has recorded prior 3-max and 6-max evidence.
pub fn validate_10max_prerequisites(prior_gates: &[GateRecord]) -> Result<(), String> {
    let has_3max_pass = prior_gates
        .iter()
        .any(|g| g.seat_count == 3 && g.verdict == GateVerdict::Pass);
    let has_6max_pass = prior_gates
        .iter()
        .any(|g| g.seat_count == 6 && g.verdict == GateVerdict::Pass);
    if !has_3max_pass {
        return Err("10-max gate requires prior 3-max PASS evidence".to_string());
    }
    if !has_6max_pass {
        return Err("10-max gate requires prior 6-max PASS evidence".to_string());
    }
    Ok(())
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

/// Canonical 6-max cash profile definition for the second validation gate.
pub fn canonical_6max_cash_profile() -> (TrainingTables, TrainingProfileConfig) {
    let tables = TrainingTables::new("bp_6max_cash", "abs_v4_p6");
    let config = TrainingProfileConfig::from_json(
        &serde_json::json!({
            "player_count": 6,
            "format": "cash",
            "abstraction_version": "abs_v4_p6",
            "blinds": "1/2",
            "ante": 0,
            "stack_bb": 50
        })
        .to_string(),
    )
    .expect("canonical 6-max cash profile config");
    (tables, config)
}

/// Canonical 10-max cash profile definition for the experimental pilot gate.
/// 10-max is explicitly experimental until 3-max and 6-max gates pass.
pub fn canonical_10max_cash_profile() -> (TrainingTables, TrainingProfileConfig) {
    let tables = TrainingTables::new("bp_10max_cash", "abs_v4_p10");
    let config = TrainingProfileConfig::from_json(
        &serde_json::json!({
            "player_count": 10,
            "format": "cash",
            "abstraction_version": "abs_v4_p10",
            "blinds": "1/2",
            "ante": 0,
            "stack_bb": 50
        })
        .to_string(),
    )
    .expect("canonical 10-max cash profile config");
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

    // --- RPM-09 spec-required tests ---

    #[test]
    fn test_6max_profile_round_trip_gate() {
        let (tables, config) = canonical_6max_cash_profile();

        assert_eq!(tables.profile.profile_key(), "bp_6max_cash");
        assert_eq!(tables.abstraction.abstraction_version(), "abs_v4_p6");
        assert!(!tables.profile.is_default_hu());
        assert!(!tables.abstraction.is_default_v1());
        assert_eq!(tables.profile.info_version(), InfoVersion::V2);

        let parsed = tables.abstraction.parsed_version().unwrap();
        assert_eq!(parsed.player_count(), 6);
        assert!(parsed.is_v4_or_newer());

        assert_eq!(config.resolved_schedule().len(), 1);
        let level = &config.resolved_schedule()[0];
        assert_eq!(level.sb, 1);
        assert_eq!(level.bb, 2);

        let json = serde_json::json!({
            "player_count": 6,
            "format": "cash",
            "abstraction_version": "abs_v4_p6",
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
    fn test_6max_scales_beyond_3max_without_schema_shortcuts() {
        let (tables_3, _) = canonical_3max_cash_profile();
        let (tables_6, _) = canonical_6max_cash_profile();

        // 6-max uses V2 schema, same as 3-max — no schema shortcut
        assert_eq!(tables_3.profile.info_version(), InfoVersion::V2);
        assert_eq!(tables_6.profile.info_version(), InfoVersion::V2);

        // Both use v4 abstraction with exact seat lookup
        assert!(tables_3.abstraction.uses_exact_seat_lookup());
        assert!(tables_6.abstraction.uses_exact_seat_lookup());
        assert!(tables_3.abstraction.uses_multiway_v4_bucketing());
        assert!(tables_6.abstraction.uses_multiway_v4_bucketing());

        // Table names are fully isolated between seat counts
        assert_ne!(tables_3.profile.blueprint(), tables_6.profile.blueprint());
        assert_ne!(
            tables_3.abstraction.isomorphism(),
            tables_6.abstraction.isomorphism()
        );
        assert_ne!(
            tables_3.abstraction.abstraction(),
            tables_6.abstraction.abstraction()
        );

        // Neither collides with default HU
        let hu = TrainingTables::default_hu();
        assert_ne!(tables_6.profile.blueprint(), hu.profile.blueprint());
        assert_ne!(
            tables_6.abstraction.isomorphism(),
            hu.abstraction.isomorphism()
        );
    }

    #[test]
    fn test_6max_gate_record_round_trip() {
        let record = GateRecord {
            profile_id: "bp_6max_cash".to_string(),
            abstraction_version: "abs_v4_p6".to_string(),
            engine_version: "v2".to_string(),
            info_version: "V2".to_string(),
            seat_count: 6,
            clustering_status: GateStatus::Untested,
            training_status: GateStatus::Untested,
            serving_status: GateStatus::Untested,
            benchmarks: GateBenchmarks {
                memory_mb: None,
                db_size_mb: None,
                clustering_runtime_secs: None,
                training_runtime_secs: None,
                query_latency_ms: None,
            },
            verdict: GateVerdict::Pending,
            notes: "awaiting 3-max gate pass".to_string(),
        };
        assert!(record.validate().is_ok());

        let json = serde_json::to_string(&record).unwrap();
        let deserialized: GateRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.profile_id, "bp_6max_cash");
        assert_eq!(deserialized.seat_count, 6);
        assert_eq!(deserialized.verdict, GateVerdict::Pending);
    }

    // --- RPM-10 spec-required tests ---

    #[test]
    fn test_benchmark_matrix_records_required_dimensions() {
        // Valid benchmark entry with all measurements
        let entry = BenchmarkEntry {
            seat_count: 3,
            profile_id: "bp_3max_cash".to_string(),
            abstraction_version: "abs_v4_p3".to_string(),
            batch_size: Some(128),
            tree_count: Some(0x10000000),
            clustering_runtime_secs: Some(45.2),
            training_throughput_iters_per_sec: Some(1200.0),
            memory_pressure_mb: Some(512.0),
            db_footprint_mb: Some(80.0),
            strategy_lookup_latency_ms: Some(2.5),
        };
        assert!(entry.validate().is_ok());

        // Entry without any measurements is rejected
        let empty = BenchmarkEntry {
            seat_count: 6,
            profile_id: "bp_6max_cash".to_string(),
            abstraction_version: "abs_v4_p6".to_string(),
            batch_size: None,
            tree_count: None,
            clustering_runtime_secs: None,
            training_throughput_iters_per_sec: None,
            memory_pressure_mb: None,
            db_footprint_mb: None,
            strategy_lookup_latency_ms: None,
        };
        let err = empty.validate().unwrap_err();
        assert!(err.contains("measurement"), "got: {}", err);

        // Entry without profile_id is rejected
        let no_profile = BenchmarkEntry {
            seat_count: 3,
            profile_id: String::new(),
            abstraction_version: "abs_v4_p3".to_string(),
            batch_size: None,
            tree_count: None,
            clustering_runtime_secs: Some(10.0),
            training_throughput_iters_per_sec: None,
            memory_pressure_mb: None,
            db_footprint_mb: None,
            strategy_lookup_latency_ms: None,
        };
        assert!(no_profile.validate().is_err());

        // Round-trip serialization preserves all dimensions
        let json = serde_json::to_string(&entry).unwrap();
        let rt: BenchmarkEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(rt.seat_count, 3);
        assert_eq!(rt.batch_size, Some(128));
        assert_eq!(rt.clustering_runtime_secs, Some(45.2));
    }

    #[test]
    fn test_quality_gate_requires_restart_determinism() {
        let gate = QualityGate {
            seat_count: 3,
            profile_id: "bp_3max_cash".to_string(),
            self_play_stability: Some(QualitySignal {
                value: 0.98,
                description: "win rate variance < 2%".to_string(),
                pass: true,
            }),
            exploitability_proxy: Some(QualitySignal {
                value: 0.05,
                description: "regret sum below threshold".to_string(),
                pass: true,
            }),
            policy_smoothness: Some(QualitySignal {
                value: 0.92,
                description: "adjacent-state policy correlation".to_string(),
                pass: true,
            }),
            restart_determinism: None,
        };
        let err = gate.validate().unwrap_err();
        assert!(
            err.contains("restart_determinism"),
            "expected restart_determinism error, got: {}",
            err
        );

        // With restart_determinism present, validation passes
        let mut complete = gate;
        complete.restart_determinism = Some(QualitySignal {
            value: 1.0,
            description: "identical output across restarts".to_string(),
            pass: true,
        });
        assert!(complete.validate().is_ok());
    }

    #[test]
    fn test_quality_gate_requires_policy_smoothness_signal() {
        let gate = QualityGate {
            seat_count: 6,
            profile_id: "bp_6max_cash".to_string(),
            self_play_stability: None,
            exploitability_proxy: None,
            policy_smoothness: None,
            restart_determinism: Some(QualitySignal {
                value: 1.0,
                description: "deterministic".to_string(),
                pass: true,
            }),
        };
        let err = gate.validate().unwrap_err();
        assert!(
            err.contains("policy_smoothness"),
            "expected policy_smoothness error, got: {}",
            err
        );
    }

    #[test]
    fn test_10max_gate_requires_prior_3max_and_6max_evidence() {
        let pass_3max = GateRecord {
            profile_id: "bp_3max_cash".to_string(),
            abstraction_version: "abs_v4_p3".to_string(),
            engine_version: "v2".to_string(),
            info_version: "V2".to_string(),
            seat_count: 3,
            clustering_status: GateStatus::Pass,
            training_status: GateStatus::Pass,
            serving_status: GateStatus::Pass,
            benchmarks: GateBenchmarks {
                memory_mb: Some(256.0),
                db_size_mb: Some(80.0),
                clustering_runtime_secs: Some(45.0),
                training_runtime_secs: Some(3600.0),
                query_latency_ms: Some(2.0),
            },
            verdict: GateVerdict::Pass,
            notes: String::new(),
        };

        let pass_6max = GateRecord {
            profile_id: "bp_6max_cash".to_string(),
            abstraction_version: "abs_v4_p6".to_string(),
            engine_version: "v2".to_string(),
            info_version: "V2".to_string(),
            seat_count: 6,
            clustering_status: GateStatus::Pass,
            training_status: GateStatus::Pass,
            serving_status: GateStatus::Pass,
            benchmarks: GateBenchmarks {
                memory_mb: Some(1024.0),
                db_size_mb: Some(400.0),
                clustering_runtime_secs: Some(200.0),
                training_runtime_secs: Some(14400.0),
                query_latency_ms: Some(5.0),
            },
            verdict: GateVerdict::Pass,
            notes: String::new(),
        };

        // No prior evidence — rejected
        let err = validate_10max_prerequisites(&[]).unwrap_err();
        assert!(err.contains("3-max"), "got: {}", err);

        // Only 3-max — rejected
        let err = validate_10max_prerequisites(&[pass_3max.clone()]).unwrap_err();
        assert!(err.contains("6-max"), "got: {}", err);

        // Only 6-max — rejected
        let err = validate_10max_prerequisites(&[pass_6max.clone()]).unwrap_err();
        assert!(err.contains("3-max"), "got: {}", err);

        // Both 3-max and 6-max PASS — accepted
        assert!(validate_10max_prerequisites(&[pass_3max.clone(), pass_6max.clone()]).is_ok());

        // 3-max PENDING + 6-max PASS — rejected
        let mut pending_3max = pass_3max;
        pending_3max.verdict = GateVerdict::Pending;
        let err = validate_10max_prerequisites(&[pending_3max, pass_6max]).unwrap_err();
        assert!(err.contains("3-max"), "got: {}", err);
    }

    // --- RPM-11 spec-required tests ---

    #[test]
    fn test_10max_profile_round_trip_pilot() {
        let (tables, config) = canonical_10max_cash_profile();

        assert_eq!(tables.profile.profile_key(), "bp_10max_cash");
        assert_eq!(tables.abstraction.abstraction_version(), "abs_v4_p10");
        assert!(!tables.profile.is_default_hu());
        assert!(!tables.abstraction.is_default_v1());
        assert_eq!(tables.profile.info_version(), InfoVersion::V2);

        let parsed = tables.abstraction.parsed_version().unwrap();
        assert_eq!(parsed.player_count(), 10);
        assert!(parsed.is_v4_or_newer());

        assert_eq!(config.resolved_schedule().len(), 1);

        // Table names are 10-max-specific
        assert_eq!(tables.abstraction.isomorphism(), "isomorphism_abs_v4_p10");
        assert_eq!(tables.profile.blueprint(), "blueprint_bp_10max_cash");
    }

    #[test]
    fn test_10max_tables_isolated_from_lower_seat_counts() {
        let (t3, _) = canonical_3max_cash_profile();
        let (t6, _) = canonical_6max_cash_profile();
        let (t10, _) = canonical_10max_cash_profile();

        // All three use V2 schema
        assert_eq!(t3.profile.info_version(), InfoVersion::V2);
        assert_eq!(t6.profile.info_version(), InfoVersion::V2);
        assert_eq!(t10.profile.info_version(), InfoVersion::V2);

        // All table names are fully distinct
        let blueprints = [
            t3.profile.blueprint(),
            t6.profile.blueprint(),
            t10.profile.blueprint(),
        ];
        let isos = [
            t3.abstraction.isomorphism(),
            t6.abstraction.isomorphism(),
            t10.abstraction.isomorphism(),
        ];
        for i in 0..3 {
            for j in (i + 1)..3 {
                assert_ne!(blueprints[i], blueprints[j]);
                assert_ne!(isos[i], isos[j]);
            }
        }
    }

    #[test]
    fn test_10max_gate_record_records_experimental_status() {
        let record = GateRecord {
            profile_id: "bp_10max_cash".to_string(),
            abstraction_version: "abs_v4_p10".to_string(),
            engine_version: "v2".to_string(),
            info_version: "V2".to_string(),
            seat_count: 10,
            clustering_status: GateStatus::Untested,
            training_status: GateStatus::Untested,
            serving_status: GateStatus::Untested,
            benchmarks: GateBenchmarks {
                memory_mb: None,
                db_size_mb: None,
                clustering_runtime_secs: None,
                training_runtime_secs: None,
                query_latency_ms: None,
            },
            verdict: GateVerdict::Pending,
            notes: "experimental pilot — requires 3-max and 6-max PASS first".to_string(),
        };
        assert!(record.validate().is_ok());

        // Cannot upgrade to PASS without benchmarks
        let mut pass_attempt = record;
        pass_attempt.verdict = GateVerdict::Pass;
        assert!(pass_attempt.validate().is_err());

        // Can record FAIL with benchmark evidence (measured limit)
        pass_attempt.verdict = GateVerdict::Fail;
        pass_attempt.benchmarks.memory_mb = Some(8192.0);
        pass_attempt.notes = "OOM during clustering at 10-max".to_string();
        assert!(pass_attempt.validate().is_ok());
    }

    // --- RPM-12 spec-required tests ---

    #[test]
    fn test_tournament_profile_uses_payout_curve_utility() {
        let raw = serde_json::json!({
            "player_count": 6,
            "format": "tournament",
            "abstraction_version": "abs_v4_p6",
            "blind_schedule": [
                { "sb": 1, "bb": 2, "ante": 0, "duration": 20 },
                { "sb": 2, "bb": 4, "ante": 1, "duration": 10 }
            ],
            "stack_bb_range": [20, 50],
            "payout_curve": [0.50, 0.25, 0.15, 0.10]
        });
        let config = TrainingProfileConfig::from_json(&raw.to_string()).unwrap();
        let payout = config.tournament_payout().expect("payout curve resolved");

        assert_eq!(payout.payouts().len(), 4);
        let sum: f32 = payout.payouts().iter().sum();
        assert!((sum - 1.0).abs() < 1e-6, "payouts must normalize to 1.0");

        // Cash profiles do not produce tournament payouts
        let cash_raw = serde_json::json!({
            "player_count": 6,
            "format": "cash",
            "abstraction_version": "abs_v4_p6",
            "blinds": "1/2",
            "stack_bb": 50
        });
        let cash_config = TrainingProfileConfig::from_json(&cash_raw.to_string()).unwrap();
        assert!(cash_config.tournament_payout().is_none());
    }

    #[test]
    fn test_tournament_utility_training_uses_profile_metadata() {
        let tables = TrainingTables::new("bp_6max_tourney", "abs_v4_p6");

        // Tournament training profile uses V2 schema and profile-scoped tables
        assert!(!tables.profile.is_default_hu());
        assert!(!tables.abstraction.is_default_v1());
        assert_eq!(tables.profile.info_version(), InfoVersion::V2);
        assert_eq!(tables.profile.blueprint(), "blueprint_bp_6max_tourney");
        assert_eq!(tables.abstraction.isomorphism(), "isomorphism_abs_v4_p6");

        // Tournament profile config resolves blind schedule for training
        let raw = serde_json::json!({
            "player_count": 6,
            "format": "tournament",
            "abstraction_version": "abs_v4_p6",
            "blind_schedule": [
                { "sb": 1, "bb": 2, "ante": 0, "weight": 3.0 },
                { "sb": 2, "bb": 4, "ante": 1, "weight": 2.0 },
                { "sb": 5, "bb": 10, "ante": 2, "weight": 1.0 }
            ],
            "stack_bb_range": [15, 40],
            "payout_curve": [0.5, 0.3, 0.2]
        });
        let config = TrainingProfileConfig::from_json(&raw.to_string()).unwrap();
        assert_eq!(config.format(), TrainingFormat::Tournament);
        assert_eq!(config.resolved_schedule().len(), 3);

        // Deterministic epoch sampling
        let tc_a = config.table_config_for_epoch(42);
        let tc_b = config.table_config_for_epoch(42);
        assert_eq!(tc_a, tc_b);
        assert_eq!(tc_a.seat_count, 6);
    }

    #[test]
    fn test_tournament_utility_docs_exclude_full_lifecycle_claims() {
        // Tournament profile config rejects configs without blind_schedule
        let no_schedule = serde_json::json!({
            "player_count": 3,
            "format": "tournament",
            "abstraction_version": "abs_v4_p3",
            "payout_curve": [0.6, 0.3, 0.1]
        });
        let err = TrainingProfileConfig::from_json(&no_schedule.to_string()).unwrap_err();
        assert!(
            err.contains("blind_schedule"),
            "tournament requires explicit schedule, got: {}",
            err
        );

        // Tournament profile rejects payout_curve longer than player_count
        let over_payout = serde_json::json!({
            "player_count": 2,
            "format": "tournament",
            "abstraction_version": "abs_v4_p2",
            "blind_schedule": [{ "sb": 1, "bb": 2, "ante": 0 }],
            "payout_curve": [0.5, 0.3, 0.2]
        });
        let err = TrainingProfileConfig::from_json(&over_payout.to_string()).unwrap_err();
        assert!(
            err.contains("payout_curve"),
            "payout cannot exceed player count, got: {}",
            err
        );

        // Tournament config without payout_curve is rejected
        let no_payout = serde_json::json!({
            "player_count": 3,
            "format": "tournament",
            "abstraction_version": "abs_v4_p3",
            "blind_schedule": [{ "sb": 1, "bb": 2, "ante": 0 }]
        });
        let err = TrainingProfileConfig::from_json(&no_payout.to_string()).unwrap_err();
        assert!(
            err.contains("payout_curve"),
            "tournament requires payout_curve, got: {}",
            err
        );
    }
}
