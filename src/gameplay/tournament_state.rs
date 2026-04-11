use crate::gameplay::TournamentPayout;
use crate::{Chips, Utility};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// Stable tournament identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct TournamentId(u64);

impl TournamentId {
    pub fn new(value: u64) -> Self {
        Self(value)
    }
}

impl From<u64> for TournamentId {
    fn from(value: u64) -> Self {
        Self::new(value)
    }
}

/// Stable tournament entrant identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct TournamentEntrantId(u64);

impl TournamentEntrantId {
    pub fn new(value: u64) -> Self {
        Self(value)
    }
}

impl From<u64> for TournamentEntrantId {
    fn from(value: u64) -> Self {
        Self::new(value)
    }
}

/// Stable live-table identifier within a tournament.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct TournamentTableId(u64);

impl TournamentTableId {
    pub fn new(value: u64) -> Self {
        Self(value)
    }
}

impl From<u64> for TournamentTableId {
    fn from(value: u64) -> Self {
        Self::new(value)
    }
}

/// First tournament format supported by the architecture line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TournamentFormat {
    Freezeout,
}

/// Event-level lifecycle state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TournamentStatus {
    Running,
    Paused,
    Complete,
}

/// Truthful boundary for between-hand operations like balancing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TournamentBoundary {
    BetweenHands,
    HandInProgress,
}

/// Blind level owned at the tournament layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TournamentBlindLevel {
    pub small_blind: Chips,
    pub big_blind: Chips,
    pub ante: Chips,
}

impl TournamentBlindLevel {
    pub fn new(small_blind: Chips, big_blind: Chips, ante: Chips) -> Self {
        Self {
            small_blind,
            big_blind,
            ante,
        }
    }
}

/// Event-level definition shared across pause/resume boundaries.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TournamentDefinition {
    pub id: TournamentId,
    pub format: TournamentFormat,
    pub blind_schedule: Vec<TournamentBlindLevel>,
    pub payout: TournamentPayout,
}

/// Resume metadata stored above any individual table transcript.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct TournamentResumeMetadata {
    pub completed_hands: u64,
    pub next_hand_number: u64,
}

/// A currently available live table in the tournament.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TournamentTable {
    pub table_id: TournamentTableId,
    pub seat_count: usize,
}

impl TournamentTable {
    pub fn new(
        table_id: TournamentTableId,
        seat_count: usize,
    ) -> Result<Self, TournamentStateError> {
        if !(2..=10).contains(&seat_count) {
            return Err(TournamentStateError::InvalidTableSize {
                table_id,
                seat_count,
            });
        }
        Ok(Self {
            table_id,
            seat_count,
        })
    }
}

/// Current seat assignment for one entrant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TableSeatAssignment {
    pub table_id: TournamentTableId,
    pub seat_index: usize,
}

impl TableSeatAssignment {
    pub fn new(table_id: TournamentTableId, seat_index: usize) -> Self {
        Self {
            table_id,
            seat_index,
        }
    }
}

/// Entrant status at the tournament level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TournamentEntrantStatus {
    Active,
    Eliminated { place: usize },
}

/// Tournament-owned participant state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TournamentEntrant {
    pub entrant_id: TournamentEntrantId,
    pub display_name: String,
    pub stack: Chips,
    pub status: TournamentEntrantStatus,
    pub assignment: Option<TableSeatAssignment>,
}

impl TournamentEntrant {
    pub fn active(
        entrant_id: u64,
        display_name: &str,
        stack: Chips,
        table_id: TournamentTableId,
        seat_index: usize,
    ) -> Self {
        Self {
            entrant_id: entrant_id.into(),
            display_name: display_name.to_string(),
            stack,
            status: TournamentEntrantStatus::Active,
            assignment: Some(TableSeatAssignment::new(table_id, seat_index)),
        }
    }

    pub fn eliminated(entrant_id: u64, display_name: &str, stack: Chips, place: usize) -> Self {
        Self {
            entrant_id: entrant_id.into(),
            display_name: display_name.to_string(),
            stack,
            status: TournamentEntrantStatus::Eliminated { place },
            assignment: None,
        }
    }

    pub fn is_active(&self) -> bool {
        matches!(self.status, TournamentEntrantStatus::Active)
    }
}

/// Planned between-hand balancing move.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TournamentSeatMove {
    pub entrant_id: TournamentEntrantId,
    pub destination: TableSeatAssignment,
}

