use super::Client;
use super::SeatAssignment;
use super::SeatKind;
use crate::gameplay::TableConfig;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex as StdMutex;
use std::sync::RwLock;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use tokio::sync::Mutex;
use tokio::sync::mpsc::UnboundedReceiver;
use tokio::sync::mpsc::UnboundedSender;
use tokio::sync::mpsc::unbounded_channel;
use tokio::sync::watch;
use tokio::task::JoinHandle;

pub type RoomId = u64;
pub type SnapshotCache = Arc<RwLock<CachedMessages>>;

#[derive(Debug, Default)]
pub struct CachedMessages {
    latest_table_state: Option<String>,
    latest_decision: Option<String>,
}

impl CachedMessages {
    pub fn latest_table_state(&self) -> Option<&str> {
        self.latest_table_state.as_deref()
    }

    pub fn latest_table_state_owned(&self) -> Option<String> {
        self.latest_table_state.clone()
    }

    pub fn latest_decision(&self) -> Option<&str> {
        self.latest_decision.as_deref()
    }

    pub fn latest_decision_owned(&self) -> Option<String> {
        self.latest_decision.clone()
    }

    pub fn set_table_state(&mut self, json: String) {
        self.latest_table_state = Some(json);
    }

    pub fn set_decision(&mut self, json: String) {
        self.latest_decision = Some(json);
    }

    pub fn clear_decision(&mut self) {
        self.latest_decision = None;
    }

