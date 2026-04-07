use super::*;

/// Explicit training environment inputs, decoupled from raw env-var reads
/// so validation logic is testable without global state mutation.
#[derive(Default)]
struct TrainingEnvInput {
    profile_id: Option<String>,
    profile_key: Option<String>,
    profile_format: Option<String>,
    profile_config_path: Option<String>,
    profile_config_json: Option<String>,
    abstraction_version: Option<String>,
    player_count: Option<String>,
}

/// Training mode parsed from command line arguments
pub enum Mode {
    Status,
    Cluster,
    Fast,
    Slow,
    Sanity,
}

impl Mode {
    pub fn from_args() -> Self {
        std::env::args()
            .find_map(|a| match a.as_str() {
                "--cluster" => Some(Self::Cluster),
                "--status" => Some(Self::Status),
                "--sanity" => Some(Self::Sanity),
                "--fast" => Some(Self::Fast),
                "--slow" => Some(Self::Slow),
                _ => None,
            })
            .unwrap_or_else(|| {
                eprintln!("Usage: trainer --status | --cluster | --sanity | --fast | --slow");
                std::process::exit(1);
            })
    }

    pub async fn run() {
        let settings = TrainingSettings::from_env().unwrap_or_else(|err| {
            eprintln!("~ trainer config error · {}", err);
            std::process::exit(1);
        });

        if let Some(profile_cfg) = &settings.profile_config {
            crate::gameplay::set_tournament_payout(profile_cfg.tournament_payout());
        } else {
            crate::gameplay::set_tournament_payout(None);
        }

        let client = crate::save::db_profile(&settings.tables, settings.profile.as_ref()).await;
        match Self::from_args() {
            Self::Cluster => {
                SlowSession::new(
                    client,
                    settings.tables,
                    settings.player_count,
                    settings.profile_config.clone(),
                )
                .await
                .pretraining()
                .await
            }
            Self::Status => {
                SlowSession::new(
                    client,
                    settings.tables,
                    settings.player_count,
                    settings.profile_config.clone(),
                )
                .await
                .status()
                .await
            }
            Self::Sanity => {
                SlowSession::new(
                    client,
                    settings.tables,
                    settings.player_count,
                    settings.profile_config.clone(),
                )
                .await
                .sanity()
                .await
            }
            Self::Fast => {
                FastSession::new(client, settings.tables, settings.player_count)
                    .await
                    .train()
                    .await
            }
            Self::Slow => {
                SlowSession::new(
                    client,
                    settings.tables,
                    settings.player_count,
                    settings.profile_config.clone(),
                )
                .await
                .train()
                .await
            }
        }
    }
}

#[derive(Clone, Debug)]
struct TrainingSettings {
    tables: crate::save::TrainingTables,
    player_count: usize,
    profile: Option<crate::save::TrainingProfileMeta>,
    profile_config: Option<crate::save::TrainingProfileConfig>,
}

impl TrainingSettings {
    fn from_env() -> Result<Self, String> {
        Self::from_env_values(&TrainingEnvInput {
            profile_id: env_optional("PROFILE_ID"),
            profile_key: env_optional("PROFILE_KEY"),
            profile_format: env_optional("PROFILE_FORMAT"),
            profile_config_path: env_optional("PROFILE_CONFIG_PATH"),
            profile_config_json: env_optional("PROFILE_CONFIG_JSON"),
            abstraction_version: env_optional("ABSTRACTION_VERSION"),
            player_count: env_optional("PLAYER_COUNT"),
        })
    }

