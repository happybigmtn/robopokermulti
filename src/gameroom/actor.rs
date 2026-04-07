use super::*;
use tokio::sync::mpsc::*;

/// Wrapper that runs a Player in its own async task.
/// Handles message passing between Room and Player implementation.
///
/// - Room unicasts YourTurn(Recall) when it's this player's turn
/// - Actor calls Player::decide and sends action back to Room
/// - Room broadcasts Play events for all game actions
/// - Actor forwards events to Player::notify
pub struct Actor {
    player: Box<dyn Player>,
    getter: UnboundedReceiver<Event>,
    sender: UnboundedSender<(usize, Event)>,
}

impl Actor {
    pub fn spawn(
        _id: usize,
        player: Box<dyn Player>,
        sender: UnboundedSender<(usize, Event)>,
    ) -> UnboundedSender<Event> {
        let (tx, rx) = unbounded_channel();
        let actor = Self {
            player,
            sender,
            getter: rx,
        };
        tokio::spawn(actor.run());
        tx
    }
    async fn run(mut self) -> ! {
        loop {
            match self.getter.recv().await {
                Some(Event::YourTurn(ref recall)) => self.act(recall).await,
                Some(ref event) => self.player.notify(event).await,
                None => continue,
            }
        }
    }
    async fn act(&mut self, recall: &crate::gameplay::Recall) {
        let action = self.player.decide(recall).await;
        let _ = self
            .sender
            .send((recall.turn().position(), Event::Play(action)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::Observation;
    use crate::gameplay::Action;
    use crate::gameplay::Recall;
    use crate::gameplay::TableConfig;
    use crate::gameplay::Turn;

    struct StubPlayer(Action);

    #[async_trait::async_trait]
    impl Player for StubPlayer {
        async fn decide(&mut self, _recall: &Recall) -> Action {
            self.0
        }

        async fn notify(&mut self, _event: &Event) {}
    }

    #[test]
    fn actor_uses_recall_turn_position_for_reply_seat() {
        let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");

        runtime.block_on(async {
            let (sender, mut receiver) = unbounded_channel();
            let inbox = Actor::spawn(0, Box::new(StubPlayer(Action::Check)), sender);
            let recall = Recall::from_actions_with_config(
                Turn::Choice(2),
                Observation::try_from("As Kd").expect("hero observation"),
                Vec::new(),
                TableConfig::for_players(4),
            );

            inbox
                .send(Event::YourTurn(recall))
                .expect("send turn event");

            let Some((seat, Event::Play(action))) = receiver.recv().await else {
                panic!("expected action reply");
            };

            assert_eq!(seat, 2);
            assert_eq!(action, Action::Check);
        });
    }
}
