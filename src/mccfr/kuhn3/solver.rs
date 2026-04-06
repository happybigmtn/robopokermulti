//! 3-player Kuhn poker solver implementing multiway CFR.
//!
//! This validates AC-2.1 (N-player utilities), AC-2.2 (N-player training),
//! and AC-2.3 (sanity tests on 3-player games).

use super::*;
use crate::mccfr::*;
use crate::*;
use std::collections::BTreeMap;

/// Information set for 3-player Kuhn poker.
/// A player's info set is determined by their card and the betting history.
///
/// History is encoded as a u16 where each 2-bit pair represents an action:
/// - 0 = nothing, 1 = check, 2 = bet, 3 = call/fold
/// This allows up to 8 actions in the history while remaining Copy.
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct Kuhn3Info {
    /// The player's card (0=J, 1=Q, 2=K)
    pub card: u8,
    /// The betting history encoded as a u16
    pub history: u16,
    /// Number of actions in history
    pub history_len: u8,
}

impl Kuhn3Info {
    /// Create a new info set
    pub fn new(card: u8, history: u16, history_len: u8) -> Self {
        Self {
            card,
            history,
            history_len,
        }
    }

    /// Create an info set from a card and action history string
    pub fn from_str(card: u8, history: &str) -> Self {
        let mut h: u16 = 0;
        for (i, c) in history.chars().enumerate() {
            let action_code = match c {
                'c' => 1, // check
                'b' => 2, // bet
                'k' => 3, // call
                'f' => 3, // fold (same code since it's distinguishable by context)
                _ => 0,
            };
            h |= (action_code as u16) << (i * 2);
        }
        Self {
            card,
            history: h,
            history_len: history.len() as u8,
        }
    }

    /// Append an action to the history
    pub fn with_action(&self, action: Kuhn3Edge) -> Self {
        let action_code = match action {
            Kuhn3Edge::Check => 1,
            Kuhn3Edge::Bet => 2,
            Kuhn3Edge::Call => 3,
            Kuhn3Edge::Fold => 3,
        };
        // Cap history at 7 actions (bits 0-14)
        if self.history_len >= 7 {
            return *self;
        }
        let new_history = self.history | ((action_code as u16) << (self.history_len * 2));
        Self {
            card: self.card,
            history: new_history,
            history_len: self.history_len + 1,
        }
    }
}

impl TreeInfo for Kuhn3Info {
    type E = Kuhn3Edge;
    type T = Kuhn3Turn;

    fn choices(&self) -> Vec<Self::E> {
        // This is a simplified implementation - actual choices depend on game state
        // For Kuhn poker, at any point you can either Check/Call or Bet/Fold
        vec![
            Kuhn3Edge::Check,
            Kuhn3Edge::Bet,
            Kuhn3Edge::Call,
            Kuhn3Edge::Fold,
        ]
    }
}

/// 3-player Kuhn solver implementing multiway CFR.
#[derive(Default)]
pub struct Kuhn3Solver {
    epochs: usize,
    /// Map from info set to edge -> (policy, regret)
    encounters: BTreeMap<Kuhn3Info, BTreeMap<Kuhn3Edge, (Probability, Utility)>>,
}

impl Kuhn3Solver {
    /// Create a new solver
    pub fn new() -> Self {
        Self::default()
    }

    /// Run CFR for a number of iterations
    pub fn train(&mut self, iterations: usize) {
        for _ in 0..iterations {
            // For each card permutation, run CFR
            for cards in Self::all_card_permutations() {
                let game = Kuhn3Game::new(cards);
                // Start with empty history (card doesn't matter at root since we update per-actor)
                let root_info = Kuhn3Info::new(0, 0, 0);
                self.cfr(&game, [1.0, 1.0, 1.0], root_info);
            }
            self.epochs += 1;
        }
    }

    /// All 6 possible card permutations (3! = 6)
    fn all_card_permutations() -> Vec<[u8; 3]> {
        vec![
            [0, 1, 2], // J Q K
            [0, 2, 1], // J K Q
            [1, 0, 2], // Q J K
            [1, 2, 0], // Q K J
            [2, 0, 1], // K J Q
            [2, 1, 0], // K Q J
        ]
    }

