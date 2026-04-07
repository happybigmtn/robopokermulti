# Robopokermulti Foundation Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Fork `robopoker` into a new `robopokermulti` codebase that can train, store, serve, and validate blueprint strategies for multiway NLHE cash games up to 10 seats, with tournament support added on top of the same foundation.

**Architecture:** Keep the current heads-up `robopoker` repo as the stable baseline and create `robopokermulti` as a hard fork after the audit-defined cut line. The new repo should unify the gameplay engine, replace heads-up-shaped abstractions and infosets with explicitly multiway ones, make profile-aware training and inference mandatory, and then extend that foundation to tournament utilities and tournament state.

**Tech Stack:** Rust, PostgreSQL, Rayon, Tokio, MCCFR/external sampling, hierarchical clustering, profile-scoped training tables.

## Living Status

This document is the active execution record for `robopokermulti`. Update it
as work lands so the current state is visible without reconstructing it from
chat history.

**Last Updated:** 2026-04-05

### Current Focus

- Keep the 3-max clustering run alive and record training readiness milestones.
- Finish the non-solver multiway foundation around analysis, hosting, and room
  lifecycle.
- Delay broad automation until seat ownership and admission semantics are
  explicit enough to avoid auto-generating the wrong architecture.
- Keep the hosting/runtime plan truthful as seat admission moves from
  room-scoped assumptions toward per-seat connection state.

### Completed Recently

- `Task 1` completed:
  - `robopokermulti` remote created from the tagged HU baseline.
  - baseline tag `robopoker-hu-baseline` published.
- `Task 2` completed:
  - fork docs created: `README.md`, `docs/ARCHITECTURE.md`,
    `docs/FORK_POLICY.md`, `docs/ROADMAP.md`.
- `Task 3` materially completed:
  - canonical engine path is now `Game`.
  - legacy `MultiwayGame` module removed from the active runtime surface.
  - stale multiway/HU contradiction in `recall.rs` removed.
- `Task 7` materially advanced:
  - seat-aware abstraction lookup is now required for versioned abstractions.
  - profile-aware hydrate paths were added for encoder/profile/solver.
  - fast training no longer hardcodes the HU default path for the new profile
    lane.
- `Task 14` materially advanced:
  - analysis API is now profile-aware instead of defaulting to static HU tables.
  - observation-style analysis queries now carry seat context.
  - policy lookup uses config-aware recall reconstruction.
