use crate::clustering::*;
use crate::gameplay::*;
use std::collections::BTreeMap;

#[derive(Default)]
pub struct Future(BTreeMap<Abstraction, Histogram>);

impl From<BTreeMap<Abstraction, Histogram>> for Future {
    fn from(map: BTreeMap<Abstraction, Histogram>) -> Self {
        Self(map)
    }
}

impl From<Future> for BTreeMap<Abstraction, Histogram> {
    fn from(future: Future) -> Self {
        future.0
    }
}

#[cfg(feature = "database")]
#[async_trait::async_trait]
impl crate::save::Streamable for Future {
    type Row = (i16, i16, f32);
    fn rows(self) -> impl Iterator<Item = Self::Row> + Send {
        self.0.into_iter().flat_map(|(from, histogram)| {
            let prev = i16::from(from);
            histogram
                .distribution()
                .into_iter()
                .map(move |(into, weight)| (prev, i16::from(into), weight))
        })
    }
}

#[cfg(feature = "database")]
impl crate::save::Schema for Future {
    fn name() -> &'static str {
        crate::save::TRANSITIONS
    }
    fn columns() -> &'static [tokio_postgres::types::Type] {
        &[
            tokio_postgres::types::Type::INT2,
            tokio_postgres::types::Type::INT2,
            tokio_postgres::types::Type::FLOAT4,
        ]
    }
    fn creates() -> &'static str {
        const_format::concatcp!(
            "CREATE TABLE IF NOT EXISTS ",
            crate::save::TRANSITIONS,
            " (
                prev       SMALLINT,
                next       SMALLINT,
                dx         REAL
            );"
        )
    }
    fn indices() -> &'static str {
        const_format::concatcp!(
            "CREATE INDEX IF NOT EXISTS idx_",
            crate::save::TRANSITIONS,
            "_dx        ON ",
            crate::save::TRANSITIONS,
            " (dx);
             CREATE INDEX IF NOT EXISTS idx_",
            crate::save::TRANSITIONS,
            "_prev_dx   ON ",
            crate::save::TRANSITIONS,
            " (prev, dx);
             CREATE INDEX IF NOT EXISTS idx_",
            crate::save::TRANSITIONS,
            "_next_dx   ON ",
            crate::save::TRANSITIONS,
            " (next, dx);
             CREATE INDEX IF NOT EXISTS idx_",
            crate::save::TRANSITIONS,
            "_prev_next ON ",
            crate::save::TRANSITIONS,
            " (prev, next);
             CREATE INDEX IF NOT EXISTS idx_",
            crate::save::TRANSITIONS,
            "_next_prev ON ",
            crate::save::TRANSITIONS,
            " (next, prev);"
        )
    }
    fn copy() -> &'static str {
        const_format::concatcp!(
            "COPY ",
            crate::save::TRANSITIONS,
            " (prev, next, dx) FROM STDIN BINARY"
        )
    }
    fn truncates() -> &'static str {
        const_format::concatcp!("TRUNCATE TABLE ", crate::save::TRANSITIONS, ";")
    }
    fn freeze() -> &'static str {
        const_format::concatcp!(
            "ALTER TABLE ",
            crate::save::TRANSITIONS,
            " SET (fillfactor = 100);
            ALTER TABLE ",
            crate::save::TRANSITIONS,
            " SET (autovacuum_enabled = false);"
        )
    }
}

#[cfg(feature = "database")]
impl Future {
    pub async fn stream_profile(
        self,
        client: &tokio_postgres::Client,
        tables: &crate::save::AbstractionTables,
    ) {
        use crate::save::{Row, Schema, Streamable};
        use tokio_postgres::binary_copy::BinaryCopyInWriter;
        let transitions = if tables.is_default_v1() {
            crate::save::TRANSITIONS.to_string()
        } else {
            tables.transitions()
        };
        let copy = format!("COPY {transitions} (prev, next, dx) FROM STDIN BINARY");
        let chunk_size = std::env::var("CLUSTER_COPY_CHUNK")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .filter(|v| *v > 0)
            .unwrap_or(250_000);

        let mut writer = Box::pin({
            let sink = client.copy_in(&copy).await.expect("copy_in");
            BinaryCopyInWriter::new(sink, <Future as Schema>::columns())
        });
        let mut total: usize = 0;
        let mut in_chunk: usize = 0;

        for row in self.rows() {
            row.write(writer.as_mut()).await;
            total += 1;
            in_chunk += 1;

            if in_chunk >= chunk_size {
                writer.as_mut().finish().await.expect("finish");
                log::info!(
                    "~ transitions copy: committed {} rows (total {})",
                    in_chunk,
                    total
                );
                writer = Box::pin({
                    let sink = client.copy_in(&copy).await.expect("copy_in");
                    BinaryCopyInWriter::new(sink, <Future as Schema>::columns())
                });
                in_chunk = 0;
            }
        }

        writer.as_mut().finish().await.expect("finish");
        log::info!(
            "~ transitions copy: committed final {} rows (total {})",
            in_chunk,
            total
        );
    }

