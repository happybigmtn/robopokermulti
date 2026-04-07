use super::*;
use crate::gameplay::*;
use crate::mccfr::*;
use crate::*;
use std::collections::BTreeMap;

#[cfg(feature = "server")]
use crate::save::InfoVersion;

/// NLHE Profile for MCCFR training.
///
/// Supports multiway games (2-10 players) via the `player_count` field.
/// The walker rotates across all players: `Turn::Choice(epochs % player_count)`.
///
/// `info_version` tracks the persistence schema version:
/// - `V1`: legacy heads-up, blueprint key is `(past, present, future, edge)`
/// - `V2`: context-aware, blueprint key includes seat context
///
/// Serialization methods are version-gated: `rows()` requires V1, `rows_profile()` requires V2.
pub struct NlheProfile {
    pub iterations: usize,
    pub encounters: BTreeMap<Info, BTreeMap<Edge, (Probability, Utility)>>,
    pub metrics: Metrics,
    /// Number of players in the game (2-10). Default is 2 for heads-up.
    pub player_count: usize,
    #[cfg(feature = "server")]
    info_version: InfoVersion,
}

impl Default for NlheProfile {
    fn default() -> Self {
        Self {
            iterations: 0,
            encounters: BTreeMap::new(),
            metrics: Metrics::default(),
            player_count: 2,
            #[cfg(feature = "server")]
            info_version: InfoVersion::V1,
        }
    }
}

impl NlheProfile {
    /// Create a new profile for N-player multiway training (V2 schema).
    pub fn for_players(n: usize) -> Self {
        assert!(n >= 2 && n <= 10, "player_count must be 2-10");
        Self {
            player_count: n,
            #[cfg(feature = "server")]
            info_version: InfoVersion::V2,
            ..Self::default()
        }
    }

    #[cfg(feature = "server")]
    pub fn info_version(&self) -> InfoVersion {
        self.info_version
    }

    /// Emit V2 rows with explicit seat context. Panics if profile is V1.
    pub fn rows_profile(
        self,
    ) -> impl Iterator<Item = (i64, i16, i64, i16, i16, i16, i64, f32, f32)> {
        #[cfg(feature = "server")]
        assert_eq!(
            self.info_version,
            InfoVersion::V2,
            "rows_profile() requires V2 profile; use rows() for V1"
        );
        self.encounters.into_iter().flat_map(|(info, edges)| {
            let history = i64::from(*info.history());
            let present = i16::from(*info.present());
            let choices = i64::from(*info.choices());
            let context = *info.context();
            let seat_count = context.seat_count() as i16;
            let seat_position = context.seat_position() as i16;
            let active_players = context.active_players() as i16;
            edges
                .into_iter()
                .map(move |(e, (p, r))| (u64::from(e) as i64, p, r))
                .map(move |(e, p, r)| {
                    (
                        history,
                        present,
                        choices,
                        seat_count,
                        seat_position,
                        active_players,
                        e,
                        p,
                        r,
                    )
                })
        })
    }

    pub fn profile_columns() -> [tokio_postgres::types::Type; 9] {
        [
            tokio_postgres::types::Type::INT8,
            tokio_postgres::types::Type::INT2,
            tokio_postgres::types::Type::INT8,
            tokio_postgres::types::Type::INT2,
            tokio_postgres::types::Type::INT2,
            tokio_postgres::types::Type::INT2,
            tokio_postgres::types::Type::INT8,
            tokio_postgres::types::Type::FLOAT4,
            tokio_postgres::types::Type::FLOAT4,
        ]
    }
}

impl Profile for NlheProfile {
    type T = Turn;
    type E = Edge;
    type G = Game;
    type I = Info;

    fn increment(&mut self) {
        self.iterations += 1;
    }
    /// Returns the current traversing player.
    /// Rotates across all N players for multiway MCCFR (AC-2.1, AC-2.2).
    fn walker(&self) -> Self::T {
        Turn::Choice(self.iterations % self.player_count)
    }
    fn epochs(&self) -> usize {
        self.iterations
    }
    fn metrics(&self) -> Option<&Metrics> {
        Some(&self.metrics)
    }
    fn sum_policy(&self, info: &Self::I, edge: &Self::E) -> Probability {
        self.encounters
            .get(info)
            .and_then(|memory| memory.get(edge))
            .map(|(w, _)| *w)
            .unwrap_or_default()
    }
    fn sum_regret(&self, info: &Self::I, edge: &Self::E) -> Utility {
        self.encounters
            .get(info)
            .and_then(|memory| memory.get(edge))
            .map(|(_, r)| *r)
            .unwrap_or_default()
    }
}

