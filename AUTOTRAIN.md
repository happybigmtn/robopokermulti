# Autotrain Pipeline

Unified training pipeline that streams data directly to PostgreSQL.

## High-Level Overview

```
                          AUTOTRAIN PIPELINE

    cargo run --bin trainer --features server -- [--status|--cluster|--fast|--slow]
                                    |
         +--------------------------+---------------------------+
         v                          v                           v
    +---------+              +--------------+             +------------+
    | status  |              |   cluster    |             |   train    |
    | (query) |              | (PreTraining)|             | (Session)  |
    +---------+              +--------------+             +------------+
                                    |                           |
                                    |                           |
                             +------+------+              +-----+-----+
                             |  PHASE 1    |              |  PHASE 2  |
                             | Clustering  |---requires---|  Blueprint|
                             | (offline)   |              |  (MCCFR)  |
                             +-------------+              +-----------+
```

1. **Clustering Phase**: Reduce 3.1 trillion poker situations into ~500 abstract buckets
2. **Blueprint Phase**: Train MCCFR strategies on the abstracted game tree

## Usage

```bash
# Check current state
cargo run --bin trainer --features server -- --status

# Run clustering only
cargo run --bin trainer --features server -- --cluster

# Run fast in-memory training (includes clustering if needed)
cargo run --bin trainer --features server -- --fast

# Run distributed training (includes clustering if needed)
cargo run --bin trainer --features server -- --slow
```

---

## Local Multiway (N-Seat) Clustering & Training

Use profile-aware tables for 3–10 seat runs. This lets you run multiple profiles in the same DB without collisions.

### 1) Pick a profile + abstraction version

Conventions that match current logs/examples:

- `PROFILE_KEY`: `bp_<seats>max_local` (e.g., `bp_3max_local`)
- `ABSTRACTION_VERSION`: `abs_v4_p<seats>` (e.g., `abs_v4_p3`)
- `PLAYER_COUNT`: `3`..`10`

### 2) Set environment

```bash
export DB_URL="postgres://robopoker:pass@localhost:54329/robopoker"
export PROFILE_KEY=bp_3max_local
export ABSTRACTION_VERSION=abs_v4_p3
export PLAYER_COUNT=3
```

### 2b) Pin local CPU usage (optional)

To run with 18 cores locally:

```bash
export RAYON_NUM_THREADS=18
```

### 3) Chunked COPY (recommended)

Clustering streams large tables into Postgres. These chunk sizes reduce memory spikes.

```bash
export CLUSTER_COPY_CHUNK=250000   # lookup/metric/transitions COPY batch size
export RIVER_COPY_CHUNK=250000     # river isomorphism COPY batch size
```

### 4) Check status (resume-safe)

```bash
cargo run --bin trainer --features server -- --status
```

If a street is already clustered, it will show a ✓ and **will be skipped** on re-run.

#### Helper script (local)

Use `scripts/cluster_local.sh` for fast restarts:

```bash
scripts/cluster_local.sh status
scripts/cluster_local.sh cluster
scripts/cluster_local.sh cluster-bg
```

Position-aware wrapper for a 4-seat validation run:

```bash
scripts/cluster_local_position_aware.sh status
scripts/cluster_local_position_aware.sh cluster
scripts/cluster_local_position_aware.sh cluster-bg
```

Defaults for the position-aware N=4 wrapper (ccx33-friendly):

```
PROFILE_KEY=bp_4max_local
ABSTRACTION_VERSION=abs_v4_p4
PLAYER_COUNT=4
RAYON_NUM_THREADS=8
```

The script uses the same env vars as above and defaults to:

```
DB_URL=postgres://robopoker:pass@localhost:54329/robopoker
PROFILE_KEY=bp_3max_local
ABSTRACTION_VERSION=abs_v4_p3
PLAYER_COUNT=3
RAYON_NUM_THREADS=18
CLUSTER_COPY_CHUNK=250000
RIVER_COPY_CHUNK=250000
```

### 5) Start clustering (resume-safe)

```bash
cargo run --bin trainer --features server -- --cluster
```

To run in the background with logs:

