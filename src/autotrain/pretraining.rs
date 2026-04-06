//! Pretraining - hierarchical clustering pipeline for poker abstractions.
//!
//! Manages clustering from scratch to postgres without disk I/O:
//! 1. River: equity-based abstractions (computed from scratch)
//! 2. Turn: k-means on river distributions (hydrates river data)
//! 3. Flop: k-means on turn distributions (hydrates turn data)
//! 4. Preflop: 1:1 isomorphism enumeration (computed from scratch)

use crate::cards::*;
use crate::clustering::*;
use crate::database::*;
use crate::save::*;
use std::sync::Arc;
use tokio_postgres::Client;

type PrefLayer = Layer<{ Street::Pref.k() }, { Street::Pref.n_isomorphisms() }>;
type FlopLayer = Layer<{ Street::Flop.k() }, { Street::Flop.n_isomorphisms() }>;
type TurnLayer = Layer<{ Street::Turn.k() }, { Street::Turn.n_isomorphisms() }>;

/// Zero-sized orchestrator for the clustering pipeline.
/// Encapsulates all clustering logic so Trainer stays clean.
pub struct PreTraining;

impl PreTraining {
    /// Run the complete clustering pipeline if needed.
    /// Returns true if any clustering was performed.
    pub async fn run(client: &Arc<Client>, tables: &TrainingTables) {
        let streets = Self::pending(client, tables).await;
        for street in streets.iter().cloned() {
            log::info!("{:<32}{:<32}", "beginning clustering", street);
            if street == Street::Rive {
                // Stream river isomorphisms directly to DB to avoid huge in-memory lookup maps.
                Lookup::stream_river_profile(client, &tables.abstraction).await;
                Metric::default()
                    .stream_profile(client, &tables.abstraction)
                    .await;
                Future::default()
                    .stream_profile(client, &tables.abstraction)
                    .await;
            } else {
                Self::cluster(street, client, tables)
                    .await
                    .stream_profile(client, &tables.abstraction)
                    .await;
            }
        }
        if streets.len() > 0 {
            Self::finalize(client, tables).await;
        }
    }
    /// Cluster a street via k-means. Dependencies loaded from postgres.
    /// Dispatches to the appropriate const-generic Layer based on street.
    async fn cluster(street: Street, client: &Arc<Client>, tables: &TrainingTables) -> Artifacts {
        match street {
            Street::Rive => Artifacts::from(Lookup::grow(street)),
            Street::Turn => TurnLayer::cluster_profile(street, client, &tables.abstraction).await,
            Street::Flop => FlopLayer::cluster_profile(street, client, &tables.abstraction).await,
            Street::Pref => PrefLayer::cluster_profile(street, client, &tables.abstraction).await,
        }
    }

    /// Collect unclustered streets in reverse order (river first).
    async fn pending(client: &Arc<Client>, tables: &TrainingTables) -> Vec<Street> {
        let mut pending = Vec::new();
        for street in Street::all().iter().rev().cloned() {
            if client.clustered_profile(&tables.abstraction, street).await {
                log::info!("{:<32}{:<32}", "skipping clustering", street);
            } else {
                pending.push(street);
            }
        }
        pending
    }

    /// Prepare tables for streaming (truncate if needed).
    #[allow(unused)]
    async fn truncate(client: &Arc<Client>, tables: &TrainingTables) {
        Metric::truncate_profile(client, &tables.abstraction).await;
        Future::truncate_profile(client, &tables.abstraction).await;
        Lookup::truncate_profile(client, &tables.abstraction).await;
    }

    /// Finalize tables after all data is streamed.
    async fn finalize(client: &Arc<Client>, tables: &TrainingTables) {
        Lookup::finalize_profile(client, &tables.abstraction).await;
        Metric::finalize_profile(client, &tables.abstraction).await;
        Future::finalize_profile(client, &tables.abstraction).await;
    }
}