impl TournamentSeatMove {
    pub fn new(entrant_id: u64, destination: TableSeatAssignment) -> Self {
        Self {
            entrant_id: entrant_id.into(),
            destination,
        }
    }
}

/// Validated tournament state above any one table transcript.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TournamentState {
    pub definition: TournamentDefinition,
    pub status: TournamentStatus,
    pub boundary: TournamentBoundary,
    pub current_level_index: usize,
    pub resume: TournamentResumeMetadata,
    pub tables: Vec<TournamentTable>,
    pub elimination_order: Vec<TournamentEntrantId>,
    entrants: BTreeMap<TournamentEntrantId, TournamentEntrant>,
}

impl TournamentState {
    /// Creates a validated tournament state snapshot.
    ///
    /// Args:
    ///   definition: Event-level metadata, blind schedule, and payout structure.
    ///   tables: Live tables available for active entrant assignments.
    ///   entrants: Entrants with stacks and current seat ownership.
    ///
    /// Returns:
    ///   A validated tournament state snapshot suitable for balancing/resume work.
    pub fn new(
        definition: TournamentDefinition,
        tables: Vec<TournamentTable>,
        entrants: Vec<TournamentEntrant>,
    ) -> Result<Self, TournamentStateError> {
        let entrants = validate_tournament_state(&definition, &tables, entrants, &[])?;
        Ok(Self {
            definition,
            status: TournamentStatus::Running,
            boundary: TournamentBoundary::BetweenHands,
            current_level_index: 0,
            resume: TournamentResumeMetadata::default(),
            tables,
            elimination_order: Vec::new(),
            entrants,
        })
    }

    pub fn entrant(&self, entrant_id: u64) -> Option<&TournamentEntrant> {
        self.entrants.get(&TournamentEntrantId::from(entrant_id))
    }

    pub fn entrants(&self) -> impl Iterator<Item = &TournamentEntrant> {
        self.entrants.values()
    }

    /// Applies a between-hand balance plan.
    ///
    /// Args:
    ///   moves: Entrant-to-seat moves to apply atomically.
    ///
    /// Returns:
    ///   Ok when every move lands on a valid empty seat at a truthful boundary.
    pub fn apply_balance_plan(
        &mut self,
        moves: &[TournamentSeatMove],
    ) -> Result<(), TournamentStateError> {
        require_between_hands(self.boundary)?;
        if moves.is_empty() {
            return Ok(());
        }

        let mut moved_entrants = BTreeSet::new();
        let mut destinations = BTreeSet::new();
        for movement in moves {
            if !moved_entrants.insert(movement.entrant_id) {
                return Err(TournamentStateError::DuplicateMoveEntrant(
                    movement.entrant_id,
                ));
            }
            if !destinations.insert((
                movement.destination.table_id,
                movement.destination.seat_index,
            )) {
                return Err(TournamentStateError::DuplicateMoveDestination {
                    table_id: movement.destination.table_id,
                    seat_index: movement.destination.seat_index,
                });
            }
            validate_assignment(&self.tables, movement.destination)?;
            let entrant = self
                .entrants
                .get(&movement.entrant_id)
                .ok_or(TournamentStateError::UnknownEntrant(movement.entrant_id))?;
            if !entrant.is_active() {
                return Err(TournamentStateError::EntrantNotActive(movement.entrant_id));
            }
        }

        let occupancy = self.occupied_seats_without(&moved_entrants);
        for movement in moves {
            if occupancy.contains(&(
                movement.destination.table_id,
                movement.destination.seat_index,
            )) {
                return Err(TournamentStateError::SeatOccupied {
                    table_id: movement.destination.table_id,
                    seat_index: movement.destination.seat_index,
                });
            }
        }

        for movement in moves {
            let entrant = self
                .entrants
                .get_mut(&movement.entrant_id)
                .ok_or(TournamentStateError::UnknownEntrant(movement.entrant_id))?;
            entrant.assignment = Some(movement.destination);
        }
        Ok(())
    }

    /// Records one elimination in finish-order sequence.
    pub fn record_elimination(
        &mut self,
        entrant_id: TournamentEntrantId,
    ) -> Result<(), TournamentStateError> {
        let place = self.active_entrant_ids().len();
        let entrant = self
            .entrants
            .get_mut(&entrant_id)
            .ok_or(TournamentStateError::UnknownEntrant(entrant_id))?;
        if !entrant.is_active() {
            return Err(TournamentStateError::EntrantNotActive(entrant_id));
        }
        entrant.status = TournamentEntrantStatus::Eliminated { place };
        entrant.assignment = None;
        self.elimination_order.push(entrant_id);
        Ok(())
    }

