use super::SeatAccess;
use super::SeatAssignment;
use super::SeatKind;
use super::SnapshotCache;
use crate::cards::Hole;
use crate::gameplay::Action;
use crate::gameplay::Recall;
use crate::gameroom::Event;
use crate::gameroom::Player;
use rand::seq::IndexedRandom;
use serde_json::Value;
use std::sync::Arc;
use std::sync::Mutex as StdMutex;
use tokio::sync::Mutex;
use tokio::sync::mpsc::UnboundedReceiver;
use tokio::sync::mpsc::UnboundedSender;

/// Network player that communicates via tokio channels.
/// Designed to bridge WebSocket connections to the Room actor system.
///
/// The tx channel sends JSON to the WebSocket client:
/// - Game state (Recall) when it's the player's turn
/// - Event notifications for all game actions
///
/// The rx channel receives JSON from the WebSocket client:
/// - Action decisions when prompted
pub struct Client {
    tx: UnboundedSender<String>,
    rx: Arc<Mutex<UnboundedReceiver<String>>>,
    cache: SnapshotCache,
    seat_index: usize,
    seat_accesses: Arc<StdMutex<Vec<SeatAccess>>>,
    seat_assignments: Arc<StdMutex<Vec<SeatAssignment>>>,
    primary_notifier: bool,
}

impl Client {
    pub fn new(
        tx: UnboundedSender<String>,
        rx: Arc<Mutex<UnboundedReceiver<String>>>,
        cache: SnapshotCache,
        seat_index: usize,
    ) -> Self {
        Self {
            tx,
            rx,
            cache,
            seat_index,
            seat_accesses: Arc::new(StdMutex::new(vec![SeatAccess {
                seat: seat_index,
                generation: 1,
                token: "local-client".to_string(),
            }])),
            seat_assignments: Arc::new(StdMutex::new(vec![SeatAssignment {
                seat: seat_index,
                kind: SeatKind::Human,
            }])),
            primary_notifier: true,
        }
    }

    pub fn shared(
        tx: UnboundedSender<String>,
        rx: Arc<Mutex<UnboundedReceiver<String>>>,
        cache: SnapshotCache,
        seat_index: usize,
        seat_accesses: Arc<StdMutex<Vec<SeatAccess>>>,
        seat_assignments: Arc<StdMutex<Vec<SeatAssignment>>>,
        primary_notifier: bool,
    ) -> Self {
        Self {
            tx,
            rx,
            cache,
            seat_index,
            seat_accesses,
            seat_assignments,
            primary_notifier,
        }
    }

    pub fn group(
        tx: UnboundedSender<String>,
        rx: Arc<Mutex<UnboundedReceiver<String>>>,
        cache: SnapshotCache,
        seat_accesses: Arc<StdMutex<Vec<SeatAccess>>>,
        seat_assignments: Arc<StdMutex<Vec<SeatAssignment>>>,
        hosted_seats: Vec<usize>,
    ) -> Vec<Self> {
        let primary_seat = {
            let assignments = seat_assignments.lock().expect("seat assignments lock");
            assignments
                .iter()
                .find(|assignment| assignment.kind == SeatKind::Human)
                .or_else(|| {
                    assignments
                        .iter()
                        .find(|assignment| assignment.kind == SeatKind::Open)
                })
                .map(|assignment| assignment.seat)
                .unwrap_or(0)
        };
        hosted_seats
            .iter()
            .map(|&seat_index| {
                Self::shared(
                    tx.clone(),
                    rx.clone(),
                    cache.clone(),
                    seat_index,
                    seat_accesses.clone(),
                    seat_assignments.clone(),
                    seat_index == primary_seat,
                )
            })
            .collect()
    }

    fn seat_assignments(&self) -> Vec<SeatAssignment> {
        self.seat_assignments
            .lock()
            .expect("seat assignments lock")
            .clone()
    }

