//! Legacy reference engine for multiway table behavior.
//!
//! `Game` in `game.rs` is the canonical runtime and training engine. This file
//! stays test-only for now so its behavior can be compared during the
//! unification work without leaving two public engine surfaces in the crate.

use super::*;
use crate::Chips;
use crate::cards::*;

/// Extended seat state for multiway games.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MultiwaySeat {
    /// Core seat state (from existing Seat)
    pub seat: Seat,
    /// Seat occupancy
    pub occupancy: Occupancy,
    /// Has this player acted this street?
    pub acted_this_street: bool,
    /// Seat index (0-based position at table)
    pub index: usize,
}

impl MultiwaySeat {
    pub fn new(index: usize, hole: Hole, stack: Chips) -> Self {
        Self {
            seat: Seat::from((hole, stack)),
            occupancy: Occupancy::Active,
            acted_this_street: false,
            index,
        }
    }

    pub fn empty(index: usize) -> Self {
        // Create a placeholder seat for empty positions
        let dummy_hole = Hole::from((
            Card::from((crate::cards::Rank::Two, crate::cards::Suit::C)),
            Card::from((crate::cards::Rank::Three, crate::cards::Suit::C)),
        ));
        Self {
            seat: Seat::from((dummy_hole, 0)),
            occupancy: Occupancy::Empty,
            acted_this_street: false,
            index,
        }
    }

    pub fn is_active(&self) -> bool {
        self.occupancy == Occupancy::Active && self.seat.state().is_active()
    }

    pub fn is_in_hand(&self) -> bool {
        self.occupancy == Occupancy::Active && self.seat.state() != State::Folding
    }

    pub fn can_act(&self) -> bool {
        self.occupancy == Occupancy::Active && self.seat.state() == State::Betting
    }

    pub fn reset_for_new_street(&mut self) {
        self.acted_this_street = false;
        self.seat.reset_stake();
    }

    pub fn reset_for_new_hand(&mut self, hole: Hole, stack: Chips) {
        self.seat = Seat::from((hole, stack));
        self.acted_this_street = false;
        if self.occupancy != Occupancy::Empty {
            self.occupancy = Occupancy::Active;
        }
    }
}

/// Multiway game state.
#[derive(Debug, Clone)]
pub struct MultiwayGame {
    /// Table configuration
    pub config: TableConfig,
    /// Current pot
    pot: Chips,
    /// Community board cards
    board: Board,
    /// Seats (indexed by position)
    seats: Vec<MultiwaySeat>,
    /// Button position (0-based seat index)
    button: usize,
    /// Current actor position (0-based seat index)
    actor: usize,
    /// Last aggressor this street (None if no aggression yet)
    last_aggressor: Option<usize>,
    /// Current posting phase
    posting_phase: PostingPhase,
    /// Small blind seat index
    sb_seat: usize,
    /// Big blind seat index
    bb_seat: usize,
}

impl MultiwayGame {
    /// Create a new multiway game with the given configuration.
    pub fn new(config: TableConfig) -> Self {
        config.validate().expect("invalid table config");

        let mut deck = Deck::new();
        let mut seats = Vec::with_capacity(config.seat_count);

        for i in 0..config.seat_count {
            let hole = deck.hole();
            seats.push(MultiwaySeat::new(i, hole, config.starting_stack));
        }

        let button = 0;
        let (sb_seat, bb_seat) = Self::compute_blind_seats(config.seat_count, button, &seats);

        let posting_phase = if config.ante > 0 {
            PostingPhase::Antes { next_seat: 0 }
        } else {
            PostingPhase::SmallBlind
        };

        Self {
            config,
            pot: 0,
            board: Board::empty(),
            seats,
            button,
            actor: sb_seat, // Will be updated when posting starts
            last_aggressor: None,
            posting_phase,
            sb_seat,
            bb_seat,
        }
    }

    /// Create a game from config, starting at the first decision point (after blinds posted).
    pub fn root(config: TableConfig) -> Self {
        let mut game = Self::new(config);
        game.complete_posting();
        game
    }