```bash
log=logs/cluster_${PROFILE_KEY}_$(date +"%Y%m%d_%H%M%S").log
nohup env DB_URL="$DB_URL" PROFILE_KEY="$PROFILE_KEY" \
  ABSTRACTION_VERSION="$ABSTRACTION_VERSION" PLAYER_COUNT="$PLAYER_COUNT" \
  RAYON_NUM_THREADS="${RAYON_NUM_THREADS:-18}" \
  CLUSTER_COPY_CHUNK="$CLUSTER_COPY_CHUNK" RIVER_COPY_CHUNK="$RIVER_COPY_CHUNK" \
  cargo run --bin trainer --features server -- --cluster > "$log" 2>&1 &
echo $! > logs/cluster_${PROFILE_KEY}.pid
```

### 6) Start training (MCCFR)

Once clustering is complete:

```bash
cargo run --bin trainer --features server -- --slow
```

### Notes

- **Resume behavior:** `--cluster` checks existing `isomorphism_<abstraction_version>` rows per street and skips streets already complete.
- **Logs:** live progress prints in the log (`~ river copy: committed …`, `kmeans hydrating`, `kmeans iterating`).
- **Tables used:** `blueprint_<profile_key>`, `epoch_<profile_key>`, `isomorphism_<abstraction_version>`, `metric_<abstraction_version>`, `transitions_<abstraction_version>`, `abstraction_<abstraction_version>`.
- **Position-aware runs:** `abs_v4_p{n}` derives seat-aware lookup shape from the abstraction version itself. `POSITION_AWARE` / `POSITION_AWARE_SEATS` no longer change v4 clustering meaning.

---

## Hetzner 6-Seat Training (N=6)

Current 6-seat allocation (per `ops/hetzner_training_inventory.md`):

- **DB host:** `ns-db-1` (5.161.124.82)
- **Coordinator + primary worker:** `ns-sim-1` (5.161.67.36)
- **Optional worker:** `ns-gw-1` (178.156.212.135)
- **Trainer host (current run):** `ns-train-1` (5.161.101.39)
- **DB name:** `robopoker_training`

### Current N=6 run (as of 2026-01-15)

On `ns-train-1`:

```text
container: trainer-cluster
PROFILE_KEY=bp_6max_cash_2026_01_14
ABSTRACTION_VERSION=abs_v4_p6
PLAYER_COUNT=6
```

### Quick status check

```bash
ssh -i ~/.ssh/hetzner_training root@5.161.101.39 "hostname && uptime"
ssh -i ~/.ssh/hetzner_training root@5.161.101.39 "docker ps --format 'table {{.Names}}\\t{{.Image}}\\t{{.Status}}'"
ssh -i ~/.ssh/hetzner_training root@5.161.67.36 "hostname && uptime"
ssh -i ~/.ssh/hetzner_training root@5.161.67.36 "docker ps --format 'table {{.Names}}\\t{{.Image}}\\t{{.Status}}'"
```

### Start coordinator (trainer)

```bash
ssh -i ~/.ssh/hetzner_training root@5.161.67.36
export DB_URL="postgres://robopoker:<password>@5.161.124.82:5432/robopoker_training"
export PROFILE_KEY=<profile_key>           # e.g., bp_6max_cash (replace with actual)
export ABSTRACTION_VERSION=<abs_version>   # e.g., abs_v4_p6 (replace with actual)
export PLAYER_COUNT=6
export RAYON_NUM_THREADS=18

docker run -d --name trainer \
  -e DB_URL="$DB_URL" \
  -e PROFILE_KEY="$PROFILE_KEY" \
  -e ABSTRACTION_VERSION="$ABSTRACTION_VERSION" \
  -e PLAYER_COUNT="$PLAYER_COUNT" \
  robopoker-trainer --slow
```

### Start worker(s)

```bash
ssh -i ~/.ssh/hetzner_training root@5.161.67.36
docker run -d --name worker \
  -e DB_URL="$DB_URL" \
  -e PROFILE_KEY="$PROFILE_KEY" \
  -e ABSTRACTION_VERSION="$ABSTRACTION_VERSION" \
  -e PLAYER_COUNT="$PLAYER_COUNT" \
  robopoker-trainer --worker
```

### Monitor progress

```bash
ssh -i ~/.ssh/hetzner_training root@5.161.67.36 "docker logs -f trainer"
```

---

## Hetzner N=4 Position-Aware Validation (CCX33)

Use a CCX33 (8c/32GB) for the first position-aware validation run.

Recommended runtime settings:

```
PROFILE_KEY=bp_4max_cash_2026_01_15
ABSTRACTION_VERSION=abs_v4_p4
PLAYER_COUNT=4
RAYON_NUM_THREADS=8
CLUSTER_COPY_CHUNK=250000
RIVER_COPY_CHUNK=250000
```