    fn normalized_seat_assignments(&self, seat_count: usize) -> Vec<SeatAssignment> {
        let seat_assignments = self.seat_assignments();
        (0..seat_count)
            .map(|seat| {
                seat_assignments
                    .iter()
                    .find(|assignment| assignment.seat == seat)
                    .cloned()
                    .unwrap_or(SeatAssignment {
                        seat,
                        kind: SeatKind::Bot,
                    })
            })
            .collect()
    }

    fn owned_seats(&self) -> Vec<usize> {
        let seat_accesses = self.seat_accesses.lock().expect("seat accesses lock");
        seat_accesses.iter().map(|access| access.seat).collect()
    }

    fn is_human(&self) -> bool {
        self.seat_assignments()
            .into_iter()
            .find(|assignment| assignment.seat == self.seat_index)
            .map(|assignment| assignment.kind == SeatKind::Human)
            .unwrap_or(false)
    }

    fn decision_payload(&self, recall: &Recall) -> Value {
        let head = recall.head();
        let config = head.config();
        let seat_assignments = self.normalized_seat_assignments(head.seat_count());
        let owned_seats = self.owned_seats();
        let seats = head.seats();
        let seats = seats
            .iter()
            .take(head.seat_count())
            .enumerate()
            .map(|(index, seat)| {
                serde_json::json!({
                    "index": index,
                    "stack": seat.stack(),
                    "stake": seat.stake(),
                    "spent": seat.spent(),
                    "state": seat.state().to_string(),
                })
            })
            .collect::<Vec<_>>();
        serde_json::json!({
            "type": "decision",
            "legal": head.legal().iter().map(|a| a.to_string()).collect::<Vec<_>>(),
            "board": recall.board().iter().map(|c| c.to_string()).collect::<Vec<_>>(),
            "pot": head.pot(),
            "hero": Hole::from(recall.seen()).to_string(),
            "hero_index": self.seat_index,
            "owned_seat": self.seat_index,
            "owned_seats": owned_seats,
            "seat_assignments": seat_assignments,
            "seat_roles": super::seat_roles_from_assignments(&seat_assignments),
            "seat_count": head.seat_count(),
            "seat_position": head.seat_position(),
            "active_players": head.active_player_count(),
            "table": {
                "seat_count": config.seat_count,
                "small_blind": config.small_blind,
                "big_blind": config.big_blind,
                "ante": config.ante,
                "starting_stack": config.starting_stack,
            },
            "seats": seats,
        })
    }

    fn event_payload(&self, event: &Event) -> Value {
        let mut payload = serde_json::json!({
            "type": "event",
            "event": format!("{:?}", event),
        });

        match event {
            Event::Play(action) => {
                payload["kind"] = serde_json::json!("play");
                payload["action"] = serde_json::json!(action.to_string());
            }
            Event::TableState(state) => {
                let seat_assignments = self.normalized_seat_assignments(state.seat_count);
                let owned_seats = self.owned_seats();
                payload["kind"] = serde_json::json!("table_state");
                payload["state"] = serde_json::json!({
                    "street": state.street.to_string(),
                    "board": state.board.iter().map(|card| card.to_string()).collect::<Vec<_>>(),
                    "pot": state.pot,
                    "owned_seat": self.seat_index,
                    "owned_seats": owned_seats,
                    "seat_assignments": seat_assignments,
                    "seat_roles": super::seat_roles_from_assignments(&seat_assignments),
                    "seat_count": state.seat_count,
                    "active_players": state.active_players,
                    "actor": state.actor,
                    "seats": state.seats.iter().map(|seat| serde_json::json!({
                        "index": seat.index,
                        "stack": seat.stack,
                        "stake": seat.stake,
                        "spent": seat.spent,
                        "state": seat.state.to_string(),
                    })).collect::<Vec<_>>(),
                });
            }
            Event::NextHand(index, meta) => {
                payload["kind"] = serde_json::json!("next_hand");
                payload["seat"] = serde_json::json!(*index);
                match meta {
                    crate::gameroom::Meta::StandUp => {
                        payload["meta"] = serde_json::json!({ "kind": "stand_up" });
                    }
                    crate::gameroom::Meta::SitDown => {
                        payload["meta"] = serde_json::json!({ "kind": "sit_down" });
                    }
                    crate::gameroom::Meta::CashOut(amount) => {
                        payload["meta"] = serde_json::json!({
                            "kind": "cash_out",
                            "amount": amount,
                        });
                    }
                }
            }
            Event::ShowHand(index, hole) => {
                payload["kind"] = serde_json::json!("show_hand");
                payload["seat"] = serde_json::json!(*index);
                payload["hole"] = serde_json::json!(hole.to_string());
            }
            Event::YourTurn(recall) => {
                let seat_assignments = self.normalized_seat_assignments(recall.head().seat_count());
                let owned_seats = self.owned_seats();
                payload["kind"] = serde_json::json!("your_turn");
                payload["hero_index"] = serde_json::json!(self.seat_index);
                payload["owned_seat"] = serde_json::json!(self.seat_index);
                payload["owned_seats"] = serde_json::json!(owned_seats);
                payload["seat_assignments"] = serde_json::json!(seat_assignments);
                payload["actor"] = serde_json::json!(recall.turn().position());
            }
        }

        payload
    }

