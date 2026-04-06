//! Session trait - unified training abstraction

use super::*;
use crate::cards::*;
use crate::database::*;
use std::sync::Arc;
use tokio_postgres::Client;

/// Unified training session interface.
/// Both fast and slow modes implement this for polymorphic training loops.
#[async_trait::async_trait]
pub trait Trainer: Send + Sync + Sized {
    /// Database client for persistence operations.
    fn client(&self) -> &Arc<Client>;
    /// Training table configuration (profile + abstraction version).
    fn tables(&self) -> &crate::save::TrainingTables;
    /// Player count for the active training run.
    fn player_count(&self) -> usize;
    /// Sync in-memory state to database on graceful exit.
    async fn sync(self);
    /// Run one training iteration.
    async fn step(&mut self);
    /// Get current epoch count.
    async fn epoch(&self) -> usize;
    /// Get final summary on completion.
    async fn summary(&self) -> String;
    /// Get training statistics if checkpoint interval has elapsed.
    async fn checkpoint(&self) -> Option<String>;

    async fn train(mut self) {
        self.pretraining().await;
        log::info!("training blueprint");
        log::info!("press 'Q + ↵' to stop gracefully");
        loop {
            self.step().await;
            if let Some(stats) = self.checkpoint().await {
                let db_epoch = self.epochs().await;
                let blueprint_rows = self.blueprint().await;
                log::info!(
                    "{} · db_epoch {} · blueprint_rows {}",
                    stats,
                    db_epoch,
                    blueprint_rows
                );
            }
            if crate::interrupted() {
                log::info!("{}", self.summary().await);
                break;
            }
        }
        self.sync().await;
    }

    async fn pretraining(&self) {
        PreTraining::run(self.client(), self.tables()).await;
    }

    async fn epochs(&self) -> usize {
        self.client().epochs_profile(&self.tables().profile).await
    }
    async fn blueprint(&self) -> usize {
        self.client()
            .blueprint_profile(&self.tables().profile)
            .await
    }
    async fn complete(&self, street: Street) -> bool {
        self.client()
            .clustered_profile(&self.tables().abstraction, street)
            .await
    }

    async fn sanity(mut self) {
        let steps = std::env::var("TRAINING_SANITY_STEPS")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(2);
        if !self.sanity_smoke().await {
            return;
        }
        log::info!("~ sanity: profile tables OK");
        self.status().await;
        let mut pending = Vec::new();
        for street in Street::all().iter().rev().cloned() {
            if !self.complete(street).await {
                pending.push(street);
            }
        }
        if !pending.is_empty() {
            log::info!(
                "~ sanity: clustering incomplete for {:?} (run --cluster first)",
                pending
            );
            return;
        }
        if steps == 0 {
            log::info!("~ sanity: skipping training steps (TRAINING_SANITY_STEPS=0)");
        } else {
            log::info!("~ sanity: running {} training steps", steps);
            for _ in 0..steps {
                self.step().await;
            }
        }
        log::info!("~ sanity: blueprint rows = {}", self.blueprint().await);
        self.sync().await;
    }