Provisioned host (2026-01-15):

- Host: `ns-train-2` (CCX33, 8c/32GB, 240GB)
- Public IP: `178.156.214.100`
- Private IP: `10.0.1.4`

Start the clustered run on the host (uses `/opt/robopoker/robopoker.env` for `DB_URL`):

```bash
ssh -i ~/.ssh/hetzner_training root@178.156.214.100
docker run -d --name trainer-cluster \
  --env-file /opt/robopoker/robopoker.env \
  -e PROFILE_KEY=bp_4max_cash_2026_01_15 \
  -e ABSTRACTION_VERSION=abs_v4_p4 \
  -e PLAYER_COUNT=4 \
  -e RAYON_NUM_THREADS=8 \
  -e CLUSTER_COPY_CHUNK=250000 \
  -e RIVER_COPY_CHUNK=250000 \
  robopoker-trainer:latest ./trainer --cluster
```

Monitor logs:

```bash
ssh -i ~/.ssh/hetzner_training root@178.156.214.100 "docker logs -f trainer-cluster"
```

Swap recommendation (64–96G):

```bash
fallocate -l 64G /swapfile
chmod 600 /swapfile
mkswap /swapfile
swapon /swapfile
echo '/swapfile none swap sw 0 0' >> /etc/fstab
```

## Phase 1: Clustering (PreTraining)

**Entry Point:** `PreTraining::run()` in `src/autotrain/pretraining.rs`

The clustering phase processes streets in **reverse order** (River -> Turn -> Flop -> Preflop) because each street's abstraction depends on the _next_ street's lookup and metric.

### Reverse Dependency Chain

```
                     CLUSTERING: REVERSE DEPENDENCY CHAIN

     +---------------+      +---------------+      +---------------+      +---------------+
     |    RIVER      |      |     TURN      |      |     FLOP      |      |   PREFLOP     |
     |  123M obs     |      |   14M obs     |      |   1.3M obs    |      |   169 obs     |
     |  K=101        |      |   K=144       |      |   K=128       |      |   K=169       |
     +-------+-------+      +-------+-------+      +-------+-------+      +-------+-------+
             |                      |                      |                      |
             | Lookup::grow()       | Layer::cluster()     | Layer::cluster()     | Layer::cluster()
             |                      |                      |                      |
             v                      v                      v                      v
     +---------------+      +---------------+      +---------------+      +---------------+
     |   produces:   |      |   produces:   |      |   produces:   |      |   produces:   |
     |   * lookup    |------|   * lookup    |------|   * lookup    |------|   * lookup    |
     |               |  ^   |   * metric    |  ^   |   * metric    |  ^   |   * metric    |
     |               |  |   |   * future    |  |   |   * future    |  |   |   * future    |
     +---------------+  |   +---------------+  |   +---------------+  |   +---------------+
                        |           |          |           |          |
                        |           |          |           |          |
                        +---loads---+          +---loads---+          +---loads---
                         lookup               lookup + metric        lookup + metric
```

### Street Parameters

| Street  | N           | K   | Metric     | Space                  | Loads                    | Produces                   |
| ------- | ----------- | --- | ---------- | ---------------------- | ------------------------ | -------------------------- |
| River   | 123,156,254 | 101 | `f32::abs` | `Probability`          | N/A                      | lookup                     |
| Turn    | 13,960,050  | 144 | `W1`       | `Density<Probability>` | Lookup River             | lookup, metric, transition |
| Flop    | 1,286,792   | 128 | `EMD`      | `Density<Abstraction>` | Lookup Turn, Metric Turn | lookup, metric, transition |
| Preflop | 169         | 169 | `EMD`      | `Density<Abstraction>` | Lookup Flop, Metric Flop | lookup, metric, transition |

