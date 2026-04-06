//! Game state for 3-player Kuhn poker.
//!
//! Cards: Jack=0, Queen=1, King=2
//! Higher card wins.
//!
//! This game uses Copy-friendly types to satisfy TreeGame requirements.

use super::*;
use crate::Utility;
use crate::mccfr::TreeGame;

/// 3-player Kuhn poker game state.
///
/// State encoding uses fixed-size arrays and bitfields for Copy compatibility.
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct Kuhn3Game {
    /// Cards dealt to each player (0=J, 1=Q, 2=K)
    cards: [u8; 3],
    /// Chips contributed by each player
    pot: [u8; 3],
    /// Folded bitfield: bit i = player i has folded
    folded: u8,
    /// Acted since last bet bitfield: bit i = player i has acted since last bet
    acted: u8,
    /// Current bet level to match
    bet_level: u8,
    /// Current actor (0, 1, 2)
    actor: u8,
    /// Has betting round completed?
    done: bool,
}

impl Kuhn3Game {
    /// Create a new game with the given card assignment.
    /// cards[i] is player i's card (0=J, 1=Q, 2=K)
    pub fn new(cards: [u8; 3]) -> Self {
        // All players ante 1 chip
        Self {
            cards,
            pot: [1, 1, 1], // Ante
            folded: 0,      // No one has folded
            acted: 0,       // No one has acted yet
            bet_level: 1,
            actor: 0, // P1 acts first
            done: false,
        }
    }

    /// Check if player i has folded
    pub fn has_folded(&self, i: usize) -> bool {
        (self.folded >> i) & 1 != 0
    }

    /// Mark player i as folded
    fn fold_player(&mut self, i: usize) {
        self.folded |= 1 << i;
    }

    /// Check if player i has acted since last bet
    fn has_acted(&self, i: usize) -> bool {
        (self.acted >> i) & 1 != 0
    }

    /// Mark player i as having acted
    fn mark_acted(&mut self, i: usize) {
        self.acted |= 1 << i;
    }

    /// Reset all acted flags (on new bet)
    fn reset_acted(&mut self) {
        self.acted = 0;
    }

    /// How many players are still in the hand?
    fn active_count(&self) -> usize {
        (0..3).filter(|&i| !self.has_folded(i)).count()
    }

    /// Find the winner among non-folded players.
    /// Returns the player index with the highest card.
    fn winner(&self) -> usize {
        let mut best_player = 0;
        let mut best_card = 0;
        for i in 0..3 {
            if !self.has_folded(i) && self.cards[i] > best_card {
                best_card = self.cards[i];
                best_player = i;
            }
        }
        best_player
    }

    /// Public accessor for done flag
    pub fn is_done(&self) -> bool {
        self.done
    }

    /// Public accessor for current actor
    pub fn actor_idx(&self) -> usize {
        self.actor as usize
    }

    /// Public accessor for a player's card
    pub fn card(&self, i: usize) -> u8 {
        self.cards[i]
    }

    /// Total pot size
    fn total_pot(&self) -> u8 {
        self.pot[0] + self.pot[1] + self.pot[2]
    }

    /// Get legal actions for current actor
    pub fn legal(&self) -> Vec<Kuhn3Edge> {
        if self.done {
            return vec![];
        }

        let my_pot = self.pot[self.actor as usize];
        let to_call = self.bet_level - my_pot;

        let mut actions = vec![];

        if to_call == 0 {
            // No bet to match: can check or bet
            actions.push(Kuhn3Edge::Check);
            actions.push(Kuhn3Edge::Bet);
        } else {
            // Facing a bet: can fold, call, or raise (bet)
            actions.push(Kuhn3Edge::Fold);
            actions.push(Kuhn3Edge::Call);
            // Simplified: no raises in this variant
        }

        actions
    }

    /// Apply an action and return the new game state
    pub fn apply_action(&self, edge: Kuhn3Edge) -> Self {
        let mut next = *self;
        let actor = self.actor as usize;

        match edge {
            Kuhn3Edge::Check => {
                next.mark_acted(actor);
            }
            Kuhn3Edge::Bet => {
                // Bet: put in 1 more chip
                next.pot[actor] += 1;
                next.bet_level = next.pot[actor];
                // Reset acted - everyone needs to act again after a bet
                next.reset_acted();
                next.mark_acted(actor);
            }
            Kuhn3Edge::Call => {
                // Match the current bet
                let to_call = next.bet_level - next.pot[actor];
                next.pot[actor] += to_call;
                next.mark_acted(actor);
            }
            Kuhn3Edge::Fold => {
                next.fold_player(actor);
                next.mark_acted(actor);
            }
        }

        // Advance to next active player
        next.advance_actor();

        // Check if betting round is complete
        next.check_round_complete();

        next
    }

