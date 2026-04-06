# Roadmap

## Phase 0

- establish the standalone repo
- tag the imported heads-up baseline
- copy over the founding multiway ExecPlan
- document fork policy and repo scope

## Phase 1

- choose one canonical gameplay engine
- remove contradictory heads-up-only assumptions
- lock down multiway cash semantics for 2-10 seats

## Phase 2

- replace heads-up-shaped river semantics
- introduce a real multiway abstraction version
- redesign infoset identity for large-table play

## Phase 3

- make training, clustering, and inference fully profile-native
- eliminate fallback to static heads-up tables
- validate 3-max and 6-max cash training

## Phase 4

- attempt 10-max cash blueprint training
- benchmark DB growth, training throughput, and query latency
- tune defaults only after benchmark data exists

## Phase 5

- keep tournament utilities profile-native
- design tournament-state extensions
- defer full tournament lifecycle until the cash foundation is stable
