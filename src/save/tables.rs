//! Table name constants and dynamic profile registry.
//!
//! Static constants for backwards compatibility with heads-up training.
//! For multiway training, use `ProfileTables` and `AbstractionTables`
//! to generate profile-scoped table names dynamically.

use crate::gameplay::{TableConfig, TournamentPayout};
use crate::{B_BLIND, STACK};
use rand::rngs::SmallRng;
use rand::{Rng, SeedableRng};
use serde::{Deserialize, Serialize};
use serde_json;

/// Blueprint persistence schema version.
///
/// V1 is the legacy heads-up schema: blueprint key is `(past, present, future, edge)`.
/// V2 is the context-aware schema: blueprint key includes
/// `(past, present, future, seat_count, seat_position, active_players, edge)`.
///
/// Mixed V1 and V2 artifacts must not silently co-exist under the same
/// profile/version namespace.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InfoVersion {
    V1,
    V2,
}

#[rustfmt::skip]
pub const ABSTRACTION: &str = "abstraction";
#[rustfmt::skip]
pub const BLUEPRINT:   &str = "blueprint";
#[rustfmt::skip]
pub const EPOCH:       &str = "epoch";
#[rustfmt::skip]
pub const ISOMORPHISM: &str = "isomorphism";
#[rustfmt::skip]
pub const METRIC:      &str = "metric";
#[rustfmt::skip]
pub const STAGING:     &str = "staging";
#[rustfmt::skip]
pub const STREET:      &str = "street";
#[rustfmt::skip]
pub const TRANSITIONS: &str = "transitions";

/// Profile-scoped table names for blueprint training.
///
/// Each training profile (defined by player count, format, blinds, etc.)
/// gets its own set of blueprint/staging/epoch tables to avoid collisions.
///
/// # Example
/// ```ignore
/// let tables = ProfileTables::new("bp_10max_cash_2026_01_13");
/// assert_eq!(tables.blueprint(), "blueprint_bp_10max_cash_2026_01_13");
/// assert_eq!(tables.staging(), "staging_bp_10max_cash_2026_01_13");
/// assert_eq!(tables.epoch(), "epoch_bp_10max_cash_2026_01_13");
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfileTables {
    profile_key: String,
}

impl ProfileTables {
    /// Create a new profile table set from a SQL-safe profile key.
    ///
    /// The profile_key must be lowercase, alphanumeric + underscore only.
    /// This is validated during creation.
    pub fn new(profile_key: impl Into<String>) -> Self {
        let key = profile_key.into();
        assert!(
            Self::is_valid_key(&key),
            "profile_key must be lowercase alphanumeric + underscore: {key}"
        );
        Self { profile_key: key }
    }

    /// Returns true if the key is a valid SQL-safe identifier.
    fn is_valid_key(key: &str) -> bool {
        !key.is_empty()
            && key
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
    }

    /// Returns the profile key.
    pub fn profile_key(&self) -> &str {
        &self.profile_key
    }

    /// Returns the blueprint table name: `blueprint_<profile_key>`.
    pub fn blueprint(&self) -> String {
        format!("blueprint_{}", self.profile_key)
    }

    /// Returns the staging table name: `staging_<profile_key>`.
    pub fn staging(&self) -> String {
        format!("staging_{}", self.profile_key)
    }

    /// Returns the epoch table name: `epoch_<profile_key>`.
    pub fn epoch(&self) -> String {
        format!("epoch_{}", self.profile_key)
    }

    /// Returns the default (heads-up) profile tables using static constants.
    pub fn default_hu() -> Self {
        // For backwards compatibility, the default uses no suffix
        Self {
            profile_key: String::new(),
        }
    }

    /// Check if this is the default heads-up profile (no suffix).
    pub fn is_default_hu(&self) -> bool {
        self.profile_key.is_empty()
    }

    /// Returns the info version for this profile's persistence schema.
    /// Default HU uses V1 (4-column key); named profiles use V2 (7-column key with context).
    pub fn info_version(&self) -> InfoVersion {
        if self.is_default_hu() {
            InfoVersion::V1
        } else {
            InfoVersion::V2
        }
    }
}

impl Default for ProfileTables {
    fn default() -> Self {
        Self::default_hu()
    }
}

