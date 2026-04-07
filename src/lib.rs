#![allow(
    dead_code,
    clippy::all,
    clippy::pedantic,
    clippy::nursery,
    clippy::cargo,
    clippy::correctness,
    clippy::style,
    clippy::complexity,
    clippy::perf,
    clippy::suspicious,
    clippy::restriction,
    clippy::blanket_clippy_restriction_lints,
    clippy::missing_docs_in_private_items
)]

pub mod cards;
pub mod dto;
pub mod gameplay;
pub mod mccfr;
pub mod transport;

#[cfg(feature = "database")]
pub mod analysis;
#[cfg(feature = "database")]
pub mod autotrain;
#[cfg(feature = "server")]
pub mod clustering;
#[cfg(feature = "database")]
pub mod database;
#[cfg(feature = "server")]
pub mod gameroom;
#[cfg(feature = "server")]
pub mod hosting;
#[cfg(feature = "server")]
pub mod players;
#[cfg(feature = "server")]
pub mod save;
#[cfg(feature = "server")]
pub mod search;
#[cfg(feature = "database")]
pub mod workers;

/// dimensional analysis types
type Chips = i16;
type Energy = f32;
type Entropy = f32;
type Utility = f32;
type Probability = f32;

// game tree parameters
const MAX_SEATS: usize = 10;
const N: usize = MAX_SEATS;
const STACK: Chips = 100;
const B_BLIND: Chips = 2;
const S_BLIND: Chips = 1;
const MAX_RAISE_REPEATS: usize = 3;
/// Maximum action edges packed into a single `Path(u64)` via 4-bit nibbles.
///
/// RPM-05 path-depth decision: 16 edges remains sufficient for current multiway
/// cash games. Worst-case per-street depth with `MAX_RAISE_REPEATS = 3` is bounded:
/// each street sees at most `3` raise/re-raise edges plus passive actions
/// (check/call/fold), and `Draw` edges separate streets.  Even 10-max preflop
/// with aggressive 3-bet/4-bet trees fits within 16 total history edges after
/// the `Draw` reset on each new street.
///
/// If deeper tournament stacks or unbounded raise-cap variants are needed,
/// widen `Path` to `u128` (32 edges) or a heap-allocated representation and
/// bump the info-version boundary.
const MAX_DEPTH_SUBGAME: usize = 16;

/// sinkhorn optimal transport parameters
const SINKHORN_TEMPERATURE: Entropy = 0.025;
const SINKHORN_ITERATIONS: usize = 128;
const SINKHORN_TOLERANCE: Energy = 0.001;

// kmeans clustering parameters
// Increased bucket counts for better strategic resolution (was 128/144/101)
// See ops/hetzner_training_inventory.md for analysis
const KMEANS_FLOP_TRAINING_ITERATIONS: usize = 20;
const KMEANS_TURN_TRAINING_ITERATIONS: usize = 24;
const KMEANS_FLOP_CLUSTER_COUNT: usize = 400;
const KMEANS_TURN_CLUSTER_COUNT: usize = 400;
const KMEANS_EQTY_CLUSTER_COUNT: usize = 150;

/// rps mccfr parameteres
const ASYMMETRIC_UTILITY: f32 = 2.0;
const CFR_BATCH_SIZE_RPS: usize = 1;
const CFR_TREE_COUNT_RPS: usize = 8192;

// nlhe mccfr parameters
const CFR_BATCH_SIZE_NLHE: usize = 128;
const CFR_TREE_COUNT_NLHE: usize = 0x10000000;

/// profile average sampling parameters
const SAMPLING_THRESHOLD: Entropy = 1.0;
const SAMPLING_ACTIVATION: Energy = 0.0;
const SAMPLING_EXPLORATION: Probability = 0.01;

// regret matching parameters, although i haven't implemented regret clamp yet
const POLICY_MIN: Probability = Probability::MIN_POSITIVE;
const REGRET_MIN: Utility = -3e5;

// training parameters
const TRAINING_LOG_INTERVAL_SECS_DEFAULT: u64 = 60;

/// Resolve the training log/checkpoint interval.
/// Can be overridden via TRAINING_LOG_INTERVAL_SECS.
pub fn training_log_interval() -> std::time::Duration {
    use std::sync::OnceLock;
    static CACHED: OnceLock<std::time::Duration> = OnceLock::new();
    *CACHED.get_or_init(|| {
        let secs = std::env::var("TRAINING_LOG_INTERVAL_SECS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .filter(|v| *v > 0)
            .unwrap_or(TRAINING_LOG_INTERVAL_SECS_DEFAULT);
        std::time::Duration::from_secs(secs)
    })
}

// regret bias parameters (weights, not probabilities—only ratios matter)
// with k=4 raises: p(fold)=50%, p(raises)=33%, p(other)=17%
const BIAS_FOLDS: Utility = 3.0;
const BIAS_RAISE: Utility = 0.5;
const BIAS_OTHER: Utility = 1.0;

/// trait for random generation, mainly (strictly?) for testing
pub trait Arbitrary {
    fn random() -> Self;
}

/// initialize logging and setup graceful interrupt listener
#[cfg(feature = "server")]
pub fn log() {
    std::fs::create_dir_all("logs").expect("create logs directory");
    let config = simplelog::ConfigBuilder::new()
        .set_location_level(log::LevelFilter::Off)
        .set_target_level(log::LevelFilter::Off)
        .set_thread_level(log::LevelFilter::Off)
        .build();
    let time = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("time moves slow")
        .as_secs();
    let file = simplelog::WriteLogger::new(
        log::LevelFilter::Debug,
        config.clone(),
        std::fs::File::create(format!("logs/{}.log", time)).expect("create log file"),
    );
    let term = simplelog::TermLogger::new(
        log::LevelFilter::Info,
        config.clone(),
        simplelog::TerminalMode::Mixed,
        simplelog::ColorChoice::Auto,
    );
    simplelog::CombinedLogger::init(vec![term, file]).expect("initialize logger");
}

#[cfg(feature = "server")]
pub fn kys() {
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.unwrap();
        println!();
        log::warn!("violent interrupt received, exiting immediately");
        std::process::exit(0);
    });
}

#[cfg(feature = "server")]
static INTERRUPTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
#[cfg(feature = "server")]
pub fn interrupted() -> bool {
    INTERRUPTED.load(std::sync::atomic::Ordering::Relaxed)
}
#[cfg(not(feature = "server"))]
pub fn interrupted() -> bool {
    false
}
#[cfg(feature = "server")]
pub fn brb() {
    std::thread::spawn(|| {
        loop {
            let ref mut buffer = String::new();
            if let Ok(_) = std::io::stdin().read_line(buffer) {
                if buffer.trim().to_uppercase() == "Q" {
                    log::warn!("graceful interrupt requested, finishing current batch...");
                    INTERRUPTED.store(true, std::sync::atomic::Ordering::Relaxed);
                    break;
                }
            }
        }
    });
}