    fn from_env_values(input: &TrainingEnvInput) -> Result<Self, String> {
        let profile_format_raw = input
            .profile_format
            .clone()
            .unwrap_or_else(|| "cash".to_string());
        let player_count = input
            .player_count
            .as_ref()
            .and_then(|v| v.parse::<usize>().ok());

        let any_set = input.profile_key.is_some()
            || input.profile_id.is_some()
            || input.abstraction_version.is_some()
            || player_count.is_some()
            || input.profile_config_path.is_some()
            || input.profile_config_json.is_some()
            || input.profile_format.is_some();

        // Profile metadata is mandatory for all new training runs.
        // Silent fallback to heads-up defaults is no longer supported.
        if !any_set {
            return Err(
                "profile metadata required: set PLAYER_COUNT, ABSTRACTION_VERSION, \
                 and optionally PROFILE_KEY (heads-up fallback is no longer supported \
                 for new training runs)"
                    .to_string(),
            );
        }

        let abstraction_version = input
            .abstraction_version
            .clone()
            .ok_or("ABSTRACTION_VERSION is required when using profile training")?;
        let player_count =
            player_count.ok_or("PLAYER_COUNT is required when using profile training")?;

        if !is_sql_safe(&abstraction_version) {
            return Err(format!(
                "invalid ABSTRACTION_VERSION '{}'",
                abstraction_version
            ));
        }
        if !(2..=10).contains(&player_count) {
            return Err(format!("PLAYER_COUNT must be 2-10 (got {})", player_count));
        }

        let profile_format = normalize_format(&profile_format_raw)?;

        let config_json = load_profile_config_json(
            input.profile_config_path.as_deref(),
            input.profile_config_json.as_deref(),
            player_count,
            &profile_format,
            input.profile_key.as_deref(),
            input.profile_id.as_deref(),
            &abstraction_version,
        )?;

        let profile_config = crate::save::TrainingProfileConfig::from_json(&config_json)
            .map_err(|err| format!("invalid PROFILE_CONFIG_JSON: {}", err))?;

        if profile_config.player_count != player_count {
            return Err(format!(
                "PROFILE_CONFIG_JSON player_count {} does not match PLAYER_COUNT {}",
                profile_config.player_count, player_count
            ));
        }

        if profile_config.format().as_str() != profile_format {
            return Err(format!(
                "PROFILE_CONFIG_JSON format '{}' does not match PROFILE_FORMAT '{}'",
                profile_config.format().as_str(),
                profile_format
            ));
        }

        let derived_profile_id = input
            .profile_id
            .clone()
            .unwrap_or_else(|| derive_profile_id(&config_json));
        let derived_profile_key = input
            .profile_key
            .clone()
            .unwrap_or_else(|| derive_profile_key(&derived_profile_id));

        if derived_profile_key.is_empty() {
            return Err("derived PROFILE_KEY is empty".to_string());
        }

        if !is_sql_safe(&derived_profile_key) {
            return Err(format!(
                "invalid PROFILE_KEY '{}' (must be lowercase alphanumeric + underscore)",
                derived_profile_key
            ));
        }

        let engine_version = format!("robopoker-{}", env!("CARGO_PKG_VERSION"));

        Ok(Self {
            tables: crate::save::TrainingTables::new(
                derived_profile_key.clone(),
                abstraction_version.clone(),
            ),
            player_count,
            profile: Some(crate::save::TrainingProfileMeta {
                profile_id: derived_profile_id,
                profile_key: derived_profile_key,
                format: profile_format,
                player_count,
                abstraction_version,
                config_json,
                engine_version,
            }),
            profile_config: Some(profile_config),
        })
    }
}

fn env_optional(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|v| !v.trim().is_empty())
}

