//! Fast in-memory training session

use super::*;
use crate::database::*;
use crate::mccfr::*;
use crate::save::*;
use std::sync::Arc;
use tokio_postgres::Client;
use tokio_postgres::binary_copy::BinaryCopyInWriter;

/// Fast in-memory training using NlheSolver.
pub struct FastSession {
    client: Arc<Client>,
    solver: NlheSolver,
    tables: crate::save::TrainingTables,
    player_count: usize,
}

impl FastSession {
    pub async fn new(
        client: Arc<Client>,
        tables: crate::save::TrainingTables,
        player_count: usize,
    ) -> Self {
        Self {
            solver: NlheSolver::hydrate_profile(client.clone(), &tables, player_count).await,
            client,
            tables,
            player_count,
        }
    }
}

#[async_trait::async_trait]
impl Trainer for FastSession {
    fn client(&self) -> &Arc<Client> {
        &self.client
    }
    fn tables(&self) -> &crate::save::TrainingTables {
        &self.tables
    }
    fn player_count(&self) -> usize {
        self.player_count
    }

    async fn step(&mut self) {
        self.solver.step();
    }

    async fn epoch(&self) -> usize {
        self.solver.profile().epochs()
    }

    async fn checkpoint(&self) -> Option<String> {
        self.solver.profile().metrics().and_then(|m| m.checkpoint())
    }

    async fn summary(&self) -> String {
        self.solver
            .profile()
            .metrics()
            .map(|m| m.summary())
            .unwrap_or_else(|| "training stopped".to_string())
    }

    async fn sync(self) {
        use crate::save::Row;
        let client = self.client;
        let epochs = self.solver.profile.epochs();
        let profile = self.solver.profile;
        client.stage_profile(&self.tables.profile).await;
        let (copy, columns) = if self.tables.profile.is_default_hu() {
            (
                format!(
                    "COPY {t} (past, present, future, edge, policy, regret) FROM STDIN BINARY",
                    t = crate::save::STAGING
                ),
                NlheProfile::columns().to_vec(),
            )
        } else {
            (
                format!(
                    "COPY {t} (past, present, future, seat_count, seat_position, active_players, edge, policy, regret) FROM STDIN BINARY",
                    t = self.tables.profile.staging()
                ),
                NlheProfile::profile_columns().to_vec(),
            )
        };
        let writer =
            BinaryCopyInWriter::new(client.copy_in(&copy).await.expect("copy_in"), &columns);
        futures::pin_mut!(writer);
        if self.tables.profile.is_default_hu() {
            for row in profile.rows() {
                row.write(writer.as_mut()).await;
            }
        } else {
            for row in profile.rows_profile() {
                row.write(writer.as_mut()).await;
            }
        }
        writer.finish().await.expect("finish stream");
        client.merge_profile(&self.tables.profile).await;
        client.stamp_profile(&self.tables.profile, epochs).await;
        log::info!("profile sync complete (epoch {})", epochs);
    }
}
