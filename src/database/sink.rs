use crate::save::*;
use crate::workers::Record;
use std::sync::Arc;
use tokio_postgres::Client;

/// Sink defines the write interface between domain types and PostgreSQL.
/// All INSERT/UPDATE queries are consolidated here.
///
/// The default implementation uses static table names for backwards
/// compatibility with heads-up training.
#[async_trait::async_trait]
pub trait Sink: Send + Sync {
    async fn submit(&self, records: Vec<Record>);
    async fn advance(&self);
}

#[async_trait::async_trait]
impl Sink for Client {
    async fn submit(&self, records: Vec<Record>) {
        #[rustfmt::skip]
        const SQL: &str = const_format::concatcp!(
            "INSERT INTO ", BLUEPRINT, " (past, present, future, edge, policy, regret) ",
            "VALUES                      ($1,   $2,      $3,     $4,   $5,     $6) ",
            "ON CONFLICT (past, present, future, edge) ",
            "DO UPDATE SET ",
                "policy = EXCLUDED.policy, ",
                "regret = EXCLUDED.regret"
        );
        for record in records {
            self.execute(
                SQL,
                &[
                    &i64::from(*record.info.history()),
                    &i16::from(*record.info.present()),
                    &i64::from(*record.info.choices()),
                    &(u64::from(record.edge) as i64),
                    &record.policy,
                    &record.regret,
                ],
            )
            .await
            .expect("blueprint upsert");
        }
    }

    async fn advance(&self) {
        #[rustfmt::skip]
        const SQL: &str = const_format::concatcp!(
            "UPDATE ", EPOCH, " ",
            "SET    value = value + 1 ",
            "WHERE  key = 'current'"
        );
        self.execute(SQL, &[]).await.expect("epoch advance");
    }
}

#[async_trait::async_trait]
impl Sink for Arc<Client> {
    async fn submit(&self, records: Vec<Record>) {
        self.as_ref().submit(records).await
    }
    async fn advance(&self) {
        self.as_ref().advance().await
    }
}

/// Profile-aware write operations for multiway training.
///
/// Uses `ProfileTables` to write to profile-specific blueprint and epoch
/// tables, allowing concurrent training of multiple profiles.
#[async_trait::async_trait]
pub trait ProfileSink: Send + Sync {
    /// Submit records to a profile-specific blueprint table.
    async fn submit_profile(&self, tables: &ProfileTables, records: Vec<Record>);
    /// Advance the epoch counter for a profile.
    async fn advance_profile(&self, tables: &ProfileTables);
}

#[async_trait::async_trait]
impl ProfileSink for Client {
    async fn submit_profile(&self, tables: &ProfileTables, records: Vec<Record>) {
        let blueprint = if tables.is_default_hu() {
            BLUEPRINT.to_string()
        } else {
            tables.blueprint()
        };
        let sql = format!(
            "INSERT INTO {blueprint} (past, present, future, edge, policy, regret) \
             VALUES ($1, $2, $3, $4, $5, $6) \
             ON CONFLICT (past, present, future, edge) \
             DO UPDATE SET policy = EXCLUDED.policy, regret = EXCLUDED.regret"
        );
        for record in records {
            self.execute(
                &sql,
                &[
                    &i64::from(*record.info.history()),
                    &i16::from(*record.info.present()),
                    &i64::from(*record.info.choices()),
                    &(u64::from(record.edge) as i64),
                    &record.policy,
                    &record.regret,
                ],
            )
            .await
            .expect("blueprint upsert");
        }
    }

    async fn advance_profile(&self, tables: &ProfileTables) {
        let epoch = if tables.is_default_hu() {
            EPOCH.to_string()
        } else {
            tables.epoch()
        };
        let sql = format!("UPDATE {epoch} SET value = value + 1 WHERE key = 'current'");
        self.execute(&sql, &[]).await.expect("epoch advance");
    }
}

