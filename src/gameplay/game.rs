use super::*;
use crate::cards::*;
use crate::*;
use std::ops::Not;
use std::sync::{OnceLock, RwLock};

type Position = usize;

static GAME_CONFIG: OnceLock<RwLock<TableConfig>> = OnceLock::new();

fn config_lock() -> &'static RwLock<TableConfig> {
    GAME_CONFIG.get_or_init(|| RwLock::new(TableConfig::heads_up()))
}

/// Set the default table configuration used by `Game::root()`.
pub fn set_table_config(config: TableConfig) {
    config.validate().expect("invalid table config");
    *config_lock().write().expect("table config lock") = config;
}

/// Get the current default table configuration used by `Game::root()`.
pub fn current_table_config() -> TableConfig {
    *config_lock().read().expect("table config lock")
}

fn dummy_hole() -> Hole {
    Hole::from((
        Card::from((Rank::Two, Suit::C)),
        Card::from((Rank::Three, Suit::C)),
    ))
}

/// Represents the memoryless state of the game in between actions.
///
/// Multiway-capable (2-10 players) while preserving Copy semantics for MCCFR.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Game {
    config: TableConfig,
    pot: Chips,
    board: Board,
    seats: [Seat; N],
    occupancy: [Occupancy; N],
    acted_this_street: [bool; N],
    button: Position,
    actor: Position,
    last_aggressor: Option<Position>,
    posting_phase: PostingPhase,
    sb_seat: Position,
    bb_seat: Position,
}

impl Default for Game {
    fn default() -> Self {
        Self::new(TableConfig::heads_up())
    }
}

impl Game {
    /// Create a new game with the given table configuration.
    /// This creates the pre-posting state (no blinds posted yet).
    pub fn new(config: TableConfig) -> Self {
        config.validate().expect("invalid table config");

        let mut deck = Deck::new();
        let seats = std::array::from_fn(|i| {
            if i < config.seat_count {
                Seat::from((deck.hole(), config.starting_stack))
            } else {
                let mut seat = Seat::from((dummy_hole(), 0));
                seat.reset_state(State::Folding);
                seat
            }
        });
        let occupancy = std::array::from_fn(|i| {
            if i < config.seat_count {
                Occupancy::Active
            } else {
                Occupancy::Empty
            }
        });
        let acted_this_street = [false; N];

        let button = 0;
        let (sb_seat, bb_seat) = Self::compute_blind_seats(config.seat_count, button, &occupancy);
        let posting_phase = if config.ante > 0 {
            PostingPhase::Antes { next_seat: 0 }
        } else {
            PostingPhase::SmallBlind
        };
        let actor = match posting_phase {
            PostingPhase::Antes { .. } => {
                Self::first_active_from(0, config.seat_count, &occupancy).unwrap_or(0)
            }
            PostingPhase::SmallBlind => sb_seat,
            PostingPhase::BigBlind => bb_seat,
            PostingPhase::Complete => sb_seat,
        };

        Self {
            config,
            pot: 0,
            board: Board::empty(),
            seats,
            occupancy,
            acted_this_street,
            button,
            actor,
            last_aggressor: None,
            posting_phase,
            sb_seat,
            bb_seat,
        }
    }

    /// Start a game at the first decision point (after blinds posted).
    pub fn root() -> Self {
        Self::root_with_config(current_table_config())
    }

    /// Start a game at the first decision point (after blinds posted)
    /// using an explicit table configuration.
    pub fn root_with_config(config: TableConfig) -> Self {
        let mut game = Self::new(config);
        game.complete_posting();
        game
    }

    /// Start a game at the first decision point with explicit per-seat stacks.
    pub fn root_with_stacks(config: TableConfig, stacks: &[Chips]) -> Self {
        let mut game = Self::with_stacks(config, stacks);
        game.complete_posting();
        game
    }

    /// Create a base game with explicit config (before posting).
    pub fn with_config(config: TableConfig) -> Self {
        Self::new(config)
    }

    /// Create a heads-up game with a custom starting stack.
    pub fn with_stack(stack: Chips) -> Self {
        Self::new(TableConfig::heads_up().with_stack(stack))
    }

    /// Create a base game with explicit per-seat stacks before posting.
    pub fn with_stacks(config: TableConfig, stacks: &[Chips]) -> Self {
        config.validate().expect("invalid table config");
        let mut game = Self::new(config);
        let mut deck = Deck::new();
        for i in 0..config.seat_count {
            let stack = *stacks.get(i).unwrap_or(&config.starting_stack);
            game.seats[i] = Seat::from((deck.hole(), stack));
            game.occupancy[i] = Occupancy::Active;
            game.acted_this_street[i] = false;
        }
        game
    }

