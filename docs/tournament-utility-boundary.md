# Tournament Utility Layer Boundary

## What This Layer Provides

The current `robopokermulti` tournament support is a **utility layer only**:

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

## What This Layer Does NOT Provide

The following are explicitly out of scope until future tournament lifecycle
work lands:

- **Registration and entrant management**: No registration flow, buy-in
  handling, or entrant roster state.

- **Multi-table balancing**: No table-break logic, seat redistribution, or
  player movement between tables.

- **Elimination and redraw**: No bust-out detection, single-table consolidation,
  or final-table transitions.

- **Blind level advancement**: Training samples blind levels statically from
  the schedule. There is no dynamic level clock, break scheduling, or
  ante-progression state machine.

- **Resume and pause**: No tournament-level pause/resume or mid-tournament
  persistence beyond what the underlying hand-level session provides.

## Dependency Chain

Future tournament lifecycle work (RT-05A, RT-05B) depends on this utility
boundary rather than blurring the distinction between utility support and
full tournament state.

## Evidence

- `test_tournament_profile_uses_payout_curve_utility`
- `test_tournament_utility_training_uses_profile_metadata`
- `test_tournament_utility_docs_exclude_full_lifecycle_claims`
- `training_profile_config_tournament_sampling_is_deterministic` (tables::tests)
- `tournament_payout_splits_ties` (tournament::tests)
- `game_payoff_uses_tournament_payout_when_set` (tournament::tests)