- New hosting/runtime foundation landed outside the original plan wording:
  - `/start` accepts table config and returns the actual room config.
  - rooms preserve configured blinds/antes/stacks across recalls.
  - structured table-state and decision payloads now expose seat/table context.
  - reconnect replay, single-active-websocket enforcement, and room close now
    behave predictably.
  - room access now uses typed auth errors instead of string-matched `anyhow`
    failures.
  - room ownership state is now stored on the room handle instead of being
    re-invented in each endpoint.
  - room access is now seat-scoped at the API boundary: room status, enter, and
    leave require both `seat` and `token`, and the backend rejects token reuse
    for the wrong seat.
  - room creation now accepts an explicit `owned_seat`, so the single-human
    hosting mode no longer assumes the human is always seated at index `0`.
  - hosting payloads now expose plural ownership metadata (`owned_seats`) and
    derive `seat_roles` from that list, which keeps the current one-human mode
    working while removing another hidden assumption from future multi-human
    admission work.
  - room ownership is now stored internally as seat-access records rather than a
    single `(owned_seat, token)` pair, so the backend state shape is ready for
    multi-seat admission even though only one owned seat is created today.
  - hosting payloads now expose explicit `seat_assignments` records with seat
    kinds, so clients can consume a stable assignment contract instead of
    reconstructing ownership from `seat_roles` alone.
  - seat assignments are now stored on the room handle and propagated through
    room start/snapshot responses, so assignment metadata no longer depends on
    recomputing from `owned_seats` at every read site.
  - actor replies now derive the acting seat from the live `Recall` turn
    position instead of a fixed actor id, which removes a blocker for future
    multiplexed hosted clients owning more than one seat.
  - hosted client transports can now represent plural owned seats internally,
    and secondary seat actors suppress duplicate public event emission, so one
    websocket/session can truthfully sit behind more than one seat-bound client
    actor without double-sending table-state/play updates.
  - room handles can now create multiple seat-bound hosted clients over shared
    transport channels via `pair_with_owned_seats`, which is the first honest
    step toward seat-claim admission without yet claiming full multi-human room
    support.
  - hosted room bootstrap is now truly multi-seat-aware:
    - `/start` accepts `owned_seats` as a real multi-seat bootstrap contract
    - rooms can start with multiple human-owned seats over one shared hosted
      transport
    - start/status responses now expose per-seat credentials through
      `seat_accesses` instead of a misleading single room token
    - wrong-seat auth now validates against the full owned-seat set instead of
      treating the first owned seat as the only legitimate one
  - hosted rooms now support claimable open seats:
    - `/start` can declare `open_seats`
    - open seats are backed by hosted seat actors from hand 1 instead of Fish
      placeholders
    - unclaimed open seats act like bots until claimed
    - post-start seat claims promote the existing open seat actor to a human
      seat in place, mint a new seat token, and update room snapshots/payload
      metadata without replacing the actor slot
  - open-seat admission is now available to new humans directly:
    - `/join/{room_id}` can claim an explicitly open seat without requiring an
      existing owner token
    - owner-driven `claim` and open-seat `join` now share the same in-place
      seat-promotion path
    - the transport/runtime model now distinguishes three truthful states per
      hosted seat: `human`, `open`, and `bot`
  - room lifecycle semantics are less self-contradictory now:
    - `/leave/{room_id}` releases only the authenticated seat instead of
      destroying the whole room
    - released human seats revert to `open` and can be joined again later
    - `/close/{room_id}` is now the explicit full-room shutdown path
  - seat-scoped hosted transport state is now restored and active:
    - `RoomHandle` again owns the room lifecycle state after an interrupted
      refactor wiped `handle.rs`
    - each hosted seat now has its own bridge channels, cache, and connected
      flag instead of sharing a single room-wide transport state
    - `/room/{room_id}` and `/enter/{room_id}` now read/write the queried
      seat's transport state instead of a room-scoped cache/connection bit
    - released seats now signal a per-seat bridge disconnect instead of only
      flipping shared connection state, so a live websocket is asked to exit
      when ownership is dropped
    - released and re-claimed seats now reset their per-seat cache state, so a
      new owner does not inherit stale cached table/decision payloads that were
      generated under the previous ownership view
    - seat access is now an explicit session contract, not just a raw token:
      each `SeatAccess` carries a per-seat `generation`, so claim/reconnect
      cycles are visible in the API instead of only inferable from token
      rotation
    - public room discovery is now explicit:
      - `GET /lobby/{room_id}` exposes public room metadata without seat tokens
      - joiners can now discover table config, open seats, claimed seats, and
        current seat assignments from the hosting API itself instead of relying
        on out-of-band room/seat knowledge
    - invite/share and resume hints are now explicit in the response contract:
      - owned-seat responses now include `seat_sessions` with `status_path`,
        `enter_path`, `leave_path`, `close_path`, and the seat/token query shape
      - lobby and room responses now include `join_options`, so clients can
        discover the exact join request shape for each open seat without
        reverse-engineering the API
    - those hints are now also promoted into grouped top-level objects:
      - `resume` wraps the per-seat session contract for owned seats
      - `invitation` wraps the public lobby path plus open-seat join options
      - clients no longer need to infer which low-level fields belong together
        when building reconnect or share flows
    - seat-scoped reconnect auth is now explicit:
      - owner-authenticated hosting operations now validate `seat`,
        `generation`, and `token` together instead of treating generation as
        display-only metadata
      - stale seat generations are rejected before status/enter/leave/close or
        owner-driven claim can reuse a seat session
      - direct tests now cover stale-generation rejection, generation rotation
        on release/rejoin, and room-close invalidation of owned seat sessions
    - hosted rooms now mint a durable invitation id:
      - each room gets a stable `invitation_id`
      - `invitation` now exposes both `invite_path` and `lobby_path`
      - `GET /invite/{invitation_id}` resolves directly to the public room view,
        so sharing no longer depends on leaking or preserving the raw room id
    - current remaining edge: the invite/discovery contract is durable-only for
      now; whether invitations should become revocable/rotatable objects is
      still an explicit follow-on product decision

### In Progress

- `Task 9` in progress:
  - canonical 3-max profile: `bp_3max_local`
  - abstraction version: `abs_v4_p3`
  - live clustering target: `isomorphism_abs_v4_p3`
  - current milestone already proven:
    - profile-aware trainer boot
    - profile table creation
    - seat-conditioned river COPY ingestion with version-defined seat lookup
  - current live status at last check:
    - trainer PID `1117989`
    - `target/debug/trainer --cluster`
    - `104,250,000` rows committed in `isomorphism_abs_v4_p3`
