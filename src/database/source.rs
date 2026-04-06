use crate::Energy;
use crate::Probability;
use crate::cards::*;
use crate::clustering::*;
use crate::gameplay::*;
use crate::mccfr::*;
use crate::save::*;
use crate::workers::*;
use const_format::concatcp;
use std::collections::BTreeMap;
use std::sync::Arc;
use tokio_postgres::Client;

/// Source defines the read interface between domain types and PostgreSQL.
/// All SELECT queries are consolidated here, decoupling SQL from business logic.
#[async_trait::async_trait]
pub trait Source: Send + Sync {
    async fn memory(&self, info: Info) -> Memory;
    async fn encode(&self, iso: Isomorphism) -> Abstraction;
    async fn equity(&self, abs: Abstraction) -> Probability;
    async fn metric(&self, street: Street) -> Metric;
    async fn distance(&self, pair: Pair) -> Energy;
    async fn strategy(&self, info: Info) -> Vec<(Edge, Probability)>;
    async fn histogram(&self, abs: Abstraction) -> Histogram;
    async fn population(&self, abs: Abstraction) -> usize;
}

#[rustfmt::skip]
#[async_trait::async_trait]
impl Source for Client {
    async fn encode(&self, iso: Isomorphism) -> Abstraction {
        const SQL: &str = concatcp!(
            "SELECT abs ",
            "FROM   ", ISOMORPHISM, " ",
            "WHERE  obs = $1"
        );
        self.query_one(SQL, &[&i64::from(iso)])
            .await
            .expect("isomorphism lookup")
            .get::<_, i16>(0)
            .into()
    }
    async fn memory(&self, info: Info) -> Memory {
        const SQL: &str = concatcp!(
            "SELECT edge, ",
                   "policy, ",
                   "regret ",
            "FROM   ", BLUEPRINT, " ",
            "WHERE  past    = $1 ",
            "AND    present = $2 ",
            "AND    future  = $3"
        );
        let data = self
            .query(
                SQL,
                &[
                    &i64::from(*info.history()),
                    &i16::from(*info.present()),
                    &i64::from(*info.choices()),
                ],
            )
            .await
            .expect("memory lookup")
            .into_iter()
            .map(|row| {
                let edge = Edge::from(row.get::<_, i64>(0) as u64);
                let policy = row.get::<_, f32>(1);
                let regret = row.get::<_, f32>(2);
                (edge, policy, regret)
            })
            .collect();
        Memory::new(info, data)
    }
    async fn strategy(&self, info: Info) -> Vec<(Edge, Probability)> {
        const SQL: &str = concatcp!(
            "SELECT edge, ",
                   "policy ",
            "FROM   ", BLUEPRINT, " ",
            "WHERE  past    = $1 ",
            "AND    present = $2 ",
            "AND    future  = $3"
        );
        self.query(
            SQL,
            &[
                &i64::from(*info.history()),
                &i16::from(*info.present()),
                &i64::from(*info.choices()),
            ],
        )
        .await
        .expect("strategy lookup")
        .into_iter()
        .map(|row| {
            let edge = Edge::from(row.get::<_, i64>(0) as u64);
            let policy = row.get::<_, f32>(1);
            (edge, policy)
        })
        .collect()
    }
    async fn equity(&self, abs: Abstraction) -> Probability {
        const SQL: &str = concatcp!(
            "SELECT equity ",
            "FROM   ", ABSTRACTION, " ",
            "WHERE  abs = $1"
        );
        self.query_one(SQL, &[&i16::from(abs)])
            .await
            .expect("equity lookup")
            .get::<_, f32>(0)
    }
    async fn population(&self, abs: Abstraction) -> usize {
        const SQL: &str = concatcp!(
            "SELECT population ",
            "FROM   ", ABSTRACTION, " ",
            "WHERE  abs = $1"
        );
        self.query_one(SQL, &[&i16::from(abs)])
            .await
            .expect("population lookup")
            .get::<_, i32>(0) as usize
    }
    async fn metric(&self, street: Street) -> Metric {
        const SQL: &str = concatcp!(
            "SELECT   get_pair_tri(a1.abs, a2.abs) AS tri, ",
                     "m.dx AS dx ",
            "FROM     ", ABSTRACTION, " a1 ",
            "JOIN     ", ABSTRACTION, " a2 ON a1.street = a2.street ",
            "JOIN     ", METRIC,      " m  ON m.tri = get_pair_tri(a1.abs, a2.abs) ",
            "WHERE    a1.street = $1 ",
            "AND      a1.abs != a2.abs"
        );
        self.query(SQL, &[&(street as i16)])
            .await
            .expect("metric lookup")
            .iter()
            .map(|row| (row.get::<_, i32>(0), row.get::<_, Energy>(1)))
            .map(|(tri, distance)| (Pair::from(tri), distance))
            .collect::<BTreeMap<Pair, Energy>>()
            .into()
    }
    async fn distance(&self, pair: Pair) -> Energy {
        const SQL: &str = concatcp!(
            "SELECT m.dx ",
            "FROM   ", METRIC, " m ",
            "WHERE  $1 = m.tri"
        );
        self.query_one(SQL, &[&i32::from(pair)])
            .await
            .expect("distance lookup")
            .get::<_, Energy>(0)
    }
    async fn histogram(&self, abs: Abstraction) -> Histogram {
        const SQL: &str = concatcp!(
            "SELECT next, ",
                   "dx ",
            "FROM   ", TRANSITIONS, " ",
            "WHERE  prev = $1"
        );
        let street = abs.street().next();
        let mass = abs.street().n_children() as f32;
        self.query(SQL, &[&i16::from(abs)])
            .await
            .expect("histogram lookup")
            .iter()
            .map(|row| (row.get::<_, i16>(0), row.get::<_, Energy>(1)))
            .map(|(next, dx)| (next, (dx * mass).round() as usize))
            .map(|(next, dx)| (Abstraction::from(next), dx))
            .fold(Histogram::empty(street), |mut h, (next, dx)| {
                h.set(next, dx);
                h
            })
    }
}

