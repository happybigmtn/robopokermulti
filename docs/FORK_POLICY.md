# Fork Policy

`robopokermulti` forked from the heads-up `robopoker` baseline to support
multiway NLHE cash blueprints, then tournament extensions, without forcing those
foundational changes back into the stable heads-up line.

## Rules

1. `robopoker` remains the conceptual upstream for heads-up bug fixes.
2. `robopokermulti` accepts upstream fixes by deliberate cherry-pick.
3. `robopokermulti` does not promise long-running structural parity with
   `robopoker`.
4. Multiway engine, abstraction, infoset, and training changes are native here,
   not compatibility layers.
5. Tournament work must build on the multiway cash foundation, not bypass it.

## Near-Term Scope

- unify the canonical gameplay engine
- redesign the abstraction stack for multiway seats
- redesign infoset identity for multiway play
- make profile-aware training and inference mandatory
- validate 3-max, 6-max, then 10-max cash training

## Explicit Non-Goals

- preserving the old heads-up abstraction contract in the new training path
- keeping optional env-flag behavior for core multiway semantics
- implementing full tournament lifecycle before 10-max cash validation