    /// Collapses remaining active entrants into one final table.
    ///
    /// Args:
    ///   final_table: The destination final table definition.
    ///
    /// Returns:
    ///   Ok when every active entrant is reseated on the new final table.
    pub fn collapse_to_final_table(
        &mut self,
        final_table: TournamentTable,
    ) -> Result<(), TournamentStateError> {
        require_between_hands(self.boundary)?;
        let active_ids = self.active_entrant_ids_in_seat_order()?;
        if active_ids.len() > final_table.seat_count {
            return Err(TournamentStateError::FinalTableTooSmall {
                needed: active_ids.len(),
                seat_count: final_table.seat_count,
            });
        }

        self.tables = vec![final_table];
        for (seat_index, entrant_id) in active_ids.into_iter().enumerate() {
            let entrant = self
                .entrants
                .get_mut(&entrant_id)
                .ok_or(TournamentStateError::UnknownEntrant(entrant_id))?;
            entrant.assignment = Some(TableSeatAssignment::new(final_table.table_id, seat_index));
        }
        Ok(())
    }

    /// Resolves payout utilities from recorded finish order.
    pub fn payouts_by_finish(
        &self,
    ) -> Result<BTreeMap<TournamentEntrantId, Utility>, TournamentStateError> {
        let finish_order = self.finish_order()?;
        let mut payouts = BTreeMap::new();
        for (index, entrant_id) in finish_order.into_iter().enumerate() {
            let payout = self
                .definition
                .payout
                .payouts()
                .get(index)
                .copied()
                .unwrap_or(0.0);
            payouts.insert(entrant_id, payout);
        }
        Ok(payouts)
    }

    fn finish_order(&self) -> Result<Vec<TournamentEntrantId>, TournamentStateError> {
        if self.status != TournamentStatus::Complete {
            return Err(TournamentStateError::TournamentNotComplete);
        }

        let active_ids = self.active_entrant_ids();
        if active_ids.len() > 1 {
            return Err(TournamentStateError::IncompleteFinishOrder);
        }

        let mut finish_order = active_ids;
        for entrant_id in self.elimination_order.iter().rev().copied() {
            if !finish_order.contains(&entrant_id) {
                finish_order.push(entrant_id);
            }
        }

        if finish_order.len() != self.entrants.len() {
            return Err(TournamentStateError::IncompleteFinishOrder);
        }
        Ok(finish_order)
    }

    fn active_entrant_ids(&self) -> Vec<TournamentEntrantId> {
        let mut ids = Vec::new();
        for entrant in self.entrants.values() {
            if entrant.is_active() {
                ids.push(entrant.entrant_id);
            }
        }
        ids
    }

    fn active_entrant_ids_in_seat_order(
        &self,
    ) -> Result<Vec<TournamentEntrantId>, TournamentStateError> {
        let mut positioned = Vec::new();
        for entrant in self.entrants.values() {
            if !entrant.is_active() {
                continue;
            }
            let assignment =
                entrant
                    .assignment
                    .ok_or(TournamentStateError::MissingActiveAssignment(
                        entrant.entrant_id,
                    ))?;
            positioned.push((
                assignment.table_id,
                assignment.seat_index,
                entrant.entrant_id,
            ));
        }
        positioned.sort();
        Ok(positioned
            .into_iter()
            .map(|(_, _, entrant_id)| entrant_id)
            .collect())
    }

