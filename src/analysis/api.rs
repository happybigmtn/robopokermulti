use crate::cards::*;
use crate::clustering::*;
use crate::database::*;
use crate::dto::*;
use crate::gameplay::*;
use crate::mccfr::*;
use crate::save::*;
use crate::transport::*;
use crate::*;
use std::sync::Arc;
use tokio_postgres::Client;

const N_SAMPLES: i64 = 5;

pub struct API {
    client: Arc<Client>,
    tables: TrainingTables,
}

impl From<Arc<Client>> for API {
    fn from(client: Arc<Client>) -> Self {
        Self {
            client,
            tables: TrainingTables::default_hu(),
        }
    }
}

impl From<(Arc<Client>, TrainingTables)> for API {
    fn from((client, tables): (Arc<Client>, TrainingTables)) -> Self {
        Self { client, tables }
    }
}

// constructor
impl API {
    pub async fn new() -> Self {
        Self::from(crate::save::db().await)
    }

    pub async fn from_env() -> anyhow::Result<Self> {
        let profile_key = env_optional("PROFILE_KEY");
        let abstraction_version = env_optional("ABSTRACTION_VERSION");
        match validate_analysis_config(profile_key, abstraction_version)? {
            None => Ok(Self::new().await),
            Some(tables) => {
                let client = crate::save::db_profile(&tables, None).await;
                Ok(Self::from((client, tables)))
            }
        }
    }

    pub fn parse_observation_target(&self, raw: &str) -> anyhow::Result<(Observation, u8)> {
        parse_observation_target_raw(raw, &self.tables.abstraction)
    }

    fn abstraction_table(&self) -> String {
        if self.tables.abstraction.is_default_v1() {
            ABSTRACTION.to_string()
        } else {
            self.tables.abstraction.abstraction()
        }
    }

    fn isomorphism_table(&self) -> String {
        if self.tables.abstraction.is_default_v1() {
            ISOMORPHISM.to_string()
        } else {
            self.tables.abstraction.isomorphism()
        }
    }

    fn metric_table(&self) -> String {
        if self.tables.abstraction.is_default_v1() {
            METRIC.to_string()
        } else {
            self.tables.abstraction.metric()
        }
    }

    fn transitions_table(&self) -> String {
        if self.tables.abstraction.is_default_v1() {
            TRANSITIONS.to_string()
        } else {
            self.tables.abstraction.transitions()
        }
    }

    fn blueprint_table(&self) -> String {
        if self.tables.profile.is_default_hu() {
            BLUEPRINT.to_string()
        } else {
            self.tables.profile.blueprint()
        }
    }
}

// global lookups
impl API {
    pub async fn obs_to_abs(
        &self,
        obs: Observation,
        seat_position: u8,
    ) -> anyhow::Result<Abstraction> {
        Ok(self
            .client
            .encode_profile(
                &self.tables.abstraction,
                Isomorphism::from(obs),
                seat_position,
            )
            .await)
    }
    pub async fn metric(&self, street: Street) -> anyhow::Result<Metric> {
        Ok(self
            .client
            .metric_profile(&self.tables.abstraction, street)
            .await)
    }
}

// equity calculations
impl API {
    pub async fn abs_equity(&self, abs: Abstraction) -> anyhow::Result<Probability> {
        Ok(self
            .client
            .equity_profile(&self.tables.abstraction, abs)
            .await)
    }
    #[rustfmt::skip]
    pub async fn obs_equity(
        &self,
        obs: Observation,
        seat_position: u8,
    ) -> anyhow::Result<Probability> {
        let iso = i64::from(Isomorphism::from(obs));
        let abstraction = self.abstraction_table();
        let isomorphism = self.isomorphism_table();
        let transitions = self.transitions_table();
        let river = format!(
            "SELECT a.equity \
             FROM   {isomorphism} e \
             JOIN   {abstraction} a ON a.abs = e.abs \
             WHERE  e.obs = $1{}",
            if self.tables.abstraction.is_default_v1() {
                String::new()
            } else {
                " AND e.seat_position = $2".to_string()
            }
        );
        let other = format!(
            "SELECT SUM(t.dx * a.equity) \
             FROM   {transitions} t \
             JOIN   {isomorphism} e ON e.abs = t.prev \
             JOIN   {abstraction} a ON a.abs = t.next \
             WHERE  e.obs = $1{}",
            if self.tables.abstraction.is_default_v1() {
                String::new()
            } else {
                " AND e.seat_position = $2".to_string()
            }
        );
        let sql = if obs.street() == Street::Rive { &river } else { &other };
        let row = if self.tables.abstraction.is_default_v1() {
            self.client
                .query_one(sql, &[&iso])
                .await
                .map_err(|e| anyhow::anyhow!("fetch observation equity: {}", e))?
        } else {
            let seat = seat_position as i16;
            self.client
                .query_one(sql, &[&iso, &seat])
                .await
                .map_err(|e| anyhow::anyhow!("fetch observation equity: {}", e))?
        };
        Ok(row
            .get::<_, f32>(0)
            .into())
    }
}