- `Task 6` still open:
  - the abstraction lookup is seat-aware, but blueprint infoset identity is
    still too thin.
  - see `docs/plans/2026-04-05-infoset-context-audit.md`.

### Active Next Queue

Manual now:
1. Decide whether hosted reconnect needs a first-class client resume flow beyond
   the current seat/token/generation contract.
2. Decide whether the durable `invitation_id` contract is enough for sharing or
   whether invitations should become revocable/rotatable objects.
3. Extend the open-seat admission flow into fuller multi-human operator/client
   UX now that the seat-scoped ownership contract is explicit.
4. Finish the infoset identity redesign (`Info v2`) before widening automation.
5. Keep monitoring the 3-max clustering run and record completion/failure data.

Safe for automation later:
1. Repetitive profile/table plumbing propagation.
2. Additional targeted tests across analysis and hosting payloads.
3. Validation doc generation and benchmark write-ups.
4. Mechanical docs synchronization after the architecture is stable.

### Status By Task

- `Task 1`: done
- `Task 2`: done
- `Task 3`: done enough to treat the engine unification slice as complete
- `Task 4`: not started
- `Task 5`: partially done
- `Task 6`: in progress
- `Task 7`: in progress
- `Task 8`: partially done
- `Task 9`: in progress
- `Task 10`: not started
- `Task 11`: not started
- `Task 12`: not started
- `Task 13`: not started
- `Task 14`: in progress
- `Task 15`: not started
- `Task 16`: not started
- `Task 17`: done

---

## Why A Separate Repo

`robopoker` already contains useful multiway scaffolding, but it is not a coherent multiway product line yet.

- `Game` claims to be multiway-capable in [src/gameplay/game.rs](/home/r/coding/bitpoker/vendor/robopoker/src/gameplay/game.rs), while a second engine still exists in [src/gameplay/multiway.rs](/home/r/coding/bitpoker/vendor/robopoker/src/gameplay/multiway.rs).
- The current abstraction stack is still heads-up-shaped:
  - `Observation::equity()` compares hero against one villain hand in [src/cards/observation.rs](/home/r/coding/bitpoker/vendor/robopoker/src/cards/observation.rs).
  - `Info` only keys on `history`, `present`, and `choices` in [src/mccfr/nlhe/info.rs](/home/r/coding/bitpoker/vendor/robopoker/src/mccfr/nlhe/info.rs).
  - `Path` truncates to `MAX_DEPTH_SUBGAME = 16` in [src/gameplay/path.rs](/home/r/coding/bitpoker/vendor/robopoker/src/gameplay/path.rs) and [src/lib.rs](/home/r/coding/bitpoker/vendor/robopoker/src/lib.rs).
- Multiway profile support exists, but fast training and several load/query paths still default back to heads-up tables in:
  - [src/autotrain/fast.rs](/home/r/coding/bitpoker/vendor/robopoker/src/autotrain/fast.rs)
  - [src/autotrain/mode.rs](/home/r/coding/bitpoker/vendor/robopoker/src/autotrain/mode.rs)
  - [src/mccfr/nlhe/profile.rs](/home/r/coding/bitpoker/vendor/robopoker/src/mccfr/nlhe/profile.rs)
  - [src/analysis/api.rs](/home/r/coding/bitpoker/vendor/robopoker/src/analysis/api.rs)
- Tournament support today is payout-aware, not tournament-complete, in [src/gameplay/tournament.rs](/home/r/coding/bitpoker/vendor/robopoker/src/gameplay/tournament.rs) and [src/save/tables.rs](/home/r/coding/bitpoker/vendor/robopoker/src/save/tables.rs).

This is too much foundational change to hide behind feature flags without damaging the heads-up line. The fork is the right boundary.

## Founding Rules

1. `robopoker` remains the heads-up baseline and bug-fix upstream.
2. `robopokermulti` starts from a tagged baseline and accepts cherry-picked upstream fixes only.
3. Multiway cash comes before tournament state.
4. Tournament utilities come before tournament lifecycle.
5. No 10-seat rollout until 3-seat and 6-seat validation gates pass.
6. Position-aware behavior must be part of the abstraction contract, not an optional env toggle.

## Success Criteria

### Phase A Success

- One canonical gameplay engine supports 2-10 players.
- No contradictory "HU only" comments or codepaths remain in the active engine/train path.

### Phase B Success

- A new abstraction version can express seat-relative multiway information.
- A new infoset key can distinguish materially different multiway states.
- Profile-aware clustering, training, and querying work without fallback to static HU tables.

