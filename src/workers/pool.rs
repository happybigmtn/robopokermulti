use super::*;
use crate::mccfr::TrainingStats;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Instant;
use tokio_postgres::Client;

pub struct Pool {
    workers: Vec<Worker>,
    started: Instant,
    checked: Mutex<Instant>,
}

impl Pool {
    pub async fn new(
        client: Arc<Client>,
        tables: crate::save::TrainingTables,
        player_count: usize,
        profile: Option<crate::save::TrainingProfileConfig>,
    ) -> Self {
        let profile = profile.map(Arc::new);
        Self {
            workers: (0..num_cpus::get())
                .map(|_| client.clone())
                .map(|client| {
                    Worker::for_profile(client, player_count, tables.clone(), profile.clone())
                })
                .collect(),
            started: Instant::now(),
            checked: Mutex::new(Instant::now()),
        }
    }
    pub async fn step(&self) {
        futures::future::join_all(self.workers.iter().map(|w| w.step())).await;
    }
    pub fn checkpoint(&self) -> Option<String> {
        let mut last = self.checked.lock().unwrap();
        if last.elapsed() >= crate::training_log_interval() {
            *last = Instant::now();
            Some(self.stats())
        } else {
            None
        }
    }
}

impl TrainingStats for Pool {
    fn epoch(&self) -> usize {
        self.workers.iter().map(|w| w.epoch()).sum()
    }
    fn nodes(&self) -> usize {
        self.workers.iter().map(|w| w.nodes()).sum()
    }
    fn infos(&self) -> usize {
        self.workers.iter().map(|w| w.infos()).sum()
    }
    fn elapsed(&self) -> std::time::Duration {
        self.started.elapsed()
    }
}