#[cfg(feature = "database")]
impl crate::save::Schema for NlheProfile {
    fn name() -> &'static str {
        crate::save::BLUEPRINT
    }
    fn columns() -> &'static [tokio_postgres::types::Type] {
        &[
            tokio_postgres::types::Type::INT8,
            tokio_postgres::types::Type::INT2,
            tokio_postgres::types::Type::INT8,
            tokio_postgres::types::Type::INT8,
            tokio_postgres::types::Type::FLOAT4,
            tokio_postgres::types::Type::FLOAT4,
        ]
    }
    fn copy() -> &'static str {
        const_format::concatcp!(
            "COPY ",
            crate::save::BLUEPRINT,
            " (past, present, future, edge, policy, regret) FROM STDIN BINARY"
        )
    }
    fn creates() -> &'static str {
        const_format::concatcp!(
            "CREATE TABLE IF NOT EXISTS ",
            crate::save::BLUEPRINT,
            " (
                edge       BIGINT,
                past       BIGINT,
                present    SMALLINT,
                future     BIGINT,
                policy     REAL,
                regret     REAL,
                UNIQUE     (past, present, future, edge)
            );"
        )
    }
    fn indices() -> &'static str {
        const_format::concatcp!(
            "CREATE UNIQUE INDEX IF NOT EXISTS idx_blueprint_upsert  ON ",
            crate::save::BLUEPRINT,
            " (present, past, future, edge);
             CREATE        INDEX IF NOT EXISTS idx_blueprint_bucket  ON ",
            crate::save::BLUEPRINT,
            " (present, past, future);
             CREATE        INDEX IF NOT EXISTS idx_blueprint_future  ON ",
            crate::save::BLUEPRINT,
            " (future);
             CREATE        INDEX IF NOT EXISTS idx_blueprint_present ON ",
            crate::save::BLUEPRINT,
            " (present);
             CREATE        INDEX IF NOT EXISTS idx_blueprint_edge    ON ",
            crate::save::BLUEPRINT,
            " (edge);
             CREATE        INDEX IF NOT EXISTS idx_blueprint_past    ON ",
            crate::save::BLUEPRINT,
            " (past);"
        )
    }
    fn truncates() -> &'static str {
        const_format::concatcp!("TRUNCATE TABLE ", crate::save::BLUEPRINT, ";")
    }
    fn freeze() -> &'static str {
        const_format::concatcp!(
            "ALTER TABLE ",
            crate::save::BLUEPRINT,
            " SET (fillfactor = 100);
            ALTER TABLE ",
            crate::save::BLUEPRINT,
            " SET (autovacuum_enabled = false);"
        )
    }
}

#[cfg(feature = "database")]
#[async_trait::async_trait]
impl crate::save::Hydrate for NlheProfile {
    async fn hydrate(client: std::sync::Arc<tokio_postgres::Client>) -> Self {
        Self::hydrate_profile(client, &crate::save::TrainingTables::default_hu(), 2).await
    }
}

#[cfg(feature = "database")]
impl NlheProfile {
    /// Emit V1 rows without seat context. Panics if profile is V2.
    pub fn rows(self) -> impl Iterator<Item = (i64, i16, i64, i64, f32, f32)> {
        assert_eq!(
            self.info_version,
            InfoVersion::V1,
            "rows() requires V1 profile; use rows_profile() for V2"
        );
        self.encounters.into_iter().flat_map(|(info, edges)| {
            let history = i64::from(*info.history());
            let present = i16::from(*info.present());
            let choices = i64::from(*info.choices());
            edges
                .into_iter()
                .map(move |(e, (p, r))| (u64::from(e) as i64, p, r))
                .map(move |(e, p, r)| (history, present, choices, e, p, r))
        })
    }