### Phase C Success

- 3-max and 6-max cash blueprints train end-to-end with reproducible profile configs.
- 10-max cash training runs complete with acceptable DB growth and convergence signals.

### Phase D Success

- Tournament utilities and blind schedules are integrated into the same profile-native training system.
- Tournament state extensions are explicitly modeled, tested, and benchmarked.

## Repo Split Plan

### Task 1: Freeze The Fork Boundary `[done]`

**Files:**
- Modify: `README.md`
- Create: `docs/plans/2026-04-05-robopokermulti-foundation.md`

**Steps:**
1. Tag the current `robopoker` commit as the heads-up baseline.
   Run: `git tag robopoker-hu-baseline`
2. Add a short note in `README.md` explaining that multiplayer and tournament research will move to `robopokermulti`.
3. Create the remote `robopokermulti` repository from this baseline tag, not from a moving branch.
4. Record the fork policy in the new repo:
   - upstream bug fixes are cherry-picked
   - no long-running rebase expectation
   - no compatibility shims back into the heads-up line

### Task 2: Create The New Repo Skeleton `[done]`

**Files:**
- Create in new repo: `README.md`
- Create in new repo: `docs/ARCHITECTURE.md`
- Create in new repo: `docs/plans/`
- Create in new repo: `docs/validation/`

**Steps:**
1. Copy the baseline source tree into `robopokermulti`.
2. Update top-level docs to define the new scope:
   - multiway cash blueprint training
   - 10-seat target
   - tournament extension later
3. Add a top-level capability matrix:
   - supported now
   - under construction
   - explicitly deferred

## Engine Foundation

### Task 3: Choose One Canonical Gameplay Engine `[done]`

**Files:**
- Modify: `src/gameplay/game.rs`
- Modify: `src/gameplay/multiway.rs`
- Modify: `src/gameplay/mod.rs`
- Modify: `src/gameplay/recall.rs`
- Test: `src/gameplay/game.rs`
- Test: `src/gameplay/multiway.rs`
- Test: `src/gameplay/recall.rs`

**Steps:**
1. Decide whether `Game` absorbs all multiway behavior or `MultiwayGame` replaces `Game`.
   Recommendation: keep `Game` as the canonical type because MCCFR already targets it.
2. Delete or quarantine the non-canonical engine path.
3. Move all remaining blind posting, occupancy, actor order, and street progression logic into the canonical engine.
4. Update `Recall` so it truthfully supports the canonical multiway engine.
5. Remove stale tests and comments that still describe multiway as unsupported.

**Verification:**
- `cargo test gameplay::game`
- `cargo test gameplay::multiway`
- `cargo test gameplay::recall`

### Task 4: Lock Down Cash-Game Multiway Semantics `[pending]`

**Files:**
- Modify: `src/gameplay/game.rs`
- Modify: `src/gameplay/showdown.rs`
- Test: `src/gameplay/game.rs`
- Test: `src/gameplay/showdown.rs`

**Steps:**
1. Add explicit tests for:
   - 3-seat and 10-seat posting order
   - short-stacked blinds
   - folded-seat skipping
   - multiway showdown settlement
   - side-pot handling at 3, 6, and 10 seats
2. Ensure `Game::payoff`, terminal settlement, and seat rotation remain valid for all supported seat counts.
3. Benchmark any paths that degrade sharply with seat count.

## Abstraction Redesign

### Task 5: Replace Heads-Up River Semantics `[in progress]`

**Files:**
- Modify: `src/cards/observation.rs`
- Modify: `src/gameplay/abstraction.rs`
- Modify: `src/clustering/lookup.rs`
- Modify: `src/clustering/layer.rs`
- Test: `src/cards/observation.rs`
- Test: `src/clustering/lookup.rs`

**Steps:**
1. Stop using single-villain showdown equity as the canonical river bucket signal.
2. Design a multiway feature vector for bucket assignment. Minimum inputs:
   - seat-relative position
   - active player count
   - hero hand strength class
   - board texture class
   - multi-opponent equity or approximate win/tie distribution
3. Introduce a new abstraction version family, for example `abs_v4_p{n}`.
4. Make clustering produce genuinely seat-conditioned rows, not duplicated `(obs, abs)` rows.
5. Remove the env-based ambiguity around whether position-aware lookup is enabled.

**Exit Condition:**
- A clustered `isomorphism_<version>` table represents real multiway abstraction differences per seat position.

