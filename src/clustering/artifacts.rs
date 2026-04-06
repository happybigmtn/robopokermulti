//! Clustering artifacts produced by a layer.

use super::*;

/// Clustering artifacts produced by a layer.
pub struct Artifacts {
    pub lookup: Lookup,
    pub metric: Metric,
    pub future: Future,
}

impl From<Lookup> for Artifacts {
    fn from(lookup: Lookup) -> Self {
        Self {
            lookup,
            metric: Metric::default(),
            future: Future::default(),
        }
    }
}

#[cfg(feature = "database")]
impl Artifacts {
    pub async fn stream(self, client: &tokio_postgres::Client) {
        use crate::save::Streamable;
        self.lookup.stream(client).await;
        self.metric.stream(client).await;
        self.future.stream(client).await;
    }

    pub async fn stream_profile(
        self,
        client: &tokio_postgres::Client,
        tables: &crate::save::AbstractionTables,
    ) {
        self.lookup.stream_profile(client, tables).await;
        self.metric.stream_profile(client, tables).await;
        self.future.stream_profile(client, tables).await;
    }
}
