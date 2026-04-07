# 10-Max Cash Pilot Gate (Experimental)

## Profile

| Field                  | Value           |
|------------------------|-----------------|
| Profile ID             | `bp_10max_cash` |
| Abstraction Version    | `abs_v4_p10`    |
| Info Version           | V2              |
| Seat Count             | 10              |
| Format                 | cash            |
| Blinds                 | 1/2             |
| Ante                   | 0               |
| Stack (BB)             | 50              |

## Gate Status

| Category   | Status  | Notes |
|------------|---------|-------|
| Clustering | pending | Awaiting 3-max and 6-max gate pass |
| Training   | pending | Awaiting 3-max and 6-max gate pass |
| Serving    | pending | Awaiting 3-max and 6-max gate pass |

## Verdict

**PENDING** -- Experimental pilot. Cannot be attempted until 3-max and 6-max
gates record PASS. Failure at 10-max is expected and allowed but must be
recorded as a measured limit.

## Prerequisites

- 3-max gate must record PASS (PENDING)
- 6-max gate must record PASS (PENDING)
- Explicit benchmark budget for RAM, database growth, and clustering runtime

## Expected Bottlenecks

The following are known scaling concerns at 10-max:

- Abstraction table combinatorial growth (10 seats x 4 streets)
- Clustering runtime per street
- Blueprint table storage (V2 schema with 10-seat context dimensions)
- Training memory pressure
- Strategy lookup latency under larger key space

## Evidence

### Infrastructure Proofs (code-level)

- Profile round-trip: `test_10max_profile_round_trip_pilot` (save::gate::tests)
- Table isolation: `test_10max_tables_isolated_from_lower_seat_counts` (save::gate::tests)
- Experimental status: `test_10max_gate_record_records_experimental_status` (save::gate::tests)
- 10-max prerequisite chain: `test_10max_gate_requires_prior_3max_and_6max_evidence` (save::gate::tests)

### Benchmarks

Not yet measured. The following must be recorded:

- Memory usage during clustering (and growth factor vs 3-max and 6-max)
- Database size after full street ingestion
- Clustering runtime per street (and whether it completes at all)
- Training runtime (fast + slow)
- Strategy query latency

## Dependencies

- RPM-08: 3-max cash validation gate (PENDING)
- RPM-09: 6-max cash validation gate (PENDING)
- RPM-10: training benchmark matrix and quality gates (PASS)

## Outcome Recording

When the pilot is attempted, this document must be updated with one of:

- **PASS**: 10-max is a supported configuration with benchmark evidence
- **FAIL**: 10-max is a measured limit with bottleneck evidence explaining why