// distance calculations
impl API {
    pub async fn abs_distance(
        &self,
        abs1: Abstraction,
        abs2: Abstraction,
    ) -> anyhow::Result<Energy> {
        if abs1.street() != abs2.street() {
            return Err(anyhow::anyhow!("abstractions must be from the same street"));
        }
        if abs1 == abs2 {
            return Ok(0 as Energy);
        }
        Ok(self
            .client
            .distance_profile(&self.tables.abstraction, Pair::from((&abs1, &abs2)))
            .await)
    }
    pub async fn obs_distance(
        &self,
        obs1: Observation,
        seat1: u8,
        obs2: Observation,
        seat2: u8,
    ) -> anyhow::Result<Energy> {
        if obs1.street() != obs2.street() {
            return Err(anyhow::anyhow!("observations must be from the same street"));
        }
        let (ref hx, ref hy, ref metric) = tokio::try_join!(
            self.obs_histogram(obs1, seat1),
            self.obs_histogram(obs2, seat2),
            self.metric(obs1.street().next())
        )?;
        Ok(Sinkhorn::from((hx, hy, metric)).minimize().cost())
    }
}

// population lookups
impl API {
    pub async fn abs_population(&self, abs: Abstraction) -> anyhow::Result<usize> {
        Ok(self
            .client
            .population_profile(&self.tables.abstraction, abs)
            .await)
    }
    #[rustfmt::skip]
    pub async fn obs_population(
        &self,
        obs: Observation,
        seat_position: u8,
    ) -> anyhow::Result<usize> {
        let iso = i64::from(Isomorphism::from(obs));
        let abstraction = self.abstraction_table();
        let isomorphism = self.isomorphism_table();
        let sql = format!(
            "SELECT population \
             FROM   {abstraction} a \
             JOIN   {isomorphism} e ON e.abs = a.abs \
             WHERE  e.obs = $1{}",
            if self.tables.abstraction.is_default_v1() {
                String::new()
            } else {
                " AND e.seat_position = $2".to_string()
            }
        );
        let row = if self.tables.abstraction.is_default_v1() {
            self.client
                .query_one(&sql, &[&iso])
                .await
                .map_err(|e| anyhow::anyhow!("fetch observation population: {}", e))?
        } else {
            let seat = seat_position as i16;
            self.client
                .query_one(&sql, &[&iso, &seat])
                .await
                .map_err(|e| anyhow::anyhow!("fetch observation population: {}", e))?
        };
        Ok(row
            .get::<_, i64>(0) as usize)
    }
}

// histogram aggregation via join
impl API {
    pub async fn abs_histogram(&self, abs: Abstraction) -> anyhow::Result<Histogram> {
        Ok(self
            .client
            .histogram_profile(&self.tables.abstraction, abs)
            .await)
    }
    #[rustfmt::skip]
    pub async fn obs_histogram(
        &self,
        obs: Observation,
        seat_position: u8,
    ) -> anyhow::Result<Histogram> {
        let idx = i64::from(Isomorphism::from(obs));
        let mass = obs.street().n_children() as f32;
        let transitions = self.transitions_table();
        let isomorphism = self.isomorphism_table();
        let sql = format!(
            "SELECT next, dx \
             FROM   {transitions} t \
             JOIN   {isomorphism} e ON e.abs = t.prev \
             WHERE  e.obs = $1{}",
            if self.tables.abstraction.is_default_v1() {
                String::new()
            } else {
                " AND e.seat_position = $2".to_string()
            }
        );
        let street = obs.street().next();
        let rows = if self.tables.abstraction.is_default_v1() {
            self.client
                .query(&sql, &[&idx])
                .await
                .map_err(|e| anyhow::anyhow!("fetch observation histogram: {}", e))?
        } else {
            let seat = seat_position as i16;
            self.client
                .query(&sql, &[&idx, &seat])
                .await
                .map_err(|e| anyhow::anyhow!("fetch observation histogram: {}", e))?
        };
        Ok(rows
            .iter()
            .map(|row| (row.get::<_, i16>(0), row.get::<_, Energy>(1)))
            .map(|(next, dx)| (next, (dx * mass).round() as usize))
            .map(|(next, dx)| (Abstraction::from(next), dx))
            .fold(Histogram::empty(street), |mut h, (next, dx)| {
                h.set(next, dx);
                h
            }))
    }
}