    /// Complete the posting phase (antes + blinds).
    fn complete_posting(&mut self) {
        // Post antes if configured
        if self.config.ante > 0 {
            for i in 0..self.seats.len() {
                if self.seats[i].is_active() {
                    let ante = self.config.ante.min(self.seats[i].seat.stack());
                    if ante > 0 {
                        self.post_chips(i, ante);
                    }
                }
            }
        }

        // Post small blind
        let sb_amount = self
            .config
            .small_blind
            .min(self.seats[self.sb_seat].seat.stack());
        if sb_amount > 0 {
            self.post_chips(self.sb_seat, sb_amount);
            // Mark as all-in if they couldn't post full blind
            if self.seats[self.sb_seat].seat.stack() == 0 {
                self.seats[self.sb_seat].seat.reset_state(State::Shoving);
            }
        }

        // Post big blind
        let bb_amount = self
            .config
            .big_blind
            .min(self.seats[self.bb_seat].seat.stack());
        if bb_amount > 0 {
            self.post_chips(self.bb_seat, bb_amount);
            // Mark as all-in if they couldn't post full blind
            if self.seats[self.bb_seat].seat.stack() == 0 {
                self.seats[self.bb_seat].seat.reset_state(State::Shoving);
            }
        }

        self.posting_phase = PostingPhase::Complete;

        // Set first actor: UTG for 3+ players, SB for heads-up
        self.actor = self.first_to_act_preflop();
    }

    /// Post chips for a specific seat (used during posting phase).
    fn post_chips(&mut self, seat_idx: usize, amount: Chips) {
        self.pot += amount;
        self.seats[seat_idx].seat.bet(amount);
    }

    /// Compute SB and BB seat indices given button position.
    fn compute_blind_seats(
        _seat_count: usize,
        button: usize,
        seats: &[MultiwaySeat],
    ) -> (usize, usize) {
        // Find occupied seats
        let occupied: Vec<usize> = seats
            .iter()
            .filter(|s| s.occupancy == Occupancy::Active)
            .map(|s| s.index)
            .collect();

        if occupied.len() < 2 {
            panic!("need at least 2 active seats for blinds");
        }

        if occupied.len() == 2 {
            // Heads-up: button is SB, other is BB
            // Find button's position in occupied list
            let btn_pos = occupied.iter().position(|&x| x == button).unwrap_or(0);
            let sb = occupied[btn_pos];
            let bb = occupied[(btn_pos + 1) % occupied.len()];
            (sb, bb)
        } else {
            // 3+ players: SB is left of button, BB is left of SB
            let btn_pos = occupied.iter().position(|&x| x == button).unwrap_or(0);
            let sb = occupied[(btn_pos + 1) % occupied.len()];
            let bb = occupied[(btn_pos + 2) % occupied.len()];
            (sb, bb)
        }
    }

    /// Get the first player to act preflop.
    fn first_to_act_preflop(&self) -> usize {
        let occupied: Vec<usize> = self
            .seats
            .iter()
            .filter(|s| s.can_act())
            .map(|s| s.index)
            .collect();

        if occupied.len() <= 2 {
            // Heads-up: SB (button) acts first preflop
            self.sb_seat
        } else {
            // 3+ players: UTG (left of BB) acts first
            let bb_pos = occupied
                .iter()
                .position(|&x| x == self.bb_seat)
                .unwrap_or(0);
            occupied[(bb_pos + 1) % occupied.len()]
        }
    }

    /// Get the first player to act postflop.
    fn first_to_act_postflop(&self) -> usize {
        let occupied: Vec<usize> = self
            .seats
            .iter()
            .filter(|s| s.can_act())
            .map(|s| s.index)
            .collect();

        if occupied.is_empty() {
            return self.sb_seat;
        }

        // First active player left of button
        let btn_pos = occupied.iter().position(|&x| x == self.button).unwrap_or(0);
        occupied[(btn_pos + 1) % occupied.len()]
    }

    /// Advance to the next player who can act.
    fn next_player(&mut self) {
        let start = self.actor;
        loop {
            self.actor = (self.actor + 1) % self.seats.len();
            if self.actor == start {
                // Full loop without finding anyone - betting round should end
                break;
            }
            if self.seats[self.actor].can_act() {
                break;
            }
        }
    }

