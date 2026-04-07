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
| Clustering | pending | Profile-native abstraction tables defined; awaiting first clustering run |
| Training   | pending | V2 schema and profile-native training path proven by RPM-06; awaiting first 3-max training run |
| Serving    | pending | Profile-native analysis API proven by RPM-07; awaiting first 3-max query proof |

## Verdict

**PENDING** -- Infrastructure proven, awaiting first end-to-end 3-max run.

## Evidence

### Infrastructure Proofs (code-level)

- Profile round-trip: `test_3max_profile_round_trip_gate` (save::gate::tests)
- Analysis routing: `test_3max_analysis_reads_profile_native_tables` (save::gate::tests)
- HU fallback rejection: `test_6max_profile_rejects_heads_up_fallback` (save::gate::tests)
- Gate record validation: `test_gate_record_requires_benchmark_fields` (save::gate::tests)
- Analysis API profile-native: RPM-07 spec tests (analysis::api::tests)
- Training schema V2: RPM-06 spec tests (autotrain::mode::tests, mccfr::nlhe::profile::tests)

### Benchmarks

Not yet measured. The following must be recorded before PASS:

- Memory usage during clustering
- Database size after full street ingestion
- Clustering runtime per street
- Training runtime (fast + slow)
- Strategy query latency

## Dependencies

- RPM-04: multiway abstraction v4 (PASS)
- RPM-05: infoset context v2 (PASS)
- RPM-06: profile-native training (PASS)
- RPM-07: profile-native analysis API (PASS)

## Next Steps

1. Run 3-max clustering with `abs_v4_p3` tables
2. Run fast + slow training with `bp_3max_cash` profile
3. Query strategy via analysis API and verify seat-relative results
4. Record benchmark measurements and update this document
5. Set verdict to PASS or FAIL with evidence