// observation similarity lookups
impl API {
    #[rustfmt::skip]
    pub async fn obs_similar(
        &self,
        obs: Observation,
        seat_position: u8,
    ) -> anyhow::Result<Vec<Observation>> {
        let iso = i64::from(Isomorphism::from(obs));
        let isomorphism = self.isomorphism_table();
        let abstraction = self.abstraction_table();
        let sql = format!(
            "WITH target AS ( \
                 SELECT abs, population \
                 FROM   {isomorphism} e \
                 JOIN   {abstraction} a ON a.abs = e.abs \
                 WHERE  e.obs = $1{} \
             ) \
             SELECT   e.obs \
             FROM     {isomorphism} e \
             JOIN     target t ON t.abs = e.abs \
             WHERE    e.obs != $1{} \
             AND      e.position  < LEAST(${}, t.population) \
             AND      e.position >= FLOOR(RANDOM() * GREATEST(t.population - ${}, 1)) \
             LIMIT    ${}",
            if self.tables.abstraction.is_default_v1() {
                String::new()
            } else {
                " AND e.seat_position = $2".to_string()
            },
            if self.tables.abstraction.is_default_v1() {
                String::new()
            } else {
                " AND e.seat_position = $2".to_string()
            },
            if self.tables.abstraction.is_default_v1() { 2 } else { 3 },
            if self.tables.abstraction.is_default_v1() { 2 } else { 3 },
            if self.tables.abstraction.is_default_v1() { 2 } else { 3 },
        );
        let rows = if self.tables.abstraction.is_default_v1() {
            self.client
                .query(&sql, &[&iso, &N_SAMPLES])
                .await
                .map_err(|e| anyhow::anyhow!("fetch similar observations: {}", e))?
        } else {
            let seat = seat_position as i16;
            self.client
                .query(&sql, &[&iso, &seat, &N_SAMPLES])
                .await
                .map_err(|e| anyhow::anyhow!("fetch similar observations: {}", e))?
        };
        Ok(rows
            .iter()
            .map(|row| row.get::<_, i64>(0))
            .map(Observation::from)
            .collect())
    }
    #[rustfmt::skip]
    pub async fn abs_similar(&self, abs: Abstraction) -> anyhow::Result<Vec<Observation>> {
        let abs = i16::from(abs);
        let abstraction = self.abstraction_table();
        let isomorphism = self.isomorphism_table();
        let sql = format!(
            "WITH target AS ( \
                 SELECT population \
                 FROM   {abstraction} \
                 WHERE  abs = $1 \
             ) \
             SELECT   obs \
             FROM     {isomorphism} e, target t \
             WHERE    e.abs = $1 \
             AND      e.position  < LEAST($2, t.population) \
             AND      e.position >= FLOOR(RANDOM() * GREATEST(t.population - $2, 1)) \
             LIMIT    $2"
        );
        Ok(self
            .client
            .query(&sql, &[&abs, &N_SAMPLES])
            .await
            .map_err(|e| anyhow::anyhow!("fetch observations similar to abstraction: {}", e))?
            .iter()
            .map(|row| row.get::<_, i64>(0))
            .map(Observation::from)
            .collect())
    }
    #[rustfmt::skip]
    pub async fn replace_obs(&self, obs: Observation, seat_position: u8) -> anyhow::Result<Observation> {
        let isomorphism = self.isomorphism_table();
        let abstraction = self.abstraction_table();
        let sql = format!(
            "WITH sample AS ( \
                 SELECT e.abs, a.population, FLOOR(RANDOM() * a.population)::INTEGER AS i \
                 FROM   {isomorphism} e \
                 JOIN   {abstraction} a ON a.abs = e.abs \
                 WHERE  e.obs = $1{} \
             ) \
             SELECT e.obs \
             FROM   sample s \
             JOIN   {isomorphism} e ON e.abs = s.abs AND e.position = s.i{} \
             LIMIT  1",
            if self.tables.abstraction.is_default_v1() {
                String::new()
            } else {
                " AND e.seat_position = $2".to_string()
            },
            if self.tables.abstraction.is_default_v1() {
                String::new()
            } else {
                " AND e.seat_position = $2".to_string()
            }
        );
        let iso = i64::from(Isomorphism::from(obs));
        let row = if self.tables.abstraction.is_default_v1() {
            self.client
                .query_one(&sql, &[&iso])
                .await
                .map_err(|e| anyhow::anyhow!("replace observation: {}", e))?
        } else {
            let seat = seat_position as i16;
            self.client
                .query_one(&sql, &[&iso, &seat])
                .await
                .map_err(|e| anyhow::anyhow!("replace observation: {}", e))?
        };
        Ok(Observation::from(row.get::<_, i64>(0)))
    }
}

// proximity lookups
impl API {
    #[rustfmt::skip]
    pub async fn abs_nearby(&self, abs: Abstraction) -> anyhow::Result<Vec<(Abstraction, Energy)>> {
        let abs = i16::from(abs);
        let abstraction = self.abstraction_table();
        let metric = self.metric_table();
        let sql = format!(
            "SELECT   a.abs, m.dx \
             FROM     {abstraction} a \
             JOIN     {metric} m ON m.tri = get_pair_tri(a.abs, $1) \
             WHERE    a.abs != $1 \
             ORDER BY m.dx ASC \
             LIMIT    $2"
        );
        Ok(self
            .client
            .query(&sql, &[&abs, &N_SAMPLES])
            .await
            .map_err(|e| anyhow::anyhow!("fetch nearby abstractions: {}", e))?
            .iter()
            .map(|row| (row.get::<_, i16>(0), row.get::<_, Energy>(1)))
            .map(|(abs, distance)| (Abstraction::from(abs), distance))
            .collect())
    }
    #[rustfmt::skip]
    pub async fn obs_nearby(
        &self,
        obs: Observation,
        seat_position: u8,
    ) -> anyhow::Result<Vec<(Abstraction, Energy)>> {
        let iso = i64::from(Isomorphism::from(obs));
        let isomorphism = self.isomorphism_table();
        let abstraction = self.abstraction_table();
        let metric = self.metric_table();
        let sql = format!(
            "SELECT   a.abs, m.dx \
             FROM     {isomorphism} e \
             JOIN     {abstraction} a ON a.street = get_street_abs(e.abs) \
             JOIN     {metric} m ON m.tri = get_pair_tri(a.abs, e.abs) \
             WHERE    e.obs = $1{} \
             AND      a.abs != e.abs \
             ORDER BY m.dx ASC \
             LIMIT    ${}",
            if self.tables.abstraction.is_default_v1() {
                String::new()
            } else {
                " AND e.seat_position = $2".to_string()
            },
            if self.tables.abstraction.is_default_v1() { 2 } else { 3 },
        );
        let rows = if self.tables.abstraction.is_default_v1() {
            self.client
                .query(&sql, &[&iso, &N_SAMPLES])
                .await
                .map_err(|e| anyhow::anyhow!("fetch nearby abstractions for observation: {}", e))?
        } else {
            let seat = seat_position as i16;
            self.client
                .query(&sql, &[&iso, &seat, &N_SAMPLES])
                .await
                .map_err(|e| anyhow::anyhow!("fetch nearby abstractions for observation: {}", e))?
        };
        Ok(rows
            .iter()
            .map(|row| (row.get::<_, i16>(0), row.get::<_, Energy>(1)))
            .map(|(abs, distance)| (Abstraction::from(abs), distance))
            .collect())
    }
}

