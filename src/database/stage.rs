use crate::save::*;
use std::sync::Arc;
use tokio_postgres::Client;

/// Stage defines bulk upload operations for fast training.
/// Manages staging table lifecycle and batch epoch updates.
///
/// The default implementation uses static table names (BLUEPRINT, STAGING, EPOCH)
/// for backwards compatibility with heads-up training.
#[async_trait::async_trait]
pub trait Stage: Send + Sync {
    async fn stage(&self);
    async fn merge(&self);
    async fn stamp(&self, n: usize);
}

#[async_trait::async_trait]
impl Stage for Client {
    async fn stage(&self) {
        let sql = format!(
            "DROP   TABLE IF EXISTS {t2};
             CREATE UNLOGGED TABLE  {t2} (LIKE {t1} INCLUDING ALL);",
            t1 = BLUEPRINT,
            t2 = STAGING
        );
        self.batch_execute(&sql).await.expect("create staging");
    }
    async fn merge(&self) {
        let sql = format!(
            "INSERT INTO   {t1} (past, present, future, edge, policy, regret)
             SELECT              past, present, future, edge, policy, regret FROM {t2}
             ON CONFLICT  (past, present, future, edge)
             DO UPDATE SET
                 policy = EXCLUDED.policy,
                 regret = EXCLUDED.regret;
             DROP TABLE    {t2};",
            t1 = BLUEPRINT,
            t2 = STAGING
        );
        self.batch_execute(&sql).await.expect("upsert blueprint");
    }
    async fn stamp(&self, n: usize) {
        let sql = format!(
            "UPDATE {t} SET value = value + $1 WHERE key = 'current'",
            t = EPOCH
        );
        self.execute(&sql, &[&(n as i64)])
            .await
            .expect("update epoch");
    }
}

#[async_trait::async_trait]
impl Stage for Arc<Client> {
    async fn stage(&self) {
        self.as_ref().stage().await
    }
    async fn merge(&self) {
        self.as_ref().merge().await
    }
    async fn stamp(&self, n: usize) {
        self.as_ref().stamp(n).await
    }
}

/// Profile-aware stage operations for multiway training.
///
/// Uses `ProfileTables` to dynamically generate table names based on
/// the profile key, allowing concurrent training of multiple profiles.
#[async_trait::async_trait]
pub trait ProfileStage: Send + Sync {
    /// Create a staging table for the profile.
    async fn stage_profile(&self, tables: &ProfileTables);
    /// Merge staging table into blueprint table for the profile.
    async fn merge_profile(&self, tables: &ProfileTables);
    /// Increment the epoch counter for the profile.
    async fn stamp_profile(&self, tables: &ProfileTables, n: usize);
}

#[async_trait::async_trait]
impl ProfileStage for Client {
    async fn stage_profile(&self, tables: &ProfileTables) {
        let (blueprint, staging) = if tables.is_default_hu() {
            (BLUEPRINT.to_string(), STAGING.to_string())
        } else {
            (tables.blueprint(), tables.staging())
        };
        let sql = format!(
            "DROP   TABLE IF EXISTS {staging};
             CREATE UNLOGGED TABLE  {staging} (LIKE {blueprint} INCLUDING ALL);"
        );
        self.batch_execute(&sql).await.expect("create staging");
    }

    async fn merge_profile(&self, tables: &ProfileTables) {
        let (blueprint, staging) = if tables.is_default_hu() {
            (BLUEPRINT.to_string(), STAGING.to_string())
        } else {
            (tables.blueprint(), tables.staging())
        };
        let sql = if tables.is_default_hu() {
            format!(
                "INSERT INTO   {blueprint} (past, present, future, edge, policy, regret)
                 SELECT                     past, present, future, edge, policy, regret FROM {staging}
                 ON CONFLICT  (past, present, future, edge)
                 DO UPDATE SET
                     policy = EXCLUDED.policy,
                     regret = EXCLUDED.regret;
                 DROP TABLE    {staging};"
            )
        } else {
            format!(
                "INSERT INTO   {blueprint} \
                 (past, present, future, seat_count, seat_position, active_players, edge, policy, regret)
                 SELECT                     past, present, future, seat_count, seat_position, active_players, edge, policy, regret FROM {staging}
                 ON CONFLICT  (past, present, future, seat_count, seat_position, active_players, edge)
                 DO UPDATE SET
                     policy = EXCLUDED.policy,
                     regret = EXCLUDED.regret;
                 DROP TABLE    {staging};"
            )
        };
        self.batch_execute(&sql).await.expect("upsert blueprint");
    }

    async fn stamp_profile(&self, tables: &ProfileTables, n: usize) {
        let epoch = if tables.is_default_hu() {
            EPOCH.to_string()
        } else {
            tables.epoch()
        };
        let sql = format!("UPDATE {epoch} SET value = value + $1 WHERE key = 'current'");
        self.execute(&sql, &[&(n as i64)])
            .await
            .expect("update epoch");
    }
}

#[async_trait::async_trait]
impl ProfileStage for Arc<Client> {
    async fn stage_profile(&self, tables: &ProfileTables) {
        self.as_ref().stage_profile(tables).await
    }
    async fn merge_profile(&self, tables: &ProfileTables) {
        self.as_ref().merge_profile(tables).await
    }
    async fn stamp_profile(&self, tables: &ProfileTables, n: usize) {
        self.as_ref().stamp_profile(tables, n).await
    }
}

