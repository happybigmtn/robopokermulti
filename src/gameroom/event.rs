use crate::Chips;
use crate::cards::*;
use crate::gameplay::*;

/// Events broadcast by Room to all participants.
/// Clean separation between game actions, meta actions, and revelations.
#[derive(Clone, Debug)]
pub enum Event {
    Play(Action),
    TableState(PublicState),
    NextHand(usize, Meta),
    ShowHand(usize, Hole),
    YourTurn(Recall),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PublicState {
    pub street: Street,
    pub board: Vec<Card>,
    pub pot: Chips,
    pub seat_count: usize,
    pub active_players: usize,
    pub actor: Option<usize>,
    pub seats: Vec<PublicSeat>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PublicSeat {
    pub index: usize,
    pub stack: Chips,
    pub stake: Chips,
    pub spent: Chips,
    pub state: State,
}

/// Meta-actions for table and player management.
/// These are not part of the core poker game logic.
/// Position is lifted to the GameEvent::Meta variant.
#[derive(Clone, Debug)]
pub enum Meta {
    StandUp,
    SitDown,
    CashOut(Chips),
}
