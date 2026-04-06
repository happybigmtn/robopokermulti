use crate::cards::*;
use crate::save::*;
use std::sync::Arc;
use tokio_postgres::Client;

/// Check defines status queries for training orchestration.
/// Consolidates existence/count checks used by Trainer and PreTraining.
///
/// The default implementation uses static table names for backwards
/// compatibility with heads-up training.
#[async_trait::async_trait]
pub trait Check: Send + Sync {
    async fn epochs(&self) -> usize;
    async fn blueprint(&self) -> usize;
    async fn clustered(&self, street: Street) -> bool;
}

#[async_trait::async_trait]
impl Check for Client {
    async fn epochs(&self) -> usize {
        let sql = format!("SELECT value FROM {t} WHERE key = 'current'", t = EPOCH);
        self.query_opt(&sql, &[])
            .await
            .ok()
            .flatten()
            .map(|r| r.get::<_, i64>(0) as usize)
            .unwrap_or(0)
    }
    async fn blueprint(&self) -> usize {
        let sql = format!("SELECT COUNT(*) FROM {t}", t = BLUEPRINT);
        self.query_opt(&sql, &[])
            .await
            .ok()
            .flatten()
            .map(|r| r.get::<_, i64>(0) as usize)
            .unwrap_or(0)
    }
    async fn clustered(&self, street: Street) -> bool {
        let sql = format!("SELECT 1 FROM {t} WHERE obs = $1", t = ISOMORPHISM);
        let obs = i64::from(Isomorphism::from(Observation::from(street)));
        self.query_opt(&sql, &[&obs]).await.ok().flatten().is_some()
    }
}

#[async_trait::async_trait]
impl Check for Arc<Client> {
    async fn epochs(&self) -> usize {
        self.as_ref().epochs().await
    }
    async fn blueprint(&self) -> usize {
        self.as_ref().blueprint().await
    }
    async fn clustered(&self, street: Street) -> bool {
        self.as_ref().clustered(street).await
    }
}

/// Profile-aware status queries for multiway training.
///
/// Uses `ProfileTables` and `AbstractionTables` to query profile-specific
/// tables, allowing concurrent training status checks across profiles.
#[async_trait::async_trait]
pub trait ProfileCheck: Send + Sync {
    /// Returns the current epoch count for a profile.
    async fn epochs_profile(&self, tables: &ProfileTables) -> usize;
    /// Returns the blueprint row count for a profile.
    async fn blueprint_profile(&self, tables: &ProfileTables) -> usize;
    /// Returns whether clustering is complete for a street in an abstraction version.
    async fn clustered_profile(&self, tables: &AbstractionTables, street: Street) -> bool;
}

#[async_trait::async_trait]
impl ProfileCheck for Client {
    async fn epochs_profile(&self, tables: &ProfileTables) -> usize {
        let epoch = if tables.is_default_hu() {
            EPOCH.to_string()
        } else {
            tables.epoch()
        };
        let sql = format!("SELECT value FROM {epoch} WHERE key = 'current'");
        self.query_opt(&sql, &[])
            .await
            .ok()
            .flatten()
            .map(|r| r.get::<_, i64>(0) as usize)
            .unwrap_or(0)
    }

    async fn blueprint_profile(&self, tables: &ProfileTables) -> usize {
        let blueprint = if tables.is_default_hu() {
            BLUEPRINT.to_string()
        } else {
            tables.blueprint()
        };
        let sql = format!("SELECT COUNT(*) FROM {blueprint}");
        self.query_opt(&sql, &[])
            .await
            .ok()
            .flatten()
            .map(|r| r.get::<_, i64>(0) as usize)
            .unwrap_or(0)
    }

    async fn clustered_profile(&self, tables: &AbstractionTables, street: Street) -> bool {
        let isomorphism = if tables.is_default_v1() {
            ISOMORPHISM.to_string()
        } else {
            tables.isomorphism()
        };
        // Use deterministic bounds check instead of random sampling.
        // Observation encoding: cards are byte-packed, so range depends on card count.
        // Each card is 1-52 (1 + card_value). Values are shifted left 8 bits per card.
        //
        // Street boundaries by number of cards (n_cards * 8 bits):
        // - Preflop: 2 cards → 16 bits → max ~13,364 (0x3434)
        // - Flop: 5 cards → 40 bits → max ~57B (0x3434343434)
        // - Turn: 6 cards → 48 bits → max ~57T (0x343434343434)
        // - River: 7 cards → 56 bits → values start at ~283T (0x01010101010101)
        //
        // The boundary between n-card and (n+1)-card encodings is:
        // min_(n+1) = 0x0101...01 (n+1 bytes of 0x01)
        let (min_obs, max_obs): (i64, i64) = match street {
            // 2 cards: 0x0101 to 0x3434
            Street::Pref => (0x0101, 0x3434),
            // 5 cards: 0x0101010101 to 0x3434343434
            Street::Flop => (0x01_01_01_01_01, 0x34_34_34_34_34),
            // 6 cards: 0x010101010101 to 0x343434343434
            Street::Turn => (0x01_01_01_01_01_01, 0x34_34_34_34_34_34),
            // 7 cards: 0x01010101010101 and above
            Street::Rive => (0x01_01_01_01_01_01_01, i64::MAX),
        };
        let sql = format!("SELECT 1 FROM {isomorphism} WHERE obs >= $1 AND obs <= $2 LIMIT 1");
        self.query_opt(&sql, &[&min_obs, &max_obs])
            .await
            .ok()
            .flatten()
            .is_some()
    }
}