// exploration panel
impl API {
    pub async fn exp_wrt_str(
        &self,
        street: Street,
        seat_position: Option<u8>,
    ) -> anyhow::Result<ApiSample> {
        let seat_position = match seat_position {
            Some(seat_position) => seat_position,
            None if self.tables.abstraction.is_default_v1() => 0,
            None => {
                return Err(anyhow::anyhow!(
                    "seat_position is required for street-level exploration with abstraction version '{}'",
                    self.tables.abstraction.abstraction_version()
                ));
            }
        };
        self.exp_wrt_obs(Observation::from(street), seat_position)
            .await
    }
    #[rustfmt::skip]
    pub async fn exp_wrt_obs(
        &self,
        obs: Observation,
        seat_position: u8,
    ) -> anyhow::Result<ApiSample> {
        let isomorphism = self.isomorphism_table();
        let abstraction = self.abstraction_table();
        let sql = format!(
            "SELECT e.obs, a.abs, a.equity::REAL, a.population::REAL / ${} AS density \
             FROM   {isomorphism} e \
             JOIN   {abstraction} a ON a.abs = e.abs \
             WHERE  e.obs = $1{}",
            if self.tables.abstraction.is_default_v1() { 2 } else { 3 },
            if self.tables.abstraction.is_default_v1() {
                String::new()
            } else {
                " AND e.seat_position = $2".to_string()
            },
        );
        let n = obs.street().n_observations() as f32;
        let iso = i64::from(Isomorphism::from(obs));
        let row = if self.tables.abstraction.is_default_v1() {
            self.client
                .query_one(&sql, &[&iso, &n])
                .await
                .map_err(|e| anyhow::anyhow!("explore with respect to observation: {}", e))?
        } else {
            let seat = seat_position as i16;
            self.client
                .query_one(&sql, &[&iso, &seat, &n])
                .await
                .map_err(|e| anyhow::anyhow!("explore with respect to observation: {}", e))?
        };
        Ok(ApiSample::from(row))
    }
    #[rustfmt::skip]
    pub async fn exp_wrt_abs(&self, abs: Abstraction) -> anyhow::Result<ApiSample> {
        let abstraction = self.abstraction_table();
        let isomorphism = self.isomorphism_table();
        let sql = format!(
            "WITH sample AS ( \
                 SELECT a.abs, a.population, a.equity, FLOOR(RANDOM() * a.population)::INTEGER AS i \
                 FROM   {abstraction} a \
                 WHERE  a.abs = $1 \
             ) \
             SELECT e.obs, s.abs, s.equity::REAL, s.population::REAL / $2 AS density \
             FROM   sample s \
             JOIN   {isomorphism} e ON e.abs = s.abs AND e.position = s.i \
             LIMIT  1"
        );
        let n = abs.street().n_isomorphisms() as f32;
        let abs = i16::from(abs);
        let row = self
            .client
            .query_one(&sql, &[&abs, &n])
            .await
            .map_err(|e| anyhow::anyhow!("explore with respect to abstraction: {}", e))?;
        Ok(ApiSample::from(row))
    }
}

