use crate::Probability;
use crate::cards::*;
use crate::clustering::*;
use crate::gameplay::*;
use rayon::prelude::*;
use std::collections::BTreeMap;
use tokio_postgres::types::Type;

#[derive(Default)]
/// this is the grand lookup table for all the Isomorphism -> Abstraction
/// mappings. we spend a lot of compute over a lot of hands (all of them!)
/// to precompute this mapping.
pub struct Lookup(BTreeMap<Isomorphism, Abstraction>);

impl From<Lookup> for BTreeMap<Isomorphism, Abstraction> {
    fn from(lookup: Lookup) -> BTreeMap<Isomorphism, Abstraction> {
        lookup.0
    }
}
impl From<BTreeMap<Isomorphism, Abstraction>> for Lookup {
    fn from(map: BTreeMap<Isomorphism, Abstraction>) -> Self {
        Self(map)
    }
}

impl Lookup {
    /// lookup the pre-computed abstraction for the outer observation
    pub fn lookup(&self, iso: &Isomorphism) -> Abstraction {
        self.0
            .get(iso)
            .cloned()
            .expect("precomputed abstraction in lookup")
    }

    /// generate the entire space of inner layers
    pub fn projections(&self) -> Vec<Histogram> {
        IsomorphismIterator::from(self.street().prev())
            .collect::<Vec<Isomorphism>>()
            .into_par_iter()
            .map(|i| self.future(&i))
            .collect::<Vec<Histogram>>()
    }

    /// distribution over potential next states. this "layer locality" is what
    /// makes imperfect recall hierarchical kmeans nice
    fn future(&self, iso: &Isomorphism) -> Histogram {
        assert!(iso.0.street() != Street::Rive);
        iso.0
            .children()
            .collect::<Vec<_>>()
            .into_par_iter()
            .map(Isomorphism::from)
            .map(|i| self.lookup(&i))
            .collect::<Vec<Abstraction>>()
            .into()
    }

    fn street(&self) -> Street {
        self.0.keys().next().expect("non empty").0.street()
    }
}

fn versioned_seat_count(tables: &crate::save::AbstractionTables) -> u8 {
    tables
        .player_count()
        .expect("versioned abstraction tables must use abs_v{generation}_p{seats} with seats >= 2")
}

fn position_band(seat_position: u8, player_count: u8) -> usize {
    if player_count <= 3 {
        return usize::from(seat_position.min(2));
    }
    if seat_position == 0 {
        return 2;
    }
    if seat_position <= player_count / 2 {
        0
    } else {
        1
    }
}

fn straight_texture(public: Hand) -> usize {
    let ranks = u16::from(public);
    let mut bits = ranks;
    for _ in 0..3 {
        bits &= bits << 1;
    }
    if bits > 0 { 1 } else { 0 }
}

fn board_texture_band(observation: Observation) -> usize {
    let public = *observation.public();
    let max_suit = Suit::all()
        .into_iter()
        .map(|suit| public.of(&suit).size())
        .max()
        .unwrap_or(0);
    let rank_bits = u16::from(public);
    let paired = public
        .size()
        .saturating_sub(rank_bits.count_ones() as usize);
    match (max_suit, paired, straight_texture(public)) {
        (4.., _, _) => 4,
        (_, 2.., _) => 3,
        (_, _, 1) => 2,
        (3, _, _) => 1,
        _ => 0,
    }
}

fn river_rank_band(observation: Observation) -> usize {
    let ranking = Evaluator::from(Hand::from(observation)).find_ranking();
    match ranking {
        Ranking::HighCard(_) => 0,
        Ranking::OnePair(_) => 1,
        Ranking::TwoPair(_, _) => 2,
        Ranking::ThreeOAK(_) => 4,
        Ranking::Straight(_) => 5,
        Ranking::Flush(_) => 6,
        Ranking::FullHouse(_, _) => 7,
        Ranking::FourOAK(_) => 8,
        Ranking::StraightFlush(_) => 9,
        Ranking::MAX => unreachable!("ranking::MAX is not a real river value"),
    }
}