    /// Check if the betting round is complete.
    pub fn is_betting_round_complete(&self) -> bool {
        // Everyone folded except one
        if self.is_everyone_folding() {
            return true;
        }

        // Everyone all-in
        if self.is_everyone_shoving() {
            return true;
        }

        // All active players have acted and matched the bet
        let active_betting: Vec<&MultiwaySeat> =
            self.seats.iter().filter(|s| s.can_act()).collect();

        if active_betting.is_empty() {
            return true;
        }

        // Check that all betting players have acted this street
        let all_acted = active_betting.iter().all(|s| s.acted_this_street);
        if !all_acted {
            return false;
        }

        // Check that all are matched to the same stake
        let effective = self.effective_stake();
        let all_matched = active_betting.iter().all(|s| s.seat.stake() == effective);
        if !all_matched {
            return false;
        }

        // If there was an aggressor, make sure everyone after them has acted
        if let Some(aggressor) = self.last_aggressor {
            // Find aggressor position in active order
            let active_order: Vec<usize> = active_betting.iter().map(|s| s.index).collect();
            if let Some(agg_pos) = active_order.iter().position(|&x| x == aggressor) {
                // Everyone after aggressor (wrapping) must have acted
                for i in 1..active_order.len() {
                    let check_idx = active_order[(agg_pos + i) % active_order.len()];
                    if !self.seats[check_idx].acted_this_street {
                        return false;
                    }
                }
            }
        }

        true
    }

    /// Check if only one player remains (everyone else folded).
    fn is_everyone_folding(&self) -> bool {
        self.seats.iter().filter(|s| s.is_in_hand()).count() == 1
    }

    /// Check if all remaining players are all-in.
    fn is_everyone_shoving(&self) -> bool {
        self.seats
            .iter()
            .filter(|s| s.is_in_hand())
            .all(|s| s.seat.state() == State::Shoving)
    }

    /// Get the effective (maximum) stake this street.
    pub fn effective_stake(&self) -> Chips {
        self.seats
            .iter()
            .filter(|s| s.is_in_hand())
            .map(|s| s.seat.stake())
            .max()
            .unwrap_or(0)
    }

    /// Get the amount needed to call for the current actor.
    pub fn to_call(&self) -> Chips {
        let effective = self.effective_stake();
        let my_stake = self.seats[self.actor].seat.stake();
        effective - my_stake
    }

    /// Get the minimum raise amount for the current actor.
    pub fn to_raise(&self) -> Chips {
        let effective = self.effective_stake();
        let my_stake = self.seats[self.actor].seat.stake();
        let relative_raise = effective - my_stake;

        // Find the second-highest stake to compute min raise increment
        let mut stakes: Vec<Chips> = self
            .seats
            .iter()
            .filter(|s| s.is_in_hand())
            .map(|s| s.seat.stake())
            .collect();
        stakes.sort_unstable();
        let second_highest = if stakes.len() >= 2 {
            stakes[stakes.len() - 2]
        } else {
            0
        };

        let marginal_raise = effective - second_highest;
        let required_raise = marginal_raise.max(self.config.big_blind);

        relative_raise + required_raise
    }

    /// Get the shove amount for the current actor.
    pub fn to_shove(&self) -> Chips {
        self.seats[self.actor].seat.stack()
    }

    /// Apply an action to the game.
    pub fn apply(&mut self, action: Action) {
        match action {
            Action::Check => {
                self.seats[self.actor].acted_this_street = true;
                self.next_player();
            }
            Action::Fold => {
                self.seats[self.actor].seat.reset_state(State::Folding);
                self.seats[self.actor].acted_this_street = true;
                self.next_player();
            }
            Action::Call(chips) => {
                self.bet(chips);
                self.seats[self.actor].acted_this_street = true;
                self.next_player();
            }
            Action::Raise(chips) | Action::Shove(chips) => {
                self.bet(chips);
                self.last_aggressor = Some(self.actor);
                self.seats[self.actor].acted_this_street = true;
                // Reset acted_this_street for others since there's been aggression
                for i in 0..self.seats.len() {
                    if i != self.actor && self.seats[i].can_act() {
                        self.seats[i].acted_this_street = false;
                    }
                }
                self.next_player();
            }
            Action::Blind(chips) => {
                self.bet(chips);
                self.next_player();
            }
            Action::Draw(cards) => {
                self.advance_street(cards);
            }
        }
    }

    /// Place a bet for the current actor.
    fn bet(&mut self, amount: Chips) {
        self.pot += amount;
        self.seats[self.actor].seat.bet(amount);
        if self.seats[self.actor].seat.stack() == 0 {
            self.seats[self.actor].seat.reset_state(State::Shoving);
        }
    }

    /// Advance to the next street.
    fn advance_street(&mut self, cards: Hand) {
        self.board.add(cards);
        self.last_aggressor = None;

        // Reset street state for all seats
        for seat in &mut self.seats {
            seat.reset_for_new_street();
        }

        // Set first actor for postflop
        self.actor = self.first_to_act_postflop();
    }

