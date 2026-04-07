use super::*;
use crate::gameplay::TableConfig;
use crate::gameroom::*;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;
use tokio::sync::Mutex;
use tokio::sync::RwLock;
use tokio::sync::mpsc::UnboundedReceiver;
use tokio::sync::mpsc::UnboundedSender;

type Tx = UnboundedSender<String>;
type Rx = Arc<Mutex<UnboundedReceiver<String>>>;
type Cache = SnapshotCache;
type Connected = Arc<AtomicBool>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RoomAccessError {
    RoomNotFound,
    InvalidToken,
    WrongGeneration {
        seat: usize,
        expected: u64,
        requested: u64,
    },
    WrongSeat {
        owned_seats: Vec<usize>,
        requested: usize,
    },
}

impl std::fmt::Display for RoomAccessError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::RoomNotFound => write!(f, "room not found"),
            Self::InvalidToken => write!(f, "invalid room access token"),
            Self::WrongGeneration {
                seat,
                expected,
                requested,
            } => write!(
                f,
                "room access generation mismatch for seat {}: expected {}, got {}",
                seat, expected, requested
            ),
            Self::WrongSeat {
                owned_seats,
                requested,
            } => write!(
                f,
                "room access seat mismatch: owned seats {:?}, got seat {}",
                owned_seats, requested
            ),
        }
    }
}

impl std::error::Error for RoomAccessError {}

#[derive(Debug)]
pub enum BridgeError {
    RoomNotFound,
    AlreadyConnected,
    CachedReplayFailed(String),
}

impl std::fmt::Display for BridgeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::RoomNotFound => write!(f, "room not found"),
            Self::AlreadyConnected => write!(f, "room already has an active client connection"),
            Self::CachedReplayFailed(message) => write!(f, "{}", message),
        }
    }
}

impl std::error::Error for BridgeError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClaimSeatError {
    Access(RoomAccessError),
    SeatUnavailable { seat: usize, kind: SeatKind },
    SeatOutOfRange { seat: usize },
}

impl std::fmt::Display for ClaimSeatError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Access(err) => write!(f, "{}", err),
            Self::SeatUnavailable { seat, kind } => {
                write!(
                    f,
                    "seat {} is not claimable (current kind: {:?})",
                    seat, kind
                )
            }
            Self::SeatOutOfRange { seat } => write!(f, "seat {} is out of range", seat),
        }
    }
}

impl std::error::Error for ClaimSeatError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JoinSeatError {
    RoomNotFound,
    SeatUnavailable { seat: usize, kind: SeatKind },
    SeatOutOfRange { seat: usize },
}

impl std::fmt::Display for JoinSeatError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::RoomNotFound => write!(f, "room not found"),
            Self::SeatUnavailable { seat, kind } => {
                write!(
                    f,
                    "seat {} is not joinable (current kind: {:?})",
                    seat, kind
                )
            }
            Self::SeatOutOfRange { seat } => write!(f, "seat {} is out of range", seat),
        }
    }
}

impl std::error::Error for JoinSeatError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LeaveSeatError {
    Access(RoomAccessError),
    SeatUnavailable { seat: usize, kind: SeatKind },
    SeatOutOfRange { seat: usize },
}

impl std::fmt::Display for LeaveSeatError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Access(err) => write!(f, "{}", err),
            Self::SeatUnavailable { seat, kind } => {
                write!(
                    f,
                    "seat {} is not releasable (current kind: {:?})",
                    seat, kind
                )
            }
            Self::SeatOutOfRange { seat } => write!(f, "seat {} is out of range", seat),
        }
    }
}

impl std::error::Error for LeaveSeatError {}

#[derive(Debug)]
pub struct RoomSnapshot {
    pub room_id: RoomId,
    pub invitation_id: String,
    pub owned_seats: Vec<usize>,
    pub seat_accesses: Vec<SeatAccess>,
    pub seat_assignments: Vec<SeatAssignment>,
    pub table: Option<TableConfig>,
    pub connected: bool,
    pub latest_table_state: Option<String>,
    pub latest_decision: Option<String>,
}

#[derive(Debug)]
pub struct PublicRoomSnapshot {
    pub room_id: RoomId,
    pub invitation_id: String,
    pub owned_seats: Vec<usize>,
    pub open_seats: Vec<usize>,
    pub connected_seats: Vec<usize>,
    pub seat_assignments: Vec<SeatAssignment>,
    pub table: Option<TableConfig>,
}