fn approximate_multiway_outcome(
    observation: Observation,
    player_count: u8,
) -> (Probability, Probability) {
    let hand = Hand::from(observation);
    let hero = Strength::from(hand);
    let (won, tied, total) = HandIterator::from((2, hand))
        .map(|villain| Hand::add(*observation.public(), villain))
        .map(|villain| Strength::from(villain))
        .map(|villain| hero.cmp(&villain))
        .fold((0_u32, 0_u32, 0_u32), |(won, tied, total), ord| match ord {
            std::cmp::Ordering::Greater => (won + 1, tied, total + 1),
            std::cmp::Ordering::Equal => (won, tied + 1, total + 1),
            std::cmp::Ordering::Less => (won, tied, total + 1),
        });
    if total == 0 {
        return (0.5, 0.0);
    }

    let opponents = i32::from(player_count.saturating_sub(1));
    let heads_up_win = won as Probability / total as Probability;
    let heads_up_tie = tied as Probability / total as Probability;
    let multiway_win = heads_up_win.powi(opponents);
    let multiway_tie = (heads_up_win + heads_up_tie).powi(opponents) - multiway_win;
    (multiway_win.clamp(0.0, 1.0), multiway_tie.clamp(0.0, 1.0))
}

fn river_strength_band(observation: Observation, player_count: u8) -> usize {
    let rank_band = river_rank_band(observation) as Probability / 9.0;
    let (multiway_win, multiway_tie) = approximate_multiway_outcome(observation, player_count);
    let blended = (0.7 * multiway_win) + (0.2 * multiway_tie) + (0.1 * rank_band);
    (blended.clamp(0.0, 0.999_999) * 10.0).floor() as usize
}

fn river_abstraction_for_version(
    observation: Observation,
    seat_position: u8,
    parsed: crate::save::ParsedAbstractionVersion,
) -> Abstraction {
    let position = position_band(seat_position, parsed.player_count());
    let texture = board_texture_band(observation);
    let strength = river_strength_band(observation, parsed.player_count());
    let index = (position * 50) + (texture * 10) + strength;
    debug_assert!(index < crate::KMEANS_EQTY_CLUSTER_COUNT);
    Abstraction::from((Street::Rive, index))
}

fn versioned_lookup_rows(
    lookup: &Lookup,
    tables: &crate::save::AbstractionTables,
) -> Vec<(i64, i16, i16)> {
    let seat_count = versioned_seat_count(tables);
    let parsed = tables.parsed_version();
    lookup
        .0
        .iter()
        .flat_map(|(iso, abs)| {
            let iso = *iso;
            let abs = *abs;
            (0..seat_count).map(move |seat| {
                let resolved =
                    if iso.0.street() == Street::Rive && tables.uses_multiway_v4_bucketing() {
                        river_abstraction_for_version(
                            iso.0,
                            seat,
                            parsed.expect("v4 abstractions must parse their version contract"),
                        )
                    } else {
                        abs
                    };
                (i64::from(iso), i16::from(resolved), i16::from(seat))
            })
        })
        .collect()
}

#[cfg(feature = "database")]
impl crate::save::Schema for Lookup {
    fn name() -> &'static str {
        crate::save::ISOMORPHISM
    }
    fn columns() -> &'static [tokio_postgres::types::Type] {
        <crate::mccfr::NlheEncoder as crate::save::Schema>::columns()
    }
    fn creates() -> &'static str {
        <crate::mccfr::NlheEncoder as crate::save::Schema>::creates()
    }
    fn indices() -> &'static str {
        <crate::mccfr::NlheEncoder as crate::save::Schema>::indices()
    }
    fn copy() -> &'static str {
        <crate::mccfr::NlheEncoder as crate::save::Schema>::copy()
    }
    fn truncates() -> &'static str {
        <crate::mccfr::NlheEncoder as crate::save::Schema>::truncates()
    }
    fn freeze() -> &'static str {
        <crate::mccfr::NlheEncoder as crate::save::Schema>::freeze()
    }
}

#[cfg(feature = "database")]
#[async_trait::async_trait]
impl crate::save::Streamable for Lookup {
    type Row = (i64, i16);
    fn rows(self) -> impl Iterator<Item = Self::Row> + Send {
        self.0
            .into_iter()
            .map(|(iso, abs)| (i64::from(iso), i16::from(abs)))
    }
}

