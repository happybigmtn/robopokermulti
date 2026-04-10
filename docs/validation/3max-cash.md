# 3-Max Cash Validation Gate

## Profile

| Field                  | Value          |
|------------------------|----------------|
| Profile ID             | `bp_3max_cash` |
| Abstraction Version    | `abs_v4_p3`    |
| Info Version           | V2             |
| Seat Count             | 3              |
| Format                 | cash           |
| Blinds                 | 1/2            |
| Ante                   | 0              |
| Stack (BB)             | 50             |

## Gate Status

| Category   | Status  | Notes |
|------------|---------|-------|
| Clustering | pending | Partial run recorded on 2026-04-10: `isomorphism_abs_v4_p3` reached `82,750,000` rows (`~7.8 GB`), but `abstraction/metric/transitions` remain empty and `scripts/pipeline_status.sh` reported no active trainer |
| Training   | pending | `blueprint_bp_3max_cash` still has `0` rows, so no fast/slow training evidence exists yet |
| Serving    | pending | No policy benchmark has been recorded, and `scripts/rpm08_3max_gate.sh query` currently drops into the interactive `convert` CLI instead of producing a one-shot proof |

## Verdict

**PENDING** -- Infrastructure is proven and partial clustering evidence exists,
but the first end-to-end 3-max clustering/training/query run has not been
completed or recorded as PASS/FAIL yet.

## Evidence

### Infrastructure Proofs (code-level)

- Profile round-trip: `test_3max_profile_round_trip_gate` (save::gate::tests)
- Analysis routing: `test_3max_analysis_reads_profile_native_tables` (save::gate::tests)
- HU fallback rejection: `test_6max_profile_rejects_heads_up_fallback` (save::gate::tests)
- Gate record validation: `test_gate_record_requires_benchmark_fields` (save::gate::tests)
- Analysis API profile-native: RPM-07 spec tests (analysis::api::tests)
- Training schema V2: RPM-06 spec tests (autotrain::mode::tests, mccfr::nlhe::profile::tests)

### Local Automation

- `scripts/local_postgres.sh` provisions the documented local Postgres runtime
  on `localhost:54329`.
- `scripts/cluster_local.sh` wraps resume-safe `status`, `sanity`, `cluster`,
  `fast`, `slow`, and seat-qualified `query` commands.
- `scripts/rpm08_3max_gate.sh` pins the canonical `bp_3max_cash` /
  `abs_v4_p3` / `PLAYER_COUNT=3` validation profile for local execution.
- `scripts/gate_report.sh` prints row counts, epoch state, registered profile
  metadata, relation sizes, active Postgres activity, trainer process stats,
  and the latest trainer log tail for the current profile;
  `scripts/rpm08_3max_report.sh` remains as the 3-max compatibility wrapper.
- `scripts/rpm08_3max_gate.sh report save` writes timestamped gate snapshots
  under `logs/validation/` for benchmark evidence capture.
- `scripts/gate_handoff.sh cluster fast-bg` can arm an unattended handoff from
  the long clustering stage into detached fast training for the same profile.
- `GATE_HANDOFF_ARM=1 scripts/gate_handoff.sh fast slow-bg` can be started
  ahead of time so the pipeline keeps advancing once fast training appears and
  later exits.
- `scripts/gate_record_from_report.sh <saved-report> save` converts a saved
  snapshot into a structured `GateRecord` JSON skeleton for later PASS/FAIL
  promotion, and it auto-loads matching saved policy-latency evidence when
  available.
- `scripts/benchmark_entry_from_report.sh <saved-report> save` converts a
  saved snapshot into a structured `BenchmarkEntry` JSON skeleton for the
  benchmark matrix, and it auto-loads matching saved policy-latency evidence
  when available.
- `scripts/policy_query_benchmark.sh save` benchmarks the `/blueprint` HTTP
  policy lookup once the profile has non-zero blueprint rows.
- `scripts/validation_artifact_index.sh save` writes a compact index of the
  latest saved artifacts for the canonical 3/6/10-max profiles.
- `scripts/pipeline_status.sh` prints the active trainer process, armed
  handoff watchers, latest saved artifacts, and the latest live clustering
  commit for the current profile.
- `TARGET_ROWS=104250000 scripts/pipeline_checkpoint.sh save` captures a
  fresh report, gate record, benchmark entry, progress snapshot, and index
  refresh for the current profile in one step.
- `TARGET_ROWS=104250000 scripts/pipeline_progress.sh` prints observed
  clustering throughput and a rough ETA based on the latest saved report/log.
- `TARGET_ROWS=104250000 scripts/pipeline_progress.sh save` writes a
  timestamped throughput/ETA snapshot under `logs/validation/`.

### Benchmarks

Not yet measured. The following must be recorded before PASS:

- Memory usage during clustering
- Database size after full street ingestion
- Clustering runtime per street
- Training runtime (fast + slow)
- Strategy query latency

### Current Partial Evidence

- `logs/validation/report_bp_3max_cash_20260410_174423.txt` records:
  - `isomorphism_rows=82750000`
  - `abstraction_rows=0`
  - `metric_rows=0`
  - `transitions_rows=0`
  - `profile_blueprint_rows=0`
- `scripts/pipeline_status.sh` on 2026-04-10 recorded `active trainer = none`
  for the canonical 3-max profile after that saved report.
- `scripts/rpm08_3max_gate.sh status` still reports only the river street as
  clustered for `abs_v4_p3`.

### Current Blockers

- The canonical clustering run is incomplete, and there is no active trainer to
  finish the remaining turn/flop/preflop plus derived table stages.
- No fast or slow training run has produced blueprint rows for
  `bp_3max_cash`, so the gate still lacks training evidence.
- The current `query` helper is not a truthful serving proof yet because it
  invokes the interactive `convert` CLI path instead of emitting a one-shot
  seat-qualified lookup result.

## Dependencies

- RPM-04: multiway abstraction v4 (PASS)
- RPM-05: infoset context v2 (PASS)
- RPM-06: profile-native training (PASS)
- RPM-07: profile-native analysis API (PASS)

## Next Steps

1. Resume the canonical `abs_v4_p3` clustering run and record where the
   remaining turn/flop/preflop work completes or fails.
2. Run fast + slow training with `bp_3max_cash` once clustering completes
   enough to populate the profile-native blueprint tables.
3. Replace or repair the current one-shot query proof so serving evidence can
   be recorded without dropping into the interactive CLI loop.
4. Record benchmark measurements and update this document with an explicit PASS
   or FAIL verdict.
