use super::*;

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

#[derive(Clone)]
struct TrainingSettings {
    tables: crate::save::TrainingTables,
    player_count: usize,
    profile: Option<crate::save::TrainingProfileMeta>,
    profile_config: Option<crate::save::TrainingProfileConfig>,
}

impl TrainingSettings {
    fn from_env() -> Result<Self, String> {
        let profile_id = env_optional("PROFILE_ID");
        let profile_key = env_optional("PROFILE_KEY");
        let profile_format = env_optional("PROFILE_FORMAT").unwrap_or_else(|| "cash".to_string());
        let profile_config_path = env_optional("PROFILE_CONFIG_PATH");
        let profile_config_json = env_optional("PROFILE_CONFIG_JSON");
        let abstraction_version = env_optional("ABSTRACTION_VERSION");
        let player_count = env_optional("PLAYER_COUNT").and_then(|v| v.parse::<usize>().ok());
        let format_set = env_optional("PROFILE_FORMAT").is_some();

        let any_set = profile_key.is_some()
            || profile_id.is_some()
            || abstraction_version.is_some()
            || player_count.is_some()
            || profile_config_path.is_some()
            || profile_config_json.is_some()
            || format_set;
        if !any_set {
            return Ok(Self {
                tables: crate::save::TrainingTables::default_hu(),
                player_count: 2,
                profile: None,
                profile_config: None,
            });
        }

        let abstraction_version = abstraction_version
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

        let profile_format = normalize_format(&profile_format)?;

        let config_json = load_profile_config_json(
            profile_config_path.as_deref(),
            profile_config_json.as_deref(),
            player_count,
            &profile_format,
            profile_key.as_deref(),
            profile_id.as_deref(),
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

        let derived_profile_id = profile_id.unwrap_or_else(|| derive_profile_id(&config_json));
        let derived_profile_key =
            profile_key.unwrap_or_else(|| derive_profile_key(&derived_profile_id));

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
