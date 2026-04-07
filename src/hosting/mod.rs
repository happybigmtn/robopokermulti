mod casino;
mod client;
mod handle;
mod server;

use serde::Serialize;

pub use casino::*;
pub use client::*;
pub use handle::*;
pub use server::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SeatKind {
    Human,
    Open,
    Bot,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SeatAssignment {
    pub seat: usize,
    pub kind: SeatKind,
}

pub(crate) fn seat_assignments(
    seat_count: usize,
    owned_seats: &[usize],
    open_seats: &[usize],
) -> Vec<SeatAssignment> {
    (0..seat_count)
        .map(|seat| SeatAssignment {
            seat,
            kind: if owned_seats.contains(&seat) {
                SeatKind::Human
            } else if open_seats.contains(&seat) {
                SeatKind::Open
            } else {
                SeatKind::Bot
            },
        })
        .collect()
}

pub(crate) fn seat_roles(
    seat_count: usize,
    owned_seats: &[usize],
    open_seats: &[usize],
) -> Vec<&'static str> {
    seat_assignments(seat_count, owned_seats, open_seats)
        .into_iter()
        .map(|assignment| match assignment.kind {
            SeatKind::Human => "human",
            SeatKind::Open => "open",
            SeatKind::Bot => "bot",
        })
        .collect()
}

pub(crate) fn owned_seats_from_assignments(assignments: &[SeatAssignment]) -> Vec<usize> {
    assignments
        .iter()
        .filter(|assignment| assignment.kind == SeatKind::Human)
        .map(|assignment| assignment.seat)
        .collect()
}

pub(crate) fn seat_roles_from_assignments(assignments: &[SeatAssignment]) -> Vec<&'static str> {
    assignments
        .iter()
        .map(|assignment| match assignment.kind {
            SeatKind::Human => "human",
            SeatKind::Open => "open",
            SeatKind::Bot => "bot",
        })
        .collect()
}