/// SQL generation helpers for profile-aware stage operations.
///
/// These functions expose the SQL strings for testing purposes.
/// The trait implementations use these internally.
pub mod profile_stage_sql {
    use crate::save::{BLUEPRINT, ProfileTables, STAGING};

    /// Generate the SQL for creating a staging table for a profile.
    pub fn stage_sql(tables: &ProfileTables) -> String {
        let (blueprint, staging) = if tables.is_default_hu() {
            (BLUEPRINT.to_string(), STAGING.to_string())
        } else {
            (tables.blueprint(), tables.staging())
        };
        format!(
            "DROP   TABLE IF EXISTS {staging};
             CREATE UNLOGGED TABLE  {staging} (LIKE {blueprint} INCLUDING ALL);"
        )
    }

    /// Generate the SQL for merging staging into blueprint for a profile.
    pub fn merge_sql(tables: &ProfileTables) -> String {
        let (blueprint, staging) = if tables.is_default_hu() {
            (BLUEPRINT.to_string(), STAGING.to_string())
        } else {
            (tables.blueprint(), tables.staging())
        };
        if tables.is_default_hu() {
            format!(
                "INSERT INTO   {blueprint} (past, present, future, edge, policy, regret)
                 SELECT                     past, present, future, edge, policy, regret FROM {staging}
                 ON CONFLICT  (past, present, future, edge)
                 DO UPDATE SET
                     policy = EXCLUDED.policy,
                     regret = EXCLUDED.regret;
                 DROP TABLE    {staging};"
            )
        } else {
            format!(
                "INSERT INTO   {blueprint} \
                 (past, present, future, seat_count, seat_position, active_players, edge, policy, regret)
                 SELECT                     past, present, future, seat_count, seat_position, active_players, edge, policy, regret FROM {staging}
                 ON CONFLICT  (past, present, future, seat_count, seat_position, active_players, edge)
                 DO UPDATE SET
                     policy = EXCLUDED.policy,
                     regret = EXCLUDED.regret;
                 DROP TABLE    {staging};"
            )
        }
    }

    /// Generate the SQL for stamping (incrementing) an epoch for a profile.
    pub fn stamp_sql(tables: &ProfileTables) -> String {
        let epoch = if tables.is_default_hu() {
            super::EPOCH.to_string()
        } else {
            tables.epoch()
        };
        format!("UPDATE {epoch} SET value = value + $1 WHERE key = 'current'")
    }
}

#[cfg(test)]
mod tests {
    use super::profile_stage_sql::*;
    use crate::save::ProfileTables;

    // ----- Profile Stage SQL Tests -----
    // AC: stage/merge/stamp operate on profile tables

    #[test]
    fn stage_sql_uses_profile_tables() {
        let tables = ProfileTables::new("bp_10max_cash");
        let sql = stage_sql(&tables);

        // Should use profile-specific table names
        assert!(sql.contains("staging_bp_10max_cash"));
        assert!(sql.contains("blueprint_bp_10max_cash"));

        // Should not use static table names
        assert!(!sql.contains(" staging;"));
        assert!(!sql.contains(" blueprint "));
    }

    #[test]
    fn merge_sql_uses_profile_tables() {
        let tables = ProfileTables::new("bp_6max_tourney");
        let sql = merge_sql(&tables);

        // Should merge from staging to blueprint using profile tables
        assert!(sql.contains("INSERT INTO   blueprint_bp_6max_tourney"));
        assert!(sql.contains("FROM staging_bp_6max_tourney"));
        assert!(sql.contains("DROP TABLE    staging_bp_6max_tourney"));
        assert!(sql.contains("seat_count"));
        assert!(sql.contains("seat_position"));
        assert!(sql.contains("active_players"));
    }

    #[test]
    fn stamp_sql_uses_profile_epoch_table() {
        let tables = ProfileTables::new("bp_3max_cash");
        let sql = stamp_sql(&tables);

        // Should update the profile-specific epoch table
        assert!(sql.contains("UPDATE epoch_bp_3max_cash"));
        assert!(sql.contains("SET value = value + $1"));
    }

    #[test]
    fn default_hu_profile_uses_static_tables() {
        let tables = ProfileTables::default_hu();

        // Stage SQL should use static names
        let stage = stage_sql(&tables);
        assert!(stage.contains(" staging;") || stage.contains(" staging "));
        assert!(stage.contains(" blueprint "));

        // Merge SQL should use static names
        let merge = merge_sql(&tables);
        assert!(merge.contains("INSERT INTO   blueprint "));
        assert!(merge.contains("FROM staging\n") || merge.contains("FROM staging "));

        // Stamp SQL should use static epoch table
        let stamp = stamp_sql(&tables);
        assert!(stamp.contains("UPDATE epoch "));
    }

    #[test]
    fn different_profiles_target_different_tables() {
        let p3 = ProfileTables::new("bp_3max");
        let p6 = ProfileTables::new("bp_6max");
        let p10 = ProfileTables::new("bp_10max");

        let stage3 = stage_sql(&p3);
        let stage6 = stage_sql(&p6);
        let stage10 = stage_sql(&p10);

        // Each profile should target distinct tables
        assert!(stage3.contains("staging_bp_3max"));
        assert!(stage6.contains("staging_bp_6max"));
        assert!(stage10.contains("staging_bp_10max"));

        // No overlap
        assert!(!stage3.contains("bp_6max"));
        assert!(!stage6.contains("bp_3max"));
    }
}