    /// Create a game with fixed holes for active seats and a custom stack size.
    pub fn with_holes_and_stack(config: TableConfig, holes: &[Hole]) -> Self {
        config.validate().expect("invalid table config");
        let mut game = Self::new(config);
        let mut deck = Deck::new();
        for i in 0..config.seat_count {
            let hole = holes.get(i).copied().unwrap_or_else(|| deck.hole());
            game.seats[i] = Seat::from((hole, config.starting_stack));
            game.occupancy[i] = Occupancy::Active;
            game.acted_this_street[i] = false;
        }
        game
    }

    /// Create a game with fixed holes and explicit per-seat stacks.
    pub fn with_holes_and_stacks(config: TableConfig, holes: &[Hole], stacks: &[Chips]) -> Self {
        config.validate().expect("invalid table config");
        let mut game = Self::new(config);
        let mut deck = Deck::new();
        for i in 0..config.seat_count {
            let hole = holes.get(i).copied().unwrap_or_else(|| deck.hole());
            let stack = *stacks.get(i).unwrap_or(&config.starting_stack);
            game.seats[i] = Seat::from((hole, stack));
            game.occupancy[i] = Occupancy::Active;
            game.acted_this_street[i] = false;
        }
        game
    }

    /// Reset all seats' hole cards to the given hole (used for Recall).
    pub fn wipe(mut self, hole: Hole) -> Self {
        for seat in self.seats.iter_mut().take(self.config.seat_count) {
            seat.reset_cards(hole);
        }
        self
    }
}

impl Game {
    pub fn seat_count(&self) -> usize {
        self.config.seat_count
    }

    pub fn n(&self) -> usize {
        self.config.seat_count
    }

    pub fn pot(&self) -> Chips {
        self.pot
    }

    pub fn seats(&self) -> [Seat; N] {
        self.seats
    }

    pub fn board(&self) -> Board {
        self.board
    }

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

    pub fn actor(&self) -> &Seat {
        self.actor_ref()
    }

    pub fn actor_idx(&self) -> Position {
        self.actor
    }

    pub fn sweat(&self) -> Observation {
        Observation::from((Hand::from(self.actor().cards()), Hand::from(self.board())))
    }

    pub fn dealer(&self) -> Turn {
        Turn::Choice(self.button)
    }

    pub fn button(&self) -> Position {
        self.button
    }

    pub fn sb_seat(&self) -> Position {
        self.sb_seat
    }

    pub fn bb_seat(&self) -> Position {
        self.bb_seat
    }

    pub fn street(&self) -> Street {
        self.board.street()
    }

    pub fn config(&self) -> TableConfig {
        self.config
    }
}

impl Game {
    pub fn consume(&mut self, action: Action) -> Self {
        self.act(action);
        self.clone()
    }

    pub fn apply(&self, action: Action) -> Self {
        assert!(self.is_allowed(&action));
        let mut child = *self;
        child.act(action);
        child
    }

    pub fn legal(&self) -> Vec<Action> {
        if self.must_stop() {
            return vec![];
        }
        if self.must_deal() {
            return vec![self.reveal()];
        }
        if self.must_post() {
            return vec![self.posts()];
        }

        let mut options = Vec::new();
        if self.may_raise() {
            options.push(self.raise());
        }
        if self.may_shove() {
            options.push(self.shove());
        }
        if self.may_call() {
            options.push(self.calls());
        }
        if self.may_fold() {
            options.push(self.folds());
        }
        if self.may_check() {
            options.push(self.check());
        }
        assert!(!options.is_empty());
        options
    }

    pub fn is_allowed(&self, action: &Action) -> bool {
        if self.must_post() {
            return matches!(action, Action::Blind(amount) if *amount == self.to_post());
        }
        match action {
            Action::Raise(raise) => {
                self.may_raise()
                    && self.must_stop().not()
                    && self.must_deal().not()
                    && *raise >= self.to_raise()
                    && *raise <= self.to_shove() - 1
            }
            Action::Draw(cards) => {
                self.must_deal()
                    && self.must_stop().not()
                    && cards.clone().all(|c| self.deck().contains(&c))
                    && cards.count() == self.board().street().next().n_revealed()
            }
            other => self.legal().contains(other),
        }
    }
}