/// Abstraction-versioned table names for clustering/isomorphism data.
///
/// Each abstraction version (defining how observations map to buckets)
/// gets its own set of tables. This allows multiple abstraction versions
/// to coexist, which is necessary for position-aware multiway abstractions.
///
/// # Example
/// ```ignore
/// let tables = AbstractionTables::new("abs_v3_p10");
/// assert_eq!(tables.abstraction(), "abstraction_abs_v3_p10");
/// assert_eq!(tables.isomorphism(), "isomorphism_abs_v3_p10");
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AbstractionTables {
    abstraction_version: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParsedAbstractionVersion {
    generation: u8,
    player_count: u8,
}

impl ParsedAbstractionVersion {
    pub const fn generation(&self) -> u8 {
        self.generation
    }

    pub const fn player_count(&self) -> u8 {
        self.player_count
    }

    pub const fn is_v4_or_newer(&self) -> bool {
        self.generation >= 4
    }
}

impl AbstractionTables {
    /// Create a new abstraction table set from a version identifier.
    ///
    /// The version must be lowercase, alphanumeric + underscore only.
    pub fn new(abstraction_version: impl Into<String>) -> Self {
        let version = abstraction_version.into();
        assert!(
            Self::is_valid_version(&version),
            "abstraction_version must be lowercase alphanumeric + underscore: {version}"
        );
        Self {
            abstraction_version: version,
        }
    }

    /// Returns true if the version is a valid SQL-safe identifier.
    fn is_valid_version(version: &str) -> bool {
        !version.is_empty()
            && version
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
    }

    /// Returns the abstraction version.
    pub fn abstraction_version(&self) -> &str {
        &self.abstraction_version
    }

    /// Parse `abs_v{generation}_p{player_count}` when this table set is versioned.
    pub fn parsed_version(&self) -> Option<ParsedAbstractionVersion> {
        if self.is_default_v1() {
            return None;
        }

        let (prefix, player_count) = self.abstraction_version.rsplit_once("_p")?;
        let generation = prefix.strip_prefix("abs_v")?;
        let generation = generation.parse::<u8>().ok()?;
        let player_count = player_count.parse::<u8>().ok()?;
        if generation == 0 || player_count < 2 {
            return None;
        }

        Some(ParsedAbstractionVersion {
            generation,
            player_count,
        })
    }

    pub fn player_count(&self) -> Option<u8> {
        self.parsed_version().map(|parsed| parsed.player_count())
    }

    pub fn uses_exact_seat_lookup(&self) -> bool {
        !self.is_default_v1()
    }

    pub fn uses_multiway_v4_bucketing(&self) -> bool {
        self.parsed_version()
            .is_some_and(|parsed| parsed.is_v4_or_newer())
    }

    /// Returns the abstraction table name: `abstraction_<version>`.
    pub fn abstraction(&self) -> String {
        format!("abstraction_{}", self.abstraction_version)
    }

    /// Returns the isomorphism table name: `isomorphism_<version>`.
    pub fn isomorphism(&self) -> String {
        format!("isomorphism_{}", self.abstraction_version)
    }

    /// Returns the metric table name: `metric_<version>`.
    pub fn metric(&self) -> String {
        format!("metric_{}", self.abstraction_version)
    }

    /// Returns the transitions table name: `transitions_<version>`.
    pub fn transitions(&self) -> String {
        format!("transitions_{}", self.abstraction_version)
    }

    /// Returns the street table name: `street_<version>`.
    pub fn street(&self) -> String {
        format!("street_{}", self.abstraction_version)
    }

    /// Returns the default abstraction tables using static constants.
    pub fn default_v1() -> Self {
        // For backwards compatibility, the default uses no suffix
        Self {
            abstraction_version: String::new(),
        }
    }

    /// Check if this is the default v1 abstraction (no suffix).
    pub fn is_default_v1(&self) -> bool {
        self.abstraction_version.is_empty()
    }
}

impl Default for AbstractionTables {
    fn default() -> Self {
        Self::default_v1()
    }
}

/// Combined training tables for a specific profile and abstraction version.
///
/// This is the main entry point for profile-aware database operations.
/// It wraps both `ProfileTables` and `AbstractionTables` together.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrainingTables {
    pub profile: ProfileTables,
    pub abstraction: AbstractionTables,
}