// neighborhood lookups
impl API {
    pub async fn nbr_any_wrt_abs(&self, wrt: Abstraction) -> anyhow::Result<ApiSample> {
        use rand::prelude::IndexedRandom;
        let ref mut rng = rand::rng();
        let abs = Abstraction::all(wrt.street())
            .into_iter()
            .filter(|&x| x != wrt)
            .collect::<Vec<_>>()
            .choose(rng)
            .copied()
            .expect("more than one abstraction option");
        self.nbr_abs_wrt_abs(wrt, abs).await
    }
    #[rustfmt::skip]
    pub async fn nbr_abs_wrt_abs(
        &self,
        wrt: Abstraction,
        abs: Abstraction,
    ) -> anyhow::Result<ApiSample> {
        let abstraction = self.abstraction_table();
        let metric = self.metric_table();
        let isomorphism = self.isomorphism_table();
        let sql = format!(
            "WITH sample AS ( \
                 SELECT r.abs, r.population, r.equity, FLOOR(RANDOM() * r.population)::INTEGER AS i, \
                        COALESCE(m.dx, 0) AS distance \
                 FROM      {abstraction} r \
                 LEFT JOIN {metric} m ON m.tri = get_pair_tri($1, $3) \
                 WHERE     r.abs = $1 \
             ), \
             random_iso AS ( \
                 SELECT e.obs, e.abs, s.equity, s.population, s.distance \
                 FROM   sample s \
                 JOIN   {isomorphism} e ON e.abs = s.abs AND e.position = s.i \
                 LIMIT  1 \
             ) \
             SELECT obs, abs, equity::REAL, population::REAL / $2 AS density, distance::REAL \
             FROM   random_iso"
        );
        let n = wrt.street().n_isomorphisms() as f32;
        let abs = i16::from(abs);
        let wrt = i16::from(wrt);
        let row = self
            .client
            .query_one(&sql, &[&abs, &n, &wrt])
            .await
            .map_err(|e| anyhow::anyhow!("fetch neighbor abstraction: {}", e))?;
        Ok(ApiSample::from(row))
    }
    #[rustfmt::skip]
    pub async fn nbr_obs_wrt_abs(
        &self,
        wrt: Abstraction,
        obs: Observation,
        seat_position: u8,
    ) -> anyhow::Result<ApiSample> {
        let isomorphism = self.isomorphism_table();
        let metric = self.metric_table();
        let abstraction = self.abstraction_table();
        let sql = format!(
            "WITH given AS ( \
                 SELECT obs, abs, get_pair_tri(abs, ${}) AS tri \
                 FROM   {isomorphism} \
                 WHERE  obs = $1{} \
             ) \
             SELECT g.obs, g.abs, a.equity::REAL, a.population::REAL / ${} AS density, \
                    COALESCE(m.dx, 0)::REAL AS distance \
             FROM   given g \
             JOIN   {metric} m ON m.tri = g.tri \
             JOIN   {abstraction} a ON a.abs = g.abs \
             LIMIT  1",
            if self.tables.abstraction.is_default_v1() { 3 } else { 4 },
            if self.tables.abstraction.is_default_v1() {
                String::new()
            } else {
                " AND seat_position = $2".to_string()
            },
            if self.tables.abstraction.is_default_v1() { 2 } else { 3 },
        );
        let n = wrt.street().n_isomorphisms() as f32;
        let iso = i64::from(Isomorphism::from(obs));
        let wrt = i16::from(wrt);
        let row = if self.tables.abstraction.is_default_v1() {
            self.client
                .query_one(&sql, &[&iso, &n, &wrt])
                .await
                .map_err(|e| anyhow::anyhow!("fetch neighbor observation: {}", e))?
        } else {
            let seat = seat_position as i16;
            self.client
                .query_one(&sql, &[&iso, &seat, &n, &wrt])
                .await
                .map_err(|e| anyhow::anyhow!("fetch neighbor observation: {}", e))?
        };
        Ok(ApiSample::from(row))
    }
}

// k-nearest neighbors lookups
impl API {
    #[rustfmt::skip]
    pub async fn kfn_wrt_abs(&self, wrt: Abstraction) -> anyhow::Result<Vec<ApiSample>> {
        let abstraction = self.abstraction_table();
        let metric = self.metric_table();
        let isomorphism = self.isomorphism_table();
        let sql = format!(
            "WITH nearest AS ( \
                 SELECT   a.abs, a.population, m.dx AS distance, \
                          FLOOR(RANDOM() * a.population)::INTEGER AS sample \
                 FROM     {abstraction} a \
                 JOIN     {metric} m ON m.tri = get_pair_tri(a.abs, $1) \
                 WHERE    a.street = $2 \
                 AND      a.abs != $1 \
                 ORDER BY m.dx DESC \
                 LIMIT    $3 \
             ) \
             SELECT   e.obs, n.abs, a.equity::REAL, a.population::REAL / $4 AS density, \
                      n.distance::REAL \
             FROM     nearest n \
             JOIN     {abstraction} a ON a.abs = n.abs \
             JOIN     {isomorphism} e ON e.abs = n.abs AND e.position = n.sample \
             ORDER BY n.distance DESC"
        );
        let n = wrt.street().n_isomorphisms() as f32;
        let s = wrt.street() as i16;
        let wrt = i16::from(wrt);
        let rows = self
            .client
            .query(&sql, &[&wrt, &s, &N_SAMPLES, &n])
            .await
            .map_err(|e| anyhow::anyhow!("fetch k-farthest neighbors: {}", e))?;
        Ok(rows.into_iter().map(ApiSample::from).collect())
    }
    #[rustfmt::skip]
    pub async fn knn_wrt_abs(&self, wrt: Abstraction) -> anyhow::Result<Vec<ApiSample>> {
        let abstraction = self.abstraction_table();
        let metric = self.metric_table();
        let isomorphism = self.isomorphism_table();
        let sql = format!(
            "WITH nearest AS ( \
                 SELECT   a.abs, a.population, m.dx AS distance, \
                          FLOOR(RANDOM() * a.population)::INTEGER AS sample \
                 FROM     {abstraction} a \
                 JOIN     {metric} m ON m.tri = get_pair_tri(a.abs, $1) \
                 WHERE    a.street = $2 \
                 AND      a.abs != $1 \
                 ORDER BY m.dx ASC \
                 LIMIT    $3 \
             ) \
             SELECT   e.obs, n.abs, a.equity::REAL, a.population::REAL / $4 AS density, \
                      n.distance::REAL \
             FROM     nearest n \
             JOIN     {abstraction} a ON a.abs = n.abs \
             JOIN     {isomorphism} e ON e.abs = n.abs AND e.position = n.sample \
             ORDER BY n.distance ASC"
        );
        let n = wrt.street().n_isomorphisms() as f32;
        let s = wrt.street() as i16;
        let wrt = i16::from(wrt);
        let rows = self
            .client
            .query(&sql, &[&wrt, &s, &N_SAMPLES, &n])
            .await
            .map_err(|e| anyhow::anyhow!("fetch k-nearest neighbors: {}", e))?;
        Ok(rows.into_iter().map(ApiSample::from).collect())
    }
    #[rustfmt::skip]
    pub async fn kgn_wrt_abs(
        &self,
        wrt: Abstraction,
        nbr: Vec<(Observation, u8)>,
    ) -> anyhow::Result<Vec<ApiSample>> {
        let isomorphism = self.isomorphism_table();
        let abstraction = self.abstraction_table();
        let metric = self.metric_table();
        let sql = if self.tables.abstraction.is_default_v1() {
            format!(
                "WITH input(obs, ord) AS ( \
                     SELECT unnest($3::BIGINT[]), generate_series(1, array_length($3, 1)) \
                 ) \
                 SELECT   e.obs, e.abs, a.equity::REAL, a.population::REAL / $1 AS density, \
                          m.dx::REAL AS distance \
                 FROM     input i \
                 JOIN     {isomorphism} e ON e.obs = i.obs \
                 JOIN     {abstraction} a ON a.abs = e.abs \
                 JOIN     {metric} m ON m.tri = get_pair_tri(a.abs, $2) \
                 ORDER BY i.ord \
                 LIMIT    $4"
            )
        } else {
            format!(
                "WITH input(obs, seat_position, ord) AS ( \
                     SELECT obs_values.obs, seat_values.seat_position, obs_values.ord \
                     FROM   unnest($3::BIGINT[]) WITH ORDINALITY AS obs_values(obs, ord) \
                     JOIN   unnest($4::SMALLINT[]) WITH ORDINALITY AS seat_values(seat_position, ord) \
                     ON     obs_values.ord = seat_values.ord \
                 ) \
                 SELECT   e.obs, e.abs, a.equity::REAL, a.population::REAL / $1 AS density, \
                          m.dx::REAL AS distance \
                 FROM     input i \
                 JOIN     {isomorphism} e ON e.obs = i.obs AND e.seat_position = i.seat_position \
                 JOIN     {abstraction} a ON a.abs = e.abs \
                 JOIN     {metric} m ON m.tri = get_pair_tri(a.abs, $2) \
                 ORDER BY i.ord \
                 LIMIT    $5"
            )
        };
        let isos = nbr
            .iter()
            .map(|(obs, _)| Isomorphism::from(*obs))
            .map(i64::from)
            .collect::<Vec<_>>();
        let seats = nbr
            .iter()
            .map(|(_, seat_position)| *seat_position as i16)
            .collect::<Vec<_>>();
        let n = wrt.street().n_isomorphisms() as f32;
        let wrt = i16::from(wrt);
        let rows = if self.tables.abstraction.is_default_v1() {
            self.client.query(&sql, &[&n, &wrt, &&isos, &N_SAMPLES]).await
        } else {
            self.client
                .query(&sql, &[&n, &wrt, &&isos, &&seats, &N_SAMPLES])
                .await
        };
        let rows = rows
            .map_err(|e| anyhow::anyhow!("fetch given neighbors: {}", e))?;
        Ok(rows.into_iter().map(ApiSample::from).collect())
    }
}