#[derive(Debug)]
pub struct RoomStart {
    pub room_id: RoomId,
    pub invitation_id: String,
    pub owned_seats: Vec<usize>,
    pub seat_accesses: Vec<SeatAccess>,
    pub seat_assignments: Vec<SeatAssignment>,
    pub table: TableConfig,
}

/// Manages active game rooms and their lifecycles.
pub struct Casino {
    rooms: RwLock<HashMap<RoomId, RoomHandle>>,
    count: AtomicU64,
}

impl Default for Casino {
    fn default() -> Self {
        Self {
            rooms: RwLock::new(HashMap::new()),
            count: AtomicU64::new(1),
        }
    }
}

impl Casino {
    pub async fn authorize(
        &self,
        id: RoomId,
        seat: usize,
        generation: u64,
        token: &str,
    ) -> Result<(), RoomAccessError> {
        let rooms = self.rooms.read().await;
        let Some(handle) = rooms.get(&id) else {
            return Err(RoomAccessError::RoomNotFound);
        };
        if !handle.owns_seat(seat) {
            return Err(RoomAccessError::WrongSeat {
                owned_seats: handle.owned_seats(),
                requested: seat,
            });
        }
        let access = handle
            .seat_access(seat)
            .expect("owned seat should have current access");
        if access.generation != generation {
            return Err(RoomAccessError::WrongGeneration {
                seat,
                expected: access.generation,
                requested: generation,
            });
        }
        if !handle.authorize(seat, generation, token) {
            return Err(RoomAccessError::InvalidToken);
        }
        Ok(())
    }

    pub async fn claim_seat(
        &self,
        id: RoomId,
        requester_seat: usize,
        requester_generation: u64,
        requester_token: &str,
        target_seat: usize,
    ) -> Result<SeatAccess, ClaimSeatError> {
        self.authorize(id, requester_seat, requester_generation, requester_token)
            .await
            .map_err(ClaimSeatError::Access)?;
        let rooms = self.rooms.read().await;
        let Some(handle) = rooms.get(&id) else {
            return Err(ClaimSeatError::Access(RoomAccessError::RoomNotFound));
        };
        handle.claim_seat(target_seat).map_err(|err| match err {
            super::SeatClaimError::SeatOutOfRange { seat } => {
                ClaimSeatError::SeatOutOfRange { seat }
            }
            super::SeatClaimError::SeatUnavailable { seat, kind } => {
                ClaimSeatError::SeatUnavailable { seat, kind }
            }
        })
    }

    pub async fn join_open_seat(
        &self,
        id: RoomId,
        target_seat: usize,
    ) -> Result<SeatAccess, JoinSeatError> {
        let rooms = self.rooms.read().await;
        let Some(handle) = rooms.get(&id) else {
            return Err(JoinSeatError::RoomNotFound);
        };
        handle.claim_seat(target_seat).map_err(|err| match err {
            super::SeatClaimError::SeatOutOfRange { seat } => {
                JoinSeatError::SeatOutOfRange { seat }
            }
            super::SeatClaimError::SeatUnavailable { seat, kind } => {
                JoinSeatError::SeatUnavailable { seat, kind }
            }
        })
    }

    pub async fn leave_seat(
        &self,
        id: RoomId,
        seat: usize,
        generation: u64,
        token: &str,
    ) -> Result<RoomSnapshot, LeaveSeatError> {
        self.authorize(id, seat, generation, token)
            .await
            .map_err(LeaveSeatError::Access)?;
        let rooms = self.rooms.read().await;
        let Some(handle) = rooms.get(&id) else {
            return Err(LeaveSeatError::Access(RoomAccessError::RoomNotFound));
        };
        handle.release_seat(seat).map_err(|err| match err {
            super::SeatReleaseError::SeatOutOfRange { seat } => {
                LeaveSeatError::SeatOutOfRange { seat }
            }
            super::SeatReleaseError::SeatUnavailable { seat, kind } => {
                LeaveSeatError::SeatUnavailable { seat, kind }
            }
        })?;
        let snapshot = handle.snapshot(seat);
        let snapshot = snapshot.read().expect("snapshot cache lock");
        Ok(RoomSnapshot {
            room_id: id,
            invitation_id: handle.invitation_id(),
            owned_seats: handle.owned_seats(),
            seat_accesses: handle.seat_accesses(),
            seat_assignments: handle.seat_assignments(),
            table: handle.table_config(),
            connected: handle.is_connected(seat),
            latest_table_state: snapshot.latest_table_state_owned(),
            latest_decision: snapshot.latest_decision_owned(),
        })
    }