- **N**: Number of isomorphic observations on this street
- **K**: Number of abstraction clusters (river uses equity buckets 0-100%)
- **Metric**: Distance function for clustering (`W1` = Wasserstein-1, `EMD` = Earth Mover's Distance)
- **Space**: Element type being compared in the metric

### Per-Street Processing Detail

#### River Clustering

```
                           RIVER CLUSTERING
                           (pretraining.rs:31)

   IsomorphismIterator::from(River)
              |
              v
   +-------------------+     +-----------------+
   | foreach 123M iso  |---->| iso.equity()    |  // Monte Carlo hand strength
   +-------------------+     +--------+--------+
                                      |
                                      v
                            +-----------------+
                            | Abstraction from|  // 0-100 equity buckets
                            | equity percent  |
                            +--------+--------+
                                     |
                                     v
                              +-------------+
                              |   Lookup    |  // stream to isomorphism table
                              +-------------+

   NO k-means (equity buckets are the abstractions directly)
   NO metric  (equity distance is just |e1 - e2|)
```

#### Turn / Flop / Preflop Clustering

```
                     TURN / FLOP / PREFLOP CLUSTERING
                     (layer.rs Layer::cluster())

   +-----------------------------------------------------------------------+
   | STEP 1: LOAD DEPENDENCIES                                             |
   |         Layer::build() loads from postgres                            |
   |                                                                       |
   |    Metric::from_street(next_street)  --> pairwise abstraction distances
   |    Lookup::from_street(next_street)  --> iso->abs mappings            |
   +-----------------------------------------------------------------------+
                        |
                        v
   +-----------------------------------------------------------------------+
   | STEP 2: BUILD HISTOGRAMS                                              |
   |         lookup.projections() for each isomorphism                     |
   |                                                                       |
   |   foreach Isomorphism on this street:                                 |
   |       +------------------------------------------------------------+  |
   |       | iso.children()  // all possible next-street observations   |  |
   |       |      |                                                     |  |
   |       |      v                                                     |  |
   |       | map to abstractions via loaded Lookup                      |  |
   |       |      |                                                     |  |
   |       |      v                                                     |  |
   |       | collect into Histogram (distribution over abstractions)    |  |
   |       +------------------------------------------------------------+  |
   +-----------------------------------------------------------------------+
                        |
                        v
   +-----------------------------------------------------------------------+
   | STEP 3: K-MEANS++ INITIALIZATION (elkan.rs init_kmeans)               |
   |                                                                       |
   |   1. Sample first centroid uniformly from dataset                     |
   |   2. For each remaining centroid:                                     |
   |      - Compute D(x)^2 = min distance to existing centroids            |
   |      - Sample next centroid with probability proportional to D(x)^2   |
   |   3. Repeat until K centroids                                         |
   +-----------------------------------------------------------------------+
                        |
                        v
   +-----------------------------------------------------------------------+
   | STEP 4: ELKAN K-MEANS ITERATIONS (elkan.rs next_eklan)                |
   |                                                                       |
   |   for t iterations:                                                   |
   |       +------------------------------------------------------------+  |
   |       | a. Compute pairwise centroid distances                     |  |
   |       | b. Compute midpoints s(c) = 1/2 min_{c'!=c} d(c,c')        |  |
   |       | c. Exclude points where upper_bound <= s(c) (triangle ineq)|  |
   |       | d. For remaining points, update bounds and assignments     |  |
   |       | e. Recompute centroids from assignments (Absorb trait)     |  |
   |       | f. Shift bounds by centroid drift                          |  |
   |       +------------------------------------------------------------+  |
   +-----------------------------------------------------------------------+
                        |
                        v
   +-----------------------------------------------------------------------+
   | STEP 5: PRODUCE ARTIFACTS & STREAM TO POSTGRES                        |
   |                                                                       |
   |   layer.lookup()  --> isomorphism table (obs, abs)                    |
   |   layer.metric()  --> metric table (xor, dx)                          |
   |   layer.future()  --> transitions table (prev, next, dx)              |
   +-----------------------------------------------------------------------+
```

### Data Flow Through Tables

```
                        CLUSTERING DATA FLOW

    Isomorphism                    K-Means                       PostgreSQL
    Space                          Clustering                    Tables
    ----------                     ----------                    ------

    River (123M)
         |
         | equity()
         v
    +----------+
    |0-100 eqty|--------------------------------------------------------> isomorphism
    +----------+                                                            (obs, abs)
         |
    =====|=====================================================================
         |
    Turn (14M)
         |
         | children() + lookup
         v
    +--------------+      +---------------+
    | Histogram    |------| Elkan K-Means |
    | per iso      |      | K=144, EMD    |
    +--------------+      +-------+-------+
                                  |
                    +-------------+-------------+
                    v             v             v
               isomorphism    metric      transitions
               (obs, abs)    (xor, dx)   (prev,next,dx)
                    |
    ================|==========================================================
                    |
    Flop (1.3M)     |
         |          | load
         | children() + lookup
         v          v
    +--------------+      +---------------+
    | Histogram    |------| Elkan K-Means |<--- metric (turn)
    | per iso      |      | K=128, EMD    |
    +--------------+      +-------+-------+
                                  |
                    +-------------+-------------+
                    v             v             v
               isomorphism    metric      transitions
                    |
    ================|==========================================================
                    |
    Preflop (169)   |
         |          | load
         | children() + lookup
         v          v
    +--------------+      +---------------+
    | Histogram    |------| 1:1 mapping   |<--- metric (flop)
    | per iso      |      | K=169         |
    +--------------+      +-------+-------+
                                  |
                    +-------------+-------------+
                    v             v             v
               isomorphism    metric      transitions
```

---

## Phase 2: Blueprint Training

**Entry Point:** `Trainer::train()` in `src/autotrain/trainer.rs`

```
                           BLUEPRINT TRAINING

                        Trainer::train()
                              |
                              | first: cluster() if needed
                              v
                 +------------------------+
                 |   require_clustering   |
                 |   PreTraining::run()   |
                 +-----------+------------+
                              |
                              | then: training loop
                              v
           +------------------+------------------+
           |                                     |
           v                                     v
    +-----------------+                  +-----------------+
    |   FastSession   |                  |   SlowSession   |
    |   (--fast)      |                  |   (--slow)      |
    +--------+--------+                  +--------+--------+
             |                                    |
             v                                    v
    +-----------------+                  +-----------------+
    |   NlheSolver    |                  |      Pool       |
    |   (in-memory)   |                  |  (distributed)  |
    +--------+--------+                  +--------+--------+
             |                                    |
             |                                    |
    =========|====================================|===========================
             |        TRAINING LOOP               |
    =========|====================================|===========================
             |                                    |
             | loop {                             | loop {
             |   solver.step()                    |   pool.step().await
             |   checkpoint()                     |   checkpoint()
             |   if Q+Enter: break                |   if Q+Enter: break
             | }                                  | }
             |                                    |
    =========|====================================|===========================
             |           SYNC                     |
    =========|====================================|===========================
             |                                    |
             v                                    v
    +-----------------+                  +-----------------+
    | client.stage()  |                  |    (no-op)      |
    | COPY rows       |                  |   direct writes |
    | client.merge()  |                  |   to blueprint  |
    | client.stamp(n) |                  |                 |
    +--------+--------+                  +--------+--------+
             |                                    |
             +--------------+---------------------+
                            v
                   +----------------+
                   |   PostgreSQL   |
                   |   ----------   |
                   |   blueprint    |
                   |   epoch        |
                   +----------------+
```

### Fast vs Slow Mode Comparison

```
+--------------------------------+--------------------------------+
|         FAST MODE              |         SLOW MODE              |
|         (fast.rs)              |         (slow.rs)              |
+--------------------------------+--------------------------------+
|                                |                                |
|  NlheSolver                    |  Pool<Worker<Postgres>>        |
|      |                         |      |                         |
|      v                         |      v                         |
|  +--------------+              |  +--------------+              |
|  |  BTreeMap    |              |  |  Worker 1    |--+           |
|  |  ----------  |              |  +--------------+  |           |
|  |  regret[k]   |              |  |  Worker 2    |--+-- async   |
|  |  policy[k]   |              |  +--------------+  |   queries |
|  |  (in-memory) |              |  |  Worker N    |--+           |
|  +--------------+              |  +--------------+              |
|         |                      |         |                      |
|         | step() is sync       |         | step() is async      |
|         | (spawn_blocking)     |         | (tokio)              |
|         v                      |         v                      |
|  +--------------+              |  +--------------+              |
|  |   100x       |              |  |   direct     |              |
|  |   faster     |              |  |   DB writes  |              |
|  |   single-box |              |  |   scales out |              |
|  +--------------+              |  +--------------+              |
|         |                      |         |                      |
|         | on graceful exit     |         | (no sync needed)     |
|         v                      |         v                      |
|  +--------------+              |  +--------------+              |
|  | sync():      |              |  |   already    |              |
|  |  stage()     |              |  |   persisted  |              |
|  |  COPY bulk   |              |  |              |              |
|  |  merge()     |              |  |              |              |
|  |  stamp(n)    |              |  |              |              |
|  +--------------+              |  +--------------+              |
|                                |                                |
|  * 100x more efficient         |  * Scales horizontally         |
|  * Memory-bound                |  * I/O-bound                   |
|  * Single machine              |  * Multi-machine ready         |
|                                |                                |
+--------------------------------+--------------------------------+
```

Both modes implement the `Trainer` trait for polymorphic training:

```rust
#[async_trait]
pub trait Trainer: Send + Sync + Sized {
    fn client(&self) -> &Arc<Client>;
    async fn sync(self);
    async fn step(&mut self);
    async fn epoch(&self) -> usize;
    async fn summary(&self) -> String;
    async fn checkpoint(&self) -> Option<String>;
}
```

---

## Database Schema

### Core Tables

```
                          POSTGRES TABLES

+-----------------------------------------------------------------------------+
|                          CLUSTERING TABLES                                  |
+-----------------------------------------------------------------------------+
|                                                                             |
|  isomorphism (~139M rows)         |  metric (~40K rows)                     |
|  ---------------------            |  -----------------                      |
|  obs   BIGINT  --> Isomorphism    |  xor   BIGINT --> Pair(abs1 ^ abs2)     |
|  abs   BIGINT  --> Abstraction    |  dx    REAL   --> EMD distance          |
|                                   |                                         |
|  * Maps every isomorphic hand     |  * Pairwise abstraction distances       |
|    to its abstraction bucket      |  * Used by previous street's EMD        |
|                                   |                                         |
+-----------------------------------+-----------------------------------------+
|                                   |                                         |
|  transitions (~29K rows)          |  epoch (1 row)                          |
|  -----------------------          |  ------------                           |
|  prev  BIGINT  --> Abstraction    |  key   TEXT   = 'current'               |
|  next  BIGINT  --> Abstraction    |  value BIGINT --> iteration count       |
|  dx    REAL    --> weight         |                                         |
|                                   |  * Training progress counter            |
|  * Distribution over next-street  |                                         |
|    abstractions per abstraction   |                                         |
|                                   |                                         |
+-----------------------------------+-----------------------------------------+

+-----------------------------------------------------------------------------+
|                          BLUEPRINT TABLE                                    |
+-----------------------------------------------------------------------------+
|                                                                             |
|  blueprint (~200M+ rows, grows with training)                               |
|  -------------------------------------------                                |
|  past    BIGINT  --> past abstraction path                                  |
|  present BIGINT  --> current abstraction                                    |
|  future  BIGINT  --> future abstraction path                                |
|  edge    BIGINT  --> action encoding                                        |
|  policy  REAL    --> strategy probability                                   |
|  regret  REAL    --> cumulative regret                                      |
|                                                                             |
|  * MCCFR strategy stored per information set                                |
|  * Upserted via staging table on graceful exit (FastSession)                |
|  * Written directly by workers (SlowSession)                                |
|                                                                             |
+-----------------------------------------------------------------------------+
```

### Derived Tables

| Table         | Columns                           | Rows | Description                   |
| ------------- | --------------------------------- | ---- | ----------------------------- |
| `abstraction` | `abs, street, population, equity` | 542  | Summary stats per abstraction |
| `street`      | `street, nobs, nabs`              | 4    | Summary stats per street      |

---

## Streaming Protocol

All data uses **PostgreSQL binary COPY** in 100k row chunks via `Streamable` trait:

```
                         BINARY COPY STREAMING

   impl Streamable for T
        |
        v
   +----------------+      +----------------+      +----------------+
   |  T::rows()     |----->| BinaryCopyIn   |----->|   PostgreSQL   |
   |  iterator      |      | Writer         |      |   table        |
   +----------------+      +----------------+      +----------------+

   Implementors:
   * Lookup  (isomorphism table)
   * Metric  (metric table)
   * Future  (transitions table)
   * Profile (blueprint table via staging)
```

---

## Resumability

| Feature           | Mechanism                                           |
| ----------------- | --------------------------------------------------- |
| Progress tracking | Queries `isomorphism` table to check completion     |
| Partial cleanup   | `truncate_street()` clears data before re-uploading |
| Epoch persistence | `epoch` table tracks MCCFR iteration count          |
| Graceful shutdown | Press `Q + Enter` to finish current batch and sync  |

---

## Key Insight

> Clustering flows **backwards** (river->preflop) because each street's abstraction depends on the _next_ street's distribution, while training flows **forwards** through the game tree building blueprint strategies via MCCFR iterations.