// histogram lookups
impl API {
    pub async fn hst_wrt_obs(
        &self,
        obs: Observation,
        seat_position: u8,
    ) -> anyhow::Result<Vec<ApiSample>> {
        if obs.street() == Street::Rive {
            self.hst_wrt_obs_on_river(obs, seat_position).await
        } else {
            self.hst_wrt_obs_on_other(obs, seat_position).await
        }
    }
    pub async fn hst_wrt_abs(&self, abs: Abstraction) -> anyhow::Result<Vec<ApiSample>> {
        if abs.street() == Street::Rive {
            self.hst_wrt_abs_on_river(abs).await
        } else {
            self.hst_wrt_abs_on_other(abs).await
        }
    }
    #[rustfmt::skip]
    async fn hst_wrt_obs_on_river(&self, obs: Observation, seat_position: u8) -> anyhow::Result<Vec<ApiSample>> {
        let isomorphism = self.isomorphism_table();
        let abstraction = self.abstraction_table();
        let sql = format!(
            "WITH sample AS ( \
                 SELECT e.obs, e.abs, a.equity, a.population, \
                        FLOOR(RANDOM() * a.population)::INTEGER AS position \
                 FROM   {isomorphism} e \
                 JOIN   {abstraction} a ON a.abs = e.abs \
                 WHERE  e.abs = (SELECT abs FROM {isomorphism} WHERE obs = $1{}) \
                 LIMIT  1 \
             ) \
             SELECT s.obs, s.abs, s.equity::REAL, 1::REAL AS density \
             FROM   sample s",
            if self.tables.abstraction.is_default_v1() {
                String::new()
            } else {
                " AND seat_position = $2".to_string()
            }
        );
        let iso = i64::from(Isomorphism::from(obs));
        let rows = if self.tables.abstraction.is_default_v1() {
            self.client
                .query(&sql, &[&iso])
                .await
                .map_err(|e| anyhow::anyhow!("fetch river observation distribution: {}", e))?
        } else {
            let seat = seat_position as i16;
            self.client
                .query(&sql, &[&iso, &seat])
                .await
                .map_err(|e| anyhow::anyhow!("fetch river observation distribution: {}", e))?
        };
        Ok(rows.into_iter().map(ApiSample::from).collect())
    }
    #[rustfmt::skip]
    async fn hst_wrt_obs_on_other(&self, obs: Observation, seat_position: u8) -> anyhow::Result<Vec<ApiSample>> {
        let isomorphism = self.isomorphism_table();
        let abstraction = self.abstraction_table();
        let sql = format!(
            "SELECT e.obs, e.abs, a.equity \
             FROM   {isomorphism} e \
             JOIN   {abstraction} a ON a.abs = e.abs \
             WHERE  e.obs = ANY($1){}",
            if self.tables.abstraction.is_default_v1() {
                String::new()
            } else {
                " AND e.seat_position = $2".to_string()
            }
        );
        let n = obs.street().n_children();
        let children = obs
            .children()
            .map(Isomorphism::from)
            .map(Observation::from)
            .collect::<Vec<_>>();
        let distinct = children
            .iter()
            .copied()
            .map(i64::from)
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        let rows = if self.tables.abstraction.is_default_v1() {
            self.client
                .query(&sql, &[&distinct])
                .await
                .map_err(|e| anyhow::anyhow!("fetch observation distribution: {}", e))?
        } else {
            let seat = seat_position as i16;
            self.client
                .query(&sql, &[&distinct, &seat])
                .await
                .map_err(|e| anyhow::anyhow!("fetch observation distribution: {}", e))?
        }
            .into_iter()
            .map(|row| {
                (
                    Observation::from(row.get::<_, i64>(0)),
                    Abstraction::from(row.get::<_, i16>(1)),
                    Probability::from(row.get::<_, f32>(2)),
                )
            })
            .map(|(obs, abs, equity)| (obs, (abs, equity)))
            .collect::<std::collections::BTreeMap<_, _>>();
        let hist = children
            .iter()
            .map(|child| rows.get(child).map(|row| (*child, row)))
            .map(|x| x.ok_or_else(|| anyhow::anyhow!("observation not found in database")))
            .collect::<anyhow::Result<Vec<_>>>()?
            .into_iter()
            .fold(
                std::collections::BTreeMap::<_, _>::new(),
                |mut btree, (obs, (abs, eqy))| {
                    btree.entry(abs).or_insert((obs, eqy, 0)).2 += 1;
                    btree
                },
            )
            .into_iter()
            .map(|(abs, (obs, eqy, pop))| ApiSample {
                obs: obs.to_string(),
                abs: abs.to_string(),
                equity: eqy.clone(),
                density: pop as Probability / n as Probability,
                distance: 0.,
            })
            .collect::<Vec<_>>();
        Ok(hist)
    }
    #[rustfmt::skip]
    async fn hst_wrt_abs_on_river(&self, abs: Abstraction) -> anyhow::Result<Vec<ApiSample>> {
        let abstraction = self.abstraction_table();
        let isomorphism = self.isomorphism_table();
        let sql = format!(
            "WITH sample AS ( \
                 SELECT a.abs, a.population, a.equity, \
                        FLOOR(RANDOM() * a.population)::INTEGER AS position \
                 FROM   {abstraction} a \
                 WHERE  a.abs = $1 \
                 LIMIT  1 \
             ) \
             SELECT e.obs, e.abs, s.equity::REAL, 1::REAL AS density \
             FROM   sample s \
             JOIN   {isomorphism} e ON e.abs = s.abs AND e.position = s.position"
        );
        let ref abs = i16::from(abs);
        let rows = self
            .client
            .query(&sql, &[abs])
            .await
            .map_err(|e| anyhow::anyhow!("fetch river abstraction distribution: {}", e))?;
        Ok(rows.into_iter().map(ApiSample::from).collect())
    }
    #[rustfmt::skip]
    async fn hst_wrt_abs_on_other(&self, abs: Abstraction) -> anyhow::Result<Vec<ApiSample>> {
        let transitions = self.transitions_table();
        let abstraction = self.abstraction_table();
        let isomorphism = self.isomorphism_table();
        let sql = format!(
            "WITH histogram AS ( \
                 SELECT p.abs, g.dx AS probability, p.population, p.equity, \
                        FLOOR(RANDOM() * p.population)::INTEGER AS i \
                 FROM   {transitions} g \
                 JOIN   {abstraction} p ON p.abs = g.next \
                 WHERE  g.prev = $1 \
             ) \
             SELECT   e.obs, t.abs, t.equity::REAL, t.probability AS density \
             FROM     histogram t \
             JOIN     {isomorphism} e ON e.abs = t.abs AND e.position = t.i \
             ORDER BY t.probability DESC"
        );
        let ref abs = i16::from(abs);
        let rows = self
            .client
            .query(&sql, &[abs])
            .await
            .map_err(|e| anyhow::anyhow!("fetch abstraction distribution: {}", e))?;
        Ok(rows.into_iter().map(ApiSample::from).collect())
    }
}