    pub async fn snapshot(
        &self,
        id: RoomId,
        seat: usize,
        generation: u64,
        token: &str,
    ) -> Result<RoomSnapshot, RoomAccessError> {
        let rooms = self.rooms.read().await;
        let Some(handle) = rooms.get(&id) else {
            return Err(RoomAccessError::RoomNotFound);
        };
        if !handle.owns_seat(seat) {
            return Err(RoomAccessError::WrongSeat {
                owned_seats: handle.owned_seats(),
                requested: seat,
            });
        }
        let access = handle
            .seat_access(seat)
            .expect("owned seat should have current access");
        if access.generation != generation {
            return Err(RoomAccessError::WrongGeneration {
                seat,
                expected: access.generation,
                requested: generation,
            });
        }
        if !handle.authorize(seat, generation, token) {
            return Err(RoomAccessError::InvalidToken);
        }
        let snapshot = handle.snapshot(seat);
        let snapshot = snapshot.read().expect("snapshot cache lock");
        Ok(RoomSnapshot {
            room_id: id,
            invitation_id: handle.invitation_id(),
            owned_seats: handle.owned_seats(),
            seat_accesses: handle.seat_accesses(),
            seat_assignments: handle.seat_assignments(),
            table: handle.table_config(),
            connected: handle.is_connected(seat),
            latest_table_state: snapshot.latest_table_state_owned(),
            latest_decision: snapshot.latest_decision_owned(),
        })
    }

    pub async fn public_snapshot(&self, id: RoomId) -> Result<PublicRoomSnapshot, RoomAccessError> {
        let rooms = self.rooms.read().await;
        let Some(handle) = rooms.get(&id) else {
            return Err(RoomAccessError::RoomNotFound);
        };
        Ok(PublicRoomSnapshot {
            room_id: id,
            invitation_id: handle.invitation_id(),
            owned_seats: handle.owned_seats(),
            open_seats: handle.open_seats(),
            connected_seats: handle.connected_seats(),
            seat_assignments: handle.seat_assignments(),
            table: handle.table_config(),
        })
    }

    pub async fn public_snapshot_by_invitation(
        &self,
        invitation_id: &str,
    ) -> Result<PublicRoomSnapshot, RoomAccessError> {
        let rooms = self.rooms.read().await;
        let Some((id, handle)) = rooms
            .iter()
            .find(|(_, handle)| handle.invitation_id() == invitation_id)
        else {
            return Err(RoomAccessError::RoomNotFound);
        };
        Ok(PublicRoomSnapshot {
            room_id: *id,
            invitation_id: handle.invitation_id(),
            owned_seats: handle.owned_seats(),
            open_seats: handle.open_seats(),
            connected_seats: handle.connected_seats(),
            seat_assignments: handle.seat_assignments(),
            table: handle.table_config(),
        })
    }

    /// Opens a new room with one HTTP client plus Fish CPU opponents.
    /// Spawns the room task and returns the room ID.
    pub async fn start(
        &self,
        config: TableConfig,
        owned_seats: Vec<usize>,
        open_seats: Vec<usize>,
    ) -> anyhow::Result<RoomStart> {
        let id = self.count.fetch_add(1, Ordering::Relaxed);
        let assignments = seat_assignments(config.seat_count, &owned_seats, &open_seats);
        let (handle, clients) =
            RoomHandle::pair_with_hosted_seats(id, owned_seats.clone(), assignments.clone());
        let mut clients_by_seat = clients
            .into_iter()
            .zip(
                assignments
                    .iter()
                    .filter(|assignment| {
                        assignment.kind == SeatKind::Human || assignment.kind == SeatKind::Open
                    })
                    .map(|assignment| assignment.seat),
            )
            .map(|(client, seat)| (seat, client))
            .collect::<HashMap<_, _>>();
        let mut room = Room::with_config(config);
        for seat in 0..config.seat_count {
            if let Some(client) = clients_by_seat.remove(&seat) {
                room.sit(client);
            } else {
                room.sit(Fish);
            }
        }
        let task = tokio::spawn(async move {
            room.run().await;
            #[allow(unreachable_code)]
            ()
        });
        let invitation_id = handle.invitation_id();
        let owned_seats = handle.owned_seats();
        let seat_accesses = handle.seat_accesses();
        let seat_assignments = handle.seat_assignments();
        handle.set_table_config(config);
        handle.attach_task(task);
        self.rooms.write().await.insert(id, handle);
        Ok(RoomStart {
            room_id: id,
            invitation_id,
            owned_seats,
            seat_accesses,
            seat_assignments,
            table: config,
        })
        .inspect(|_| log::info!("opened room {}", id))
    }

