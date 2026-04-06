use crate::Chips;

/// Table configuration for multiway games.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TableConfig {
    /// Number of seats at the table (2-10)
    pub seat_count: usize,
    /// Small blind amount
    pub small_blind: Chips,
    /// Big blind amount
    pub big_blind: Chips,
    /// Ante amount (0 = no ante)
    pub ante: Chips,
    /// Starting stack for each player
    pub starting_stack: Chips,
}

impl Default for TableConfig {
    fn default() -> Self {
        Self {
            seat_count: 2,
            small_blind: crate::S_BLIND,
            big_blind: crate::B_BLIND,
            ante: 0,
            starting_stack: crate::STACK,
        }
    }
}

impl TableConfig {
    /// Create a heads-up table config (2 players)
    pub fn heads_up() -> Self {
        Self::default()
    }

    /// Create a table config for N players
    pub fn for_players(n: usize) -> Self {
        assert!(n >= 2 && n <= 10, "player count must be 2-10");
        Self {
            seat_count: n,
            ..Self::default()
        }
    }

    /// Create a table config with custom blinds
    pub fn with_blinds(mut self, small: Chips, big: Chips) -> Self {
        self.small_blind = small;
        self.big_blind = big;
        self
    }

    /// Create a table config with ante
    pub fn with_ante(mut self, ante: Chips) -> Self {
        self.ante = ante;
        self
    }

    /// Create a table config with custom starting stack
    pub fn with_stack(mut self, stack: Chips) -> Self {
        self.starting_stack = stack;
        self
    }

    /// Validate the config
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.seat_count < 2 || self.seat_count > 10 {
            return Err("seat count must be 2-10");
        }
        if self.small_blind <= 0 {
            return Err("small blind must be positive");
        }
        if self.big_blind <= 0 {
            return Err("big blind must be positive");
        }
        if self.big_blind < self.small_blind {
            return Err("big blind must be >= small blind");
        }
        if self.starting_stack < self.big_blind {
            return Err("starting stack must be >= big blind");
        }
        if self.ante < 0 {
            return Err("ante must be non-negative");
        }
        Ok(())
    }
}

/// Seat occupancy state for multiway tables.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Occupancy {
    /// Seat is empty (no player)
    Empty,
    /// Player is sitting out (not participating in current hand)
    SittingOut,
    /// Player is active in the current hand
    Active,
}

/// Phase of the posting sequence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PostingPhase {
    /// Collecting antes from all players
    Antes { next_seat: usize },
    /// Posting small blind
    SmallBlind,
    /// Posting big blind
    BigBlind,
    /// Posting complete, ready for action
    Complete,
}