#[async_trait::async_trait]
impl ProfileCheck for Arc<Client> {
    async fn epochs_profile(&self, tables: &ProfileTables) -> usize {
        self.as_ref().epochs_profile(tables).await
    }
    async fn blueprint_profile(&self, tables: &ProfileTables) -> usize {
        self.as_ref().blueprint_profile(tables).await
    }
    async fn clustered_profile(&self, tables: &AbstractionTables, street: Street) -> bool {
        self.as_ref().clustered_profile(tables, street).await
    }
}

/// SQL generation helpers for profile-aware status queries.
///
/// These functions expose the SQL strings for testing purposes.
pub mod profile_check_sql {
    use crate::save::{AbstractionTables, BLUEPRINT, EPOCH, ISOMORPHISM, ProfileTables};

    /// Generate the SQL for querying epoch count for a profile.
    pub fn epochs_sql(tables: &ProfileTables) -> String {
        let epoch = if tables.is_default_hu() {
            EPOCH.to_string()
        } else {
            tables.epoch()
        };
        format!("SELECT value FROM {epoch} WHERE key = 'current'")
    }

    /// Generate the SQL for counting blueprint rows for a profile.
    pub fn blueprint_count_sql(tables: &ProfileTables) -> String {
        let blueprint = if tables.is_default_hu() {
            BLUEPRINT.to_string()
        } else {
            tables.blueprint()
        };
        format!("SELECT COUNT(*) FROM {blueprint}")
    }

    /// Generate the SQL for checking if clustering is complete for an abstraction version.
    pub fn clustered_sql(tables: &AbstractionTables) -> String {
        let isomorphism = if tables.is_default_v1() {
            ISOMORPHISM.to_string()
        } else {
            tables.isomorphism()
        };
        format!("SELECT 1 FROM {isomorphism} WHERE obs = $1")
    }
}

#[cfg(test)]
mod tests {
    use super::profile_check_sql::*;
    use crate::save::{AbstractionTables, ProfileTables};

    // ----- Profile Check SQL Tests -----
    // AC: training status queries count rows in profile tables

    #[test]
    fn epochs_sql_uses_profile_epoch_table() {
        let tables = ProfileTables::new("bp_10max_cash");
        let sql = epochs_sql(&tables);

        // Should query the profile-specific epoch table
        assert!(sql.contains("epoch_bp_10max_cash"));
        assert!(sql.contains("SELECT value FROM"));
        assert!(sql.contains("WHERE key = 'current'"));
    }

    #[test]
    fn blueprint_count_sql_uses_profile_blueprint_table() {
        let tables = ProfileTables::new("bp_6max_tourney");
        let sql = blueprint_count_sql(&tables);

        // Should count from profile-specific blueprint table
        assert!(sql.contains("SELECT COUNT(*) FROM blueprint_bp_6max_tourney"));
    }

    #[test]
    fn clustered_sql_uses_abstraction_isomorphism_table() {
        let tables = AbstractionTables::new("abs_v3_p10");
        let sql = clustered_sql(&tables);

        // Should query the versioned isomorphism table
        assert!(sql.contains("isomorphism_abs_v3_p10"));
        assert!(sql.contains("SELECT 1 FROM"));
        assert!(sql.contains("WHERE obs = $1"));
    }

    #[test]
    fn default_hu_profile_uses_static_tables() {
        let tables = ProfileTables::default_hu();

        // Epochs SQL should use static epoch table
        let epochs = epochs_sql(&tables);
        assert!(epochs.contains("FROM epoch WHERE") || epochs.contains("FROM epoch "));

        // Blueprint count SQL should use static blueprint table
        let count = blueprint_count_sql(&tables);
        assert!(count.contains("FROM blueprint") && !count.contains("FROM blueprint_"));
    }

    #[test]
    fn default_v1_abstraction_uses_static_tables() {
        let tables = AbstractionTables::default_v1();

        // Clustered SQL should use static isomorphism table
        let clustered = clustered_sql(&tables);
        assert!(
            clustered.contains("FROM isomorphism WHERE") || clustered.contains("FROM isomorphism ")
        );
        assert!(!clustered.contains("isomorphism_"));
    }

    #[test]
    fn different_profiles_query_different_tables() {
        let p3 = ProfileTables::new("bp_3max");
        let p6 = ProfileTables::new("bp_6max");
        let p10 = ProfileTables::new("bp_10max");

        let count3 = blueprint_count_sql(&p3);
        let count6 = blueprint_count_sql(&p6);
        let count10 = blueprint_count_sql(&p10);

        // Each profile queries its own table
        assert!(count3.contains("blueprint_bp_3max"));
        assert!(count6.contains("blueprint_bp_6max"));
        assert!(count10.contains("blueprint_bp_10max"));

        // No cross-profile queries
        assert!(!count3.contains("bp_6max"));
        assert!(!count6.contains("bp_10max"));
    }

    #[test]
    fn different_abstraction_versions_query_different_tables() {
        let v2 = AbstractionTables::new("abs_v2_p6");
        let v3 = AbstractionTables::new("abs_v3_p6");

        let c2 = clustered_sql(&v2);
        let c3 = clustered_sql(&v3);

        // Each version queries its own isomorphism table
        assert!(c2.contains("isomorphism_abs_v2_p6"));
        assert!(c3.contains("isomorphism_abs_v3_p6"));
    }
}