    /// Advance to the next non-folded player
    fn advance_actor(&mut self) {
        for _ in 0..3 {
            self.actor = (self.actor + 1) % 3;
            if !self.has_folded(self.actor as usize) {
                break;
            }
        }
    }

    /// Check if the betting round is complete
    fn check_round_complete(&mut self) {
        // Round is complete if:
        // 1. Only one player remains (all others folded)
        if self.active_count() <= 1 {
            self.done = true;
            return;
        }

        // 2. All active players have acted since last bet and are matched
        let all_active_acted = (0..3).all(|i| self.has_folded(i) || self.has_acted(i));
        let all_matched = (0..3).all(|i| self.has_folded(i) || self.pot[i] == self.bet_level);

        if all_active_acted && all_matched {
            self.done = true;
        }
    }
}

impl TreeGame for Kuhn3Game {
    type E = Kuhn3Edge;
    type T = Kuhn3Turn;

    fn root() -> Self {
        // Default card assignment: J to P1, Q to P2, K to P3
        Self::new([0, 1, 2])
    }

    fn turn(&self) -> Self::T {
        if self.done {
            Kuhn3Turn::Terminal
        } else {
            match self.actor {
                0 => Kuhn3Turn::P1,
                1 => Kuhn3Turn::P2,
                2 => Kuhn3Turn::P3,
                _ => unreachable!(),
            }
        }
    }

    fn apply(&self, edge: Self::E) -> Self {
        self.apply_action(edge)
    }

    fn payoff(&self, turn: Self::T) -> Utility {
        assert!(self.done, "payoff called on non-terminal state");

        let player = turn.index();
        let winner = self.winner();
        let total = self.total_pot() as Utility;
        let contributed = self.pot[player] as Utility;

        if player == winner {
            // Winner gets the pot minus their contribution
            total - contributed
        } else {
            // Loser loses their contribution
            -contributed
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_check_king_wins() {
        // P1=J, P2=Q, P3=K
        let game = Kuhn3Game::new([0, 1, 2]);
        assert_eq!(game.turn(), Kuhn3Turn::P1);

        // P1 checks
        let game = game.apply_action(Kuhn3Edge::Check);
        assert_eq!(game.turn(), Kuhn3Turn::P2);

        // P2 checks
        let game = game.apply_action(Kuhn3Edge::Check);
        assert_eq!(game.turn(), Kuhn3Turn::P3);

        // P3 checks
        let game = game.apply_action(Kuhn3Edge::Check);
        assert_eq!(game.turn(), Kuhn3Turn::Terminal);

        // P3 (King) wins
        assert_eq!(game.winner(), 2);
        assert_eq!(game.payoff(Kuhn3Turn::P3), 2.0); // Wins 3-chip pot, contributed 1
        assert_eq!(game.payoff(Kuhn3Turn::P1), -1.0); // Loses 1 chip
        assert_eq!(game.payoff(Kuhn3Turn::P2), -1.0); // Loses 1 chip
    }

    #[test]
    fn test_bet_and_fold() {
        // P1=K, P2=Q, P3=J
        let game = Kuhn3Game::new([2, 1, 0]);

        // P1 bets (with King)
        let game = game.apply_action(Kuhn3Edge::Bet);

        // P2 folds
        let game = game.apply_action(Kuhn3Edge::Fold);
        assert!(game.has_folded(1));

        // P3 folds
        let game = game.apply_action(Kuhn3Edge::Fold);
        assert!(game.has_folded(2));
        assert!(game.is_done());

        // P1 wins
        assert_eq!(game.winner(), 0);
        // P1 bet (pot[0]=2), others anted (pot[1]=1, pot[2]=1)
        // Total pot = 4, P1 contributed 2, payoff = 4-2 = 2
        assert_eq!(game.payoff(Kuhn3Turn::P1), 2.0);
        assert_eq!(game.payoff(Kuhn3Turn::P2), -1.0);
        assert_eq!(game.payoff(Kuhn3Turn::P3), -1.0);
    }

    #[test]
    fn test_bet_and_call() {
        // P1=J, P2=Q, P3=K
        let game = Kuhn3Game::new([0, 1, 2]);

        // P1 bets (bluffing with Jack)
        let game = game.apply_action(Kuhn3Edge::Bet);

        // P2 calls
        let game = game.apply_action(Kuhn3Edge::Call);

        // P3 calls
        let game = game.apply_action(Kuhn3Edge::Call);
        assert!(game.is_done());

        // P3 (King) wins the 6-chip pot
        assert_eq!(game.winner(), 2);
        assert_eq!(game.payoff(Kuhn3Turn::P3), 4.0); // Wins 6, contributed 2
        assert_eq!(game.payoff(Kuhn3Turn::P1), -2.0); // Loses 2
    }
}
