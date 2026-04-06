use crate::{Chips, Utility};
use std::sync::{Arc, OnceLock, RwLock};

use super::Game;

/// Tournament payout curve (utilities are normalized to sum to 1.0).
#[derive(Debug, Clone)]
pub struct TournamentPayout {
    payouts: Vec<Utility>,
}

impl TournamentPayout {
    /// Create a payout curve from raw weights (normalized to sum to 1.0).
    pub fn new(payouts: Vec<Utility>) -> Result<Self, &'static str> {
        if payouts.is_empty() {
            return Err("payout curve must include at least one entry");
        }
        if payouts.iter().any(|p| *p < 0.0) {
            return Err("payout curve entries must be non-negative");
        }
        let total: Utility = payouts.iter().sum();
        if total <= 0.0 {
            return Err("payout curve total must be positive");
        }
        let normalized = payouts.into_iter().map(|p| p / total).collect();
        Ok(Self {
            payouts: normalized,
        })
    }

    pub fn payouts(&self) -> &[Utility] {
        &self.payouts
    }

    /// Compute tournament utilities for a vector of final stacks.
    pub fn utilities_for_stacks(&self, stacks: &[Chips]) -> Vec<Utility> {
        if stacks.is_empty() {
            return Vec::new();
        }

        let mut ranked: Vec<(usize, i64)> = stacks
            .iter()
            .enumerate()
            .map(|(idx, stack)| (idx, *stack as i64))
            .collect();

        ranked.sort_by(|(_, a), (_, b)| b.cmp(a));

        let mut utilities = vec![0.0; stacks.len()];
        let mut rank = 0usize;
        while rank < ranked.len() {
            let stack_value = ranked[rank].1;
            let mut end = rank + 1;
            while end < ranked.len() && ranked[end].1 == stack_value {
                end += 1;
            }

            let payout_total: Utility = (rank..end)
                .map(|idx| self.payouts.get(idx).copied().unwrap_or(0.0))
                .sum();
            let split = if payout_total > 0.0 {
                payout_total / (end - rank) as Utility
            } else {
                0.0
            };

            for idx in rank..end {
                let seat = ranked[idx].0;
                utilities[seat] = split;
            }

            rank = end;
        }

        utilities
    }

    /// Compute tournament utilities for a terminal game state.
    pub fn utilities_for_game(&self, game: &Game) -> Vec<Utility> {
        let seat_count = game.seat_count();
        if seat_count == 0 {
            return Vec::new();
        }

        let settlements = game.settlements();
        let seats = game.seats();
        let stacks: Vec<Chips> = (0..seat_count)
            .map(|i| {
                let seat_stack = seats[i].stack();
                let reward = settlements.get(i).map(|s| s.pnl().reward()).unwrap_or(0);
                seat_stack + reward
            })
            .collect();

        self.utilities_for_stacks(&stacks)
    }
}

static TOURNAMENT_PAYOUT: OnceLock<RwLock<Option<Arc<TournamentPayout>>>> = OnceLock::new();

fn payout_lock() -> &'static RwLock<Option<Arc<TournamentPayout>>> {
    TOURNAMENT_PAYOUT.get_or_init(|| RwLock::new(None))
}

/// Set the tournament payout curve. When set, Game::payoff uses tournament utilities.
pub fn set_tournament_payout(payout: Option<TournamentPayout>) {
    *payout_lock().write().expect("tournament payout lock") = payout.map(Arc::new);
}

/// Return the current tournament payout (if configured).
pub fn current_tournament_payout() -> Option<Arc<TournamentPayout>> {
    payout_lock()
        .read()
        .expect("tournament payout lock")
        .clone()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gameplay::{TableConfig, Turn, set_tournament_payout};
    use crate::mccfr::TreeGame;
    use std::sync::{Mutex, OnceLock};

    fn test_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    #[test]
    fn tournament_payout_splits_ties() {
        let payout = TournamentPayout::new(vec![0.6, 0.3, 0.1]).unwrap();
        let utilities = payout.utilities_for_stacks(&[10, 10, 10]);
        let expected = (0.6 + 0.3 + 0.1) / 3.0;
        for u in utilities.iter().take(3) {
            assert!((*u - expected).abs() < 1e-6);
        }
    }

    #[test]
    fn game_payoff_uses_tournament_payout_when_set() {
        let _guard = test_lock().lock().unwrap();
        let payout = TournamentPayout::new(vec![1.0, 0.0]).unwrap();
        set_tournament_payout(Some(payout));

        let config = TableConfig::heads_up();
        let game = Game::root_with_config(config).apply(crate::gameplay::Action::Fold);
        assert_eq!(game.turn(), Turn::Terminal);

        let p0 = game.payoff(Turn::Choice(0));
        let p1 = game.payoff(Turn::Choice(1));
        assert!((p0 - 0.0).abs() < 1e-6);
        assert!((p1 - 1.0).abs() < 1e-6);

        set_tournament_payout(None);
    }
}
