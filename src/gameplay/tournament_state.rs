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

/// First tournament format supported by the lifecycle line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TournamentFormat {
    Freezeout,
}

/// Registration policy owned at the tournament layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TournamentRegistrationConfig {
    pub max_entrants: usize,
    pub starting_stack: Chips,
    pub late_registration_allowed: bool,
}

impl TournamentRegistrationConfig {
    pub fn freezeout(
        max_entrants: usize,
        starting_stack: Chips,
    ) -> Result<Self, TournamentStateError> {
        if max_entrants < 2 {
            return Err(TournamentStateError::InvalidEntrantCapacity { max_entrants });
        }
        if starting_stack <= 0 {
            return Err(TournamentStateError::InvalidStartingStack { starting_stack });
        }
        Ok(Self {
            max_entrants,
            starting_stack,
            late_registration_allowed: false,
        })
    }
}

/// Event-level lifecycle state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TournamentStatus {
    Announced,
    Registering,
    Running,
    OnBreak,
    Balancing,
    FinalTable,
    Completed,
    Cancelled,
}

/// Truthful boundary for between-hand operations.
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
    pub registration: TournamentRegistrationConfig,
    pub blind_schedule: Vec<TournamentBlindLevel>,
    pub payout: TournamentPayout,
}

/// Resume metadata stored above any individual table transcript.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TournamentResumeMetadata {
    pub completed_hands: u64,
    pub next_hand_number: u64,
}

impl Default for TournamentResumeMetadata {
    fn default() -> Self {
        Self {
            completed_hands: 0,
            next_hand_number: 1,
        }
    }
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

/// Final bust-out record for one entrant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TournamentElimination {
    pub place: usize,
    pub table_id: TournamentTableId,
    pub hand_number: u64,
    pub tied_at_boundary: bool,
}

impl TournamentElimination {
    pub fn new(
        place: usize,
        table_id: TournamentTableId,
        hand_number: u64,
        tied_at_boundary: bool,
    ) -> Self {
        Self {
            place,
            table_id,
            hand_number,
            tied_at_boundary,
        }
    }
}

/// Entrant status at the tournament level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TournamentEntrantStatus {
    Registered,
    Active,
    Eliminated(TournamentElimination),
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
    pub fn registered(entrant_id: u64, display_name: &str, stack: Chips) -> Self {
        Self {
            entrant_id: entrant_id.into(),
            display_name: display_name.to_string(),
            stack,
            status: TournamentEntrantStatus::Registered,
            assignment: None,
        }
    }

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

    pub fn eliminated(
        entrant_id: u64,
        display_name: &str,
        stack: Chips,
        elimination: TournamentElimination,
    ) -> Self {
        Self {
            entrant_id: entrant_id.into(),
            display_name: display_name.to_string(),
            stack,
            status: TournamentEntrantStatus::Eliminated(elimination),
            assignment: None,
        }
    }

    pub fn is_registered(&self) -> bool {
        matches!(self.status, TournamentEntrantStatus::Registered)
    }

    pub fn is_active(&self) -> bool {
        matches!(self.status, TournamentEntrantStatus::Active)
    }
}

/// Planned between-hand balancing or seating move.
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

/// Reason the event is waiting at an operator-visible pause boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TournamentPauseReason {
    ScheduledBreak,
    Operator,
}

/// Persisted pause metadata for a paused or break-bound tournament.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TournamentPauseState {
    pub reason: TournamentPauseReason,
    pub resume_status: TournamentStatus,
}

/// Operator-facing summary of event state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TournamentOperatorView {
    pub tournament_id: TournamentId,
    pub status: TournamentStatus,
    pub registration_open: bool,
    pub current_level_index: usize,
    pub current_level: TournamentBlindLevel,
    pub pending_level_index: Option<usize>,
    pub pending_pause_reason: Option<TournamentPauseReason>,
    pub pause_state: Option<TournamentPauseState>,
    pub tables: Vec<TournamentTable>,
    pub registered_entrants: usize,
    pub active_entrants: usize,
    pub eliminated_entrants: usize,
    pub pending_balance_plan: Vec<TournamentSeatMove>,
}

/// Player-facing summary of one entrant's current tournament state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TournamentPlayerView {
    pub tournament_id: TournamentId,
    pub tournament_status: TournamentStatus,
    pub registration_open: bool,
    pub current_level_index: Option<usize>,
    pub current_level: Option<TournamentBlindLevel>,
    pub entrant: TournamentEntrant,
    pub pending_assignment: Option<TableSeatAssignment>,
    pub payout: Option<Utility>,
}