    fn occupied_seats_without(
        &self,
        excluded: &BTreeSet<TournamentEntrantId>,
    ) -> BTreeSet<(TournamentTableId, usize)> {
        let mut occupied = BTreeSet::new();
        for entrant in self.entrants.values() {
            if excluded.contains(&entrant.entrant_id) {
                continue;
            }
            if let Some(assignment) = entrant.assignment {
                occupied.insert((assignment.table_id, assignment.seat_index));
            }
        }
        occupied
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TournamentStateError {
    EmptyBlindSchedule,
    InvalidLevelIndex {
        level_index: usize,
        schedule_len: usize,
    },
    InvalidTableSize {
        table_id: TournamentTableId,
        seat_count: usize,
    },
    DuplicateEntrant(TournamentEntrantId),
    UnknownEntrant(TournamentEntrantId),
    MissingTable(TournamentTableId),
    InvalidSeat {
        table_id: TournamentTableId,
        seat_index: usize,
    },
    DuplicateAssignment {
        table_id: TournamentTableId,
        seat_index: usize,
    },
    MissingActiveAssignment(TournamentEntrantId),
    EliminatedEntrantAssigned(TournamentEntrantId),
    RequiresBetweenHands,
    EntrantNotActive(TournamentEntrantId),
    SeatOccupied {
        table_id: TournamentTableId,
        seat_index: usize,
    },
    DuplicateMoveEntrant(TournamentEntrantId),
    DuplicateMoveDestination {
        table_id: TournamentTableId,
        seat_index: usize,
    },
    FinalTableTooSmall {
        needed: usize,
        seat_count: usize,
    },
    TournamentNotComplete,
    IncompleteFinishOrder,
}

impl std::fmt::Display for TournamentStateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyBlindSchedule => write!(f, "tournament blind schedule must not be empty"),
            Self::InvalidLevelIndex {
                level_index,
                schedule_len,
            } => write!(
                f,
                "current level {} is outside blind schedule of length {}",
                level_index, schedule_len
            ),
            Self::InvalidTableSize {
                table_id,
                seat_count,
            } => write!(
                f,
                "table {:?} has invalid seat count {} (expected 2-10)",
                table_id, seat_count
            ),
            Self::DuplicateEntrant(entrant_id) => {
                write!(f, "duplicate tournament entrant {:?}", entrant_id)
            }
            Self::UnknownEntrant(entrant_id) => {
                write!(f, "unknown tournament entrant {:?}", entrant_id)
            }
            Self::MissingTable(table_id) => write!(f, "unknown tournament table {:?}", table_id),
            Self::InvalidSeat {
                table_id,
                seat_index,
            } => write!(f, "invalid seat {} for table {:?}", seat_index, table_id),
            Self::DuplicateAssignment {
                table_id,
                seat_index,
            } => write!(
                f,
                "seat {} on table {:?} is assigned to more than one entrant",
                seat_index, table_id
            ),
            Self::MissingActiveAssignment(entrant_id) => write!(
                f,
                "active entrant {:?} must have a current table assignment",
                entrant_id
            ),
            Self::EliminatedEntrantAssigned(entrant_id) => write!(
                f,
                "eliminated entrant {:?} must not keep a live seat assignment",
                entrant_id
            ),
            Self::RequiresBetweenHands => {
                write!(f, "tournament balancing is only allowed between hands")
            }
            Self::EntrantNotActive(entrant_id) => write!(
                f,
                "entrant {:?} is not currently active in the tournament",
                entrant_id
            ),
            Self::SeatOccupied {
                table_id,
                seat_index,
            } => write!(
                f,
                "seat {} on table {:?} is already occupied",
                seat_index, table_id
            ),
            Self::DuplicateMoveEntrant(entrant_id) => write!(
                f,
                "balance plan contains entrant {:?} more than once",
                entrant_id
            ),
            Self::DuplicateMoveDestination {
                table_id,
                seat_index,
            } => write!(
                f,
                "balance plan targets seat {} on table {:?} more than once",
                seat_index, table_id
            ),
            Self::FinalTableTooSmall { needed, seat_count } => write!(
                f,
                "final table has {} seats but needs {}",
                seat_count, needed
            ),
            Self::TournamentNotComplete => {
                write!(f, "finish-order payouts require a completed tournament")
            }
            Self::IncompleteFinishOrder => {
                write!(f, "finish order does not cover every tournament entrant")
            }
        }
    }
}

impl std::error::Error for TournamentStateError {}