    fn should_emit_event(&self, event: &Event) -> bool {
        match event {
            Event::Play(_) | Event::TableState(_) | Event::NextHand(_, _) => self.primary_notifier,
            Event::ShowHand(index, _) => *index == self.seat_index && self.is_human(),
            Event::YourTurn(recall) => {
                recall.turn().position() == self.seat_index && self.is_human()
            }
        }
    }

    fn send_decision(&self, json: String) {
        self.cache
            .write()
            .expect("snapshot cache lock")
            .set_decision(json.clone());
        self.tx
            .send(json)
            .inspect_err(|e| log::error!("failed to send decision state: {}", e))
            .ok();
    }

    fn send_event(&self, event: &Event, json: String) {
        let mut cache = self.cache.write().expect("snapshot cache lock");
        match event {
            Event::TableState(_) => cache.set_table_state(json.clone()),
            _ => {}
        }
        cache.clear_decision();
        drop(cache);
        let _ = self.tx.send(json);
    }
}

#[async_trait::async_trait]
impl Player for Client {
    async fn decide(&mut self, recall: &Recall) -> Action {
        if !self.is_human() {
            let ref mut rng = rand::rng();
            return recall
                .head()
                .legal()
                .choose(rng)
                .copied()
                .expect("non empty legal actions conditional on being asked to move");
        }
        let state = self.decision_payload(recall);
        self.send_decision(state.to_string());
        loop {
            match self
                .rx
                .lock()
                .await
                .recv()
                .await
                .and_then(|s| Action::try_from(s.as_str()).ok())
            {
                Some(action) if recall.head().is_allowed(&action) => return action,
                Some(_) => log::warn!("invalid action from client, retrying"),
                None => return recall.head().passive(),
            }
        }
    }
    async fn notify(&mut self, event: &Event) {
        if !self.should_emit_event(event) {
            return;
        }
        let json = self.event_payload(event);
        self.send_event(event, json.to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::Observation;
    use crate::gameplay::TableConfig;
    use crate::hosting::CachedMessages;
    use std::sync::RwLock;
    use tokio::sync::mpsc::unbounded_channel;

    #[test]
    fn decision_payload_includes_table_and_seat_context() {
        let config = TableConfig::for_players(4)
            .with_blinds(2, 4)
            .with_ante(1)
            .with_stack(200);
        let recall = Recall::from_actions_with_config(
            crate::gameplay::Turn::Choice(0),
            Observation::try_from("As Kd").expect("hero observation"),
            Vec::new(),
            config,
        );
        let (tx, client_rx) = unbounded_channel();
        let cache = Arc::new(RwLock::new(CachedMessages::default()));
        let client = Client::new(tx, Arc::new(Mutex::new(client_rx)), cache, 0);

        let payload = client.decision_payload(&recall);

        assert_eq!(payload["type"], "decision");
        assert_eq!(payload["seat_count"], 4);
        assert_eq!(payload["active_players"], 4);
        assert_eq!(payload["table"]["small_blind"], 2);
        assert_eq!(payload["table"]["big_blind"], 4);
        assert_eq!(payload["table"]["ante"], 1);
        assert_eq!(payload["hero"], "KdAs");
        assert_eq!(payload["owned_seat"], 0);
        assert_eq!(payload["owned_seats"][0], 0);
        assert_eq!(payload["seat_assignments"][0]["seat"], 0);
        assert_eq!(payload["seat_assignments"][0]["kind"], "human");
        assert_eq!(payload["seat_assignments"][1]["kind"], "bot");
        assert_eq!(
            payload["seat_roles"].as_array().expect("seat roles").len(),
            4
        );
        assert_eq!(payload["seat_roles"][0], "human");
        assert_eq!(payload["seat_roles"][1], "bot");
        assert_eq!(payload["seats"].as_array().expect("seat array").len(), 4);
    }

    #[test]
    fn show_hand_event_payload_is_structured() {
        let (tx, client_rx) = unbounded_channel();
        let cache = Arc::new(RwLock::new(CachedMessages::default()));
        let client = Client::new(tx, Arc::new(Mutex::new(client_rx)), cache, 0);
        let payload = client.event_payload(&Event::ShowHand(
            2,
            Hole::try_from("As Kd").expect("hole cards"),
        ));

        assert_eq!(payload["type"], "event");
        assert_eq!(payload["kind"], "show_hand");
        assert_eq!(payload["seat"], 2);
        assert_eq!(payload["hole"], "KdAs");
    }

    #[test]
    fn table_state_event_payload_is_structured() {
        let (tx, client_rx) = unbounded_channel();
        let cache = Arc::new(RwLock::new(CachedMessages::default()));
        let client = Client::new(tx, Arc::new(Mutex::new(client_rx)), cache, 0);
        let payload = client.event_payload(&Event::TableState(crate::gameroom::PublicState {
            street: crate::cards::Street::Flop,
            board: vec![
                crate::cards::Card::try_from("As").expect("card"),
                crate::cards::Card::try_from("Kd").expect("card"),
                crate::cards::Card::try_from("Qc").expect("card"),
            ],
            pot: 12,
            seat_count: 4,
            active_players: 3,
            actor: Some(2),
            seats: vec![crate::gameroom::PublicSeat {
                index: 0,
                stack: 198,
                stake: 2,
                spent: 2,
                state: crate::gameplay::State::Betting,
            }],
        }));

        assert_eq!(payload["type"], "event");
        assert_eq!(payload["kind"], "table_state");
        assert_eq!(payload["state"]["street"], "flop");
        assert_eq!(payload["state"]["owned_seat"], 0);
        assert_eq!(payload["state"]["owned_seats"][0], 0);
        assert_eq!(payload["state"]["seat_assignments"][0]["kind"], "human");
        assert_eq!(payload["state"]["seat_assignments"][1]["kind"], "bot");
        assert_eq!(payload["state"]["seat_roles"][0], "human");
        assert_eq!(payload["state"]["seat_roles"][1], "bot");
        assert_eq!(payload["state"]["pot"], 12);
        assert_eq!(payload["state"]["seat_count"], 4);
        assert_eq!(payload["state"]["active_players"], 3);
    }

    #[test]
    fn send_event_caches_latest_table_state_and_clears_decision() {
        let (tx, _rx) = unbounded_channel();
        let (_client_tx, client_rx) = unbounded_channel();
        let cache = Arc::new(RwLock::new(CachedMessages::default()));
        cache
            .write()
            .expect("cache lock")
            .set_decision("decision".to_string());
        let client = Client::new(tx, Arc::new(Mutex::new(client_rx)), cache.clone(), 0);
        let event = Event::TableState(crate::gameroom::PublicState {
            street: crate::cards::Street::Flop,
            board: Vec::new(),
            pot: 4,
            seat_count: 2,
            active_players: 2,
            actor: Some(0),
            seats: Vec::new(),
        });

        client.send_event(&event, "{\"type\":\"event\"}".to_string());

        let cache = cache.read().expect("cache lock");
        assert_eq!(cache.latest_table_state(), Some("{\"type\":\"event\"}"));
        assert_eq!(cache.latest_decision(), None);
    }

    #[test]
    fn shared_client_payload_includes_all_owned_seats() {
        let (tx, client_rx) = unbounded_channel();
        let cache = Arc::new(RwLock::new(CachedMessages::default()));
        let client = Client::shared(
            tx,
            Arc::new(Mutex::new(client_rx)),
            cache,
            2,
            Arc::new(StdMutex::new(vec![
                SeatAccess {
                    seat: 0,
                    generation: 1,
                    token: "a".to_string(),
                },
                SeatAccess {
                    seat: 2,
                    generation: 1,
                    token: "b".to_string(),
                },
            ])),
            Arc::new(StdMutex::new(vec![
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
                    kind: SeatKind::Human,
                },
            ])),
            false,
        );
        let recall = Recall::from_actions_with_config(
            crate::gameplay::Turn::Choice(2),
            Observation::try_from("As Kd").expect("hero observation"),
            Vec::new(),
            TableConfig::for_players(4),
        );

        let payload = client.decision_payload(&recall);

        assert_eq!(payload["hero_index"], 2);
        assert_eq!(payload["owned_seat"], 2);
        assert_eq!(payload["owned_seats"][0], 0);
        assert_eq!(payload["owned_seats"][1], 2);
        assert_eq!(payload["seat_assignments"][0]["kind"], "human");
        assert_eq!(payload["seat_assignments"][1]["kind"], "bot");
        assert_eq!(payload["seat_assignments"][2]["kind"], "human");
    }

    #[test]
    fn secondary_shared_client_suppresses_public_events() {
        let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");

        runtime.block_on(async {
            let (tx, mut outbound_rx) = unbounded_channel();
            let (_client_tx, client_rx) = unbounded_channel();
            let cache = Arc::new(RwLock::new(CachedMessages::default()));
            let mut client = Client::shared(
                tx,
                Arc::new(Mutex::new(client_rx)),
                cache,
                2,
                Arc::new(StdMutex::new(vec![
                    SeatAccess {
                        seat: 0,
                        generation: 1,
                        token: "a".to_string(),
                    },
                    SeatAccess {
                        seat: 2,
                        generation: 1,
                        token: "b".to_string(),
                    },
                ])),
                Arc::new(StdMutex::new(vec![
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
                        kind: SeatKind::Human,
                    },
                ])),
                false,
            );

            client.notify(&Event::Play(Action::Check)).await;

            assert!(outbound_rx.try_recv().is_err());
        });
    }

    #[test]
    fn open_shared_client_uses_open_assignment_in_payloads() {
        let (tx, client_rx) = unbounded_channel();
        let cache = Arc::new(RwLock::new(CachedMessages::default()));
        let client = Client::shared(
            tx,
            Arc::new(Mutex::new(client_rx)),
            cache,
            2,
            Arc::new(StdMutex::new(vec![SeatAccess {
                seat: 0,
                generation: 1,
                token: "a".to_string(),
            }])),
            Arc::new(StdMutex::new(vec![
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
            ])),
            false,
        );

        let payload = client.event_payload(&Event::TableState(crate::gameroom::PublicState {
            street: crate::cards::Street::Flop,
            board: Vec::new(),
            pot: 12,
            seat_count: 3,
            active_players: 3,
            actor: Some(2),
            seats: Vec::new(),
        }));

        assert_eq!(payload["state"]["seat_assignments"][2]["kind"], "open");
        assert_eq!(payload["state"]["seat_roles"][2], "open");
        assert_eq!(payload["state"]["owned_seats"][0], 0);
    }
}