    /// Get the current turn indicator.
    pub fn turn(&self) -> Turn {
        if self.posting_phase != PostingPhase::Complete {
            return Turn::Choice(self.actor);
        }

        if self.is_everyone_folding() {
            return Turn::Terminal;
        }

        if self.board.street() == Street::Rive && self.is_betting_round_complete() {
            return Turn::Terminal;
        }

        if self.is_betting_round_complete() {
            return Turn::Chance;
        }

        Turn::Choice(self.actor)
    }

    // Accessors
    pub fn pot(&self) -> Chips {
        self.pot
    }

    pub fn board(&self) -> Board {
        self.board
    }

    pub fn street(&self) -> Street {
        self.board.street()
    }

    pub fn seats(&self) -> &[MultiwaySeat] {
        &self.seats
    }

    pub fn button(&self) -> usize {
        self.button
    }

    pub fn actor_idx(&self) -> usize {
        self.actor
    }

    pub fn actor(&self) -> &MultiwaySeat {
        &self.seats[self.actor]
    }

    pub fn sb_seat(&self) -> usize {
        self.sb_seat
    }

    pub fn bb_seat(&self) -> usize {
        self.bb_seat
    }

    pub fn last_aggressor(&self) -> Option<usize> {
        self.last_aggressor
    }

    pub fn posting_phase(&self) -> PostingPhase {
        self.posting_phase
    }

    /// Get legal actions for current actor.
    pub fn legal(&self) -> Vec<Action> {
        if self.turn() == Turn::Terminal {
            return vec![];
        }

        if self.turn() == Turn::Chance {
            let cards = Deck::from(self.remaining_cards()).deal(self.board.street());
            return vec![Action::Draw(cards)];
        }

        let mut options = Vec::new();

        let to_call = self.to_call();
        let to_shove = self.to_shove();
        let to_raise = self.to_raise();

        // Check is legal when no bet to call
        if to_call == 0 {
            options.push(Action::Check);
        }

        // Fold is legal when there's a bet to call
        if to_call > 0 {
            options.push(Action::Fold);
        }

        // Call is legal when there's a bet and we have chips
        if to_call > 0 && to_call < to_shove {
            options.push(Action::Call(to_call));
        }

        // Raise is legal when min raise is less than shove
        if to_raise < to_shove {
            options.push(Action::Raise(to_raise));
        }

        // Shove is always legal if we have chips
        if to_shove > 0 {
            options.push(Action::Shove(to_shove));
        }

        options
    }

    /// Get remaining cards in the deck.
    fn remaining_cards(&self) -> Hand {
        let mut used = Hand::from(self.board);
        for seat in &self.seats {
            if seat.occupancy == Occupancy::Active {
                used = Hand::or(used, Hand::from(seat.seat.cards()));
            }
        }
        used.complement()
    }

    /// Move the button to the next occupied seat.
    /// Call this between hands to rotate dealer position.
    pub fn move_button(&mut self) {
        let occupied: Vec<usize> = self
            .seats
            .iter()
            .filter(|s| s.occupancy == Occupancy::Active)
            .map(|s| s.index)
            .collect();

        if occupied.len() < 2 {
            return; // Can't rotate with fewer than 2 players
        }

        // Find current button position in occupied list
        let btn_pos = occupied.iter().position(|&x| x == self.button).unwrap_or(0);

        // Move to next occupied seat
        self.button = occupied[(btn_pos + 1) % occupied.len()];

        // Recompute blind positions
        let (sb, bb) = Self::compute_blind_seats(self.seats.len(), self.button, &self.seats);
        self.sb_seat = sb;
        self.bb_seat = bb;
    }

    /// Start a new hand with the current seat layout.
    /// Deals new cards, resets stacks to starting amount, rotates button,
    /// and posts blinds.
    pub fn new_hand(&mut self) {
        // Move button
        self.move_button();

        // Deal new cards and reset stacks
        let mut deck = Deck::new();
        for seat in &mut self.seats {
            if seat.occupancy == Occupancy::Active {
                let hole = deck.hole();
                seat.reset_for_new_hand(hole, self.config.starting_stack);
            }
        }

        // Reset game state
        self.pot = 0;
        self.board = Board::empty();
        self.last_aggressor = None;

        // Set up posting phase
        self.posting_phase = if self.config.ante > 0 {
            PostingPhase::Antes { next_seat: 0 }
        } else {
            PostingPhase::SmallBlind
        };

        self.actor = self.sb_seat;
    }

    /// Start a new hand and complete posting (ready for first decision).
    pub fn new_hand_ready(&mut self) {
        self.new_hand();
        self.complete_posting();
    }
}
