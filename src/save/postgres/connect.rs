use crate::autotrain::*;
use crate::clustering::*;
use crate::mccfr::*;
use crate::save::*;
use std::sync::Arc;
use tokio_postgres::Client;

async fn create_training_profile_table(client: &Client) {
    let sql = "CREATE TABLE IF NOT EXISTS training_profile (
        profile_id          TEXT PRIMARY KEY,
        profile_key         TEXT NOT NULL,
        format              TEXT NOT NULL,
        player_count        INTEGER NOT NULL,
        config_json         TEXT NOT NULL,
        abstraction_version TEXT NOT NULL,
        engine_version      TEXT NOT NULL,
        created_at          TIMESTAMPTZ NOT NULL DEFAULT NOW()
    );
    CREATE UNIQUE INDEX IF NOT EXISTS idx_training_profile_key
        ON training_profile (profile_key);";
    client
        .batch_execute(sql)
        .await
        .expect("create training_profile");
}

async fn register_training_profile(client: &Client, profile: &TrainingProfileMeta) {
    let sql = "INSERT INTO training_profile
        (profile_id, profile_key, format, player_count, config_json, abstraction_version, engine_version)
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        ON CONFLICT (profile_id) DO UPDATE SET
            profile_key = EXCLUDED.profile_key,
            format = EXCLUDED.format,
            player_count = EXCLUDED.player_count,
            config_json = EXCLUDED.config_json,
            abstraction_version = EXCLUDED.abstraction_version,
            engine_version = EXCLUDED.engine_version";
    client
        .execute(
            sql,
            &[
                &profile.profile_id,
                &profile.profile_key,
                &profile.format,
                &(profile.player_count as i32),
                &profile.config_json,
                &profile.abstraction_version,
                &profile.engine_version,
            ],
        )
        .await
        .expect("register training_profile");
}

/// Get a database connection, run migrations, and return the client.
///
/// Uses static table names (BLUEPRINT, EPOCH, etc.) for backwards
/// compatibility with heads-up training.
pub async fn db() -> Arc<Client> {
    log::info!("connecting to database");
    let tls = tokio_postgres::tls::NoTls;
    let ref url = std::env::var("DB_URL").expect("DB_URL must be set");
    let (client, connection) = tokio_postgres::connect(url, tls)
        .await
        .expect("database connection failed");
    tokio::spawn(connection);
    client
        .execute("SET client_min_messages TO WARNING", &[])
        .await
        .expect("set client_min_messages");
    client
        .batch_execute(&Epoch::creates())
        .await
        .expect("epoch");
    client
        .batch_execute(&Metric::creates())
        .await
        .expect("metric");
    client
        .batch_execute(&Future::creates())
        .await
        .expect("transitions");
    client
        .batch_execute(&Lookup::creates())
        .await
        .expect("isomorphism");
    client
        .batch_execute(&NlheProfile::creates())
        .await
        .expect("blueprint");
    Arc::new(client)
}

/// Get a database connection for profile-aware multiway training.
///
/// Creates profile-specific tables using `ProfileTables` and `AbstractionTables`
/// for the given training configuration. Falls back to static table names
/// when tables.is_default_hu() is true.
pub async fn db_profile(
    tables: &TrainingTables,
    profile: Option<&TrainingProfileMeta>,
) -> Arc<Client> {
    log::info!(
        "connecting to database (profile: {}, abstraction: {})",
        tables.profile.profile_key(),
        tables.abstraction.abstraction_version()
    );
    let tls = tokio_postgres::tls::NoTls;
    let ref url = std::env::var("DB_URL").expect("DB_URL must be set");
    let (client, connection) = tokio_postgres::connect(url, tls)
        .await
        .expect("database connection failed");
    tokio::spawn(connection);
    client
        .execute("SET client_min_messages TO WARNING", &[])
        .await
        .expect("set client_min_messages");

    // Create profile tables
    create_profile_tables(&client, &tables.profile).await;

    // Create abstraction tables
    create_abstraction_tables(&client, &tables.abstraction).await;

    if let Some(profile) = profile {
        create_training_profile_table(&client).await;
        register_training_profile(&client, profile).await;
    }

    Arc::new(client)
}

