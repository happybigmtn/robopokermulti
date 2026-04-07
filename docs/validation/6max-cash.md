# 6-Max Cash Validation Gate

## Profile

| Field                  | Value          |
|------------------------|----------------|
| Profile ID             | `bp_6max_cash` |
| Abstraction Version    | `abs_v4_p6`    |
| Info Version           | V2             |
| Seat Count             | 6              |
| Format                 | cash           |
| Blinds                 | 1/2            |
| Ante                   | 0              |
| Stack (BB)             | 50             |

## Gate Status

| Category   | Status  | Notes |
|------------|---------|-------|
| Clustering | pending | Profile-native abstraction tables defined; awaiting clustering run |
| Training   | pending | V2 schema proven; awaiting 6-max training run |
| Serving    | pending | Profile-native analysis API proven; awaiting 6-max query proof |

## Verdict

**PENDING** -- Downstream of 3-max gate. Infrastructure proven, awaiting first
end-to-end 6-max run after 3-max passes.

## Evidence

### Infrastructure Proofs (code-level)

- Profile round-trip: `test_6max_profile_round_trip_gate` (save::gate::tests)
- Schema scaling: `test_6max_scales_beyond_3max_without_schema_shortcuts` (save::gate::tests)
- Gate record round-trip: `test_6max_gate_record_round_trip` (save::gate::tests)
- HU fallback rejection: `test_6max_profile_rejects_heads_up_fallback` (save::gate::tests)
- Analysis API profile-native: RPM-07 spec tests (analysis::api::tests)

### Benchmarks

Not yet measured. The following must be recorded before PASS:

- Memory usage during clustering (and comparison to 3-max)
- Database size after full street ingestion (and growth factor vs 3-max)
- Clustering runtime per street
- Training runtime (fast + slow)
- Strategy query latency

## Prerequisites

- 3-max gate must record PASS before 6-max gate can be evaluated
- RPM-08 3-max gate (PENDING)

## Dependencies

- RPM-04: multiway abstraction v4 (PASS)
- RPM-05: infoset context v2 (PASS)
- RPM-06: profile-native training (PASS)
- RPM-07: profile-native analysis API (PASS)
- RPM-08: 3-max cash validation gate (PENDING)

## Next Steps

1. Await 3-max gate PASS
2. Run 6-max clustering with `abs_v4_p6` tables
3. Run fast + slow training with `bp_6max_cash` profile
4. Compare storage growth and runtime to 3-max benchmarks
5. Query strategy via analysis API and verify seat-relative results
6. Record benchmark measurements and update this document
7. Set verdict to PASS or FAIL with evidence