#[cfg(feature = "database")]
impl Lookup {
    pub async fn from_street(client: &tokio_postgres::Client, street: Street) -> Self {
        const SQL: &str =
            const_format::concatcp!("SELECT obs, abs FROM ", crate::save::ISOMORPHISM);
        client
            .query(SQL, &[])
            .await
            .expect("query")
            .into_iter()
            .map(|row| (row.get::<_, i64>(0), row.get::<_, i16>(1)))
            .filter(|(obs, _)| Street::from(*obs) == street)
            .map(|(obs, abs)| (Isomorphism::from(obs), Abstraction::from(abs)))
            .collect::<BTreeMap<_, _>>()
            .into()
    }

    pub async fn from_street_profile(
        client: &tokio_postgres::Client,
        tables: &crate::save::AbstractionTables,
        street: Street,
    ) -> Self {
        let isomorphism = if tables.is_default_v1() {
            crate::save::ISOMORPHISM.to_string()
        } else {
            tables.isomorphism()
        };
        let sql = if tables.is_default_v1() {
            format!("SELECT obs, abs FROM {isomorphism}")
        } else {
            format!("SELECT obs, abs FROM {isomorphism} WHERE seat_position = 0")
        };
        client
            .query(&sql, &[])
            .await
            .expect("query")
            .into_iter()
            .map(|row| (row.get::<_, i64>(0), row.get::<_, i16>(1)))
            .filter(|(obs, _)| Street::from(*obs) == street)
            .map(|(obs, abs)| (Isomorphism::from(obs), Abstraction::from(abs)))
            .collect::<BTreeMap<_, _>>()
            .into()
    }

    pub async fn stream_profile(
        self,
        client: &tokio_postgres::Client,
        tables: &crate::save::AbstractionTables,
    ) {
        use crate::save::{Row, Schema, Streamable};
        use tokio_postgres::binary_copy::BinaryCopyInWriter;
        let isomorphism = if tables.is_default_v1() {
            crate::save::ISOMORPHISM.to_string()
        } else {
            tables.isomorphism()
        };
        let seat_count = if tables.is_default_v1() {
            None
        } else {
            Some(versioned_seat_count(tables))
        };
        let position_aware = !tables.is_default_v1();
        let copy = if position_aware {
            format!("COPY {isomorphism} (obs, abs, seat_position) FROM STDIN BINARY")
        } else {
            format!("COPY {isomorphism} (obs, abs) FROM STDIN BINARY")
        };
        let chunk_size = std::env::var("CLUSTER_COPY_CHUNK")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .filter(|v| *v > 0)
            .unwrap_or(250_000);

        let columns = if position_aware {
            vec![Type::INT8, Type::INT2, Type::INT2]
        } else {
            <crate::mccfr::NlheEncoder as Schema>::columns().to_vec()
        };

        let mut writer = Box::pin({
            let sink = client.copy_in(&copy).await.expect("copy_in");
            BinaryCopyInWriter::new(sink, &columns)
        });
        let mut total: usize = 0;
        let mut in_chunk: usize = 0;

        if seat_count.is_some() {
            for row in versioned_lookup_rows(&self, tables) {
                row.write(writer.as_mut()).await;
                total += 1;
                in_chunk += 1;

                if in_chunk >= chunk_size {
                    writer.as_mut().finish().await.expect("finish");
                    log::info!(
                        "~ lookup copy: committed {} rows (total {})",
                        in_chunk,
                        total
                    );
                    writer = Box::pin({
                        let sink = client.copy_in(&copy).await.expect("copy_in");
                        BinaryCopyInWriter::new(sink, &columns)
                    });
                    in_chunk = 0;
                }
            }
        } else {
            for row in self.rows() {
                row.write(writer.as_mut()).await;
                total += 1;
                in_chunk += 1;

                if in_chunk >= chunk_size {
                    writer.as_mut().finish().await.expect("finish");
                    log::info!(
                        "~ lookup copy: committed {} rows (total {})",
                        in_chunk,
                        total
                    );
                    writer = Box::pin({
                        let sink = client.copy_in(&copy).await.expect("copy_in");
                        BinaryCopyInWriter::new(sink, &columns)
                    });
                    in_chunk = 0;
                }
            }
        }

        writer.as_mut().finish().await.expect("finish");
        log::info!(
            "~ lookup copy: committed final {} rows (total {})",
            in_chunk,
            total
        );
    }

