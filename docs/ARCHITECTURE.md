# Architecture

This repo is expected to diverge from the original `robopoker` architecture in
three places:

1. Gameplay engine
   There must be one canonical multiway engine for 2-10 seats.

2. Abstraction and infosets
   Multiway abstractions and blueprint keys must encode seat-relative,
   player-count-aware strategic context.

3. Training and serving
   Profile-aware tables, clustering, training, and strategy queries must be the
   default path rather than an optional extension.

The founding implementation roadmap lives in
`docs/plans/2026-04-05-robopokermulti-foundation.md`.