// blueprint lookups
impl API {
    #[rustfmt::skip]
    pub async fn policy(&self, recall: Recall) -> anyhow::Result<Option<ApiStrategy>> {
        let recall = recall.validate()?;
        let seat_position = recall.head().seat_position() as u8;
        let present = self.obs_to_abs(recall.seen(), seat_position).await?;
        let info = recall.bind(present);
        let decisions = self
            .client
            .strategy_profile(&self.tables.profile, info.clone())
            .await
            .into_iter()
            .map(|(edge, mass)| Decision { edge, mass })
            .collect::<Vec<_>>();
        match decisions.len() {
            0 => Ok(None),
            _ => Ok(Some(ApiStrategy::from(Strategy::from((info, decisions))))),
        }
    }
}

fn env_optional(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .filter(|value| !value.trim().is_empty())
}

/// Validate profile+version config without touching the database.
/// Returns `None` for default HU (both absent), `Some(tables)` for explicit profile,
/// or an error if only one of the two is provided.
fn validate_analysis_config(
    profile_key: Option<String>,
    abstraction_version: Option<String>,
) -> anyhow::Result<Option<TrainingTables>> {
    if profile_key.is_none() && abstraction_version.is_none() {
        return Ok(None);
    }
    let profile_key = profile_key
        .ok_or_else(|| anyhow::anyhow!("PROFILE_KEY is required with ABSTRACTION_VERSION"))?;
    let abstraction_version = abstraction_version
        .ok_or_else(|| anyhow::anyhow!("ABSTRACTION_VERSION is required with PROFILE_KEY"))?;
    Ok(Some(TrainingTables::new(profile_key, abstraction_version)))
}