impl Game {
    fn act(&mut self, action: Action) {
        assert!(self.is_allowed(&action));
        if self.must_post() {
            self.post(action);
            return;
        }
        match action {
            Action::Check => {
                self.acted_this_street[self.actor] = true;
                self.next_player();
            }
            Action::Fold => {
                self.fold();
                self.acted_this_street[self.actor] = true;
                self.next_player();
            }
            Action::Call(chips) | Action::Blind(chips) => {
                self.bet(chips);
                self.acted_this_street[self.actor] = true;
                self.next_player();
            }
            Action::Raise(chips) | Action::Shove(chips) => {
                self.bet(chips);
                self.last_aggressor = Some(self.actor);
                self.acted_this_street[self.actor] = true;
                for i in 0..self.config.seat_count {
                    if i != self.actor && self.can_act(i) {
                        self.acted_this_street[i] = false;
                    }
                }
                self.next_player();
            }
            Action::Draw(cards) => {
                self.advance_street(cards);
            }
        }
    }

    fn post(&mut self, action: Action) {
        if let Action::Blind(chips) = action {
            self.bet(chips);
            match self.posting_phase {
                PostingPhase::Antes { next_seat } => {
                    let next = next_seat + 1;
                    if next >= self.config.seat_count {
                        self.posting_phase = PostingPhase::SmallBlind;
                        self.actor = self.sb_seat;
                    } else {
                        let next_actor =
                            Self::first_active_from(next, self.config.seat_count, &self.occupancy)
                                .unwrap_or(self.sb_seat);
                        self.posting_phase = PostingPhase::Antes {
                            next_seat: next_actor,
                        };
                        self.actor = next_actor;
                    }
                }
                PostingPhase::SmallBlind => {
                    self.posting_phase = PostingPhase::BigBlind;
                    self.actor = self.bb_seat;
                }
                PostingPhase::BigBlind => {
                    self.posting_phase = PostingPhase::Complete;
                    self.actor = self.first_to_act_preflop();
                }
                PostingPhase::Complete => {}
            }
        }
    }

    fn bet(&mut self, bet: Chips) {
        assert!(self.actor_ref().stack() >= bet);
        self.pot += bet;
        self.actor_mut().bet(bet);
        if self.actor_ref().stack() == 0 {
            self.allin();
        }
    }

    fn allin(&mut self) {
        self.actor_mut().reset_state(State::Shoving);
    }

    fn fold(&mut self) {
        self.actor_mut().reset_state(State::Folding);
    }

    fn show(&mut self, hand: Hand) {
        self.board.add(hand);
    }

    fn advance_street(&mut self, cards: Hand) {
        self.show(cards);
        self.last_aggressor = None;
        for i in 0..self.config.seat_count {
            self.seats[i].reset_stake();
            self.acted_this_street[i] = false;
        }
        self.actor = self.first_to_act_postflop();
    }
}

impl Game {
    fn next_player(&mut self) {
        if self.is_everyone_alright() {
            return;
        }
        let start = self.actor;
        loop {
            self.actor = (self.actor + 1) % self.config.seat_count;
            if self.actor == start {
                break;
            }
            if self.can_act(self.actor) {
                break;
            }
        }
    }
}

impl Game {
    pub fn must_stop(&self) -> bool {
        self.is_everyone_folding()
            || (self.board.street() == Street::Rive && self.is_betting_round_complete())
    }

    pub fn must_deal(&self) -> bool {
        self.posting_phase == PostingPhase::Complete
            && self.street() != Street::Rive
            && self.is_betting_round_complete()
    }

    pub fn must_post(&self) -> bool {
        self.posting_phase != PostingPhase::Complete
    }

    fn is_everyone_alright(&self) -> bool {
        self.is_everyone_calling() || self.is_everyone_folding() || self.is_everyone_shoving()
    }

    fn is_everyone_calling(&self) -> bool {
        self.is_everyone_touched() && self.is_everyone_matched()
    }

    fn is_everyone_touched(&self) -> bool {
        self.active_betting_indices()
            .iter()
            .all(|&i| self.acted_this_street[i])
    }

    fn is_everyone_matched(&self) -> bool {
        let stake = self.effective_stake();
        self.active_betting_indices()
            .iter()
            .all(|&i| self.seats[i].stake() == stake)
    }

    fn is_everyone_shoving(&self) -> bool {
        let in_hand = self.in_hand_indices();
        !in_hand.is_empty()
            && in_hand
                .iter()
                .all(|&i| self.seats[i].state() == State::Shoving)
    }

    fn is_everyone_folding(&self) -> bool {
        self.in_hand_indices().len() <= 1
    }