    pub fn clear(&mut self) {
        self.latest_table_state = None;
        self.latest_decision = None;
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SeatAccess {
    pub seat: usize,
    pub generation: u64,
    pub token: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SeatClaimError {
    SeatUnavailable { seat: usize, kind: SeatKind },
    SeatOutOfRange { seat: usize },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SeatReleaseError {
    SeatUnavailable { seat: usize, kind: SeatKind },
    SeatOutOfRange { seat: usize },
}

type Tx = UnboundedSender<String>;
type Rx = Arc<Mutex<UnboundedReceiver<String>>>;

struct SeatTransport {
    tx: Tx,
    rx: Rx,
    cache: SnapshotCache,
    connected: Arc<AtomicBool>,
    disconnect: watch::Sender<u64>,
}

impl SeatTransport {
    fn new() -> (Self, ClientTransport) {
        let (tx_outgoing, rx_outgoing) = unbounded_channel::<String>();
        let (tx_incoming, rx_incoming) = unbounded_channel::<String>();
        let (disconnect, _) = watch::channel(0_u64);
        let cache = Arc::new(RwLock::new(CachedMessages::default()));
        let connected = Arc::new(AtomicBool::new(false));
        (
            Self {
                tx: tx_incoming,
                rx: Arc::new(Mutex::new(rx_outgoing)),
                cache: cache.clone(),
                connected,
                disconnect,
            },
            ClientTransport {
                tx: tx_outgoing,
                rx: Arc::new(Mutex::new(rx_incoming)),
                cache,
            },
        )
    }

    fn disconnect_receiver(&self) -> watch::Receiver<u64> {
        self.disconnect.subscribe()
    }

    fn request_disconnect(&self) {
        let next = *self.disconnect.borrow() + 1;
        let _ = self.disconnect.send(next);
    }
}

struct ClientTransport {
    tx: Tx,
    rx: Rx,
    cache: SnapshotCache,
}

/// Handle to communicate with a running room.
/// Stores seat-scoped channel endpoints for bridging WebSocket to Client
/// players.
pub struct RoomHandle {
    pub id: RoomId,
    invitation_id: String,
    transports: HashMap<usize, SeatTransport>,
    seat_accesses: Arc<StdMutex<Vec<SeatAccess>>>,
    seat_assignments: Arc<StdMutex<Vec<SeatAssignment>>>,
    seat_generations: StdMutex<HashMap<usize, u64>>,
    table_config: StdMutex<Option<TableConfig>>,
    task: StdMutex<Option<JoinHandle<()>>>,
}

impl RoomHandle {
    pub fn pair_with_owned_seats(
        id: RoomId,
        seat_count: usize,
        owned_seats: Vec<usize>,
    ) -> (Self, Vec<Client>) {
        let assignments = super::seat_assignments(seat_count, &owned_seats, &[]);
        Self::pair_with_hosted_seats(id, owned_seats, assignments)
    }

    pub fn pair_with_hosted_seats(
        id: RoomId,
        owned_seats: Vec<usize>,
        seat_assignments: Vec<SeatAssignment>,
    ) -> (Self, Vec<Client>) {
        let seat_generations = StdMutex::new(HashMap::new());
        let seat_accesses = Arc::new(StdMutex::new(
            owned_seats
                .into_iter()
                .map(|seat| Self::issue_access(&seat_generations, seat))
                .collect::<Vec<_>>(),
        ));
        let seat_assignments = Arc::new(StdMutex::new(seat_assignments));
        let hosted_seats = {
            let assignments = seat_assignments.lock().expect("seat assignments lock");
            assignments
                .iter()
                .filter(|assignment| {
                    assignment.kind == SeatKind::Human || assignment.kind == SeatKind::Open
                })
                .map(|assignment| assignment.seat)
                .collect::<Vec<_>>()
        };

        let mut transports = HashMap::new();
        let clients = hosted_seats
            .into_iter()
            .map(|seat| {
                let (transport, client_transport) = SeatTransport::new();
                transports.insert(seat, transport);
                Client::shared(
                    client_transport.tx,
                    client_transport.rx,
                    client_transport.cache,
                    seat,
                    seat_accesses.clone(),
                    seat_assignments.clone(),
                    true,
                )
            })
            .collect::<Vec<_>>();
        (
            Self {
                id,
                invitation_id: Self::new_invitation_id(),
                transports,
                seat_accesses,
                seat_assignments,
                seat_generations,
                table_config: StdMutex::new(None),
                task: StdMutex::new(None),
            },
            clients,
        )
    }

    fn new_invitation_id() -> String {
        format!("room-{:016x}", rand::random::<u64>())
    }

    fn issue_access(seat_generations: &StdMutex<HashMap<usize, u64>>, seat: usize) -> SeatAccess {
        let mut seat_generations = seat_generations.lock().expect("seat generations lock");
        let generation = seat_generations.entry(seat).or_insert(0);
        *generation += 1;
        SeatAccess {
            seat,
            generation: *generation,
            token: format!("seat-{}-{:016x}", seat, rand::random::<u64>()),
        }
    }

    pub fn seat_access(&self, seat: usize) -> Option<SeatAccess> {
        self.seat_accesses
            .lock()
            .expect("seat accesses lock")
            .iter()
            .find(|access| access.seat == seat)
            .cloned()
    }

    pub fn authorize(&self, seat: usize, generation: u64, token: &str) -> bool {
        self.seat_access(seat)
            .map(|access| access.generation == generation && access.token == token)
            .unwrap_or(false)
    }

    pub fn owns_seat(&self, seat: usize) -> bool {
        let seat_accesses = self.seat_accesses.lock().expect("seat accesses lock");
        seat_accesses.iter().any(|access| access.seat == seat)
    }

    pub fn owned_seats(&self) -> Vec<usize> {
        let seat_accesses = self.seat_accesses.lock().expect("seat accesses lock");
        seat_accesses.iter().map(|access| access.seat).collect()
    }

    pub fn invitation_id(&self) -> String {
        self.invitation_id.clone()
    }

    pub fn open_seats(&self) -> Vec<usize> {
        self.seat_assignments
            .lock()
            .expect("seat assignments lock")
            .iter()
            .filter(|assignment| assignment.kind == SeatKind::Open)
            .map(|assignment| assignment.seat)
            .collect()
    }

    pub fn connected_seats(&self) -> Vec<usize> {
        self.transports
            .iter()
            .filter(|(_, transport)| transport.connected.load(Ordering::Acquire))
            .map(|(seat, _)| *seat)
            .collect()
    }

    pub fn seat_accesses(&self) -> Vec<SeatAccess> {
        self.seat_accesses
            .lock()
            .expect("seat accesses lock")
            .clone()
    }

    pub fn seat_assignments(&self) -> Vec<SeatAssignment> {
        self.seat_assignments
            .lock()
            .expect("seat assignments lock")
            .clone()
    }

    pub fn claim_seat(&self, seat: usize) -> Result<SeatAccess, SeatClaimError> {
        let mut assignments = self.seat_assignments.lock().expect("seat assignments lock");
        let Some(assignment) = assignments
            .iter_mut()
            .find(|assignment| assignment.seat == seat)
        else {
            return Err(SeatClaimError::SeatOutOfRange { seat });
        };
        if assignment.kind != SeatKind::Open {
            return Err(SeatClaimError::SeatUnavailable {
                seat,
                kind: assignment.kind,
            });
        }
        assignment.kind = SeatKind::Human;
        drop(assignments);

        let access = Self::issue_access(&self.seat_generations, seat);
        let mut seat_accesses = self.seat_accesses.lock().expect("seat accesses lock");
        seat_accesses.push(access.clone());
        seat_accesses.sort_by_key(|entry| entry.seat);
        drop(seat_accesses);
        self.reset_transport_cache(seat);
        Ok(access)
    }

    pub fn release_seat(&self, seat: usize) -> Result<(), SeatReleaseError> {
        let mut assignments = self.seat_assignments.lock().expect("seat assignments lock");
        let Some(assignment) = assignments
            .iter_mut()
            .find(|assignment| assignment.seat == seat)
        else {
            return Err(SeatReleaseError::SeatOutOfRange { seat });
        };
        if assignment.kind != SeatKind::Human {
            return Err(SeatReleaseError::SeatUnavailable {
                seat,
                kind: assignment.kind,
            });
        }
        assignment.kind = SeatKind::Open;
        drop(assignments);

        let mut seat_accesses = self.seat_accesses.lock().expect("seat accesses lock");
        seat_accesses.retain(|access| access.seat != seat);
        drop(seat_accesses);

        if let Some(transport) = self.transports.get(&seat) {
            self.reset_transport_cache(seat);
            transport.request_disconnect();
        }
        Ok(())
    }

    fn reset_transport_cache(&self, seat: usize) {
        if let Some(transport) = self.transports.get(&seat) {
            transport
                .cache
                .write()
                .expect("snapshot cache lock")
                .clear();
        }
    }

    pub fn channels(
        &self,
        seat: usize,
    ) -> Option<(Tx, Rx, SnapshotCache, Arc<AtomicBool>, watch::Receiver<u64>)> {
        self.transports.get(&seat).map(|transport| {
            (
                transport.tx.clone(),
                transport.rx.clone(),
                transport.cache.clone(),
                transport.connected.clone(),
                transport.disconnect_receiver(),
            )
        })
    }

    pub fn connected_flag(&self, seat: usize) -> Arc<AtomicBool> {
        self.transports
            .get(&seat)
            .map(|transport| transport.connected.clone())
            .expect("transport exists for hosted seat")
    }

    pub fn is_connected(&self, seat: usize) -> bool {
        self.connected_flag(seat).load(Ordering::Acquire)
    }

    pub fn snapshot(&self, seat: usize) -> SnapshotCache {
        self.transports
            .get(&seat)
            .map(|transport| transport.cache.clone())
            .expect("transport exists for hosted seat")
    }

    pub fn set_table_config(&self, config: TableConfig) {
        *self.table_config.lock().expect("table config lock") = Some(config);
    }

    pub fn table_config(&self) -> Option<TableConfig> {
        *self.table_config.lock().expect("table config lock")
    }

    pub fn attach_task(&self, task: JoinHandle<()>) {
        *self.task.lock().expect("task lock") = Some(task);
    }

    pub fn abort_task(&self) {
        if let Some(task) = self.task.lock().expect("task lock").take() {
            task.abort();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn room_handle_stores_seat_access() {
        let (handle, _clients) = RoomHandle::pair_with_owned_seats(7, 4, vec![0, 2]);

        assert_eq!(handle.id, 7);
        assert!(handle.invitation_id().starts_with("room-"));
        assert_eq!(handle.owned_seats(), vec![0, 2]);
        assert_eq!(handle.seat_accesses().len(), 2);
        assert_eq!(handle.seat_assignments()[0].kind, SeatKind::Human);
        assert_eq!(handle.seat_assignments()[1].kind, SeatKind::Bot);
        assert_eq!(handle.seat_assignments()[2].kind, SeatKind::Human);
    }

    #[test]
    fn claim_seat_promotes_open_assignment_to_human() {
        let (handle, _clients) = RoomHandle::pair_with_hosted_seats(
            7,
            vec![0],
            vec![
                SeatAssignment {
                    seat: 0,
                    kind: SeatKind::Human,
                },
                SeatAssignment {
                    seat: 1,
                    kind: SeatKind::Bot,
                },
                SeatAssignment {
                    seat: 2,
                    kind: SeatKind::Open,
                },
            ],
        );

        let access = handle.claim_seat(2).expect("claim open seat");

        assert_eq!(access.seat, 2);
        assert_eq!(handle.owned_seats(), vec![0, 2]);
        assert_eq!(handle.seat_assignments()[2].kind, SeatKind::Human);
    }

    #[test]
    fn release_seat_demotes_human_assignment_to_open() {
        let (handle, _clients) = RoomHandle::pair_with_owned_seats(7, 4, vec![0, 2]);

        handle.release_seat(2).expect("release seat");

        assert_eq!(handle.owned_seats(), vec![0]);
        assert_eq!(handle.seat_assignments()[2].kind, SeatKind::Open);
    }

    #[test]
    fn release_seat_clears_cached_snapshot_state() {
        let (handle, _clients) = RoomHandle::pair_with_owned_seats(7, 4, vec![0, 2]);
        {
            let snapshot = handle.snapshot(2);
            let mut snapshot = snapshot.write().expect("snapshot cache lock");
            snapshot.set_table_state("{\"kind\":\"table_state\"}".to_string());
            snapshot.set_decision("{\"kind\":\"decision\"}".to_string());
        }

        handle.release_seat(2).expect("release seat");

        let snapshot = handle.snapshot(2);
        let snapshot = snapshot.read().expect("snapshot cache lock");
        assert_eq!(snapshot.latest_table_state(), None);
        assert_eq!(snapshot.latest_decision(), None);
    }

    #[test]
    fn claim_seat_rotates_credentials_after_release() {
        let (handle, _clients) = RoomHandle::pair_with_owned_seats(7, 4, vec![0, 2]);
        let old_access = handle
            .seat_accesses()
            .into_iter()
            .find(|access| access.seat == 2)
            .expect("original seat access");

        handle.release_seat(2).expect("release seat");
        let access = handle.claim_seat(2).expect("reclaim seat");

        assert!(!handle.authorize(2, old_access.generation, &old_access.token));
        assert!(handle.authorize(2, access.generation, &access.token));
        assert_eq!(access.generation, old_access.generation + 1);
    }
}