    /// Core CFR recursion
    fn cfr(
        &mut self,
        game: &Kuhn3Game,
        reach: [Probability; 3],
        info_base: Kuhn3Info,
    ) -> [Utility; 3] {
        if game.is_done() {
            // Terminal node: return payoffs
            return [
                game.payoff(Kuhn3Turn::P1),
                game.payoff(Kuhn3Turn::P2),
                game.payoff(Kuhn3Turn::P3),
            ];
        }

        let actor = game.actor_idx();
        let card = game.card(actor);
        let info = Kuhn3Info::new(card, info_base.history, info_base.history_len);

        let actions = game.legal();
        if actions.is_empty() {
            return [0.0, 0.0, 0.0];
        }

        // Get current strategy via regret matching
        let strategy = self.get_strategy(&info, &actions);

        // Compute action values and expected value
        let mut action_values: BTreeMap<Kuhn3Edge, [Utility; 3]> = BTreeMap::new();
        let mut expected_value = [0.0f32; 3];

        for (i, &action) in actions.iter().enumerate() {
            // Update reach for this action
            let mut new_reach = reach;
            new_reach[actor] *= strategy[i];

            let new_info = info_base.with_action(action);
            let child = game.apply_action(action);
            let values = self.cfr(&child, new_reach, new_info);

            action_values.insert(action, values);
            for p in 0..3 {
                expected_value[p] += strategy[i] * values[p];
            }
        }

        // Update regrets for the acting player (if it's their traversal turn)
        if actor == self.epochs % 3 {
            let cfr_reach: Probability = reach
                .iter()
                .enumerate()
                .filter(|&(i, _)| i != actor)
                .map(|(_, &r)| r)
                .product();

            for &action in &actions {
                let values = action_values.get(&action).unwrap();
                let regret = cfr_reach * (values[actor] - expected_value[actor]);

                let entry = self
                    .encounters
                    .entry(info)
                    .or_default()
                    .entry(action)
                    .or_insert((0.0, 0.0));
                entry.1 += regret; // Add to regret
            }
        }

        // Update average strategy
        let my_reach = reach[actor];
        for (i, &action) in actions.iter().enumerate() {
            let entry = self
                .encounters
                .entry(info)
                .or_default()
                .entry(action)
                .or_insert((0.0, 0.0));
            entry.0 += my_reach * strategy[i]; // Add to policy sum
        }

        expected_value
    }

    /// Get current strategy via regret matching
    fn get_strategy(&self, info: &Kuhn3Info, actions: &[Kuhn3Edge]) -> Vec<Probability> {
        let mut positive_regrets = Vec::new();
        let mut sum = 0.0f32;

        for &action in actions {
            let regret = self
                .encounters
                .get(info)
                .and_then(|m| m.get(&action))
                .map(|(_, r)| r.max(0.0))
                .unwrap_or(0.0);
            positive_regrets.push(regret.max(POLICY_MIN));
            sum += regret.max(POLICY_MIN);
        }

        if sum > 0.0 {
            positive_regrets.iter().map(|&r| r / sum).collect()
        } else {
            // Uniform strategy
            let n = actions.len() as f32;
            vec![1.0 / n; actions.len()]
        }
    }

    /// Get the average (Nash) strategy for an info set
    pub fn get_average_strategy(
        &self,
        info: &Kuhn3Info,
        actions: &[Kuhn3Edge],
    ) -> Vec<(Kuhn3Edge, Probability)> {
        let mut sum = 0.0f32;
        let mut policies = Vec::new();

        for &action in actions {
            let policy = self
                .encounters
                .get(info)
                .and_then(|m| m.get(&action))
                .map(|(p, _)| *p)
                .unwrap_or(0.0);
            policies.push((action, policy.max(POLICY_MIN)));
            sum += policy.max(POLICY_MIN);
        }

        if sum > 0.0 {
            policies.iter().map(|(a, p)| (*a, p / sum)).collect()
        } else {
            let n = actions.len() as f32;
            policies.iter().map(|(a, _)| (*a, 1.0 / n)).collect()
        }
    }
}

impl Profile for Kuhn3Solver {
    type T = Kuhn3Turn;
    type E = Kuhn3Edge;
    type G = Kuhn3Game;
    type I = Kuhn3Info;

    fn increment(&mut self) {
        self.epochs += 1;
    }

    fn epochs(&self) -> usize {
        self.epochs
    }