/// Validated tournament state above any one table transcript.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TournamentState {
    pub definition: TournamentDefinition,
    pub status: TournamentStatus,
    pub boundary: TournamentBoundary,
    pub registration_open: bool,
    pub current_level_index: usize,
    pub pending_level_index: Option<usize>,
    pub pending_pause_reason: Option<TournamentPauseReason>,
    pub pause_state: Option<TournamentPauseState>,
    pub resume: TournamentResumeMetadata,
    pub tables: Vec<TournamentTable>,
    pub elimination_order: Vec<TournamentEntrantId>,
    pub pending_balance_plan: Vec<TournamentSeatMove>,
    balancing_resume_status: Option<TournamentStatus>,
    entrants: BTreeMap<TournamentEntrantId, TournamentEntrant>,
}

impl TournamentState {
    /// Creates a validated running tournament snapshot.
    ///
    /// Args:
    ///   definition: Event-level metadata, blind schedule, and payout structure.
    ///   tables: Live tables available for active entrant assignments.
    ///   entrants: Entrants with stacks and current seat ownership.
    ///
    /// Returns:
    ///   A validated tournament state snapshot suitable for resume work.
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
            registration_open: false,
            current_level_index: 0,
            pending_level_index: None,
            pending_pause_reason: None,
            pause_state: None,
            resume: TournamentResumeMetadata::default(),
            tables,
            elimination_order: Vec::new(),
            pending_balance_plan: Vec::new(),
            balancing_resume_status: None,
            entrants,
        })
    }

    /// Creates an announced tournament before registration opens.
    pub fn announced(
        definition: TournamentDefinition,
        tables: Vec<TournamentTable>,
    ) -> Result<Self, TournamentStateError> {
        let entrants = validate_tournament_state(&definition, &tables, Vec::new(), &[])?;
        Ok(Self {
            definition,
            status: TournamentStatus::Announced,
            boundary: TournamentBoundary::BetweenHands,
            registration_open: false,
            current_level_index: 0,
            pending_level_index: None,
            pending_pause_reason: None,
            pause_state: None,
            resume: TournamentResumeMetadata::default(),
            tables,
            elimination_order: Vec::new(),
            pending_balance_plan: Vec::new(),
            balancing_resume_status: None,
            entrants,
        })
    }

    pub fn current_level(&self) -> TournamentBlindLevel {
        self.definition.blind_schedule[self.current_level_index]
    }

    pub fn entrant(&self, entrant_id: u64) -> Option<&TournamentEntrant> {
        self.entrants.get(&TournamentEntrantId::from(entrant_id))
    }

    pub fn entrants(&self) -> impl Iterator<Item = &TournamentEntrant> {
        self.entrants.values()
    }

    pub fn operator_view(&self) -> TournamentOperatorView {
        TournamentOperatorView {
            tournament_id: self.definition.id,
            status: self.status,
            registration_open: self.registration_open,
            current_level_index: self.current_level_index,
            current_level: self.current_level(),
            pending_level_index: self.pending_level_index,
            pending_pause_reason: self.pending_pause_reason,
            pause_state: self.pause_state,
            tables: self.tables.clone(),
            registered_entrants: self.registered_count(),
            active_entrants: self.active_count(),
            eliminated_entrants: self.eliminated_count(),
            pending_balance_plan: self.pending_balance_plan.clone(),
        }
    }

    pub fn player_view(&self, entrant_id: TournamentEntrantId) -> Option<TournamentPlayerView> {
        let entrant = self.entrants.get(&entrant_id)?.clone();
        let pending_assignment = self.pending_balance_plan.iter().find_map(|movement| {
            if movement.entrant_id == entrant_id {
                Some(movement.destination)
            } else {
                None
            }
        });
        let payout = self
            .payouts_by_finish()
            .ok()
            .and_then(|payouts| payouts.get(&entrant_id).copied());
        let current_level = match self.status {
            TournamentStatus::Announced | TournamentStatus::Registering => None,
            _ => Some(self.current_level()),
        };
        let current_level_index = current_level.map(|_| self.current_level_index);
        Some(TournamentPlayerView {
            tournament_id: self.definition.id,
            tournament_status: self.status,
            registration_open: self.registration_open,
            current_level_index,
            current_level,
            entrant,
            pending_assignment,
            payout,
        })
    }

    pub fn open_registration(&mut self) -> Result<(), TournamentStateError> {
        require_status(
            self.status,
            &[TournamentStatus::Announced],
            "open registration",
        )?;
        self.registration_open = true;
        self.status = TournamentStatus::Registering;
        Ok(())
    }

    pub fn close_registration(&mut self) -> Result<(), TournamentStateError> {
        if !self.registration_open {
            return Err(TournamentStateError::RegistrationClosed);
        }
        self.registration_open = false;
        if self.status == TournamentStatus::Registering {
            self.status = TournamentStatus::Announced;
        }
        Ok(())
    }

    pub fn register_entrant(
        &mut self,
        entrant_id: TournamentEntrantId,
        display_name: &str,
    ) -> Result<(), TournamentStateError> {
        if !self.registration_open {
            return Err(TournamentStateError::RegistrationClosed);
        }
        if self.entrants.len() >= self.definition.registration.max_entrants {
            return Err(TournamentStateError::RegistrationFull {
                max_entrants: self.definition.registration.max_entrants,
            });
        }
        if self.entrants.contains_key(&entrant_id) {
            return Err(TournamentStateError::DuplicateEntrant(entrant_id));
        }
        if matches!(
            self.status,
            TournamentStatus::Running | TournamentStatus::FinalTable
        ) && !self.definition.registration.late_registration_allowed
        {
            return Err(TournamentStateError::InvalidStatus {
                action: "late register entrant",
                status: self.status,
            });
        }
        if matches!(
            self.status,
            TournamentStatus::Running | TournamentStatus::FinalTable
        ) {
            require_between_hands(self.boundary, "late registration")?;
        } else {
            require_status(
                self.status,
                &[TournamentStatus::Registering],
                "register entrant",
            )?;
        }

        let entrant = TournamentEntrant::registered(
            entrant_id.0,
            display_name,
            self.definition.registration.starting_stack,
        );
        self.entrants.insert(entrant_id, entrant);
        Ok(())
    }

    pub fn start_event(
        &mut self,
        assignments: &[TournamentSeatMove],
    ) -> Result<(), TournamentStateError> {
        require_status(
            self.status,
            &[TournamentStatus::Announced, TournamentStatus::Registering],
            "start tournament",
        )?;
        let registered = self.registered_count();
        if registered < 2 {
            return Err(TournamentStateError::NotEnoughEntrantsToStart { registered });
        }
        if assignments.len() != registered {
            return Err(TournamentStateError::StartAssignmentsIncomplete {
                expected: registered,
                actual: assignments.len(),
            });
        }
        self.seat_registered_entrants(assignments)?;
        self.status = TournamentStatus::Running;
        self.registration_open = self.definition.registration.late_registration_allowed;
        self.resume.next_hand_number = 1;
        Ok(())
    }

    pub fn seat_registered_entrants(
        &mut self,
        assignments: &[TournamentSeatMove],
    ) -> Result<(), TournamentStateError> {
        require_between_hands(self.boundary, "seat registered entrants")?;
        validate_seat_moves(
            &self.tables,
            &self.entrants,
            assignments,
            TournamentSeatMoveKind::RegisteredEntrant,
        )?;

        let occupied = self.occupied_seats_without(&BTreeSet::new());
        for movement in assignments {
            if occupied.contains(&(
                movement.destination.table_id,
                movement.destination.seat_index,
            )) {
                return Err(TournamentStateError::SeatOccupied {
                    table_id: movement.destination.table_id,
                    seat_index: movement.destination.seat_index,
                });
            }
        }

        for movement in assignments {
            let entrant = self
                .entrants
                .get_mut(&movement.entrant_id)
                .ok_or(TournamentStateError::UnknownEntrant(movement.entrant_id))?;
            entrant.status = TournamentEntrantStatus::Active;
            entrant.assignment = Some(movement.destination);
        }
        Ok(())
    }

    pub fn start_hand(&mut self) -> Result<(), TournamentStateError> {
        require_status(
            self.status,
            &[TournamentStatus::Running, TournamentStatus::FinalTable],
            "start hand",
        )?;
        require_between_hands(self.boundary, "start hand")?;
        if !self.pending_balance_plan.is_empty() {
            return Err(TournamentStateError::InvalidStatus {
                action: "start hand with pending balance plan",
                status: TournamentStatus::Balancing,
            });
        }
        self.boundary = TournamentBoundary::HandInProgress;
        Ok(())
    }

    pub fn finish_hand(&mut self) -> Result<(), TournamentStateError> {
        if self.boundary != TournamentBoundary::HandInProgress {
            return Err(TournamentStateError::InvalidStatus {
                action: "finish hand",
                status: self.status,
            });
        }
        self.boundary = TournamentBoundary::BetweenHands;
        self.resume.completed_hands += 1;
        self.resume.next_hand_number = self.resume.completed_hands + 1;
        if let Some(level_index) = self.pending_level_index.take() {
            self.current_level_index = level_index;
        }
        if let Some(reason) = self.pending_pause_reason.take() {
            let resume_status = self.play_resume_status();
            self.pause_state = Some(TournamentPauseState {
                reason,
                resume_status,
            });
            self.status = TournamentStatus::OnBreak;
        }
        Ok(())
    }

    pub fn request_level_advance(
        &mut self,
        level_index: usize,
    ) -> Result<(), TournamentStateError> {
        validate_level_index(&self.definition.blind_schedule, level_index)?;
        require_status(
            self.status,
            &[
                TournamentStatus::Running,
                TournamentStatus::FinalTable,
                TournamentStatus::OnBreak,
            ],
            "advance blind level",
        )?;
        if self.boundary == TournamentBoundary::HandInProgress {
            self.pending_level_index = Some(level_index);
        } else {
            self.current_level_index = level_index;
            self.pending_level_index = None;
        }
        Ok(())
    }

    pub fn start_break(&mut self) -> Result<(), TournamentStateError> {
        self.request_pause(TournamentPauseReason::ScheduledBreak)
    }

    pub fn pause_event(&mut self) -> Result<(), TournamentStateError> {
        self.request_pause(TournamentPauseReason::Operator)
    }

    pub fn resume_event(&mut self) -> Result<(), TournamentStateError> {
        require_status(
            self.status,
            &[TournamentStatus::OnBreak],
            "resume tournament",
        )?;
        require_between_hands(self.boundary, "resume tournament")?;
        let pause_state = self
            .pause_state
            .ok_or(TournamentStateError::InvalidStatus {
                action: "resume tournament without pause state",
                status: self.status,
            })?;
        self.status = pause_state.resume_status;
        self.pause_state = None;
        Ok(())
    }

    pub fn publish_balance_plan(
        &mut self,
        moves: &[TournamentSeatMove],
    ) -> Result<(), TournamentStateError> {
        require_status(
            self.status,
            &[TournamentStatus::Running, TournamentStatus::FinalTable],
            "publish balance plan",
        )?;
        require_between_hands(self.boundary, "publish balance plan")?;
        validate_seat_moves(
            &self.tables,
            &self.entrants,
            moves,
            TournamentSeatMoveKind::ActiveEntrant,
        )?;

        let moved_entrants = moves.iter().map(|movement| movement.entrant_id).collect();
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

        self.pending_balance_plan = moves.to_vec();
        self.balancing_resume_status = Some(self.play_resume_status());
        self.status = TournamentStatus::Balancing;
        Ok(())
    }

    pub fn apply_balance_plan(
        &mut self,
        moves: &[TournamentSeatMove],
    ) -> Result<(), TournamentStateError> {
        self.publish_balance_plan(moves)?;
        self.apply_published_balance_plan()
    }

    pub fn apply_published_balance_plan(&mut self) -> Result<(), TournamentStateError> {
        require_status(
            self.status,
            &[TournamentStatus::Balancing],
            "apply balance plan",
        )?;
        let moves = self.pending_balance_plan.clone();
        let moved_entrants = moves.iter().map(|movement| movement.entrant_id).collect();
        let occupancy = self.occupied_seats_without(&moved_entrants);
        for movement in &moves {
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

        for movement in &moves {
            let entrant = self
                .entrants
                .get_mut(&movement.entrant_id)
                .ok_or(TournamentStateError::UnknownEntrant(movement.entrant_id))?;
            entrant.assignment = Some(movement.destination);
        }

        self.pending_balance_plan.clear();
        self.status = self
            .balancing_resume_status
            .take()
            .unwrap_or(TournamentStatus::Running);
        Ok(())
    }

    pub fn record_elimination(
        &mut self,
        entrant_id: TournamentEntrantId,
        table_id: TournamentTableId,
        hand_number: u64,
        tied_at_boundary: bool,
    ) -> Result<(), TournamentStateError> {
        require_between_hands(self.boundary, "record elimination")?;
        let place = self.active_entrant_ids().len();
        let entrant = self
            .entrants
            .get_mut(&entrant_id)
            .ok_or(TournamentStateError::UnknownEntrant(entrant_id))?;
        if !entrant.is_active() {
            return Err(TournamentStateError::EntrantNotActive(entrant_id));
        }
        entrant.status = TournamentEntrantStatus::Eliminated(TournamentElimination::new(
            place,
            table_id,
            hand_number,
            tied_at_boundary,
        ));
        entrant.assignment = None;
        self.elimination_order.push(entrant_id);
        Ok(())
    }

    pub fn collapse_to_final_table(
        &mut self,
        final_table: TournamentTable,
    ) -> Result<(), TournamentStateError> {
        require_between_hands(self.boundary, "collapse to final table")?;
        let active_ids = self.active_entrant_ids_in_seat_order()?;
        if active_ids.len() > final_table.seat_count {
            return Err(TournamentStateError::FinalTableTooSmall {
                needed: active_ids.len(),
                seat_count: final_table.seat_count,
            });
        }

        self.tables = vec![final_table];
        self.pending_balance_plan.clear();
        self.balancing_resume_status = None;
        self.status = TournamentStatus::FinalTable;
        for (seat_index, entrant_id) in active_ids.into_iter().enumerate() {
            let entrant = self
                .entrants
                .get_mut(&entrant_id)
                .ok_or(TournamentStateError::UnknownEntrant(entrant_id))?;
            entrant.assignment = Some(TableSeatAssignment::new(final_table.table_id, seat_index));
        }
        Ok(())
    }

    pub fn complete_event(&mut self) -> Result<(), TournamentStateError> {
        require_between_hands(self.boundary, "complete tournament")?;
        if self.active_count() > 1 {
            return Err(TournamentStateError::IncompleteFinishOrder);
        }
        self.registration_open = false;
        self.pending_level_index = None;
        self.pending_pause_reason = None;
        self.pause_state = None;
        self.pending_balance_plan.clear();
        self.balancing_resume_status = None;
        self.status = TournamentStatus::Completed;
        Ok(())
    }

    pub fn cancel_event(&mut self) {
        self.registration_open = false;
        self.pending_level_index = None;
        self.pending_pause_reason = None;
        self.pause_state = None;
        self.pending_balance_plan.clear();
        self.balancing_resume_status = None;
        self.status = TournamentStatus::Cancelled;
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

    fn request_pause(&mut self, reason: TournamentPauseReason) -> Result<(), TournamentStateError> {
        require_status(
            self.status,
            &[TournamentStatus::Running, TournamentStatus::FinalTable],
            "pause tournament",
        )?;
        if self.boundary == TournamentBoundary::HandInProgress {
            self.pending_pause_reason = Some(reason);
            return Ok(());
        }
        self.pause_state = Some(TournamentPauseState {
            reason,
            resume_status: self.play_resume_status(),
        });
        self.status = TournamentStatus::OnBreak;
        Ok(())
    }

    fn finish_order(&self) -> Result<Vec<TournamentEntrantId>, TournamentStateError> {
        if self.status != TournamentStatus::Completed {
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

    fn play_resume_status(&self) -> TournamentStatus {
        match self.status {
            TournamentStatus::FinalTable => TournamentStatus::FinalTable,
            TournamentStatus::Balancing => self
                .balancing_resume_status
                .unwrap_or(TournamentStatus::Running),
            _ => TournamentStatus::Running,
        }
    }

    fn active_entrant_ids(&self) -> Vec<TournamentEntrantId> {
        self.entrants
            .values()
            .filter(|entrant| entrant.is_active())
            .map(|entrant| entrant.entrant_id)
            .collect()
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

    fn registered_count(&self) -> usize {
        self.entrants
            .values()
            .filter(|entrant| entrant.is_registered())
            .count()
    }

    fn active_count(&self) -> usize {
        self.entrants
            .values()
            .filter(|entrant| entrant.is_active())
            .count()
    }

    fn eliminated_count(&self) -> usize {
        self.entrants.len() - self.registered_count() - self.active_count()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TournamentSeatMoveKind {
    RegisteredEntrant,
    ActiveEntrant,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TournamentStateError {
    EmptyBlindSchedule,
    InvalidEntrantCapacity {
        max_entrants: usize,
    },
    InvalidStartingStack {
        starting_stack: Chips,
    },
    InvalidLevelIndex {
        level_index: usize,
        schedule_len: usize,
    },
    InvalidTableSize {
        table_id: TournamentTableId,
        seat_count: usize,
    },
    InvalidStatus {
        action: &'static str,
        status: TournamentStatus,
    },
    RegistrationClosed,
    RegistrationFull {
        max_entrants: usize,
    },
    NotEnoughEntrantsToStart {
        registered: usize,
    },
    StartAssignmentsIncomplete {
        expected: usize,
        actual: usize,
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
    RegisteredEntrantAssigned(TournamentEntrantId),
    EliminatedEntrantAssigned(TournamentEntrantId),
    RequiresBetweenHands {
        action: &'static str,
    },
    EntrantNotActive(TournamentEntrantId),
    EntrantNotRegistered(TournamentEntrantId),
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
            Self::InvalidEntrantCapacity { max_entrants } => write!(
                f,
                "tournament entrant capacity must be at least 2, got {}",
                max_entrants
            ),
            Self::InvalidStartingStack { starting_stack } => write!(
                f,
                "tournament starting stack must be positive, got {}",
                starting_stack
            ),
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
            Self::InvalidStatus { action, status } => {
                write!(f, "cannot {} while tournament is {:?}", action, status)
            }
            Self::RegistrationClosed => write!(f, "tournament registration is closed"),
            Self::RegistrationFull { max_entrants } => write!(
                f,
                "tournament registration is full at {} entrants",
                max_entrants
            ),
            Self::NotEnoughEntrantsToStart { registered } => write!(
                f,
                "tournament needs at least 2 registered entrants, got {}",
                registered
            ),
            Self::StartAssignmentsIncomplete { expected, actual } => write!(
                f,
                "tournament start needs {} seat assignments but received {}",
                expected, actual
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
            Self::RegisteredEntrantAssigned(entrant_id) => write!(
                f,
                "registered entrant {:?} must not keep a live seat assignment",
                entrant_id
            ),
            Self::EliminatedEntrantAssigned(entrant_id) => write!(
                f,
                "eliminated entrant {:?} must not keep a live seat assignment",
                entrant_id
            ),
            Self::RequiresBetweenHands { action } => {
                write!(f, "{} is only allowed between hands", action)
            }
            Self::EntrantNotActive(entrant_id) => write!(
                f,
                "entrant {:?} is not currently active in the tournament",
                entrant_id
            ),
            Self::EntrantNotRegistered(entrant_id) => write!(
                f,
                "entrant {:?} is not currently waiting for seat activation",
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
    if definition.registration.max_entrants < 2 {
        return Err(TournamentStateError::InvalidEntrantCapacity {
            max_entrants: definition.registration.max_entrants,
        });
    }
    if definition.registration.starting_stack <= 0 {
        return Err(TournamentStateError::InvalidStartingStack {
            starting_stack: definition.registration.starting_stack,
        });
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
            (TournamentEntrantStatus::Registered, Some(_)) => {
                return Err(TournamentStateError::RegisteredEntrantAssigned(
                    entrant.entrant_id,
                ));
            }
            (TournamentEntrantStatus::Registered, None) => {}
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
            (TournamentEntrantStatus::Eliminated(_), Some(_)) => {
                return Err(TournamentStateError::EliminatedEntrantAssigned(
                    entrant.entrant_id,
                ));
            }
            (TournamentEntrantStatus::Eliminated(_), None) => {}
        }
    }

    for entrant_id in elimination_order {
        if !entrant_map.contains_key(entrant_id) {
            return Err(TournamentStateError::UnknownEntrant(*entrant_id));
        }
    }

    Ok(entrant_map)
}

fn validate_level_index(
    blind_schedule: &[TournamentBlindLevel],
    level_index: usize,
) -> Result<(), TournamentStateError> {
    if level_index >= blind_schedule.len() {
        return Err(TournamentStateError::InvalidLevelIndex {
            level_index,
            schedule_len: blind_schedule.len(),
        });
    }
    Ok(())
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

fn validate_seat_moves(
    tables: &[TournamentTable],
    entrants: &BTreeMap<TournamentEntrantId, TournamentEntrant>,
    moves: &[TournamentSeatMove],
    kind: TournamentSeatMoveKind,
) -> Result<(), TournamentStateError> {
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
        validate_assignment(tables, movement.destination)?;
        let entrant = entrants
            .get(&movement.entrant_id)
            .ok_or(TournamentStateError::UnknownEntrant(movement.entrant_id))?;
        match kind {
            TournamentSeatMoveKind::RegisteredEntrant if !entrant.is_registered() => {
                return Err(TournamentStateError::EntrantNotRegistered(
                    movement.entrant_id,
                ));
            }
            TournamentSeatMoveKind::ActiveEntrant if !entrant.is_active() => {
                return Err(TournamentStateError::EntrantNotActive(movement.entrant_id));
            }
            _ => {}
        }
    }
    Ok(())
}

fn require_between_hands(
    boundary: TournamentBoundary,
    action: &'static str,
) -> Result<(), TournamentStateError> {
    if boundary != TournamentBoundary::BetweenHands {
        return Err(TournamentStateError::RequiresBetweenHands { action });
    }
    Ok(())
}

fn require_status(
    status: TournamentStatus,
    allowed: &[TournamentStatus],
    action: &'static str,
) -> Result<(), TournamentStateError> {
    if allowed.contains(&status) {
        Ok(())
    } else {
        Err(TournamentStateError::InvalidStatus { action, status })
    }
}

#[cfg(test)]
mod tests {
    use super::{
        TableSeatAssignment, TournamentBlindLevel, TournamentBoundary, TournamentDefinition,
        TournamentElimination, TournamentEntrant, TournamentEntrantId, TournamentEntrantStatus,
        TournamentFormat, TournamentId, TournamentPauseReason, TournamentRegistrationConfig,
        TournamentResumeMetadata, TournamentSeatMove, TournamentState, TournamentStatus,
        TournamentTable, TournamentTableId,
    };
    use crate::gameplay::TournamentPayout;
    use serde_json::json;

    fn definition() -> TournamentDefinition {
        TournamentDefinition {
            id: TournamentId::new(7),
            format: TournamentFormat::Freezeout,
            registration: TournamentRegistrationConfig::freezeout(6, 1_500).unwrap(),
            blind_schedule: vec![
                TournamentBlindLevel::new(25, 50, 0),
                TournamentBlindLevel::new(50, 100, 10),
            ],
            payout: TournamentPayout::new(vec![0.5, 0.3, 0.2]).unwrap(),
        }
    }

    fn build_running_state() -> TournamentState {
        TournamentState::new(
            definition(),
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

    fn build_announced_state() -> TournamentState {
        TournamentState::announced(
            definition(),
            vec![
                TournamentTable::new(TournamentTableId::new(1), 3).unwrap(),
                TournamentTable::new(TournamentTableId::new(2), 3).unwrap(),
            ],
        )
        .unwrap()
    }

    #[test]
    fn registration_flow_opens_closes_and_starts_with_visible_state() {
        let mut state = build_announced_state();
        state.open_registration().unwrap();
        state
            .register_entrant(TournamentEntrantId::new(1), "alice")
            .unwrap();
        state
            .register_entrant(TournamentEntrantId::new(2), "bob")
            .unwrap();
        state
            .register_entrant(TournamentEntrantId::new(3), "cara")
            .unwrap();
        state.close_registration().unwrap();
        assert_eq!(state.status, TournamentStatus::Announced);
        assert!(!state.registration_open);

        state
            .start_event(&[
                TournamentSeatMove::new(1, TableSeatAssignment::new(TournamentTableId::new(1), 0)),
                TournamentSeatMove::new(2, TableSeatAssignment::new(TournamentTableId::new(1), 1)),
                TournamentSeatMove::new(3, TableSeatAssignment::new(TournamentTableId::new(2), 0)),
            ])
            .unwrap();

        let operator = state.operator_view();
        assert_eq!(operator.status, TournamentStatus::Running);
        assert_eq!(operator.active_entrants, 3);
        assert_eq!(operator.current_level_index, 0);
        assert_eq!(operator.current_level.big_blind, 50);

        let player = state.player_view(TournamentEntrantId::new(2)).unwrap();
        assert_eq!(player.entrant.stack, 1_500);
        assert_eq!(
            player.entrant.assignment,
            Some(TableSeatAssignment::new(TournamentTableId::new(1), 1))
        );
        assert_eq!(player.current_level.unwrap().big_blind, 50);
    }

    #[test]
    fn blind_level_changes_only_after_current_hand_finishes() {
        let mut state = build_running_state();
        state.start_hand().unwrap();
        state.request_level_advance(1).unwrap();

        assert_eq!(state.current_level_index, 0);
        assert_eq!(state.pending_level_index, Some(1));

        state.finish_hand().unwrap();

        assert_eq!(state.boundary, TournamentBoundary::BetweenHands);
        assert_eq!(state.current_level_index, 1);
        assert_eq!(state.current_level().big_blind, 100);
        assert_eq!(state.pending_level_index, None);
    }

    #[test]
    fn entrants_persist_across_table_reassignment() {
        let mut state = TournamentState::new(
            definition(),
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
            .publish_balance_plan(&[TournamentSeatMove::new(
                3,
                TableSeatAssignment::new(TournamentTableId::new(1), 2),
            )])
            .unwrap();
        assert_eq!(state.status, TournamentStatus::Balancing);
        assert_eq!(
            state
                .player_view(TournamentEntrantId::new(3))
                .unwrap()
                .pending_assignment,
            Some(TableSeatAssignment::new(TournamentTableId::new(1), 2))
        );

        state.apply_published_balance_plan().unwrap();

        let entrant = state.entrant(3).unwrap();
        assert_eq!(state.status, TournamentStatus::Running);
        assert_eq!(entrant.stack, 900);
        assert_eq!(
            entrant.assignment.unwrap().table_id,
            TournamentTableId::new(1)
        );
        assert_eq!(entrant.assignment.unwrap().seat_index, 2);
    }

    #[test]
    fn balancing_requires_between_hand_boundary() {
        let mut state = build_running_state();
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
    fn elimination_records_place_table_hand_and_final_payout() {
        let mut state = TournamentState::new(
            definition(),
            vec![TournamentTable::new(TournamentTableId::new(1), 3).unwrap()],
            vec![
                TournamentEntrant::active(1, "alice", 2_100, TournamentTableId::new(1), 0),
                TournamentEntrant::active(2, "bob", 1_400, TournamentTableId::new(1), 1),
                TournamentEntrant::active(3, "cara", 900, TournamentTableId::new(1), 2),
            ],
        )
        .unwrap();

        state
            .record_elimination(
                TournamentEntrantId::new(3),
                TournamentTableId::new(1),
                5,
                false,
            )
            .unwrap();
        let eliminated = state.player_view(TournamentEntrantId::new(3)).unwrap();
        assert_eq!(
            eliminated.entrant.status,
            TournamentEntrantStatus::Eliminated(TournamentElimination::new(
                3,
                TournamentTableId::new(1),
                5,
                false,
            ))
        );

        state
            .record_elimination(
                TournamentEntrantId::new(2),
                TournamentTableId::new(1),
                8,
                false,
            )
            .unwrap();
        state.complete_event().unwrap();

        let winner = state.player_view(TournamentEntrantId::new(1)).unwrap();
        let runner_up = state.player_view(TournamentEntrantId::new(2)).unwrap();
        assert_eq!(winner.payout, Some(0.5));
        assert_eq!(runner_up.payout, Some(0.3));
    }

    #[test]
    fn final_table_transition_preserves_stacks_and_elimination_order() {
        let mut state = TournamentState::new(
            definition(),
            vec![
                TournamentTable::new(TournamentTableId::new(1), 3).unwrap(),
                TournamentTable::new(TournamentTableId::new(2), 3).unwrap(),
            ],
            vec![
                TournamentEntrant::active(1, "alice", 2_100, TournamentTableId::new(1), 0),
                TournamentEntrant::active(2, "bob", 1_400, TournamentTableId::new(1), 1),
                TournamentEntrant::active(3, "cara", 900, TournamentTableId::new(2), 0),
                TournamentEntrant::eliminated(
                    4,
                    "dana",
                    0,
                    TournamentElimination::new(4, TournamentTableId::new(2), 3, false),
                ),
            ],
        )
        .unwrap();
        state.elimination_order = vec![4.into()];
        state
            .collapse_to_final_table(TournamentTable::new(TournamentTableId::new(9), 3).unwrap())
            .unwrap();

        assert_eq!(state.status, TournamentStatus::FinalTable);
        assert_eq!(state.tables.len(), 1);
        assert_eq!(state.tables[0].table_id, TournamentTableId::new(9));
        assert_eq!(state.elimination_order, vec![4.into()]);
        assert_eq!(state.entrant(1).unwrap().stack, 2_100);
        assert_eq!(state.entrant(2).unwrap().stack, 1_400);
        assert_eq!(state.entrant(3).unwrap().stack, 900);
    }

    #[test]
    fn pause_and_resume_preserve_level_metadata_and_assignments() {
        let mut state = build_running_state();
        state.current_level_index = 1;
        state.resume = TournamentResumeMetadata {
            completed_hands: 12,
            next_hand_number: 13,
        };

        state.start_break().unwrap();
        assert_eq!(state.status, TournamentStatus::OnBreak);
        assert_eq!(
            state.pause_state.unwrap().reason,
            TournamentPauseReason::ScheduledBreak
        );

        let encoded = serde_json::to_value(&state).unwrap();
        let decoded: TournamentState = serde_json::from_value(encoded.clone()).unwrap();

        assert_eq!(decoded.status, TournamentStatus::OnBreak);
        assert_eq!(decoded.resume.completed_hands, 12);
        assert_eq!(decoded.current_level_index, 1);
        assert_eq!(
            decoded.entrant(11).unwrap().assignment,
            Some(TableSeatAssignment::new(TournamentTableId::new(1), 0))
        );
        assert_eq!(encoded["definition"]["format"], json!("freezeout"));

        let mut resumed = decoded;
        resumed.resume_event().unwrap();
        assert_eq!(resumed.status, TournamentStatus::Running);
    }
}