fn is_sql_safe(value: &str) -> bool {
    value
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

fn normalize_format(value: &str) -> Result<String, String> {
    let lowered = value.trim().to_ascii_lowercase();
    match lowered.as_str() {
        "cash" | "tournament" => Ok(lowered),
        _ => Err(format!(
            "invalid PROFILE_FORMAT '{}' (expected 'cash' or 'tournament')",
            value
        )),
    }
}

fn load_profile_config_json(
    config_path: Option<&str>,
    config_inline: Option<&str>,
    player_count: usize,
    format: &str,
    profile_key: Option<&str>,
    profile_id: Option<&str>,
    abstraction_version: &str,
) -> Result<String, String> {
    let raw = if let Some(path) = config_path {
        std::fs::read_to_string(path)
            .map_err(|err| format!("failed to read PROFILE_CONFIG_PATH '{}': {}", path, err))?
    } else if let Some(inline) = config_inline {
        inline.to_string()
    } else {
        let mut config = serde_json::json!({
            "player_count": player_count,
            "format": format,
            "abstraction_version": abstraction_version,
        });
        if let Some(key) = profile_key {
            config["profile_key"] = serde_json::Value::String(key.to_string());
        }
        if let Some(id) = profile_id {
            config["profile_id"] = serde_json::Value::String(id.to_string());
        }
        if let Some(blinds) = env_optional("PROFILE_BLINDS") {
            config["blinds"] = serde_json::Value::String(blinds);
        }
        if let Some(ante) = env_optional("PROFILE_ANTE").and_then(|v| v.parse::<u32>().ok()) {
            config["ante"] = serde_json::Value::Number(ante.into());
        }
        if let Some(stack) = env_optional("PROFILE_STACK_BB").and_then(|v| v.parse::<u32>().ok()) {
            config["stack_bb"] = serde_json::Value::Number(stack.into());
        }
        config.to_string()
    };

    serde_json::from_str::<serde_json::Value>(&raw)
        .map_err(|err| format!("invalid PROFILE_CONFIG_JSON: {}", err))?;
    Ok(raw)
}

fn derive_profile_id(config_json: &str) -> String {
    let hash = fnv1a_64(config_json.as_bytes());
    format!("bp-{:016x}", hash)
}

fn derive_profile_key(profile_id: &str) -> String {
    let mut key = String::new();
    for ch in profile_id.chars() {
        let c = ch.to_ascii_lowercase();
        if c.is_ascii_lowercase() || c.is_ascii_digit() {
            key.push(c);
        } else {
            key.push('_');
        }
    }
    while key.contains("__") {
        key = key.replace("__", "_");
    }
    key.trim_matches('_').to_string()
}

fn fnv1a_64(bytes: &[u8]) -> u64 {
    const FNV_OFFSET: u64 = 0xcbf29ce484222325;
    const FNV_PRIME: u64 = 0x100000001b3;
    let mut hash = FNV_OFFSET;
    for b in bytes {
        hash ^= *b as u64;
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    fn minimal_multiway_input(player_count: usize) -> TrainingEnvInput {
        TrainingEnvInput {
            player_count: Some(player_count.to_string()),
            abstraction_version: Some(format!("abs_v4_p{player_count}")),
            ..Default::default()
        }
    }

    /// RPM-06 AC: fast training requires profile metadata.
    /// Both fast and slow modes share the same TrainingSettings::from_env_values
    /// entry point, so this validates the shared config path that fast training uses.
    #[test]
    fn test_fast_training_requires_profile_metadata() {
        let input = TrainingEnvInput::default();
        let result = TrainingSettings::from_env_values(&input);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.contains("profile metadata required"),
            "expected profile-required error, got: {err}"
        );
    }

    /// RPM-06 AC: slow training requires profile metadata.
    /// Validates the same shared config path from the slow-training perspective.
    #[test]
    fn test_slow_training_requires_profile_metadata() {
        let input = TrainingEnvInput::default();
        let result = TrainingSettings::from_env_values(&input);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .contains("heads-up fallback is no longer supported"),
            "error should explain that HU fallback is removed"
        );
    }

    /// RPM-06 AC: multiway training never produces default-HU tables.
    #[test]
    fn test_multiway_training_rejects_heads_up_fallback() {
        for n in [3, 6, 10] {
            let input = minimal_multiway_input(n);
            let settings = TrainingSettings::from_env_values(&input)
                .unwrap_or_else(|e| panic!("valid {n}-player config rejected: {e}"));
            assert!(
                !settings.tables.profile.is_default_hu(),
                "{n}-player training must not produce default HU profile tables"
            );
            assert!(
                !settings.tables.abstraction.is_default_v1(),
                "{n}-player training must not produce default V1 abstraction tables"
            );
            assert_eq!(settings.player_count, n);
            assert!(
                settings.profile.is_some(),
                "{n}-player training must produce profile metadata"
            );
        }
    }

    /// Verify that setting only PLAYER_COUNT without ABSTRACTION_VERSION errors.
    #[test]
    fn test_partial_profile_metadata_rejected() {
        let input = TrainingEnvInput {
            player_count: Some("6".to_string()),
            ..Default::default()
        };
        let result = TrainingSettings::from_env_values(&input);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .contains("ABSTRACTION_VERSION is required"),
            "partial metadata should require ABSTRACTION_VERSION"
        );
    }

    /// Verify that valid 2-player profile training works (explicit, not fallback).
    #[test]
    fn test_explicit_heads_up_profile_accepted() {
        let input = minimal_multiway_input(2);
        let settings = TrainingSettings::from_env_values(&input)
            .expect("explicit 2-player profile should be accepted");
        assert_eq!(settings.player_count, 2);
        assert!(!settings.tables.profile.is_default_hu());
        assert!(settings.profile.is_some());
    }
}