    async fn sanity_smoke(&self) -> bool {
        use crate::clustering::Pair;
        use crate::gameplay::{Abstraction, Edge, Path};
        use crate::save::{ABSTRACTION, BLUEPRINT, EPOCH, ISOMORPHISM, METRIC, TRANSITIONS};

        let tables = self.tables();
        let profile = &tables.profile;
        let abstraction = &tables.abstraction;
        let blueprint = if profile.is_default_hu() {
            BLUEPRINT.to_string()
        } else {
            profile.blueprint()
        };
        let epoch = if profile.is_default_hu() {
            EPOCH.to_string()
        } else {
            profile.epoch()
        };
        let abstraction_table = if abstraction.is_default_v1() {
            ABSTRACTION.to_string()
        } else {
            abstraction.abstraction()
        };
        let isomorphism_table = if abstraction.is_default_v1() {
            ISOMORPHISM.to_string()
        } else {
            abstraction.isomorphism()
        };
        let metric_table = if abstraction.is_default_v1() {
            METRIC.to_string()
        } else {
            abstraction.metric()
        };
        let transitions_table = if abstraction.is_default_v1() {
            TRANSITIONS.to_string()
        } else {
            abstraction.transitions()
        };

        let client = self.client().as_ref();
        if let Err(err) = client.batch_execute("BEGIN").await {
            log::error!("~ sanity: failed to start transaction: {}", err);
            return false;
        }

        let observation =
            Observation::try_from("As Ah").unwrap_or_else(|_| Observation::from(Street::Pref));
        let obs = i64::from(Isomorphism::from(observation));
        let abs0 = Abstraction::from((Street::Pref, 0));
        let abs1_index = if Street::Pref.n_abstractions() > 1 {
            1
        } else {
            0
        };
        let abs1 = Abstraction::from((Street::Pref, abs1_index));
        let abs0_i16 = i16::from(abs0);
        let abs1_i16 = i16::from(abs1);
        let abs0_val = abs0_i16 as i64;
        let tri_val = i32::from(Pair::from((&abs0, &abs1)));
        let past = i64::from(Path::default());
        let future = i64::from(Path::default());
        let present = i16::from(abs0);
        let edge = u64::from(Edge::Check) as i64;
        let seat_count = self.player_count() as i16;
        let seat_position = 0_i16;
        let active_players = seat_count;

        let epoch_sql = format!(
            "INSERT INTO {epoch} (key, value) VALUES ('current', 0) \
             ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value"
        );
        if let Err(err) = client.execute(&epoch_sql, &[]).await {
            log::error!("~ sanity: epoch write failed: {}", err);
            let _ = client.batch_execute("ROLLBACK").await;
            return false;
        }

        let blueprint_result = if profile.is_default_hu() {
            let blueprint_sql = format!(
                "INSERT INTO {blueprint} (past, present, future, edge, policy, regret) \
                 VALUES ($1, $2, $3, $4, $5, $6) \
                 ON CONFLICT (past, present, future, edge) \
                 DO UPDATE SET policy = EXCLUDED.policy, regret = EXCLUDED.regret"
            );
            client
                .execute(
                    &blueprint_sql,
                    &[&past, &present, &future, &edge, &0.5_f32, &0.0_f32],
                )
                .await
        } else {
            let blueprint_sql = format!(
                "INSERT INTO {blueprint} \
                 (past, present, future, seat_count, seat_position, active_players, edge, policy, regret) \
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9) \
                 ON CONFLICT (past, present, future, seat_count, seat_position, active_players, edge) \
                 DO UPDATE SET policy = EXCLUDED.policy, regret = EXCLUDED.regret"
            );
            client
                .execute(
                    &blueprint_sql,
                    &[
                        &past,
                        &present,
                        &future,
                        &seat_count,
                        &seat_position,
                        &active_players,
                        &edge,
                        &0.5_f32,
                        &0.0_f32,
                    ],
                )
                .await
        };
        if let Err(err) = blueprint_result {
            log::error!("~ sanity: blueprint write failed: {}", err);
            let _ = client.batch_execute("ROLLBACK").await;
            return false;
        }

        let abstraction_sql = format!(
            "INSERT INTO {abstraction_table} (abs, street, equity, population) \
             VALUES ($1, $2, $3, $4) \
             ON CONFLICT (abs) DO UPDATE SET \
               street = EXCLUDED.street, \
               equity = EXCLUDED.equity, \
               population = EXCLUDED.population"
        );
        if let Err(err) = client
            .execute(
                &abstraction_sql,
                &[&abs0_val, &(Street::Pref as i16), &0.5_f32, &1_i32],
            )
            .await
        {
            log::error!("~ sanity: abstraction write failed: {}", err);
            let _ = client.batch_execute("ROLLBACK").await;
            return false;
        }

        let isomorphism_sql = if abstraction.is_default_v1() {
            format!(
                "INSERT INTO {isomorphism_table} (obs, abs) \
                 VALUES ($1, $2) \
                 ON CONFLICT (obs) DO UPDATE SET \
                   abs = EXCLUDED.abs"
            )
        } else {
            format!(
                "INSERT INTO {isomorphism_table} (obs, abs, seat_position) \
                 VALUES ($1, $2, $3) \
                 ON CONFLICT (obs, seat_position) DO UPDATE SET \
                   abs = EXCLUDED.abs, \
                   seat_position = EXCLUDED.seat_position"
            )
        };
        let result = if abstraction.is_default_v1() {
            client.execute(&isomorphism_sql, &[&obs, &abs0_i16]).await
        } else {
            client
                .execute(&isomorphism_sql, &[&obs, &abs0_i16, &0_i16])
                .await
        };
        if let Err(err) = result {
            log::error!("~ sanity: isomorphism write failed: {}", err);
            let _ = client.batch_execute("ROLLBACK").await;
            return false;
        }

        let metric_sql = format!("INSERT INTO {metric_table} (tri, dx) VALUES ($1, $2)");
        if let Err(err) = client.execute(&metric_sql, &[&tri_val, &0.0_f32]).await {
            log::error!("~ sanity: metric write failed: {}", err);
            let _ = client.batch_execute("ROLLBACK").await;
            return false;
        }

        let transitions_sql = if abstraction.is_default_v1() {
            format!("INSERT INTO {transitions_table} (prev, next, dx) VALUES ($1, $2, $3)")
        } else {
            format!(
                "INSERT INTO {transitions_table} (prev, next, dx) VALUES ($1, $2, $3) \
                 ON CONFLICT (prev, next) DO UPDATE SET dx = EXCLUDED.dx"
            )
        };
        if let Err(err) = client
            .execute(&transitions_sql, &[&abs0_i16, &abs1_i16, &0.0_f32])
            .await
        {
            log::error!("~ sanity: transitions write failed: {}", err);
            let _ = client.batch_execute("ROLLBACK").await;
            return false;
        }

        if let Err(err) = client.batch_execute("ROLLBACK").await {
            log::warn!("~ sanity: rollback failed: {}", err);
        }
        log::info!("~ sanity: database smoke test OK");
        true
    }

    async fn status(&self) {
        fn commas(n: usize) -> String {
            n.to_string()
                .as_bytes()
                .rchunks(3)
                .rev()
                .map(|c| std::str::from_utf8(c).unwrap())
                .collect::<Vec<_>>()
                .join(",")
        }
        log::info!("┌────────────┬───────────────┐");
        log::info!("│ Street     │ Clustered     │");
        log::info!("├────────────┼───────────────┤");
        for street in Street::all().iter().rev().cloned() {
            let done = self.complete(street).await;
            let mark = if done { "✓" } else { " " };
            log::info!(
                "│ {:?}{} │       {}       │",
                street,
                " ".repeat(10 - format!("{:?}", street).len()),
                mark
            );
        }
        log::info!("├────────────┼───────────────┤");
        log::info!("│ Epoch      │ {:>13} │", commas(self.epochs().await));
        log::info!("│ Blueprint  │ {:>13} │", commas(self.blueprint().await));
        log::info!("└────────────┴───────────────┘");
    }
}