    /// Stream river isomorphisms directly to Postgres without materializing the full lookup map.
    ///
    /// This avoids allocating a massive in-memory BTreeMap during river clustering.
    pub async fn stream_river_profile(
        client: &tokio_postgres::Client,
        tables: &crate::save::AbstractionTables,
    ) {
        use crate::save::{Row, Schema};
        use tokio::sync::mpsc;
        use tokio_postgres::binary_copy::BinaryCopyInWriter;

        let isomorphism = if tables.is_default_v1() {
            crate::save::ISOMORPHISM.to_string()
        } else {
            tables.isomorphism()
        };
        let seat_count = if tables.is_default_v1() {
            None
        } else {
            Some(versioned_seat_count(tables))
        };
        let parsed = tables.parsed_version();
        let position_aware = !tables.is_default_v1();
        let copy = if position_aware {
            format!("COPY {isomorphism} (obs, abs, seat_position) FROM STDIN BINARY")
        } else {
            format!("COPY {isomorphism} (obs, abs) FROM STDIN BINARY")
        };
        let chunk_size = std::env::var("RIVER_COPY_CHUNK")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .filter(|v| *v > 0)
            .unwrap_or(1_000_000);

        let columns = if position_aware {
            vec![Type::INT8, Type::INT2, Type::INT2]
        } else {
            <crate::mccfr::NlheEncoder as Schema>::columns().to_vec()
        };

        let mut writer = Box::pin({
            let sink = client.copy_in(&copy).await.expect("copy_in");
            BinaryCopyInWriter::new(sink, &columns)
        });

        // Bounded channel keeps memory usage low while still parallelizing equity computation.
        let (tx, mut rx) = mpsc::channel::<(i64, i16)>(20_000);
        let handle = std::thread::spawn(move || {
            IsomorphismIterator::from(Street::Rive)
                .par_bridge()
                .for_each_with(tx, |tx, iso| {
                    let abs = Abstraction::from(iso.0.equity());
                    let _ = tx.blocking_send((i64::from(iso), i16::from(abs)));
                });
        });

        let mut total: usize = 0;
        let mut in_chunk: usize = 0;

        while let Some(row) = rx.recv().await {
            if let Some(seat_count) = seat_count {
                let observation = Isomorphism::from(row.0).0;
                for seat in 0..seat_count {
                    let abs = if tables.uses_multiway_v4_bucketing() {
                        i16::from(river_abstraction_for_version(
                            observation,
                            seat,
                            parsed.expect("v4 abstractions must parse their version contract"),
                        ))
                    } else {
                        row.1
                    };
                    (row.0, abs, seat as i16).write(writer.as_mut()).await;
                    total += 1;
                    in_chunk += 1;

                    if in_chunk >= chunk_size {
                        writer.as_mut().finish().await.expect("finish");
                        log::info!(
                            "~ river copy: committed {} rows (total {})",
                            in_chunk,
                            total
                        );
                        writer = Box::pin({
                            let sink = client.copy_in(&copy).await.expect("copy_in");
                            BinaryCopyInWriter::new(sink, &columns)
                        });
                        in_chunk = 0;
                    }
                }
            } else {
                row.write(writer.as_mut()).await;
                total += 1;
                in_chunk += 1;

                if in_chunk >= chunk_size {
                    writer.as_mut().finish().await.expect("finish");
                    log::info!(
                        "~ river copy: committed {} rows (total {})",
                        in_chunk,
                        total
                    );
                    writer = Box::pin({
                        let sink = client.copy_in(&copy).await.expect("copy_in");
                        BinaryCopyInWriter::new(sink, &columns)
                    });
                    in_chunk = 0;
                }
            }
        }
        // Finish the final (possibly partial) chunk.
        writer.as_mut().finish().await.expect("finish");
        log::info!(
            "~ river copy: committed final {} rows (total {})",
            in_chunk,
            total
        );

        if let Err(err) = handle.join() {
            std::panic::resume_unwind(err);
        }
    }

    pub async fn truncate_profile(
        client: &tokio_postgres::Client,
        tables: &crate::save::AbstractionTables,
    ) {
        let isomorphism = if tables.is_default_v1() {
            crate::save::ISOMORPHISM.to_string()
        } else {
            tables.isomorphism()
        };
        let sql = format!("TRUNCATE TABLE {isomorphism};");
        client
            .batch_execute(&sql)
            .await
            .expect("truncate isomorphism");
    }