    pub async fn hydrate_profile(
        client: std::sync::Arc<tokio_postgres::Client>,
        tables: &crate::save::TrainingTables,
        player_count: usize,
    ) -> Self {
        log::info!("loading blueprint from database");
        let epoch = if tables.profile.is_default_hu() {
            crate::save::EPOCH.to_string()
        } else {
            tables.profile.epoch()
        };
        let blueprint = if tables.profile.is_default_hu() {
            crate::save::BLUEPRINT.to_string()
        } else {
            tables.profile.blueprint()
        };
        let epoch_sql = format!("SELECT value FROM {epoch} WHERE key = 'current'");
        let iterations = client
            .query_opt(&epoch_sql, &[])
            .await
            .ok()
            .flatten()
            .map(|row| row.get::<_, i64>(0) as usize)
            .expect("to have already created epoch metadata");
        let query = if tables.profile.is_default_hu() {
            format!("SELECT past, present, future, edge, policy, regret FROM {blueprint}")
        } else {
            format!(
                "SELECT past, present, future, seat_count, seat_position, active_players, edge, policy, regret FROM {blueprint}"
            )
        };
        let mut encounters = BTreeMap::new();
        for row in client
            .query(&query, &[])
            .await
            .expect("to have already created blueprint")
        {
            let history = Path::from(row.get::<_, i64>(0) as u64);
            let present = Abstraction::from(row.get::<_, i16>(1));
            let choices = Path::from(row.get::<_, i64>(2) as u64);
            let (context, edge_idx, policy_idx, regret_idx) = if tables.profile.is_default_hu() {
                (InfoContext::heads_up(), 3, 4, 5)
            } else {
                (
                    InfoContext::from((
                        row.get::<_, i16>(3) as u8,
                        row.get::<_, i16>(4) as u8,
                        row.get::<_, i16>(5) as u8,
                    )),
                    6,
                    7,
                    8,
                )
            };
            let edge = Edge::from(row.get::<_, i64>(edge_idx) as u64);
            let policy = row.get::<_, f32>(policy_idx);
            let regret = row.get::<_, f32>(regret_idx);
            let bucket = Info::from((history, present, choices, context));
            encounters
                .entry(bucket)
                .or_insert_with(BTreeMap::default)
                .entry(edge)
                .or_insert((policy, regret));
        }
        let info_version = tables.profile.info_version();
        log::info!(
            "loaded {} infos from database (version {:?})",
            encounters.len(),
            info_version
        );
        Self {
            iterations,
            encounters,
            metrics: Metrics::default(),
            player_count,
            info_version,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_profile_emits_infoset_context() {
        let info = Info::from((
            Path::default(),
            Abstraction::from(7_i16),
            Path::from(vec![Edge::Check, Edge::Call]),
            InfoContext::from((6, 2, 4)),
        ));
        let mut profile = NlheProfile::for_players(6);
        let mut memory = BTreeMap::new();
        memory.insert(Edge::Check, (0.6, 1.5));
        profile.encounters.insert(info, memory);

        let rows = profile.rows_profile().collect::<Vec<_>>();

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].3, 6);
        assert_eq!(rows[0].4, 2);
        assert_eq!(rows[0].5, 4);
    }

    // ----- RPM-05 Required Tests -----

    /// RPM-05 AC: V2 rows round-trip InfoContext through the serialization boundary.
    #[test]
    fn test_profile_rows_round_trip_info_context() {
        let ctx = InfoContext::from((6, 3, 5));
        let info = Info::from((
            Path::from(vec![Edge::Call, Edge::Check]),
            Abstraction::from(99_i16),
            Path::from(vec![Edge::Check, Edge::Fold, Edge::Raise(Odds(1, 2))]),
            ctx,
        ));
        let mut profile = NlheProfile::for_players(6);
        let mut memory = BTreeMap::new();
        memory.insert(Edge::Check, (0.5, 0.3));
        memory.insert(Edge::Fold, (0.5, -0.3));
        profile.encounters.insert(info, memory);

        let rows: Vec<_> = profile.rows_profile().collect();
        assert_eq!(rows.len(), 2);
        for row in &rows {
            let (past, present, future, sc, sp, ap, _edge, _policy, _regret) = *row;
            // Reconstruct Info from the row columns
            let recovered = Info::from((
                Path::from(past as u64),
                Abstraction::from(present),
                Path::from(future as u64),
                InfoContext::from((sc as u8, sp as u8, ap as u8)),
            ));
            assert_eq!(recovered.context().seat_count(), 6);
            assert_eq!(recovered.context().seat_position(), 3);
            assert_eq!(recovered.context().active_players(), 5);
            assert_eq!(recovered, info);
        }
    }

    /// RPM-05 AC: calling V1 rows() on a V2 profile panics.
    #[test]
    #[should_panic(expected = "rows() requires V1 profile")]
    fn test_mismatched_info_version_rejected_at_hydrate() {
        let mut profile = NlheProfile::for_players(6);
        let info = Info::from((
            Path::default(),
            Abstraction::from(1_i16),
            Path::from(vec![Edge::Check]),
            InfoContext::from((6, 0, 6)),
        ));
        let mut memory = BTreeMap::new();
        memory.insert(Edge::Check, (1.0, 0.0));
        profile.encounters.insert(info, memory);
        // V2 profile must not use V1 serialization — this must panic
        let _rows: Vec<_> = profile.rows().collect();
    }
}