    pub async fn truncate_profile(
        client: &tokio_postgres::Client,
        tables: &crate::save::AbstractionTables,
    ) {
        let transitions = if tables.is_default_v1() {
            crate::save::TRANSITIONS.to_string()
        } else {
            tables.transitions()
        };
        let sql = format!("TRUNCATE TABLE {transitions};");
        client
            .batch_execute(&sql)
            .await
            .expect("truncate transitions");
    }

    pub async fn finalize_profile(
        client: &tokio_postgres::Client,
        tables: &crate::save::AbstractionTables,
    ) {
        let transitions = if tables.is_default_v1() {
            crate::save::TRANSITIONS.to_string()
        } else {
            tables.transitions()
        };
        let sql = format!(
            "CREATE INDEX IF NOT EXISTS idx_{transitions}_dx ON {transitions} (dx);
             CREATE INDEX IF NOT EXISTS idx_{transitions}_prev_dx ON {transitions} (prev, dx);
             CREATE INDEX IF NOT EXISTS idx_{transitions}_next_dx ON {transitions} (next, dx);
             CREATE INDEX IF NOT EXISTS idx_{transitions}_prev_next ON {transitions} (prev, next);
             CREATE INDEX IF NOT EXISTS idx_{transitions}_next_prev ON {transitions} (next, prev);
             ALTER TABLE {transitions} SET (fillfactor = 100);
             ALTER TABLE {transitions} SET (autovacuum_enabled = false);"
        );
        client
            .batch_execute(&sql)
            .await
            .expect("finalize transitions");
    }
}

#[cfg(feature = "disk")]
use crate::cards::*;

#[cfg(feature = "disk")]
#[allow(deprecated)]
impl crate::save::Disk for Future {
    fn name() -> &'static str {
        crate::save::TRANSITIONS
    }
    fn grow(street: Street) -> Self {
        unreachable!("you have no business making transition table from scratch {street}")
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
        log::info!("{:<32}{:<32}", "loading     transitions", path.display());
        use byteorder::BE;
        use byteorder::ReadBytesExt;
        use std::fs::File;
        use std::io::BufReader;
        use std::io::Read;
        use std::io::Seek;
        use std::io::SeekFrom;
        let ref mass = street.n_children() as f32;
        let ref file = File::open(path).expect(&format!("open {}", path.display()));
        let mut future = BTreeMap::new();
        let mut reader = BufReader::new(file);
        let ref mut buffer = [0u8; 2];
        reader.seek(SeekFrom::Start(19)).expect("seek past header");
        while reader.read_exact(buffer).is_ok() {
            match u16::from_be_bytes(buffer.clone()) {
                3 => {
                    reader.read_u32::<BE>().expect("from abstraction");
                    let from = reader.read_i16::<BE>().expect("read from abstraction");
                    reader.read_u32::<BE>().expect("into abstraction");
                    let into = reader.read_i16::<BE>().expect("read into abstraction");
                    reader.read_u32::<BE>().expect("weight");
                    let weight = reader.read_f32::<BE>().expect("read weight");
                    future
                        .entry(Abstraction::from(from))
                        .or_insert_with(|| Histogram::empty(street.next()))
                        .set(Abstraction::from(into), (weight * mass) as usize);
                    continue;
                }
                0xFFFF => break,
                n => panic!("unexpected number of fields: {}", n),
            }
        }
        Self(future)
    }

    fn save(&self) {
        const N_FIELDS: u16 = 3;
        let street = self
            .0
            .keys()
            .next()
            .copied()
            .unwrap_or_else(|| Abstraction::from(0f32))
            .street();
        let ref path = Self::path(street);
        let ref mut file = File::create(path).expect(&format!("touch {}", path.display()));
        use byteorder::BE;
        use byteorder::WriteBytesExt;
        use std::fs::File;
        use std::io::Write;
        log::info!("{:<32}{:<32}", "saving      transition", path.display());
        file.write_all(Self::header()).expect("header");
        for (from, histogram) in self.0.iter() {
            for into in histogram.support() {
                file.write_u16::<BE>(N_FIELDS).unwrap();
                file.write_u32::<BE>(size_of::<i16>() as u32).unwrap();
                file.write_i16::<BE>(i16::from(*from)).unwrap();
                file.write_u32::<BE>(size_of::<i16>() as u32).unwrap();
                file.write_i16::<BE>(i16::from(into)).unwrap();
                file.write_u32::<BE>(size_of::<f32>() as u32).unwrap();
                file.write_f32::<BE>(histogram.density(&into)).unwrap();
            }
        }
        file.write_u16::<BE>(Self::footer()).expect("trailer");
    }
}