fn validate_tournament_state(
    definition: &TournamentDefinition,
    tables: &[TournamentTable],
    entrants: Vec<TournamentEntrant>,
    elimination_order: &[TournamentEntrantId],
) -> Result<BTreeMap<TournamentEntrantId, TournamentEntrant>, TournamentStateError> {
    if definition.blind_schedule.is_empty() {
        return Err(TournamentStateError::EmptyBlindSchedule);
    }

    let mut table_sizes = BTreeMap::new();
    for table in tables {
        table_sizes.insert(table.table_id, table.seat_count);
    }

    let mut occupants = BTreeSet::new();
    let mut entrant_map = BTreeMap::new();
    for entrant in entrants {
        if entrant_map
            .insert(entrant.entrant_id, entrant.clone())
            .is_some()
        {
            return Err(TournamentStateError::DuplicateEntrant(entrant.entrant_id));
        }
        match (entrant.status, entrant.assignment) {
            (TournamentEntrantStatus::Active, Some(assignment)) => {
                validate_assignment_against_sizes(&table_sizes, assignment)?;
                if !occupants.insert((assignment.table_id, assignment.seat_index)) {
                    return Err(TournamentStateError::DuplicateAssignment {
                        table_id: assignment.table_id,
                        seat_index: assignment.seat_index,
                    });
                }
            }
            (TournamentEntrantStatus::Active, None) => {
                return Err(TournamentStateError::MissingActiveAssignment(
                    entrant.entrant_id,
                ));
            }
            (TournamentEntrantStatus::Eliminated { .. }, Some(_)) => {
                return Err(TournamentStateError::EliminatedEntrantAssigned(
                    entrant.entrant_id,
                ));
            }
            (TournamentEntrantStatus::Eliminated { .. }, None) => {}
        }
    }

    for entrant_id in elimination_order {
        if !entrant_map.contains_key(entrant_id) {
            return Err(TournamentStateError::UnknownEntrant(*entrant_id));
        }
    }

    Ok(entrant_map)
}

fn validate_assignment(
    tables: &[TournamentTable],
    assignment: TableSeatAssignment,
) -> Result<(), TournamentStateError> {
    let mut table_sizes = BTreeMap::new();
    for table in tables {
        table_sizes.insert(table.table_id, table.seat_count);
    }
    validate_assignment_against_sizes(&table_sizes, assignment)
}

fn validate_assignment_against_sizes(
    table_sizes: &BTreeMap<TournamentTableId, usize>,
    assignment: TableSeatAssignment,
) -> Result<(), TournamentStateError> {
    let seat_count = table_sizes
        .get(&assignment.table_id)
        .copied()
        .ok_or(TournamentStateError::MissingTable(assignment.table_id))?;
    if assignment.seat_index >= seat_count {
        return Err(TournamentStateError::InvalidSeat {
            table_id: assignment.table_id,
            seat_index: assignment.seat_index,
        });
    }
    Ok(())
}