    pub async fn finalize_profile(
        client: &tokio_postgres::Client,
        tables: &crate::save::AbstractionTables,
    ) {
        let isomorphism = if tables.is_default_v1() {
            crate::save::ISOMORPHISM.to_string()
        } else {
            tables.isomorphism()
        };
        let sql = if tables.is_default_v1() {
            format!(
                "CREATE INDEX IF NOT EXISTS idx_{isomorphism}_covering ON {isomorphism} (obs, abs);
                 CREATE INDEX IF NOT EXISTS idx_{isomorphism}_abs_obs ON {isomorphism} (abs, obs);
                 CREATE INDEX IF NOT EXISTS idx_{isomorphism}_abs ON {isomorphism} (abs);
                 CREATE INDEX IF NOT EXISTS idx_{isomorphism}_obs ON {isomorphism} (obs);
                 ALTER TABLE {isomorphism} SET (fillfactor = 100);
                 ALTER TABLE {isomorphism} SET (autovacuum_enabled = false);"
            )
        } else {
            format!(
                "CREATE INDEX IF NOT EXISTS idx_{isomorphism}_covering ON {isomorphism} (obs, seat_position) INCLUDE (abs);
                 CREATE INDEX IF NOT EXISTS idx_{isomorphism}_abs_obs ON {isomorphism} (abs, obs);
                 CREATE INDEX IF NOT EXISTS idx_{isomorphism}_abs_seat ON {isomorphism} (abs, seat_position);
                 CREATE INDEX IF NOT EXISTS idx_{isomorphism}_obs ON {isomorphism} (obs);
                 CREATE INDEX IF NOT EXISTS idx_{isomorphism}_seat ON {isomorphism} (seat_position);
                 ALTER TABLE {isomorphism} SET (fillfactor = 100);
                 ALTER TABLE {isomorphism} SET (autovacuum_enabled = false);"
            )
        };
        client
            .batch_execute(&sql)
            .await
            .expect("finalize isomorphism");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    fn river_observation() -> Observation {
        Observation::try_from("Ah Qd ~ Jc Ts 8c 4d 2h").expect("river observation")
    }

    #[test]
    fn test_multiway_river_bucket_distinguishes_seat_position() {
        let parsed = crate::save::AbstractionTables::new("abs_v4_p6")
            .parsed_version()
            .expect("parsed version");
        let observation = river_observation();

        let button = river_abstraction_for_version(observation, 0, parsed);
        let middle = river_abstraction_for_version(observation, 3, parsed);

        assert_ne!(button, middle);
    }

    #[test]
    fn test_multiway_river_bucket_distinguishes_active_player_count() {
        let observation = river_observation();
        let three_handed = crate::save::AbstractionTables::new("abs_v4_p3")
            .parsed_version()
            .expect("parsed version");
        let six_handed = crate::save::AbstractionTables::new("abs_v4_p6")
            .parsed_version()
            .expect("parsed version");

        let short_handed = river_abstraction_for_version(observation, 1, three_handed);
        let full_ring = river_abstraction_for_version(observation, 1, six_handed);

        assert_ne!(short_handed, full_ring);
    }

    #[test]
    fn test_clustered_rows_are_seat_conditioned() {
        let tables = crate::save::AbstractionTables::new("abs_v4_p4");
        let iso = Isomorphism::from(river_observation());
        let lookup = Lookup::from(BTreeMap::from([(iso, Abstraction::from(iso.0.equity()))]));

        let rows = versioned_lookup_rows(&lookup, &tables);

        assert_eq!(rows.len(), 4);
        let seat_zero_abs = rows[0].1;
        assert!(rows.iter().any(|row| row.1 != seat_zero_abs));
    }

    #[test]
    fn test_legacy_env_flag_does_not_change_v4_meaning() {
        let _guard = env_lock().lock().expect("env lock");
        let previous = std::env::var("POSITION_AWARE_SEATS").ok();
        unsafe {
            std::env::set_var("POSITION_AWARE_SEATS", "9");
        }

        let tables = crate::save::AbstractionTables::new("abs_v4_p3");
        let seat_count = versioned_seat_count(&tables);

        match previous {
            Some(value) => unsafe {
                std::env::set_var("POSITION_AWARE_SEATS", value);
            },
            None => unsafe {
                std::env::remove_var("POSITION_AWARE_SEATS");
            },
        }

        assert_eq!(seat_count, 3);
    }

    #[test]
    #[ignore]
    #[cfg(feature = "disk")]
    fn persistence() {
        let street = Street::Pref;
        let lookup = Lookup::grow(street);
        lookup.save();
        let loaded = Lookup::load(street);
        std::iter::empty()
            .chain(lookup.0.iter().zip(loaded.0.iter()))
            .chain(loaded.0.iter().zip(lookup.0.iter()))
            .all(|((s1, l1), (s2, l2))| s1 == s2 && l1 == l2);
    }
}

impl Lookup {
    /// abstractions for River are calculated once via obs.equity
    /// abstractions for Preflop are equivalent to just enumerating isomorphisms
    pub fn grow(street: Street) -> Self {
        match street {
            Street::Rive => IsomorphismIterator::from(Street::Rive)
                .par_bridge()
                .map(|iso| (iso, Abstraction::from(iso.0.equity())))
                .collect::<Vec<_>>()
                .into_iter()
                .collect::<BTreeMap<_, _>>()
                .into(),
            Street::Pref => IsomorphismIterator::from(Street::Pref)
                .enumerate()
                .map(|(k, iso)| (iso, Abstraction::from((Street::Pref, k))))
                .collect::<BTreeMap<_, _>>()
                .into(),
            Street::Flop | Street::Turn => panic!("lookup must be learned via layer for {street}"),
        }
    }
}

#[cfg(feature = "disk")]
#[allow(deprecated)]
impl crate::save::Disk for Lookup {
    fn name() -> &'static str {
        crate::save::ISOMORPHISM
    }
    fn grow(street: Street) -> Self {
        Lookup::grow(street)
    }
    fn sources() -> Vec<std::path::PathBuf> {
        Street::all()
            .iter()
            .rev()
            .copied()
            .map(Self::path)
            .collect()
    }
    fn load(street: Street) -> Self {
        let ref path = Self::path(street);
        log::info!("{:<32}{:<32}", "loading     lookup", path.display());
        use byteorder::BE;
        use byteorder::ReadBytesExt;
        use std::fs::File;
        use std::io::BufReader;
        use std::io::Read;
        use std::io::Seek;
        use std::io::SeekFrom;
        let ref file = File::open(path).expect(&format!("open {}", path.display()));
        let mut lookup = BTreeMap::new();
        let mut reader = BufReader::new(file);
        let ref mut buffer = [0u8; 2];
        reader.seek(SeekFrom::Start(19)).expect("seek past header");
        while reader.read_exact(buffer).is_ok() {
            match u16::from_be_bytes(buffer.clone()) {
                2 => {
                    assert!(8 == reader.read_u32::<BE>().expect("observation length"));
                    let iso = reader.read_i64::<BE>().expect("read observation");
                    assert!(2 == reader.read_u32::<BE>().expect("abstraction length"));
                    let abs = reader.read_i16::<BE>().expect("read abstraction");
                    let observation = Isomorphism::from(iso);
                    let abstraction = Abstraction::from(abs);
                    lookup.insert(observation, abstraction);
                }
                0xFFFF => break,
                n => panic!("unexpected number of fields: {}", n),
            }
        }
        Self(lookup)
    }
    fn save(&self) {
        const N_FIELDS: u16 = 2;
        let street = self.street();
        let ref path = Self::path(street);
        let ref mut file = File::create(path).expect(&format!("touch {}", path.display()));
        use byteorder::BE;
        use byteorder::WriteBytesExt;
        use std::fs::File;
        use std::io::Write;
        log::info!("{:<32}{:<32}", "saving      lookup", path.display());
        file.write_all(Self::header()).expect("header");
        for (Isomorphism(obs), abs) in self.0.iter() {
            file.write_u16::<BE>(N_FIELDS).unwrap();
            file.write_u32::<BE>(size_of::<i64>() as u32).unwrap();
            file.write_i64::<BE>(i64::from(*obs)).unwrap();
            file.write_u32::<BE>(size_of::<i16>() as u32).unwrap();
            file.write_i16::<BE>(i16::from(*abs)).unwrap();
        }
        file.write_u16::<BE>(Self::footer()).expect("trailer");
    }
}