#[async_trait::async_trait]
impl Source for Arc<Client> {
    async fn encode(&self, iso: Isomorphism) -> Abstraction {
        self.as_ref().encode(iso).await
    }

    async fn memory(&self, info: Info) -> Memory {
        self.as_ref().memory(info).await
    }

    async fn strategy(&self, info: Info) -> Vec<(Edge, Probability)> {
        self.as_ref().strategy(info).await
    }

    async fn equity(&self, abs: Abstraction) -> Probability {
        self.as_ref().equity(abs).await
    }

    async fn population(&self, abs: Abstraction) -> usize {
        self.as_ref().population(abs).await
    }

    async fn metric(&self, street: Street) -> Metric {
        self.as_ref().metric(street).await
    }

    async fn distance(&self, pair: Pair) -> Energy {
        self.as_ref().distance(pair).await
    }

    async fn histogram(&self, abs: Abstraction) -> Histogram {
        self.as_ref().histogram(abs).await
    }
}

fn encode_profile_sql(tables: &AbstractionTables) -> String {
    let isomorphism = if tables.is_default_v1() {
        ISOMORPHISM.to_string()
    } else {
        tables.isomorphism()
    };
    if tables.is_default_v1() {
        format!("SELECT abs FROM {isomorphism} WHERE obs = $1")
    } else {
        format!("SELECT abs FROM {isomorphism} WHERE obs = $1 AND seat_position = $2")
    }
}

/// Profile-aware read operations for multiway training.
///
/// Uses `ProfileTables` and `AbstractionTables` to query profile-specific
/// blueprint and abstraction tables.
#[async_trait::async_trait]
pub trait ProfileSource: Send + Sync {
    async fn memory_profile(&self, tables: &ProfileTables, info: Info) -> Memory;
    async fn encode_profile(
        &self,
        tables: &AbstractionTables,
        iso: Isomorphism,
        seat_position: u8,
    ) -> Abstraction;
    async fn equity_profile(&self, tables: &AbstractionTables, abs: Abstraction) -> Probability;
    async fn metric_profile(&self, tables: &AbstractionTables, street: Street) -> Metric;
    async fn distance_profile(&self, tables: &AbstractionTables, pair: Pair) -> Energy;
    async fn strategy_profile(
        &self,
        tables: &ProfileTables,
        info: Info,
    ) -> Vec<(Edge, Probability)>;
    async fn histogram_profile(&self, tables: &AbstractionTables, abs: Abstraction) -> Histogram;
    async fn population_profile(&self, tables: &AbstractionTables, abs: Abstraction) -> usize;
}