impl TrainingTables {
    /// Create a new training table set.
    pub fn new(profile_key: impl Into<String>, abstraction_version: impl Into<String>) -> Self {
        Self {
            profile: ProfileTables::new(profile_key),
            abstraction: AbstractionTables::new(abstraction_version),
        }
    }

    /// Returns the default heads-up training tables (no suffixes).
    pub fn default_hu() -> Self {
        Self {
            profile: ProfileTables::default_hu(),
            abstraction: AbstractionTables::default_v1(),
        }
    }
}

/// Metadata describing a training profile for multiway runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrainingProfileMeta {
    pub profile_id: String,
    pub profile_key: String,
    pub format: String,
    pub player_count: usize,
    pub abstraction_version: String,
    pub config_json: String,
    pub engine_version: String,
}

/// Training format for profile configurations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TrainingFormat {
    Cash,
    Tournament,
}

impl TrainingFormat {
    pub fn as_str(&self) -> &'static str {
        match self {
            TrainingFormat::Cash => "cash",
            TrainingFormat::Tournament => "tournament",
        }
    }
}

/// Blind level definition (for tournament schedules or cash defaults).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BlindLevel {
    pub sb: u32,
    pub bb: u32,
    #[serde(default)]
    pub ante: u32,
    /// Optional duration (hands) used as a sampling weight.
    #[serde(default)]
    pub duration: Option<u32>,
    /// Optional sampling weight (overrides duration if provided).
    #[serde(default)]
    pub weight: Option<f32>,
    /// Optional stack size override for this level, in BB.
    #[serde(default)]
    pub stack_bb: Option<u32>,
}

impl BlindLevel {
    fn weight_value(&self) -> f32 {
        if let Some(weight) = self.weight {
            weight.max(0.0)
        } else if let Some(duration) = self.duration {
            (duration as f32).max(0.0)
        } else {
            1.0
        }
    }
}

/// Parsed training profile configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrainingProfileConfig {
    pub player_count: usize,
    pub format: TrainingFormat,
    pub abstraction_version: String,
    #[serde(default)]
    pub blinds: Option<String>,
    #[serde(default)]
    pub ante: Option<u32>,
    #[serde(default)]
    pub blind_schedule: Option<Vec<BlindLevel>>,
    #[serde(default)]
    pub stack_bb: Option<u32>,
    #[serde(default)]
    pub stack_bb_range: Option<[u32; 2]>,
    #[serde(default)]
    pub payout_curve: Option<Vec<f32>>,
    #[serde(skip)]
    resolved_schedule: Vec<BlindLevel>,
    #[serde(skip)]
    resolved_payout: Option<TournamentPayout>,
}

impl TrainingProfileConfig {
    pub fn from_json(raw: &str) -> Result<Self, String> {
        let mut cfg: TrainingProfileConfig =
            serde_json::from_str(raw).map_err(|err| format!("invalid profile config: {err}"))?;
        cfg.validate_and_resolve()?;
        Ok(cfg)
    }

    pub fn format(&self) -> TrainingFormat {
        self.format
    }

    pub fn resolved_schedule(&self) -> &[BlindLevel] {
        &self.resolved_schedule
    }

    pub fn tournament_payout(&self) -> Option<TournamentPayout> {
        self.resolved_payout.clone()
    }

    pub fn table_config_for_epoch(&self, epoch: usize) -> TableConfig {
        let mut rng = SmallRng::seed_from_u64(seed_for_epoch(epoch, self.player_count as u64));
        let level = sample_blind_level(&self.resolved_schedule, &mut rng);
        let stack_bb = level
            .stack_bb
            .or_else(|| sample_stack_bb(self, &mut rng))
            .unwrap_or(default_stack_bb());
        let stack = stack_bb.saturating_mul(level.bb).min(i16::MAX as u32) as i16;

        TableConfig::for_players(self.player_count)
            .with_blinds(level.sb as i16, level.bb as i16)
            .with_ante(level.ante as i16)
            .with_stack(stack)
    }