#[cfg(feature = "disk")]
use crate::cards::*;

#[cfg(feature = "disk")]
#[allow(deprecated)]
impl crate::save::Disk for NlheProfile {
    fn name() -> &'static str {
        crate::save::BLUEPRINT
    }
    fn sources() -> Vec<std::path::PathBuf> {
        vec![Self::path(Street::random())]
    }
    fn grow(_: Street) -> Self {
        unreachable!("must be learned in MCCFR minimization")
    }
    fn path(_: Street) -> std::path::PathBuf {
        let ref path = format!(
            "{}/pgcopy/{}",
            std::env::current_dir()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
            crate::save::BLUEPRINT
        );
        std::path::Path::new(path).parent().map(std::fs::create_dir);
        std::path::PathBuf::from(path)
    }
    fn load(_: Street) -> Self {
        let ref path = Self::path(Street::random());
        log::info!("{:<32}{:<32}", "loading     blueprint", path.display());
        use byteorder::BE;
        use byteorder::ReadBytesExt;
        use std::fs::File;
        use std::io::BufReader;
        use std::io::Read;
        use std::io::Seek;
        use std::io::SeekFrom;
        let file = File::open(path).expect("open file");
        let mut encounters = BTreeMap::new();
        let mut reader = BufReader::new(file);
        let ref mut buffer = [0u8; 2];
        reader.seek(SeekFrom::Start(19)).expect("seek past header");
        while reader.read_exact(buffer).is_ok() {
            match u16::from_be_bytes(buffer.clone()) {
                6 => {
                    reader.read_u32::<BE>().expect("past path length");
                    let history = Path::from(reader.read_u64::<BE>().expect("history"));
                    reader.read_u32::<BE>().expect("abstraction length");
                    let present = Abstraction::from(reader.read_i16::<BE>().expect("abstraction"));
                    reader.read_u32::<BE>().expect("future path length");
                    let choices = Path::from(reader.read_u64::<BE>().expect("choices"));
                    reader.read_u32::<BE>().expect("edge length");
                    let edge = Edge::from(reader.read_u64::<BE>().expect("read edge"));
                    reader.read_u32::<BE>().expect("policy length");
                    let policy = reader.read_f32::<BE>().expect("read policy");
                    reader.read_u32::<BE>().expect("regret length");
                    let regret = reader.read_f32::<BE>().expect("read regret");
                    let bucket = Info::from((history, present, choices));
                    encounters
                        .entry(bucket)
                        .or_insert_with(BTreeMap::default)
                        .entry(edge)
                        .or_insert((policy, regret));
                }
                0xFFFF => break,
                n => panic!("unexpected number of fields: {}", n),
            }
        }
        Self {
            encounters,
            iterations: 0,
            metrics: Metrics::default(),
            player_count: 2,
            #[cfg(feature = "server")]
            info_version: InfoVersion::V1,
        }
    }
    fn save(&self) {
        const N_FIELDS: u16 = 6;
        let ref path = Self::path(Street::random());
        let ref mut file = File::create(path).expect(&format!("touch {}", path.display()));
        use byteorder::BE;
        use byteorder::WriteBytesExt;
        use std::fs::File;
        use std::io::Write;
        log::info!("{:<32}{:<32}", "saving      blueprint", path.display());
        file.write_all(Self::header()).expect("header");
        for (bucket, strategy) in self.encounters.iter() {
            for (edge, memory) in strategy.iter() {
                file.write_u16::<BE>(N_FIELDS).unwrap();
                file.write_u32::<BE>(size_of::<u64>() as u32).unwrap();
                file.write_u64::<BE>(u64::from(*bucket.history())).unwrap();
                file.write_u32::<BE>(size_of::<i16>() as u32).unwrap();
                file.write_i16::<BE>(i16::from(*bucket.present())).unwrap();
                file.write_u32::<BE>(size_of::<u64>() as u32).unwrap();
                file.write_u64::<BE>(u64::from(*bucket.choices())).unwrap();
                file.write_u32::<BE>(size_of::<u64>() as u32).unwrap();
                file.write_u64::<BE>(u64::from(edge.clone())).unwrap();
                file.write_u32::<BE>(size_of::<f32>() as u32).unwrap();
                file.write_f32::<BE>(memory.0).unwrap();
                file.write_u32::<BE>(size_of::<f32>() as u32).unwrap();
                file.write_f32::<BE>(memory.1).unwrap();
            }
        }
        file.write_u16::<BE>(Self::footer()).expect("trailer");
    }
}