#[async_trait::async_trait]
impl ProfileSource for Client {
    async fn encode_profile(
        &self,
        tables: &AbstractionTables,
        iso: Isomorphism,
        seat_position: u8,
    ) -> Abstraction {
        let sql = encode_profile_sql(tables);
        if tables.is_default_v1() {
            self.query_one(&sql, &[&i64::from(iso)])
                .await
                .expect("isomorphism lookup")
                .get::<_, i16>(0)
                .into()
        } else {
            self.query_one(&sql, &[&i64::from(iso), &(seat_position as i16)])
                .await
                .expect("isomorphism lookup")
                .get::<_, i16>(0)
                .into()
        }
    }

    async fn memory_profile(&self, tables: &ProfileTables, info: Info) -> Memory {
        let blueprint = if tables.is_default_hu() {
            BLUEPRINT.to_string()
        } else {
            tables.blueprint()
        };
        let sql = format!(
            "SELECT edge, policy, regret FROM {blueprint} \
             WHERE past = $1 AND present = $2 AND future = $3"
        );
        let data = self
            .query(
                &sql,
                &[
                    &i64::from(*info.history()),
                    &i16::from(*info.present()),
                    &i64::from(*info.choices()),
                ],
            )
            .await
            .expect("memory lookup")
            .into_iter()
            .map(|row| {
                let edge = Edge::from(row.get::<_, i64>(0) as u64);
                let policy = row.get::<_, f32>(1);
                let regret = row.get::<_, f32>(2);
                (edge, policy, regret)
            })
            .collect();
        Memory::new(info, data)
    }

    async fn strategy_profile(
        &self,
        tables: &ProfileTables,
        info: Info,
    ) -> Vec<(Edge, Probability)> {
        let blueprint = if tables.is_default_hu() {
            BLUEPRINT.to_string()
        } else {
            tables.blueprint()
        };
        let sql = format!(
            "SELECT edge, policy FROM {blueprint} \
             WHERE past = $1 AND present = $2 AND future = $3"
        );
        self.query(
            &sql,
            &[
                &i64::from(*info.history()),
                &i16::from(*info.present()),
                &i64::from(*info.choices()),
            ],
        )
        .await
        .expect("strategy lookup")
        .into_iter()
        .map(|row| {
            let edge = Edge::from(row.get::<_, i64>(0) as u64);
            let policy = row.get::<_, f32>(1);
            (edge, policy)
        })
        .collect()
    }

    async fn equity_profile(&self, tables: &AbstractionTables, abs: Abstraction) -> Probability {
        let abstraction = if tables.is_default_v1() {
            ABSTRACTION.to_string()
        } else {
            tables.abstraction()
        };
        let sql = format!("SELECT equity FROM {abstraction} WHERE abs = $1");
        self.query_one(&sql, &[&i16::from(abs)])
            .await
            .expect("equity lookup")
            .get::<_, f32>(0)
    }

    async fn population_profile(&self, tables: &AbstractionTables, abs: Abstraction) -> usize {
        let abstraction = if tables.is_default_v1() {
            ABSTRACTION.to_string()
        } else {
            tables.abstraction()
        };
        let sql = format!("SELECT population FROM {abstraction} WHERE abs = $1");
        self.query_one(&sql, &[&i16::from(abs)])
            .await
            .expect("population lookup")
            .get::<_, i32>(0) as usize
    }