    fn validate_and_resolve(&mut self) -> Result<(), String> {
        if !(2..=10).contains(&self.player_count) {
            return Err(format!(
                "player_count must be 2-10 (got {})",
                self.player_count
            ));
        }

        if let Some(range) = self.stack_bb_range {
            if range[0] == 0 || range[1] == 0 {
                return Err("stack_bb_range values must be positive".to_string());
            }
            if range[0] > range[1] {
                return Err("stack_bb_range min must be <= max".to_string());
            }
        }

        if let Some(stack_bb) = self.stack_bb {
            if stack_bb == 0 {
                return Err("stack_bb must be positive".to_string());
            }
        }

        let schedule = match self.format {
            TrainingFormat::Cash => resolve_cash_schedule(self)?,
            TrainingFormat::Tournament => resolve_tournament_schedule(self)?,
        };

        for level in &schedule {
            validate_blind_level(level)?;
        }

        self.resolved_schedule = schedule;
        self.resolved_payout = match self.format {
            TrainingFormat::Cash => None,
            TrainingFormat::Tournament => {
                let payouts = self
                    .payout_curve
                    .as_ref()
                    .ok_or("payout_curve is required for tournament profiles")?;
                if payouts.is_empty() {
                    return Err("payout_curve must include at least one entry".to_string());
                }
                if payouts.len() > self.player_count {
                    return Err("payout_curve length cannot exceed player_count".to_string());
                }
                let payouts: Vec<f32> = payouts
                    .iter()
                    .map(|p| {
                        if *p < 0.0 {
                            Err("payout_curve entries must be non-negative".to_string())
                        } else {
                            Ok(*p)
                        }
                    })
                    .collect::<Result<_, _>>()?;
                Some(TournamentPayout::new(payouts).map_err(|e| e.to_string())?)
            }
        };

        Ok(())
    }
}

fn resolve_cash_schedule(cfg: &TrainingProfileConfig) -> Result<Vec<BlindLevel>, String> {
    if let Some(schedule) = &cfg.blind_schedule {
        if schedule.is_empty() {
            return Err("blind_schedule must not be empty".to_string());
        }
        return Ok(schedule.clone());
    }

    let (sb, bb) = if let Some(blinds) = &cfg.blinds {
        parse_blinds(blinds)?
    } else {
        (crate::S_BLIND as u32, crate::B_BLIND as u32)
    };

    Ok(vec![BlindLevel {
        sb,
        bb,
        ante: cfg.ante.unwrap_or(0),
        duration: None,
        weight: None,
        stack_bb: cfg.stack_bb,
    }])
}

fn resolve_tournament_schedule(cfg: &TrainingProfileConfig) -> Result<Vec<BlindLevel>, String> {
    match &cfg.blind_schedule {
        Some(levels) if !levels.is_empty() => Ok(levels.clone()),
        _ => Err("blind_schedule is required for tournament profiles".to_string()),
    }
}

fn validate_blind_level(level: &BlindLevel) -> Result<(), String> {
    if level.sb == 0 {
        return Err("blind level small blind must be positive".to_string());
    }
    if level.bb == 0 {
        return Err("blind level big blind must be positive".to_string());
    }
    if level.bb < level.sb {
        return Err("blind level big blind must be >= small blind".to_string());
    }
    if level.sb > i16::MAX as u32 || level.bb > i16::MAX as u32 || level.ante > i16::MAX as u32 {
        return Err("blind level exceeds chip precision".to_string());
    }
    Ok(())
}

fn parse_blinds(blinds: &str) -> Result<(u32, u32), String> {
    let trimmed = blinds.trim();
    let parts: Vec<&str> = trimmed.split('/').collect();
    if parts.len() != 2 {
        return Err(format!("invalid blinds format '{}'", blinds));
    }
    let sb = parts[0]
        .trim()
        .parse::<u32>()
        .map_err(|_| format!("invalid small blind '{}'", parts[0]))?;
    let bb = parts[1]
        .trim()
        .parse::<u32>()
        .map_err(|_| format!("invalid big blind '{}'", parts[1]))?;
    Ok((sb, bb))
}

fn sample_blind_level(levels: &[BlindLevel], rng: &mut impl Rng) -> BlindLevel {
    if levels.len() == 1 {
        return levels[0].clone();
    }
    let total: f32 = levels.iter().map(|l| l.weight_value()).sum();
    if total <= 0.0 {
        return levels[0].clone();
    }
    let mut target = rng.random::<f32>() * total;
    for level in levels {
        target -= level.weight_value();
        if target <= 0.0 {
            return level.clone();
        }
    }
    levels.last().cloned().unwrap_or_else(|| levels[0].clone())
}