/// Create profile-specific blueprint/staging/epoch tables.
async fn create_profile_tables(client: &Client, tables: &ProfileTables) {
    if tables.is_default_hu() {
        // Use static schemas for default HU profile
        client
            .batch_execute(&Epoch::creates())
            .await
            .expect("epoch");
        client
            .batch_execute(&NlheProfile::creates())
            .await
            .expect("blueprint");
    } else {
        // Create profile-specific tables
        let blueprint = tables.blueprint();
        let epoch = tables.epoch();

        // Epoch table (key/value for training metadata)
        let epoch_sql = format!(
            "CREATE TABLE IF NOT EXISTS {epoch} (
                key   TEXT PRIMARY KEY,
                value BIGINT NOT NULL
            );
            INSERT INTO {epoch} (key, value)
            VALUES ('current', 0)
            ON CONFLICT (key) DO NOTHING;"
        );
        client
            .batch_execute(&epoch_sql)
            .await
            .expect("create epoch");

        // Blueprint table (strategy data)
        let blueprint_sql = format!(
            "CREATE TABLE IF NOT EXISTS {blueprint} (
                edge       BIGINT,
                past       BIGINT,
                present    SMALLINT,
                future     BIGINT,
                policy     REAL,
                regret     REAL,
                UNIQUE     (past, present, future, edge)
            );"
        );
        client
            .batch_execute(&blueprint_sql)
            .await
            .expect("create blueprint");
    }
}

/// Create abstraction-versioned clustering tables.
async fn create_abstraction_tables(client: &Client, tables: &AbstractionTables) {
    if tables.is_default_v1() {
        // Use static schemas for default v1 abstraction
        client
            .batch_execute(&Metric::creates())
            .await
            .expect("metric");
        client
            .batch_execute(&Future::creates())
            .await
            .expect("transitions");
        client
            .batch_execute(&Lookup::creates())
            .await
            .expect("isomorphism");
    } else {
        // Create versioned abstraction tables
        let abstraction = tables.abstraction();
        let isomorphism = tables.isomorphism();
        let metric = tables.metric();
        let transitions = tables.transitions();

        // Abstraction table (cluster centroids)
        let abstraction_sql = format!(
            "CREATE TABLE IF NOT EXISTS {abstraction} (
                abs        BIGINT PRIMARY KEY,
                street     SMALLINT NOT NULL,
                equity     REAL NOT NULL,
                population INTEGER NOT NULL
            );"
        );
        client
            .batch_execute(&abstraction_sql)
            .await
            .expect("create abstraction");

        // Isomorphism table (observation -> abstraction mapping)
        // Note: abs uses SMALLINT to match binary copy schema (obs BIGINT, abs SMALLINT).
        // seat_position supports position-aware multiway abstractions.
        let isomorphism_sql = format!(
            "CREATE TABLE IF NOT EXISTS {isomorphism} (
                obs           BIGINT NOT NULL,
                abs           SMALLINT NOT NULL,
                seat_position SMALLINT NOT NULL DEFAULT 0,
                PRIMARY KEY (obs, seat_position)
            );"
        );
        client
            .batch_execute(&isomorphism_sql)
            .await
            .expect("create isomorphism");

        // Metric table (cluster distances)
        let metric_sql = format!(
            "CREATE TABLE IF NOT EXISTS {metric} (
                tri INTEGER,
                dx  REAL NOT NULL
            );"
        );
        client
            .batch_execute(&metric_sql)
            .await
            .expect("create metric");

        let tri_fn_sql = "CREATE OR REPLACE FUNCTION get_pair_tri(abs1 SMALLINT, abs2 SMALLINT) RETURNS INTEGER AS
        $$ DECLARE
            street INTEGER;
            i1 INTEGER;
            i2 INTEGER;
            lo INTEGER;
            hi INTEGER;
        BEGIN
            street := (abs1 >> 10) & 63;
            i1 := abs1 & 1023;
            i2 := abs2 & 1023;
            IF i1 < i2 THEN
                lo := i1;
                hi := i2;
            ELSE
                lo := i2;
                hi := i1;
            END IF;
            IF hi = 0 THEN
                RETURN (street << 30);
            ELSE
                RETURN (street << 30) | (hi * (hi - 1) / 2 + lo);
            END IF;
        END;
        $$ LANGUAGE plpgsql;";
        client
            .batch_execute(tri_fn_sql)
            .await
            .expect("create get_pair_tri");

        // Transitions table (street transitions)
        let transitions_sql = format!(
            "CREATE TABLE IF NOT EXISTS {transitions} (
                prev SMALLINT NOT NULL,
                next SMALLINT NOT NULL,
                dx   REAL NOT NULL,
                PRIMARY KEY (prev, next)
            );"
        );
        client
            .batch_execute(&transitions_sql)
            .await
            .expect("create transitions");
    }
}