    pub fn is_betting_round_complete(&self) -> bool {
        if self.is_everyone_folding() || self.is_everyone_shoving() {
            return true;
        }
        let active_betting = self.active_betting_indices();
        if active_betting.is_empty() {
            return true;
        }
        if !active_betting.iter().all(|&i| self.acted_this_street[i]) {
            return false;
        }
        let effective = self.effective_stake();
        if !active_betting
            .iter()
            .all(|&i| self.seats[i].stake() == effective)
        {
            return false;
        }
        if let Some(aggressor) = self.last_aggressor {
            if let Some(agg_pos) = active_betting.iter().position(|&i| i == aggressor) {
                for i in 1..active_betting.len() {
                    let check_idx = active_betting[(agg_pos + i) % active_betting.len()];
                    if !self.acted_this_street[check_idx] {
                        return false;
                    }
                }
            }
        }
        true
    }
}

impl Game {
    pub fn may_fold(&self) -> bool {
        matches!(self.turn(), Turn::Choice(_)) && self.to_call() > 0
    }

    pub fn may_call(&self) -> bool {
        matches!(self.turn(), Turn::Choice(_))
            && self.may_fold()
            && self.to_call() < self.to_shove()
    }

    pub fn may_check(&self) -> bool {
        matches!(self.turn(), Turn::Choice(_)) && self.effective_stake() == self.actor_ref().stake()
    }

    pub fn may_raise(&self) -> bool {
        matches!(self.turn(), Turn::Choice(_)) && self.to_raise() < self.to_shove()
    }

    pub fn may_shove(&self) -> bool {
        matches!(self.turn(), Turn::Choice(_)) && self.to_shove() > 0
    }
}

impl Game {
    pub fn to_call(&self) -> Chips {
        self.effective_stake() - self.actor_ref().stake()
    }

    pub fn to_post(&self) -> Chips {
        match self.posting_phase {
            PostingPhase::Antes { .. } => self.config.ante.min(self.actor_ref().stack()),
            PostingPhase::SmallBlind => self.config.small_blind.min(self.actor_ref().stack()),
            PostingPhase::BigBlind => self.config.big_blind.min(self.actor_ref().stack()),
            PostingPhase::Complete => 0,
        }
    }

    pub fn to_shove(&self) -> Chips {
        self.actor_ref().stack()
    }

    pub fn to_raise(&self) -> Chips {
        let mut stakes: Vec<Chips> = self
            .in_hand_indices()
            .iter()
            .map(|&i| self.seats[i].stake())
            .collect();
        stakes.sort_unstable();
        let most = stakes.last().copied().unwrap_or(0);
        let second = if stakes.len() >= 2 {
            stakes[stakes.len() - 2]
        } else {
            0
        };
        let relative_raise = most - self.actor_ref().stake();
        let marginal_raise = most - second;
        let required_raise = std::cmp::max(marginal_raise, self.config.big_blind);
        relative_raise + required_raise
    }

    pub fn raise(&self) -> Action {
        Action::Raise(self.to_raise())
    }

    pub fn shove(&self) -> Action {
        Action::Shove(self.to_shove())
    }

    pub fn calls(&self) -> Action {
        Action::Call(self.to_call())
    }

    pub fn posts(&self) -> Action {
        Action::Blind(self.to_post())
    }

    pub fn folds(&self) -> Action {
        Action::Fold
    }

    pub fn check(&self) -> Action {
        Action::Check
    }

    pub fn passive(&self) -> Action {
        if self.may_check() {
            Action::Check
        } else {
            Action::Fold
        }
    }

    pub fn reveal(&self) -> Action {
        Action::Draw(self.deck().deal(self.street()))
    }
}

impl Game {
    pub fn settlements(&self) -> Vec<Settlement> {
        assert!(self.must_stop(), "non terminal game state:\n{}", self);
        Showdown::from(self.ledger()).settle()
    }

    fn ledger(&self) -> Vec<Settlement> {
        (0..self.config.seat_count)
            .map(|position| self.settlement(position))
            .collect()
    }

    fn settlement(&self, position: usize) -> Settlement {
        let seat = &self.seats[position];
        let strength = Strength::from(Hand::add(
            Hand::from(seat.cards()),
            Hand::from(self.board()),
        ));
        Settlement::from((seat.spent(), seat.state(), strength))
    }
}

impl Game {
    pub fn draw(&self) -> Hand {
        self.deck().deal(self.street())
    }

    pub fn deck(&self) -> Deck {
        let mut removed = Hand::from(self.board);
        for i in 0..self.config.seat_count {
            if self.occupancy[i] == Occupancy::Active {
                let hole = Hand::from(self.seats[i].cards());
                removed = Hand::or(removed, hole);
            }
        }
        Deck::from(removed.complement())
    }
}