fn sample_stack_bb(cfg: &TrainingProfileConfig, rng: &mut impl Rng) -> Option<u32> {
    if let Some(range) = cfg.stack_bb_range {
        return Some(rng.random_range(range[0]..=range[1]));
    }
    cfg.stack_bb
}

fn default_stack_bb() -> u32 {
    let default_bb = B_BLIND as u32;
    let default_stack = STACK as u32;
    default_stack / default_bb.max(1)
}

fn seed_for_epoch(epoch: usize, salt: u64) -> u64 {
    (epoch as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ salt
}

impl Default for TrainingTables {
    fn default() -> Self {
        Self::default_hu()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ----- Table Name Generation Tests -----
    // AC: schema creation uses `blueprint_<profile_key>` + `epoch_<profile_key>`

    #[test]
    fn profile_tables_generates_correct_names() {
        let tables = ProfileTables::new("bp_10max_cash_2026_01_13");
        assert_eq!(tables.blueprint(), "blueprint_bp_10max_cash_2026_01_13");
        assert_eq!(tables.staging(), "staging_bp_10max_cash_2026_01_13");
        assert_eq!(tables.epoch(), "epoch_bp_10max_cash_2026_01_13");
    }

    #[test]
    fn abstraction_tables_generates_correct_names() {
        let tables = AbstractionTables::new("abs_v3_p10");
        assert_eq!(tables.abstraction(), "abstraction_abs_v3_p10");
        assert_eq!(tables.isomorphism(), "isomorphism_abs_v3_p10");
        assert_eq!(tables.metric(), "metric_abs_v3_p10");
        assert_eq!(tables.transitions(), "transitions_abs_v3_p10");
        assert_eq!(tables.street(), "street_abs_v3_p10");
    }

    #[test]
    fn training_tables_combines_profile_and_abstraction() {
        let tables = TrainingTables::new("bp_6max_cash", "abs_v3_p6");
        assert_eq!(tables.profile.blueprint(), "blueprint_bp_6max_cash");
        assert_eq!(tables.abstraction.isomorphism(), "isomorphism_abs_v3_p6");
    }

    // ----- Validation Tests -----

    #[test]
    #[should_panic(expected = "profile_key must be lowercase")]
    fn profile_tables_rejects_uppercase() {
        ProfileTables::new("BP_INVALID");
    }

    #[test]
    #[should_panic(expected = "profile_key must be lowercase")]
    fn profile_tables_rejects_special_chars() {
        ProfileTables::new("bp-with-dashes");
    }

    #[test]
    #[should_panic(expected = "abstraction_version must be lowercase")]
    fn abstraction_tables_rejects_invalid() {
        AbstractionTables::new("abs.v3.p10");
    }

    // ----- Default HU Compatibility Tests -----
    // AC: backwards compatibility with heads-up training

    #[test]
    fn default_hu_profile_is_recognized() {
        let tables = ProfileTables::default_hu();
        assert!(tables.is_default_hu());
        assert!(tables.profile_key().is_empty());
    }

    #[test]
    fn default_v1_abstraction_is_recognized() {
        let tables = AbstractionTables::default_v1();
        assert!(tables.is_default_v1());
        assert!(tables.abstraction_version().is_empty());
    }

    #[test]
    fn non_default_profiles_not_recognized_as_default() {
        let tables = ProfileTables::new("bp_3max_cash");
        assert!(!tables.is_default_hu());
    }

    // ----- SQL-Safe Identifier Tests -----
    // Verify that generated names are valid SQL identifiers

    #[test]
    fn profile_key_with_numbers_is_valid() {
        let tables = ProfileTables::new("bp_10max_cash_20260113");
        assert_eq!(tables.epoch(), "epoch_bp_10max_cash_20260113");
    }

    #[test]
    fn all_lowercase_with_underscores_valid() {
        let tables = ProfileTables::new("a_b_c_d_1_2_3");
        assert_eq!(tables.blueprint(), "blueprint_a_b_c_d_1_2_3");
    }

    // ----- Multiple Profile Isolation Tests -----
    // AC: Multiple profiles do not collide

    #[test]
    fn different_profiles_have_distinct_tables() {
        let p3 = ProfileTables::new("bp_3max_cash");
        let p6 = ProfileTables::new("bp_6max_cash");
        let p10 = ProfileTables::new("bp_10max_cash");

        // Blueprint tables are distinct
        assert_ne!(p3.blueprint(), p6.blueprint());
        assert_ne!(p6.blueprint(), p10.blueprint());
        assert_ne!(p3.blueprint(), p10.blueprint());

        // Epoch tables are distinct
        assert_ne!(p3.epoch(), p6.epoch());
        assert_ne!(p6.epoch(), p10.epoch());

        // Staging tables are distinct
        assert_ne!(p3.staging(), p6.staging());
    }

    #[test]
    fn different_abstraction_versions_have_distinct_tables() {
        let v2 = AbstractionTables::new("abs_v2_p3");
        let v3 = AbstractionTables::new("abs_v3_p3");

        assert_ne!(v2.isomorphism(), v3.isomorphism());
        assert_ne!(v2.abstraction(), v3.abstraction());
        assert_ne!(v2.metric(), v3.metric());
    }

    #[test]
    fn abstraction_tables_parse_generation_and_player_count() {
        let tables = AbstractionTables::new("abs_v4_p6");
        let parsed = tables.parsed_version().expect("parsed version");

        assert_eq!(parsed.generation(), 4);
        assert_eq!(parsed.player_count(), 6);
        assert_eq!(tables.player_count(), Some(6));
        assert!(tables.uses_exact_seat_lookup());
        assert!(tables.uses_multiway_v4_bucketing());
    }

    #[test]
    fn abstraction_tables_reject_invalid_version_contract() {
        let tables = AbstractionTables::new("abs_versioned");

        assert_eq!(tables.parsed_version(), None);
        assert_eq!(tables.player_count(), None);
        assert!(!tables.uses_multiway_v4_bucketing());
    }

    #[test]
    fn training_profile_config_resolves_cash_schedule() {
        let raw = serde_json::json!({
            "player_count": 2,
            "format": "cash",
            "abstraction_version": "abs_v3_p2",
            "blinds": "1/2",
            "ante": 1,
            "stack_bb": 50
        })
        .to_string();

        let cfg = TrainingProfileConfig::from_json(&raw).expect("config parses");
        assert_eq!(cfg.format(), TrainingFormat::Cash);
        assert_eq!(cfg.resolved_schedule().len(), 1);
        let level = &cfg.resolved_schedule()[0];
        assert_eq!(level.sb, 1);
        assert_eq!(level.bb, 2);
        assert_eq!(level.ante, 1);
    }

    #[test]
    fn training_profile_config_tournament_sampling_is_deterministic() {
        let raw = serde_json::json!({
            "player_count": 3,
            "format": "tournament",
            "abstraction_version": "abs_v3_p3",
            "blind_schedule": [
                { "sb": 1, "bb": 2, "ante": 0, "duration": 10 },
                { "sb": 2, "bb": 4, "ante": 1, "duration": 10 }
            ],
            "stack_bb_range": [20, 25],
            "payout_curve": [0.5, 0.3, 0.2]
        })
        .to_string();

        let cfg = TrainingProfileConfig::from_json(&raw).expect("config parses");
        assert_eq!(cfg.resolved_schedule().len(), 2);
        assert!(cfg.tournament_payout().is_some());

        let config_a = cfg.table_config_for_epoch(7);
        let config_b = cfg.table_config_for_epoch(7);
        assert_eq!(config_a, config_b);
        assert!(matches!(config_a.small_blind, 1 | 2));
        assert!(matches!(config_a.big_blind, 2 | 4));
        assert!(config_a.starting_stack >= 20 * config_a.big_blind);
    }

    // ----- InfoVersion Tests -----

    #[test]
    fn default_hu_profile_is_v1() {
        assert_eq!(ProfileTables::default_hu().info_version(), InfoVersion::V1);
    }

    #[test]
    fn named_profile_is_v2() {
        let tables = ProfileTables::new("bp_6max_cash");
        assert_eq!(tables.info_version(), InfoVersion::V2);
    }
}
