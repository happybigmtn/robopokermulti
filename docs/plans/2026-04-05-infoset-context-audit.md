# Infoset Context Audit

## Goal

Record the remaining training-readiness gap after engine unification and
seat-aware abstraction enforcement: the canonical runtime now supports multiway
cash semantics, but the blueprint identity still under-encodes multiway state.

## Current State

`Info` is still keyed as `(history, present, choices)` in
`src/mccfr/nlhe/info.rs`.

That key is used in all of these places:

- in-memory training profile storage
- database blueprint reads and writes
- hydration from persisted blueprint rows
- strategy API serialization

This means any missing context in `Info` is not just an encoder detail. It is a
schema-level omission.

## Findings

### 1. Seat-relative abstraction is looked up, but not persisted in the infoset key

`Worker::encode()` computes a button-relative `seat_position` and uses it for
`encode_profile(...)`.

But once the abstraction is returned, `Worker::seed()` and `Worker::info()`
still construct `Info` from only:

- `history`
- `present`
- `choices`

So the abstraction lookup is seat-aware while the blueprint key is not.

Implication:

- Two strategically different multiway states can still collide if they share
  the same packed path, abstraction bucket, and legal edge set.

### 2. Blueprint persistence still uses a heads-up-shaped compound key

`NlheProfile` persists rows as:

- `past`
- `present`
- `future`
- `edge`

with uniqueness on `(past, present, future, edge)`.

No persisted field captures:

- seat count
- acting seat position
- active-player topology
- stack-band context
- pot-band context

Implication:

- A multiway profile can only distinguish states through the abstraction bucket
  and packed action path.
- Any future seat-context extension is a real schema change, not a local refactor.

### 3. API strategy responses cannot round-trip multiway infoset context

`ApiStrategy` serializes only:

- `history`
- `present`
- `choices`
- `accumulated`

This mirrors the current `Info` shape and means any future context expansion
must also update API DTOs and any external tools that consume them.

### 4. Path depth is still globally capped at 16 edges

`Path` packs edges into a `u64` and truncates at `MAX_DEPTH_SUBGAME = 16`.

That is workable for current heads-up subgames, but it is a poor default for:

- 6-max and 10-max cash action histories
- deeper late-street multi-raise branches
- tournament spots with more forced action and more distinct stack situations

Implication:

- Even before abstraction quality, the key space is pressure-limited by the
  packed path representation.

### 5. `Recall` is config-aware, but not infoset-context-aware

`Recall` now handles table config correctly for posting prefixes and base game
construction, but `Recall::bind()` still produces `Info` from:

- reversed `path()`
- supplied abstraction
- recomputed `choices`

It does not carry any explicit seat metadata into the bound infoset.

Implication:

- Multiway inference will inherit the same under-specified infoset identity as
  training unless the `Info` schema changes.

## Recommendation

Treat the next major training change as `Info v2`, not as an incremental patch.

`Info v2` should introduce an explicit context component, for example:

- `seat_count`
- `seat_position`
- `active_player_count`
- optional compact active-seat mask

Potential later additions:

- pot-size band
- effective-stack band
- blind/ante level band

Recommended shape:

- `Info { history, present, choices, context }`

## Required Follow-On Work

1. Define a compact `InfoContext` type and its invariants.
2. Update `Worker::seed()` and `Worker::info()` to populate context from `Game`.
3. Update `Info::from_game()`, `Info::from_tree()`, and `Info::from_path()`.
4. Create a new blueprint schema version that persists context explicitly.
5. Update `NlheProfile::rows()` and hydration logic to read and write the new key.
6. Update `ApiStrategy` to serialize and deserialize infoset context.
7. Revisit `Path` packing and decide whether `u64` remains acceptable.

## Non-Goals

This document does not propose solving:

- the actual multiway abstraction feature design
- tournament utility/state modeling
- bot/PvE strategy generation

It only scopes the remaining identity gap between:

- a multiway-capable game engine
- a truly multiway-capable blueprint key