    /// Closes a room and removes it from the casino.
    pub async fn close(&self, id: RoomId) -> anyhow::Result<()> {
        self.rooms
            .write()
            .await
            .remove(&id)
            .map(|handle| {
                handle.abort_task();
                log::info!("closed room {}", id);
            })
            .ok_or_else(|| anyhow::anyhow!("room not found"))
    }

    /// Gets seat-scoped channel endpoints for WebSocket bridging.
    pub async fn channels(
        &self,
        id: RoomId,
        seat: usize,
    ) -> Result<(Tx, Rx, Cache, Connected, tokio::sync::watch::Receiver<u64>), BridgeError> {
        self.rooms
            .read()
            .await
            .get(&id)
            .and_then(|h| h.channels(seat))
            .ok_or(BridgeError::RoomNotFound)
    }

    /// Spawns WebSocket bridge between client and room channels.
    pub async fn bridge(
        &self,
        id: RoomId,
        seat: usize,
        mut session: actix_ws::Session,
        mut stream: actix_ws::MessageStream,
    ) -> Result<(), BridgeError> {
        use futures::StreamExt;
        let (tx, rx, cache, connected, mut disconnect) = self.channels(id, seat).await?;
        log::info!("client connected to room {} seat {}", id, seat);
        if connected.swap(true, Ordering::AcqRel) {
            return Err(BridgeError::AlreadyConnected);
        }
        let cached = cache.read().expect("snapshot cache lock");
        if let Some(json) = cached.latest_table_state() {
            if let Err(err) = session.text(json.to_string()).await {
                connected.store(false, Ordering::Release);
                return Err(BridgeError::CachedReplayFailed(format!(
                    "failed to send cached table state: {}",
                    err
                )));
            }
        }
        if let Some(json) = cached.latest_decision() {
            if let Err(err) = session.text(json.to_string()).await {
                connected.store(false, Ordering::Release);
                return Err(BridgeError::CachedReplayFailed(format!(
                    "failed to send cached decision: {}",
                    err
                )));
            }
        }
        drop(cached);
        actix_web::rt::spawn(async move {
            'sesh: loop {
                tokio::select! {
                    biased;
                    changed = disconnect.changed() => match changed {
                        Ok(()) => break 'sesh,
                        Err(_) => break 'sesh,
                    },
                    msg = async { rx.lock().await.recv().await } => match msg {
                        Some(json) => if session.text(json).await.is_err() { break 'sesh },
                        None => break 'sesh,
                    },
                    msg = stream.next() => match msg {
                        Some(Ok(actix_ws::Message::Text(text))) => if tx.send(text.to_string()).is_err() { break 'sesh },
                        Some(Ok(actix_ws::Message::Close(_))) => break 'sesh,
                        Some(Err(_)) => break 'sesh,
                        None => break 'sesh,
                        _ => continue 'sesh,
                    },
                }
            }
            connected.store(false, Ordering::Release);
        });
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn authorize_rejects_missing_room() {
        let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");
        let casino = Casino::default();

        let err = runtime
            .block_on(async { casino.authorize(999, 0, 1, "token").await })
            .expect_err("missing room should fail");

        assert_eq!(err, RoomAccessError::RoomNotFound);
    }

    #[test]
    fn authorize_rejects_invalid_token() {
        let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");
        let casino = Casino::default();

        runtime.block_on(async {
            let start = casino
                .start(TableConfig::heads_up(), vec![0], Vec::new())
                .await
                .expect("start room");

            let err = casino
                .authorize(
                    start.room_id,
                    start.owned_seats[0],
                    start.seat_accesses[0].generation,
                    "wrong-token",
                )
                .await
                .expect_err("invalid token should fail");
            assert_eq!(err, RoomAccessError::InvalidToken);

            casino.close(start.room_id).await.expect("close room");
        });
    }

    #[test]
    fn authorize_rejects_wrong_seat() {
        let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");
        let casino = Casino::default();

        runtime.block_on(async {
            let start = casino
                .start(TableConfig::heads_up(), vec![0], Vec::new())
                .await
                .expect("start room");

            let err = casino
                .authorize(
                    start.room_id,
                    start.owned_seats[0] + 1,
                    start.seat_accesses[0].generation,
                    &start.seat_accesses[0].token,
                )
                .await
                .expect_err("wrong seat should fail");
            assert_eq!(
                err,
                RoomAccessError::WrongSeat {
                    owned_seats: start.owned_seats.clone(),
                    requested: start.owned_seats[0] + 1,
                }
            );

            casino.close(start.room_id).await.expect("close room");
        });
    }

    #[test]
    fn start_preserves_owned_seat_choice() {
        let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");
        let casino = Casino::default();

        runtime.block_on(async {
            let start = casino
                .start(TableConfig::for_players(4), vec![2], Vec::new())
                .await
                .expect("start room");

            assert_eq!(start.owned_seats, vec![2]);
            assert!(start.invitation_id.starts_with("room-"));
            assert_eq!(start.seat_accesses.len(), 1);
            assert_eq!(start.seat_assignments[2].kind, SeatKind::Human);
            assert_eq!(start.seat_assignments[0].kind, SeatKind::Bot);

            let snapshot = casino
                .snapshot(
                    start.room_id,
                    start.owned_seats[0],
                    start.seat_accesses[0].generation,
                    &start.seat_accesses[0].token,
                )
                .await
                .expect("snapshot");
            assert_eq!(snapshot.owned_seats, vec![2]);
            assert_eq!(snapshot.seat_accesses.len(), 1);
            assert_eq!(snapshot.seat_assignments, start.seat_assignments);

            casino.close(start.room_id).await.expect("close room");
        });
    }

    #[test]
    fn start_accepts_multiple_owned_seats() {
        let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");
        let casino = Casino::default();

        runtime.block_on(async {
            let start = casino
                .start(TableConfig::for_players(4), vec![0, 2], Vec::new())
                .await
                .expect("start room");

            assert_eq!(start.owned_seats, vec![0, 2]);
            assert_eq!(start.seat_accesses.len(), 2);
            assert_eq!(start.seat_assignments[0].kind, SeatKind::Human);
            assert_eq!(start.seat_assignments[1].kind, SeatKind::Bot);
            assert_eq!(start.seat_assignments[2].kind, SeatKind::Human);

            let snapshot = casino
                .snapshot(
                    start.room_id,
                    2,
                    start.seat_accesses[1].generation,
                    &start.seat_accesses[1].token,
                )
                .await
                .expect("snapshot");
            assert_eq!(snapshot.owned_seats, vec![0, 2]);
            assert_eq!(snapshot.seat_accesses.len(), 2);

            casino.close(start.room_id).await.expect("close room");
        });
    }

    #[test]
    fn claim_seat_promotes_open_seat_after_start() {
        let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");
        let casino = Casino::default();

        runtime.block_on(async {
            let start = casino
                .start(TableConfig::for_players(4), vec![0], vec![2])
                .await
                .expect("start room");

            let claimed = casino
                .claim_seat(
                    start.room_id,
                    0,
                    start.seat_accesses[0].generation,
                    &start.seat_accesses[0].token,
                    2,
                )
                .await
                .expect("claim open seat");

            assert_eq!(claimed.seat, 2);

            let snapshot = casino
                .snapshot(start.room_id, 2, claimed.generation, &claimed.token)
                .await
                .expect("snapshot");
            assert_eq!(snapshot.owned_seats, vec![0, 2]);
            assert_eq!(snapshot.seat_assignments[2].kind, SeatKind::Human);

            casino.close(start.room_id).await.expect("close room");
        });
    }

    #[test]
    fn join_open_seat_claims_without_existing_owner_token() {
        let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");
        let casino = Casino::default();

        runtime.block_on(async {
            let start = casino
                .start(TableConfig::for_players(4), vec![0], vec![2])
                .await
                .expect("start room");

            let joined = casino
                .join_open_seat(start.room_id, 2)
                .await
                .expect("join open seat");

            let snapshot = casino
                .snapshot(start.room_id, 2, joined.generation, &joined.token)
                .await
                .expect("snapshot");
            assert_eq!(snapshot.owned_seats, vec![0, 2]);
            assert_eq!(snapshot.seat_assignments[2].kind, SeatKind::Human);

            casino.close(start.room_id).await.expect("close room");
        });
    }

    #[test]
    fn leave_seat_releases_only_that_seat() {
        let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");
        let casino = Casino::default();

        runtime.block_on(async {
            let start = casino
                .start(TableConfig::for_players(4), vec![0, 2], Vec::new())
                .await
                .expect("start room");

            let snapshot = casino
                .leave_seat(
                    start.room_id,
                    2,
                    start.seat_accesses[1].generation,
                    &start.seat_accesses[1].token,
                )
                .await
                .expect("leave seat");

            assert_eq!(snapshot.owned_seats, vec![0]);
            assert_eq!(snapshot.seat_assignments[2].kind, SeatKind::Open);
            assert_eq!(snapshot.seat_assignments[0].kind, SeatKind::Human);

            casino.close(start.room_id).await.expect("close room");
        });
    }

    #[test]
    fn public_snapshot_reports_open_and_owned_seats() {
        let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");
        let casino = Casino::default();

        runtime.block_on(async {
            let start = casino
                .start(TableConfig::for_players(4), vec![0], vec![2])
                .await
                .expect("start room");

            let snapshot = casino
                .public_snapshot(start.room_id)
                .await
                .expect("public snapshot");

            assert_eq!(snapshot.owned_seats, vec![0]);
            assert_eq!(snapshot.invitation_id, start.invitation_id);
            assert_eq!(snapshot.open_seats, vec![2]);
            assert!(snapshot.connected_seats.is_empty());
            assert_eq!(snapshot.seat_assignments[2].kind, SeatKind::Open);

            casino.close(start.room_id).await.expect("close room");
        });
    }

    #[test]
    fn authorize_rejects_wrong_generation() {
        let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");
        let casino = Casino::default();

        runtime.block_on(async {
            let start = casino
                .start(TableConfig::heads_up(), vec![0], Vec::new())
                .await
                .expect("start room");

            let err = casino
                .authorize(
                    start.room_id,
                    start.owned_seats[0],
                    start.seat_accesses[0].generation + 1,
                    &start.seat_accesses[0].token,
                )
                .await
                .expect_err("wrong generation should fail");
            assert_eq!(
                err,
                RoomAccessError::WrongGeneration {
                    seat: start.owned_seats[0],
                    expected: start.seat_accesses[0].generation,
                    requested: start.seat_accesses[0].generation + 1,
                }
            );

            casino.close(start.room_id).await.expect("close room");
        });
    }

    #[test]
    fn join_open_seat_mints_new_generation_after_release() {
        let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");
        let casino = Casino::default();

        runtime.block_on(async {
            let start = casino
                .start(TableConfig::for_players(4), vec![0], vec![2])
                .await
                .expect("start room");

            let joined = casino
                .join_open_seat(start.room_id, 2)
                .await
                .expect("join open seat");
            casino
                .leave_seat(start.room_id, 2, joined.generation, &joined.token)
                .await
                .expect("leave joined seat");
            let rejoined = casino
                .join_open_seat(start.room_id, 2)
                .await
                .expect("rejoin open seat");

            assert_eq!(rejoined.seat, 2);
            assert_eq!(rejoined.generation, joined.generation + 1);

            casino.close(start.room_id).await.expect("close room");
        });
    }

    #[test]
    fn close_invalidates_all_owned_seat_sessions() {
        let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");
        let casino = Casino::default();

        runtime.block_on(async {
            let start = casino
                .start(TableConfig::for_players(4), vec![0, 2], Vec::new())
                .await
                .expect("start room");

            casino.close(start.room_id).await.expect("close room");

            let err = casino
                .snapshot(
                    start.room_id,
                    start.seat_accesses[0].seat,
                    start.seat_accesses[0].generation,
                    &start.seat_accesses[0].token,
                )
                .await
                .expect_err("closed room should invalidate sessions");
            assert_eq!(err, RoomAccessError::RoomNotFound);
        });
    }

    #[test]
    fn public_snapshot_by_invitation_resolves_room() {
        let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");
        let casino = Casino::default();

        runtime.block_on(async {
            let start = casino
                .start(TableConfig::for_players(4), vec![0], vec![2])
                .await
                .expect("start room");

            let snapshot = casino
                .public_snapshot_by_invitation(&start.invitation_id)
                .await
                .expect("invitation snapshot");

            assert_eq!(snapshot.room_id, start.room_id);
            assert_eq!(snapshot.invitation_id, start.invitation_id);
            assert_eq!(snapshot.open_seats, vec![2]);

            casino.close(start.room_id).await.expect("close room");
        });
    }
}
