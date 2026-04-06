//! Slow distributed training session

use super::*;
use crate::workers::*;
use std::sync::Arc;
use tokio_postgres::Client;

/// Slow distributed training using Worker<Postgres>.
pub struct SlowSession {
    client: Arc<Client>,
    pool: Pool,
    tables: crate::save::TrainingTables,
}

impl SlowSession {
    pub async fn new(
        client: Arc<Client>,
        tables: crate::save::TrainingTables,
        player_count: usize,
        profile_config: Option<crate::save::TrainingProfileConfig>,
    ) -> Self {
        Self {
            pool: Pool::new(client.clone(), tables.clone(), player_count, profile_config).await,
            client,
            tables,
        }
    }
}

#[async_trait::async_trait]
impl Trainer for SlowSession {
    fn client(&self) -> &Arc<Client> {
        &self.client
    }
    fn tables(&self) -> &crate::save::TrainingTables {
        &self.tables
    }
    async fn step(&mut self) {
        self.pool.step().await;
    }
    async fn epoch(&self) -> usize {
        self.pool.epoch()
    }
    async fn checkpoint(&self) -> Option<String> {
        self.pool.checkpoint()
    }
    async fn summary(&self) -> String {
        self.pool.summary()
    }
    async fn sync(self) {
        // SlowSession writes directly to DB, no sync needed
    }
}