    /// Walker rotates across all 3 players (key for multiway MCCFR).
    fn walker(&self) -> Self::T {
        match self.epochs % 3 {
            0 => Kuhn3Turn::P1,
            1 => Kuhn3Turn::P2,
            _ => Kuhn3Turn::P3,
        }
    }

    fn sum_policy(&self, info: &Self::I, edge: &Self::E) -> Probability {
        self.encounters
            .get(info)
            .and_then(|m| m.get(edge))
            .map(|(p, _)| *p)
            .unwrap_or(0.0)
    }

    fn sum_regret(&self, info: &Self::I, edge: &Self::E) -> Utility {
        self.encounters
            .get(info)
            .and_then(|m| m.get(edge))
            .map(|(_, r)| *r)
            .unwrap_or(0.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test that walker rotates across all 3 players (AC-2.1)
    #[test]
    fn test_walker_rotates_3_players() {
        let mut solver = Kuhn3Solver::new();

        assert_eq!(solver.walker(), Kuhn3Turn::P1);
        solver.increment();
        assert_eq!(solver.walker(), Kuhn3Turn::P2);
        solver.increment();
        assert_eq!(solver.walker(), Kuhn3Turn::P3);
        solver.increment();
        assert_eq!(solver.walker(), Kuhn3Turn::P1); // Wraps around
    }

    /// Test that CFR converges to reasonable strategies (AC-2.3)
    #[test]
    fn test_3player_kuhn_converges() {
        let mut solver = Kuhn3Solver::new();
        solver.train(10000);

        // With King (card=2), first to act should bet frequently
        let king_first = Kuhn3Info::new(2, 0, 0);
        let strategy =
            solver.get_average_strategy(&king_first, &[Kuhn3Edge::Check, Kuhn3Edge::Bet]);
        let bet_prob = strategy
            .iter()
            .find(|(a, _)| *a == Kuhn3Edge::Bet)
            .map(|(_, p)| *p)
            .unwrap_or(0.0);

        // King should bet at least 30% of the time (not a bluff-only strategy)
        assert!(
            bet_prob > 0.3,
            "King should bet frequently, got {}",
            bet_prob
        );

        // With Jack (card=0), first to act should check frequently
        let jack_first = Kuhn3Info::new(0, 0, 0);
        let strategy =
            solver.get_average_strategy(&jack_first, &[Kuhn3Edge::Check, Kuhn3Edge::Bet]);
        let check_prob = strategy
            .iter()
            .find(|(a, _)| *a == Kuhn3Edge::Check)
            .map(|(_, p)| *p)
            .unwrap_or(0.0);

        // Jack should check at least 50% of the time (weak hand)
        assert!(
            check_prob > 0.5,
            "Jack should check frequently, got {}",
            check_prob
        );
    }

    /// Test that all 3 players' regrets are updated (AC-2.2)
    #[test]
    fn test_all_players_have_regrets() {
        let mut solver = Kuhn3Solver::new();
        solver.train(1000);

        // Check that we have info sets for cards 0, 1, and 2 at the root (history_len == 0)
        let mut has_jack = false;
        let mut has_queen = false;
        let mut has_king = false;

        for (info, _) in solver.encounters.iter() {
            if info.history_len == 0 {
                match info.card {
                    0 => has_jack = true,
                    1 => has_queen = true,
                    2 => has_king = true,
                    _ => {}
                }
            }
        }

        assert!(has_jack, "Should have info sets for Jack holder");
        assert!(has_queen, "Should have info sets for Queen holder");
        assert!(has_king, "Should have info sets for King holder");
    }

    /// Test utility computation for 3 players (AC-2.1)
    #[test]
    fn test_3player_utility_sum_zero() {
        let game = Kuhn3Game::new([0, 1, 2]);

        // All check
        let game = game.apply_action(Kuhn3Edge::Check);
        let game = game.apply_action(Kuhn3Edge::Check);
        let game = game.apply_action(Kuhn3Edge::Check);

        let u1 = game.payoff(Kuhn3Turn::P1);
        let u2 = game.payoff(Kuhn3Turn::P2);
        let u3 = game.payoff(Kuhn3Turn::P3);

        // Sum of utilities should be zero (zero-sum game)
        let sum = u1 + u2 + u3;
        assert!(
            sum.abs() < 0.001,
            "Utilities should sum to zero, got {}",
            sum
        );
    }
}
