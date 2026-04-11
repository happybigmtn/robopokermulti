# Tournament Utility Layer Boundary

## What This Repo Provides Today

The current `robopokermulti` tournament support now has **two distinct
surfaces**:

- **Payout-curve utilities**: `TournamentPayout` normalizes and splits payouts
  by final-stack ranking, including tie-splitting for equal stacks
  (`src/gameplay/tournament.rs`).

- **Tournament-format profile configuration**: `TrainingProfileConfig` accepts
  `"format": "tournament"` with explicit `blind_schedule`, `payout_curve`,
  optional `stack_bb_range`, and per-level `weight`/`duration` sampling
  (`src/save/tables.rs`).

- **Profile-native training on frozen-state samples**: Tournament training runs
  use the same V2 schema and profile-scoped tables as cash training. Epoch
  sampling draws blind levels proportionally from the schedule, producing
  deterministic table configurations per epoch.

- **Shared tournament state and lifecycle control**:
  `src/gameplay/tournament_state.rs` now carries explicit tournament
  registration, blind-level progression, between-hand balancing, break/pause,
  final-table collapse, elimination records, completion, and serializable
  operator/player views above any one hand transcript.

## What This Repo Still Does NOT Provide

The following are still explicitly out of scope for the live product/runtime
surface:

- **Live tournament runtime orchestration**: No hosted or networked
  tournament-mode room runtime exists yet. The lifecycle code is a shared
  state/controller layer, not a complete operator service or player product.

- **Tournament-specific UI/runtime plumbing**: No dedicated tournament TUI,
  transport, or hosting workflow exists yet for the registration, movement,
  break, or payout surfaces.

- **Automatic clock-driven control**: Blind levels and break transitions are
  operator-driven state changes today. There is no wall-clock scheduler or
  autonomous event controller.

- **Full tournament product integration**: Late-registration policy, seating,
  and event progress are modeled in shared state, but they are not yet wired
  into a user-facing product flow.

## Dependency Chain

The training/profile utilities remain separate from the lifecycle controller so
future tournament runtime work can build on both without blurring them into one
claim.

## Evidence

- `test_tournament_profile_uses_payout_curve_utility`
- `test_tournament_utility_training_uses_profile_metadata`
- `test_tournament_utility_docs_exclude_full_lifecycle_claims`
- `training_profile_config_tournament_sampling_is_deterministic` (tables::tests)
- `tournament_payout_splits_ties` (tournament::tests)
- `game_payoff_uses_tournament_payout_when_set` (tournament::tests)
- `registration_flow_opens_closes_and_starts_with_visible_state`
- `blind_level_changes_only_after_current_hand_finishes`
- `elimination_records_place_table_hand_and_final_payout`
- `pause_and_resume_preserve_level_metadata_and_assignments`