impl Game {
    fn actor_ref(&self) -> &Seat {
        &self.seats[self.actor]
    }

    fn actor_mut(&mut self) -> &mut Seat {
        &mut self.seats[self.actor]
    }
}

impl Game {
    fn effective_stake(&self) -> Chips {
        self.in_hand_indices()
            .iter()
            .map(|&i| self.seats[i].stake())
            .max()
            .unwrap_or(0)
    }
}

impl Game {
    pub const fn blinds() -> [Action; 2] {
        [Action::Blind(Self::sblind()), Action::Blind(Self::bblind())]
    }

    pub const fn bblind() -> Chips {
        crate::B_BLIND
    }

    pub const fn sblind() -> Chips {
        crate::S_BLIND
    }
}

impl Game {
    pub fn actionize(&self, edge: &crate::gameplay::edge::Edge) -> Action {
        crate::mccfr::Info::actionize(self, *edge)
    }

    pub fn edgify(&self, action: Action) -> crate::gameplay::edge::Edge {
        crate::mccfr::Info::edgify(self, action, 0)
    }
}

impl Game {
    fn active_betting_indices(&self) -> Vec<usize> {
        (0..self.config.seat_count)
            .filter(|&i| self.can_act(i))
            .collect()
    }

    fn in_hand_indices(&self) -> Vec<usize> {
        (0..self.config.seat_count)
            .filter(|&i| self.is_in_hand(i))
            .collect()
    }

    fn can_act(&self, idx: usize) -> bool {
        self.occupancy[idx] == Occupancy::Active && self.seats[idx].state() == State::Betting
    }

    fn is_in_hand(&self, idx: usize) -> bool {
        self.occupancy[idx] == Occupancy::Active && self.seats[idx].state() != State::Folding
    }

    fn compute_blind_seats(
        seat_count: usize,
        button: usize,
        occupancy: &[Occupancy; N],
    ) -> (usize, usize) {
        let occupied: Vec<usize> = (0..seat_count)
            .filter(|&i| occupancy[i] == Occupancy::Active)
            .collect();
        if occupied.len() < 2 {
            panic!("need at least 2 active seats for blinds");
        }
        let btn_pos = occupied.iter().position(|&x| x == button).unwrap_or(0);
        if occupied.len() == 2 {
            let sb = occupied[btn_pos];
            let bb = occupied[(btn_pos + 1) % occupied.len()];
            (sb, bb)
        } else {
            let sb = occupied[(btn_pos + 1) % occupied.len()];
            let bb = occupied[(btn_pos + 2) % occupied.len()];
            (sb, bb)
        }
    }

    fn first_active_from(
        start: usize,
        seat_count: usize,
        occupancy: &[Occupancy; N],
    ) -> Option<usize> {
        for i in 0..seat_count {
            let idx = (start + i) % seat_count;
            if occupancy[idx] == Occupancy::Active {
                return Some(idx);
            }
        }
        None
    }

    fn first_to_act_preflop(&self) -> usize {
        let occupied: Vec<usize> = (0..self.config.seat_count)
            .filter(|&i| self.can_act(i))
            .collect();
        if occupied.len() <= 2 {
            self.sb_seat
        } else {
            let bb_pos = occupied
                .iter()
                .position(|&x| x == self.bb_seat)
                .unwrap_or(0);
            occupied[(bb_pos + 1) % occupied.len()]
        }
    }

    fn first_to_act_postflop(&self) -> usize {
        let occupied: Vec<usize> = (0..self.config.seat_count)
            .filter(|&i| self.can_act(i))
            .collect();
        if occupied.is_empty() {
            return self.sb_seat;
        }
        let btn_pos = occupied.iter().position(|&x| x == self.button).unwrap_or(0);
        occupied[(btn_pos + 1) % occupied.len()]
    }

    fn complete_posting(&mut self) {
        if self.config.ante > 0 {
            for i in 0..self.config.seat_count {
                if self.occupancy[i] == Occupancy::Active {
                    let ante = self.config.ante.min(self.seats[i].stack());
                    if ante > 0 {
                        self.pot += ante;
                        self.seats[i].bet(ante);
                        if self.seats[i].stack() == 0 {
                            self.seats[i].reset_state(State::Shoving);
                        }
                    }
                }
            }
        }

        let sb_amount = self
            .config
            .small_blind
            .min(self.seats[self.sb_seat].stack());
        if sb_amount > 0 {
            self.pot += sb_amount;
            self.seats[self.sb_seat].bet(sb_amount);
            if self.seats[self.sb_seat].stack() == 0 {
                self.seats[self.sb_seat].reset_state(State::Shoving);
            }
        }

        let bb_amount = self.config.big_blind.min(self.seats[self.bb_seat].stack());
        if bb_amount > 0 {
            self.pot += bb_amount;
            self.seats[self.bb_seat].bet(bb_amount);
            if self.seats[self.bb_seat].stack() == 0 {
                self.seats[self.bb_seat].reset_state(State::Shoving);
            }
        }

        self.posting_phase = PostingPhase::Complete;
        self.actor = self.first_to_act_preflop();
    }