/// Parse an observation target string into `(Observation, seat_position)` given
/// the current abstraction context. V1 abstractions default seat to 0; V2
/// abstractions require the `<obs>@<seat>` suffix.
fn parse_observation_target_raw(
    raw: &str,
    abstraction: &AbstractionTables,
) -> anyhow::Result<(Observation, u8)> {
    let (raw_obs, seat_position) = match raw.rsplit_once('@') {
        Some((obs, seat)) => {
            let seat_position = seat
                .parse::<u8>()
                .map_err(|_| anyhow::anyhow!("invalid seat position '{}'", seat))?;
            (obs.trim(), Some(seat_position))
        }
        None => (raw.trim(), None),
    };
    let obs = Observation::try_from(raw_obs)
        .map_err(|_| anyhow::anyhow!("invalid observation format"))?;
    let seat_position = match seat_position {
        Some(seat_position) => seat_position,
        None if abstraction.is_default_v1() => 0,
        None => {
            return Err(anyhow::anyhow!(
                "seat-qualified observation required for abstraction version '{}'; use '<observation>@<seat_position>'",
                abstraction.abstraction_version()
            ));
        }
    };
    Ok((obs, seat_position))
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- RPM-07 spec-required tests ---

    #[test]
    fn test_analysis_requires_profile_id() {
        let result = validate_analysis_config(None, Some("abs_v4_p6".into()));
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("PROFILE_KEY is required"),
            "expected PROFILE_KEY error, got: {}",
            err
        );
    }

    #[test]
    fn test_analysis_requires_abstraction_version() {
        let result = validate_analysis_config(Some("bp_6max_cash".into()), None);
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("ABSTRACTION_VERSION is required"),
            "expected ABSTRACTION_VERSION error, got: {}",
            err
        );
    }

    #[test]
    fn test_strategy_lookup_is_deterministic_for_profile() {
        let tables_a = TrainingTables::new("bp_6max_cash", "abs_v4_p6");
        let tables_b = TrainingTables::new("bp_6max_cash", "abs_v4_p6");
        assert_eq!(tables_a.profile.blueprint(), tables_b.profile.blueprint());
        assert_eq!(
            tables_a.abstraction.abstraction(),
            tables_b.abstraction.abstraction()
        );
        assert_eq!(
            tables_a.abstraction.isomorphism(),
            tables_b.abstraction.isomorphism()
        );
        assert_eq!(
            tables_a.abstraction.transitions(),
            tables_b.abstraction.transitions()
        );
        assert_eq!(tables_a.abstraction.metric(), tables_b.abstraction.metric());

        let obs_raw = "2c 3c@1";
        let (obs_a, seat_a) = parse_observation_target_raw(obs_raw, &tables_a.abstraction).unwrap();
        let (obs_b, seat_b) = parse_observation_target_raw(obs_raw, &tables_b.abstraction).unwrap();
        assert_eq!(obs_a, obs_b);
        assert_eq!(seat_a, seat_b);
    }

    #[test]
    fn test_seat_relative_query_matches_trained_profile_rows() {
        let abs = AbstractionTables::new("abs_v4_p6");

        let (obs_s0, seat_s0) = parse_observation_target_raw("2c 3c@0", &abs).unwrap();
        let (obs_s1, seat_s1) = parse_observation_target_raw("2c 3c@1", &abs).unwrap();
        assert_eq!(obs_s0, obs_s1);
        assert_ne!(seat_s0, seat_s1);
        assert_eq!(seat_s0, 0);
        assert_eq!(seat_s1, 1);

        // V2 abstraction tables route to profile-scoped names, not static defaults
        assert_eq!(abs.isomorphism(), "isomorphism_abs_v4_p6");
        assert_eq!(abs.abstraction(), "abstraction_abs_v4_p6");
    }

    #[test]
    fn test_profile_version_mismatch_rejected() {
        let abs_v2 = AbstractionTables::new("abs_v4_p6");

        // V2 abstraction rejects observation without seat qualifier
        let result = parse_observation_target_raw("2c 3c", &abs_v2);
        let err = result.unwrap_err();
        assert!(
            err.to_string()
                .contains("seat-qualified observation required"),
            "expected seat-qualified error, got: {}",
            err
        );

        // V1 default abstraction accepts observation without seat qualifier
        let abs_v1 = AbstractionTables::default_v1();
        let (obs, seat) = parse_observation_target_raw("2c 3c", &abs_v1).unwrap();
        assert_eq!(seat, 0);
        assert_eq!(obs, Observation::try_from("2c 3c").unwrap());
    }
}