#[async_trait::async_trait]
impl ProfileSink for Arc<Client> {
    async fn submit_profile(&self, tables: &ProfileTables, records: Vec<Record>) {
        self.as_ref().submit_profile(tables, records).await
    }
    async fn advance_profile(&self, tables: &ProfileTables) {
        self.as_ref().advance_profile(tables).await
    }
}

/// SQL generation helpers for profile-aware write operations.
///
/// These functions expose the SQL strings for testing purposes.
pub mod profile_sink_sql {
    use crate::save::{BLUEPRINT, EPOCH, ProfileTables};

    /// Generate the SQL for submitting records to a profile blueprint table.
    pub fn submit_sql(tables: &ProfileTables) -> String {
        let blueprint = if tables.is_default_hu() {
            BLUEPRINT.to_string()
        } else {
            tables.blueprint()
        };
        format!(
            "INSERT INTO {blueprint} (past, present, future, edge, policy, regret) \
             VALUES ($1, $2, $3, $4, $5, $6) \
             ON CONFLICT (past, present, future, edge) \
             DO UPDATE SET policy = EXCLUDED.policy, regret = EXCLUDED.regret"
        )
    }

    /// Generate the SQL for advancing the epoch counter for a profile.
    pub fn advance_sql(tables: &ProfileTables) -> String {
        let epoch = if tables.is_default_hu() {
            EPOCH.to_string()
        } else {
            tables.epoch()
        };
        format!("UPDATE {epoch} SET value = value + 1 WHERE key = 'current'")
    }
}

#[cfg(test)]
mod tests {
    use super::profile_sink_sql::*;
    use crate::save::ProfileTables;

    // ----- Profile Sink SQL Tests -----
    // AC: blueprint writes go to profile tables

    #[test]
    fn submit_sql_uses_profile_blueprint_table() {
        let tables = ProfileTables::new("bp_10max_cash");
        let sql = submit_sql(&tables);

        // Should insert into profile-specific blueprint table
        assert!(sql.contains("INSERT INTO blueprint_bp_10max_cash"));
        assert!(sql.contains("ON CONFLICT"));
        assert!(sql.contains("DO UPDATE SET"));
    }

    #[test]
    fn advance_sql_uses_profile_epoch_table() {
        let tables = ProfileTables::new("bp_6max_tourney");
        let sql = advance_sql(&tables);

        // Should update profile-specific epoch table
        assert!(sql.contains("UPDATE epoch_bp_6max_tourney"));
        assert!(sql.contains("SET value = value + 1"));
    }

    #[test]
    fn default_hu_profile_uses_static_tables() {
        let tables = ProfileTables::default_hu();

        // Submit SQL should use static blueprint table
        let submit = submit_sql(&tables);
        assert!(submit.contains("INSERT INTO blueprint "));
        assert!(!submit.contains("blueprint_"));

        // Advance SQL should use static epoch table
        let advance = advance_sql(&tables);
        assert!(advance.contains("UPDATE epoch "));
        assert!(!advance.contains("epoch_"));
    }

    #[test]
    fn different_profiles_write_to_different_tables() {
        let p3 = ProfileTables::new("bp_3max");
        let p6 = ProfileTables::new("bp_6max");
        let p10 = ProfileTables::new("bp_10max");

        let submit3 = submit_sql(&p3);
        let submit6 = submit_sql(&p6);
        let submit10 = submit_sql(&p10);

        // Each profile writes to its own table
        assert!(submit3.contains("blueprint_bp_3max"));
        assert!(submit6.contains("blueprint_bp_6max"));
        assert!(submit10.contains("blueprint_bp_10max"));

        // No cross-profile writes
        assert!(!submit3.contains("bp_6max"));
        assert!(!submit6.contains("bp_10max"));
    }

    #[test]
    fn submit_sql_has_correct_columns() {
        let tables = ProfileTables::new("bp_test");
        let sql = submit_sql(&tables);

        // Should have all required columns
        assert!(sql.contains("past"));
        assert!(sql.contains("present"));
        assert!(sql.contains("future"));
        assert!(sql.contains("edge"));
        assert!(sql.contains("policy"));
        assert!(sql.contains("regret"));
    }
}