    fn move_button(&mut self) {
        let occupied: Vec<usize> = (0..self.config.seat_count)
            .filter(|&i| self.occupancy[i] == Occupancy::Active)
            .collect();
        if occupied.len() < 2 {
            return;
        }
        let btn_pos = occupied.iter().position(|&x| x == self.button).unwrap_or(0);
        self.button = occupied[(btn_pos + 1) % occupied.len()];
        let (sb, bb) =
            Self::compute_blind_seats(self.config.seat_count, self.button, &self.occupancy);
        self.sb_seat = sb;
        self.bb_seat = bb;
    }
}

impl Game {
    fn reconcile_occupancy_after_settlement(&mut self) -> usize {
        let mut active = 0;
        for i in 0..self.config.seat_count {
            if self.seats[i].stack() > 0 {
                self.occupancy[i] = Occupancy::Active;
                active += 1;
            } else {
                self.occupancy[i] = Occupancy::Empty;
                self.seats[i].reset_state(State::Folding);
                self.seats[i].reset_stake();
                self.seats[i].reset_spent();
            }
        }
        active
    }

    pub fn next(mut self) -> Option<Self> {
        assert!(self.turn() == Turn::Terminal);
        let settlements = self.settlements();
        for (i, settlement) in settlements.iter().enumerate() {
            self.seats[i].win(settlement.pnl().reward());
        }
        let active = self.reconcile_occupancy_after_settlement();
        if active < 2 {
            return None;
        }
        self.pot = 0;
        self.board.clear();
        self.reset_for_new_hand(None);
        Some(self)
    }

    pub fn next_with_holes(mut self, holes: &[Hole]) -> Option<Self> {
        assert!(self.turn() == Turn::Terminal);
        let settlements = self.settlements();
        for (i, settlement) in settlements.iter().enumerate() {
            self.seats[i].win(settlement.pnl().reward());
        }
        let active = self.reconcile_occupancy_after_settlement();
        if active < 2 {
            return None;
        }
        self.pot = 0;
        self.board.clear();
        self.reset_for_new_hand(Some(holes));
        Some(self)
    }

    fn reset_for_new_hand(&mut self, holes: Option<&[Hole]>) {
        self.move_button();
        let mut deck = Deck::new();
        for i in 0..self.config.seat_count {
            if self.occupancy[i] == Occupancy::Active {
                let hole = holes
                    .and_then(|h| h.get(i).copied())
                    .unwrap_or_else(|| deck.hole());
                self.seats[i].reset_state(State::Betting);
                self.seats[i].reset_cards(hole);
                self.seats[i].reset_stake();
                self.seats[i].reset_spent();
                self.acted_this_street[i] = false;
            }
        }
        self.last_aggressor = None;
        self.posting_phase = if self.config.ante > 0 {
            PostingPhase::Antes { next_seat: 0 }
        } else {
            PostingPhase::SmallBlind
        };
        self.actor = match self.posting_phase {
            PostingPhase::Antes { .. } => {
                Self::first_active_from(0, self.config.seat_count, &self.occupancy).unwrap_or(0)
            }
            PostingPhase::SmallBlind => self.sb_seat,
            PostingPhase::BigBlind => self.bb_seat,
            PostingPhase::Complete => self.sb_seat,
        };
        self.complete_posting();
    }
}

impl crate::mccfr::TreeGame for Game {
    type E = crate::gameplay::edge::Edge;
    type T = crate::gameplay::turn::Turn;
    fn root() -> Self {
        Self::root()
    }
    fn turn(&self) -> Self::T {
        self.turn()
    }
    fn apply(&self, edge: Self::E) -> Self {
        self.apply(self.actionize(&edge))
    }
    fn payoff(&self, turn: Self::T) -> crate::Utility {
        if let Some(payout) = current_tournament_payout() {
            let utilities = payout.utilities_for_game(self);
            return utilities.get(turn.position()).copied().unwrap_or(0.0);
        }

        self.settlements()
            .get(turn.position())
            .map(|settlement| settlement.won() as crate::Utility)
            .expect("player index in bounds")
    }
}