### Task 6: Redesign Infoset Identity `[in progress]`

**Files:**
- Modify: `src/mccfr/nlhe/info.rs`
- Modify: `src/gameplay/path.rs`
- Modify: `src/gameplay/edge.rs`
- Modify: `src/workers/worker.rs`
- Modify: `src/mccfr/nlhe/profile.rs`
- Test: `src/mccfr/nlhe/info.rs`
- Test: `src/workers/worker.rs`

**Steps:**
1. Define a new infoset schema that explicitly includes more than `(history, present, choices)`.
2. Add explicit fields or a packed representation for:
   - seat-relative position
   - active-player count
   - stack band / effective-stack band
   - pot-size band
   - blind/ante band
   - action-history features that survive beyond 16 half-bytes
3. Decide whether `Path(u64)` survives as a compact suffix only, or gets replaced.
4. Version the blueprint table schema if the key changes.

**Exit Condition:**
- Two strategically different 10-max spots no longer collide just because they share the same board bucket and short suffix history.

## Training Plumbing

### Task 7: Make Profile-Native Training Mandatory `[in progress]`

**Files:**
- Modify: `src/autotrain/mode.rs`
- Modify: `src/autotrain/fast.rs`
- Modify: `src/autotrain/slow.rs`
- Modify: `src/autotrain/trainer.rs`
- Modify: `src/mccfr/nlhe/solver.rs`
- Modify: `src/mccfr/nlhe/profile.rs`
- Modify: `src/database/source.rs`
- Modify: `src/database/sink.rs`
- Modify: `src/database/stage.rs`
- Test: `src/autotrain/`
- Test: `src/database/`

**Steps:**
1. Remove any path that silently falls back to default heads-up tables during multiway runs.
2. Add profile-aware hydrate/load APIs for solver, encoder, and profile state.
3. Extend `FastSession` so profile-native non-HU training works locally.
4. Make profile metadata required for all new `robopokermulti` runs.
5. Replace env-driven `POSITION_AWARE` branching with abstraction-version-defined behavior.

**Verification:**
- run a 3-seat fast local smoke training
- run a 3-seat slow profile-aware training
- confirm profile and abstraction tables are the only ones touched

### Task 8: Harden Postgres Schema For Multiway Scale `[in progress]`

**Files:**
- Modify: `src/save/tables.rs`
- Modify: `src/save/postgres/connect.rs`
- Modify: `src/save/postgres/row.rs`
- Modify: `src/database/check.rs`
- Modify: `src/database/source.rs`
- Modify: `src/database/sink.rs`
- Test: `src/save/tables.rs`
- Test: `src/database/check.rs`

**Steps:**
1. Version blueprint schemas when the infoset key changes.
2. Revisit indices for 10-max lookup and blueprint serving patterns.
3. Add explicit migration tests for:
   - profile table creation
   - abstraction table creation
   - position-aware lookup rows
   - large COPY ingestion
4. Add row-count and integrity checks for clustered abstractions at each seat count.

## Cash Blueprint Rollout

### Task 9: Ship 3-Max As The First Real Validation Gate `[in progress]`

**Files:**
- Modify: `AUTOTRAIN.md`
- Create: `docs/validation/3max-cash.md`
- Test: relevant gameplay, clustering, and training modules

**Steps:**
1. Define one canonical 3-max cash profile and abstraction version.
2. Train it end-to-end with fast and slow modes.
3. Validate:
   - clustering completion
   - blueprint table growth
   - strategy query correctness
   - reproducibility across restarts
4. Record benchmark numbers and failure modes.

### Task 10: Expand To 6-Max `[pending]`

**Files:**
- Modify: `AUTOTRAIN.md`
- Create: `docs/validation/6max-cash.md`

**Steps:**
1. Repeat the 3-max validation flow at 6 seats.
2. Measure how DB size, clustering time, and training throughput scale.
3. Tune abstraction counts, batch size, and traversal settings only after data is collected.
4. Do not start 10-max until 6-max stability is acceptable.

### Task 11: Expand To 9/10-Max `[pending]`

**Files:**
- Modify: `AUTOTRAIN.md`
- Create: `docs/validation/10max-cash.md`
- Modify: `src/lib.rs`
- Modify: training benchmarks as needed

**Steps:**
1. Run a constrained 9-max or 10-max pilot with production-like configs.
2. Measure:
   - isomorphism table row growth
   - blueprint row growth
   - epoch throughput
   - memory pressure
   - strategy lookup latency
3. Re-tune only after identifying the dominant bottlenecks.
4. Lock a supported 10-max configuration before calling the system viable.