fn require_between_hands(boundary: TournamentBoundary) -> Result<(), TournamentStateError> {
    if boundary != TournamentBoundary::BetweenHands {
        return Err(TournamentStateError::RequiresBetweenHands);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        TableSeatAssignment, TournamentBlindLevel, TournamentBoundary, TournamentDefinition,
        TournamentEntrant, TournamentFormat, TournamentId, TournamentResumeMetadata,
        TournamentSeatMove, TournamentState, TournamentStatus, TournamentTable, TournamentTableId,
    };
    use crate::gameplay::TournamentPayout;
    use serde_json::json;

    fn build_state() -> TournamentState {
        TournamentState::new(
            TournamentDefinition {
                id: TournamentId::new(7),
                format: TournamentFormat::Freezeout,
                blind_schedule: vec![
                    TournamentBlindLevel::new(25, 50, 0),
                    TournamentBlindLevel::new(50, 100, 10),
                ],
                payout: TournamentPayout::new(vec![0.5, 0.3, 0.2]).unwrap(),
            },
            vec![TournamentTable::new(TournamentTableId::new(1), 3).unwrap()],
            vec![TournamentEntrant::active(
                11,
                "alice",
                1_500,
                TournamentTableId::new(1),
                0,
            )],
        )
        .unwrap()
    }

    #[test]
    fn entrants_persist_across_table_reassignment() {
        let mut state = TournamentState::new(
            TournamentDefinition {
                id: TournamentId::new(77),
                format: TournamentFormat::Freezeout,
                blind_schedule: vec![TournamentBlindLevel::new(50, 100, 10)],
                payout: TournamentPayout::new(vec![0.5, 0.3, 0.2]).unwrap(),
            },
            vec![
                TournamentTable::new(TournamentTableId::new(1), 3).unwrap(),
                TournamentTable::new(TournamentTableId::new(2), 3).unwrap(),
            ],
            vec![
                TournamentEntrant::active(1, "alice", 1_500, TournamentTableId::new(1), 0),
                TournamentEntrant::active(2, "bob", 1_250, TournamentTableId::new(1), 1),
                TournamentEntrant::active(3, "cara", 900, TournamentTableId::new(2), 0),
            ],
        )
        .unwrap();

        state
            .apply_balance_plan(&[TournamentSeatMove::new(
                3,
                TableSeatAssignment::new(TournamentTableId::new(1), 2),
            )])
            .unwrap();

        let entrant = state.entrant(3).unwrap();
        assert_eq!(entrant.stack, 900);
        assert_eq!(
            entrant.assignment.unwrap().table_id,
            TournamentTableId::new(1)
        );
        assert_eq!(entrant.assignment.unwrap().seat_index, 2);
    }

    #[test]
    fn balancing_requires_between_hand_boundary() {
        let mut state = build_state();
        state.boundary = TournamentBoundary::HandInProgress;

        let error = state
            .apply_balance_plan(&[TournamentSeatMove::new(
                11,
                TableSeatAssignment::new(TournamentTableId::new(1), 1),
            )])
            .unwrap_err();

        assert!(error.to_string().contains("between hands"));
    }

    #[test]
    fn final_table_transition_preserves_stacks_and_elimination_order() {
        let mut state = TournamentState::new(
            TournamentDefinition {
                id: TournamentId::new(88),
                format: TournamentFormat::Freezeout,
                blind_schedule: vec![TournamentBlindLevel::new(100, 200, 25)],
                payout: TournamentPayout::new(vec![0.5, 0.3, 0.2]).unwrap(),
            },
            vec![
                TournamentTable::new(TournamentTableId::new(1), 3).unwrap(),
                TournamentTable::new(TournamentTableId::new(2), 3).unwrap(),
            ],
            vec![
                TournamentEntrant::active(1, "alice", 2_100, TournamentTableId::new(1), 0),
                TournamentEntrant::active(2, "bob", 1_400, TournamentTableId::new(1), 1),
                TournamentEntrant::active(3, "cara", 900, TournamentTableId::new(2), 0),
                TournamentEntrant::eliminated(4, "dana", 0, 4),
            ],
        )
        .unwrap();
        state.elimination_order = vec![4.into()];
        state
            .collapse_to_final_table(TournamentTable::new(TournamentTableId::new(9), 3).unwrap())
            .unwrap();

        assert_eq!(state.tables.len(), 1);
        assert_eq!(state.tables[0].table_id, TournamentTableId::new(9));
        assert_eq!(state.elimination_order, vec![4.into()]);
        assert_eq!(state.entrant(1).unwrap().stack, 2_100);
        assert_eq!(state.entrant(2).unwrap().stack, 1_400);
        assert_eq!(state.entrant(3).unwrap().stack, 900);
    }

    #[test]
    fn payout_follows_recorded_finish_order() {
        let mut state = TournamentState::new(
            TournamentDefinition {
                id: TournamentId::new(99),
                format: TournamentFormat::Freezeout,
                blind_schedule: vec![TournamentBlindLevel::new(100, 200, 25)],
                payout: TournamentPayout::new(vec![0.5, 0.3, 0.2]).unwrap(),
            },
            vec![TournamentTable::new(TournamentTableId::new(1), 4).unwrap()],
            vec![
                TournamentEntrant::active(1, "alice", 3_000, TournamentTableId::new(1), 0),
                TournamentEntrant::eliminated(2, "bob", 0, 2),
                TournamentEntrant::eliminated(3, "cara", 0, 3),
                TournamentEntrant::eliminated(4, "dana", 0, 4),
            ],
        )
        .unwrap();
        state.status = TournamentStatus::Complete;
        state.elimination_order = vec![4.into(), 3.into(), 2.into()];

        let payouts = state.payouts_by_finish().unwrap();
        assert_eq!(payouts.get(&1.into()).copied(), Some(0.5));
        assert_eq!(payouts.get(&2.into()).copied(), Some(0.3));
        assert_eq!(payouts.get(&3.into()).copied(), Some(0.2));
        assert_eq!(payouts.get(&4.into()).copied(), Some(0.0));
    }

    #[test]
    fn paused_state_round_trips_metadata_and_assignments() {
        let mut state = build_state();
        state.status = TournamentStatus::Paused;
        state.resume = TournamentResumeMetadata {
            completed_hands: 12,
            next_hand_number: 13,
        };
        state.current_level_index = 1;

        let encoded = serde_json::to_value(&state).unwrap();
        let decoded: TournamentState = serde_json::from_value(encoded.clone()).unwrap();

        assert_eq!(decoded.status, TournamentStatus::Paused);
        assert_eq!(decoded.resume.completed_hands, 12);
        assert_eq!(decoded.current_level_index, 1);
        assert_eq!(
            decoded.entrant(11).unwrap().assignment,
            Some(TableSeatAssignment::new(TournamentTableId::new(1), 0))
        );
        assert_eq!(encoded["definition"]["format"], json!("freezeout"));
    }
}