impl std::fmt::Display for Game {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        for seat in self.seats.iter().take(self.config.seat_count) {
            writeln!(
                f,
                "{:>3} {:>3} {:<6}",
                seat.state(),
                seat.cards(),
                seat.stack()
            )?;
        }
        writeln!(f, "Pot   {}", self.pot())?;
        writeln!(f, "Board {}", self.board())?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_root_heads_up() {
        let game = Game::root_with_config(TableConfig::heads_up());
        assert_eq!(game.board().street(), Street::Pref);
        assert_eq!(game.actor().state(), State::Betting);
        assert_eq!(game.pot(), Game::sblind() + Game::bblind());
        assert_eq!(game.turn(), Turn::Choice(game.button()));
    }

    #[test]
    fn root_three_handed_positions_and_pot() {
        let config = TableConfig::for_players(3);
        let game = Game::root_with_config(config);

        assert_eq!(game.button(), 0);
        assert_eq!(game.sb_seat(), 1);
        assert_eq!(game.bb_seat(), 2);
        assert_eq!(game.actor_idx(), 0);
        assert_eq!(game.pot(), config.small_blind + config.big_blind);
        assert_eq!(game.seats[1].stake(), config.small_blind);
        assert_eq!(game.seats[2].stake(), config.big_blind);
    }

    #[test]
    fn root_six_handed_positions_and_action_order() {
        let config = TableConfig::for_players(6);
        let game = Game::root_with_config(config);

        assert_eq!(game.button(), 0);
        assert_eq!(game.sb_seat(), 1);
        assert_eq!(game.bb_seat(), 2);
        assert_eq!(game.actor_idx(), 3);
        assert_eq!(game.pot(), config.small_blind + config.big_blind);
    }

    #[test]
    fn root_with_ante_collects_before_blinds() {
        let config = TableConfig::for_players(3).with_ante(1);
        let game = Game::root_with_config(config);

        let expected = 3 * config.ante + config.small_blind + config.big_blind;
        assert_eq!(game.pot(), expected);
        assert_eq!(game.actor_idx(), 0);
    }

    #[test]
    fn heads_up_uses_button_as_small_blind() {
        let game = Game::root_with_config(TableConfig::heads_up());

        assert_eq!(game.button(), 0);
        assert_eq!(game.sb_seat(), 0);
        assert_eq!(game.bb_seat(), 1);
        assert_eq!(game.actor_idx(), 0);
    }

    #[test]
    fn compute_blind_seats_skips_empty_seats() {
        let mut occupancy = [Occupancy::Empty; N];
        occupancy[0] = Occupancy::Active;
        occupancy[1] = Occupancy::Active;
        occupancy[3] = Occupancy::Active;

        let (sb, bb) = Game::compute_blind_seats(4, 0, &occupancy);
        assert_eq!((sb, bb), (1, 3));
    }

    #[test]
    fn move_button_rotates_through_occupied_seats() {
        let mut game = Game::root_with_config(TableConfig::for_players(3));

        assert_eq!(game.button(), 0);
        assert_eq!(game.sb_seat(), 1);
        assert_eq!(game.bb_seat(), 2);

        game.move_button();
        assert_eq!(game.button(), 1);
        assert_eq!(game.sb_seat(), 2);
        assert_eq!(game.bb_seat(), 0);

        game.move_button();
        assert_eq!(game.button(), 2);
        assert_eq!(game.sb_seat(), 0);
        assert_eq!(game.bb_seat(), 1);

        game.move_button();
        assert_eq!(game.button(), 0);
        assert_eq!(game.sb_seat(), 1);
        assert_eq!(game.bb_seat(), 2);
    }

    #[test]
    fn move_button_skips_empty_seats() {
        let mut game = Game::root_with_config(TableConfig::for_players(4));
        game.occupancy[2] = Occupancy::Empty;

        assert_eq!(game.button(), 0);

        game.move_button();
        assert_eq!(game.button(), 1);

        game.move_button();
        assert_eq!(game.button(), 3);

        game.move_button();
        assert_eq!(game.button(), 0);
    }

    #[test]
    fn short_stack_small_blind_posts_partial_and_shoves() {
        let config = TableConfig::for_players(3).with_blinds(2, 4);
        let mut game = Game::with_stacks(
            config,
            &[config.starting_stack, config.small_blind - 1, config.starting_stack],
        );

        game.complete_posting();

        assert_eq!(game.seats[1].state(), State::Shoving);
        assert_eq!(game.seats[1].stack(), 0);
        assert_eq!(game.seats[1].stake(), config.small_blind - 1);
    }