## Tournament Extension

### Task 12: Separate Tournament Utility Support From Tournament State `[pending]`

**Files:**
- Modify: `src/gameplay/tournament.rs`
- Modify: `src/save/tables.rs`
- Modify: `AUTOTRAIN.md`
- Create: `docs/validation/tournament-utility.md`

**Steps:**
1. Keep current payout-curve support as the first tournament layer.
2. Validate tournament-utility training on single-table frozen-state samples.
3. Document clearly that this is not yet full tournament lifecycle support.

### Task 13: Design True Tournament State `[pending]`

**Files:**
- Create: `docs/ARCHITECTURE.md`
- Create: `docs/plans/<future tournament plan>.md`
- Modify later: `src/gameplay/`
- Modify later: `src/autotrain/`

**Steps:**
1. Decide whether tournament state belongs inside the core game object or in a wrapper state machine.
2. Model:
   - eliminations
   - blind-level advancement
   - table balancing
   - seat redraw/final table
   - payout boundary transitions
3. Only after that design is stable should tournament blueprint work move beyond utility overlays.

## Serving And Analysis

### Task 14: Make Strategy Queries Profile-Native `[in progress]`

**Files:**
- Modify: `src/analysis/api.rs`
- Modify: `src/analysis/server.rs`
- Modify: `src/database/source.rs`
- Test: `src/analysis/`

**Steps:**
1. Stop defaulting to static HU tables in the analysis API.
2. Require callers to specify profile and abstraction version.
3. Make strategy and abstraction lookups deterministic for multiway profiles.
4. Add seat-relative query tests so served policy matches the trained profile.

## Benchmarking And Training Science

### Task 15: Build A Real Training Benchmark Matrix `[pending]`

**Files:**
- Create: `benches/multiway_training.rs`
- Create: `docs/validation/benchmark-matrix.md`
- Modify: `AUTOTRAIN.md`

**Steps:**
1. Benchmark clustering and training across 3, 6, and 10 seats.
2. Compare:
   - abstraction versions
   - batch sizes
   - tree counts
   - DB footprint
3. Record convergence proxies, not just throughput.
4. Use the benchmark matrix to decide default production profiles.

### Task 16: Add Blueprint Quality Evaluation `[pending]`

**Files:**
- Create: `src/bin/eval.rs` or equivalent
- Create: `docs/validation/quality-gates.md`
- Modify: `src/analysis/`

**Steps:**
1. Define measurable quality gates:
   - self-play stability
   - exploitability proxy
   - policy smoothness across adjacent states
   - restart determinism
2. Automate those checks for 3-max and 6-max before attempting 10-max.

## Documentation And Governance

### Task 17: Document The Fork Contract `[done]`

**Files:**
- Modify in new repo: `README.md`
- Create in new repo: `docs/FORK_POLICY.md`
- Create in new repo: `docs/ROADMAP.md`

**Steps:**
1. Explain what `robopokermulti` is and is not.
2. Declare the baseline tag it forked from.
3. Define how upstream bug fixes are imported.
4. Define which features belong only in the multiway repo.

## Initial Recommended Sequence

1. Tag `robopoker` baseline and create `robopokermulti`.
2. Unify the gameplay engine.
3. Redesign abstraction semantics.
4. Redesign infoset identity.
5. Make training and inference fully profile-native.
6. Validate 3-max cash.
7. Validate 6-max cash.
8. Attempt 10-max cash.
9. Only then extend tournament support.

## Explicit Non-Goals For The First Fork Window

- No attempt to keep `robopoker` and `robopokermulti` structurally identical.
- No compatibility layer that preserves old heads-up abstraction schemas in the new repo.
- No full tournament lifecycle until 10-max cash blueprint training is stable.
- No bot/PvE work inside this first multiway-training foundation phase.

## Recommended First Execution Slice

If work starts immediately, the best first slice is:

1. Create `robopokermulti` from the baseline tag.
2. Delete or quarantine the duplicate non-canonical engine path.
3. Remove the `POSITION_AWARE` optionality and turn it into a versioned abstraction contract.
4. Write a design doc for the new multiway abstraction and infoset schema before changing trainer code.

Plan complete and saved to `docs/plans/2026-04-05-robopokermulti-foundation.md`. Two execution options:

1. Subagent-Driven (this session) - implement the first slice here in small reviewed steps.
2. Parallel Session (separate) - open a fresh session in the future `robopokermulti` repo and execute this as the founding roadmap.