    async fn metric_profile(&self, tables: &AbstractionTables, street: Street) -> Metric {
        let abstraction = if tables.is_default_v1() {
            ABSTRACTION.to_string()
        } else {
            tables.abstraction()
        };
        let metric = if tables.is_default_v1() {
            METRIC.to_string()
        } else {
            tables.metric()
        };
        let sql = format!(
            "SELECT get_pair_tri(a1.abs, a2.abs) AS tri, m.dx AS dx \
             FROM {abstraction} a1 \
             JOIN {abstraction} a2 ON a1.street = a2.street \
             JOIN {metric} m ON m.tri = get_pair_tri(a1.abs, a2.abs) \
             WHERE a1.street = $1 AND a1.abs != a2.abs"
        );
        self.query(&sql, &[&(street as i16)])
            .await
            .expect("metric lookup")
            .iter()
            .map(|row| (row.get::<_, i32>(0), row.get::<_, Energy>(1)))
            .map(|(tri, distance)| (Pair::from(tri), distance))
            .collect::<BTreeMap<Pair, Energy>>()
            .into()
    }

    async fn distance_profile(&self, tables: &AbstractionTables, pair: Pair) -> Energy {
        let metric = if tables.is_default_v1() {
            METRIC.to_string()
        } else {
            tables.metric()
        };
        let sql = format!("SELECT m.dx FROM {metric} m WHERE $1 = m.tri");
        self.query_one(&sql, &[&i32::from(pair)])
            .await
            .expect("distance lookup")
            .get::<_, Energy>(0)
    }

    async fn histogram_profile(&self, tables: &AbstractionTables, abs: Abstraction) -> Histogram {
        let transitions = if tables.is_default_v1() {
            TRANSITIONS.to_string()
        } else {
            tables.transitions()
        };
        let sql = format!("SELECT next, dx FROM {transitions} WHERE prev = $1");
        let street = abs.street().next();
        let mass = abs.street().n_children() as f32;
        self.query(&sql, &[&i16::from(abs)])
            .await
            .expect("histogram lookup")
            .iter()
            .map(|row| (row.get::<_, i16>(0), row.get::<_, Energy>(1)))
            .map(|(next, dx)| (next, (dx * mass).round() as usize))
            .map(|(next, dx)| (Abstraction::from(next), dx))
            .fold(Histogram::empty(street), |mut h, (next, dx)| {
                h.set(next, dx);
                h
            })
    }
}

#[async_trait::async_trait]
impl ProfileSource for Arc<Client> {
    async fn encode_profile(
        &self,
        tables: &AbstractionTables,
        iso: Isomorphism,
        seat_position: u8,
    ) -> Abstraction {
        self.as_ref()
            .encode_profile(tables, iso, seat_position)
            .await
    }
    async fn memory_profile(&self, tables: &ProfileTables, info: Info) -> Memory {
        self.as_ref().memory_profile(tables, info).await
    }
    async fn strategy_profile(
        &self,
        tables: &ProfileTables,
        info: Info,
    ) -> Vec<(Edge, Probability)> {
        self.as_ref().strategy_profile(tables, info).await
    }
    async fn equity_profile(&self, tables: &AbstractionTables, abs: Abstraction) -> Probability {
        self.as_ref().equity_profile(tables, abs).await
    }
    async fn population_profile(&self, tables: &AbstractionTables, abs: Abstraction) -> usize {
        self.as_ref().population_profile(tables, abs).await
    }
    async fn metric_profile(&self, tables: &AbstractionTables, street: Street) -> Metric {
        self.as_ref().metric_profile(tables, street).await
    }
    async fn distance_profile(&self, tables: &AbstractionTables, pair: Pair) -> Energy {
        self.as_ref().distance_profile(tables, pair).await
    }
    async fn histogram_profile(&self, tables: &AbstractionTables, abs: Abstraction) -> Histogram {
        self.as_ref().histogram_profile(tables, abs).await
    }
}

#[cfg(test)]
mod tests {
    use super::encode_profile_sql;
    use crate::save::AbstractionTables;

    #[test]
    fn default_abstraction_lookup_uses_obs_only_query() {
        let sql = encode_profile_sql(&AbstractionTables::default_v1());
        assert_eq!(sql, "SELECT abs FROM isomorphism WHERE obs = $1");
    }

    #[test]
    fn versioned_abstraction_lookup_requires_seat_position() {
        let sql = encode_profile_sql(&AbstractionTables::new("abs_v2_p6"));
        assert_eq!(
            sql,
            "SELECT abs FROM isomorphism_abs_v2_p6 WHERE obs = $1 AND seat_position = $2"
        );
    }
}