    #[test]
    fn short_stack_big_blind_posts_partial_and_shoves() {
        let config = TableConfig::for_players(3);
        let mut game = Game::with_stacks(config, &[config.starting_stack, config.starting_stack, 1]);

        game.complete_posting();

        assert_eq!(game.seats[2].state(), State::Shoving);
        assert_eq!(game.seats[2].stack(), 0);
        assert_eq!(game.seats[2].stake(), 1);
    }

    #[test]
    fn postflop_action_starts_left_of_button() {
        let config = TableConfig::for_players(3);
        let mut game = Game::root_with_config(config);

        game = game.apply(Action::Call(game.to_call()));
        game = game.apply(Action::Call(game.to_call()));
        game = game.apply(Action::Check);

        assert_eq!(game.turn(), Turn::Chance);
        let Action::Draw(flop) = game.reveal() else {
            panic!("expected flop reveal");
        };
        game = game.apply(Action::Draw(flop));

        assert_eq!(game.actor_idx(), 1);
    }

    #[test]
    fn betting_round_ends_after_aggressor_is_called() {
        let mut game = Game::root_with_config(TableConfig::for_players(3));

        game = game.apply(Action::Raise(game.to_raise()));
        assert_eq!(game.actor_idx(), 1);
        assert!(!game.is_betting_round_complete());

        game = game.apply(Action::Call(game.to_call()));
        assert_eq!(game.actor_idx(), 2);
        assert!(!game.is_betting_round_complete());

        game = game.apply(Action::Call(game.to_call()));
        assert!(game.is_betting_round_complete());
    }

    #[test]
    fn betting_round_reopens_after_reraise() {
        let mut game = Game::root_with_config(TableConfig::for_players(3).with_stack(100));

        game = game.apply(Action::Raise(game.to_raise()));
        assert_eq!(game.last_aggressor, Some(0));

        game = game.apply(Action::Raise(game.to_raise()));
        assert_eq!(game.last_aggressor, Some(1));
        assert!(!game.is_betting_round_complete());

        game = game.apply(Action::Fold);
        assert_eq!(game.actor_idx(), 0);
        assert!(!game.is_betting_round_complete());

        game = game.apply(Action::Call(game.to_call()));
        assert!(game.is_betting_round_complete());
    }

    #[test]
    fn next_removes_busted_and_keeps_short_stack() {
        let config = TableConfig::for_players(3);
        let mut game = Game::root_with_config(config);

        // Force terminal: only seat 2 remains in-hand.
        game.seats[0].reset_state(State::Folding);
        game.seats[1].reset_state(State::Folding);
        game.seats[2].reset_state(State::Betting);
        assert_eq!(game.turn(), Turn::Terminal);

        // Seat 1 busted, seat 2 is short (below BB) but still alive.
        let hole0 = game.seats[0].cards();
        let hole1 = game.seats[1].cards();
        let hole2 = game.seats[2].cards();
        game.seats[0] = Seat::from((hole0, 5));
        game.seats[1] = Seat::from((hole1, 0));
        game.seats[2] = Seat::from((hole2, 1));
        game.seats[0].reset_state(State::Folding);
        game.seats[1].reset_state(State::Folding);
        game.seats[2].reset_state(State::Betting);

        let next = game.next().expect("should continue with 2 active seats");
        assert_eq!(next.occupancy[1], Occupancy::Empty);
        assert_eq!(next.occupancy[2], Occupancy::Active);
    }

    #[test]
    fn next_stops_when_single_active_remains() {
        let config = TableConfig::for_players(3);
        let mut game = Game::root_with_config(config);

        // Force terminal: only seat 2 remains in-hand.
        game.seats[0].reset_state(State::Folding);
        game.seats[1].reset_state(State::Folding);
        game.seats[2].reset_state(State::Betting);
        assert_eq!(game.turn(), Turn::Terminal);

        // Two seats busted, only one has chips.
        let hole0 = game.seats[0].cards();
        let hole1 = game.seats[1].cards();
        let hole2 = game.seats[2].cards();
        game.seats[0] = Seat::from((hole0, 0));
        game.seats[1] = Seat::from((hole1, 0));
        game.seats[2] = Seat::from((hole2, 5));
        game.seats[0].reset_state(State::Folding);
        game.seats[1].reset_state(State::Folding);
        game.seats[2].reset_state(State::Betting);

        assert!(game.next().is_none());
    }
}
