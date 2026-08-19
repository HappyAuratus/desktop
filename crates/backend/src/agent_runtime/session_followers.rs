use super::support::runtime_internal;
use crate::BackendError;
use agent_client_protocol_schema::v1::{SessionUpdate, StopReason};
use ora_contracts::LoadSessionEvent;
use std::collections::HashMap;
use tokio::sync::mpsc;

/// Extends a loaded session conversation with the updates from its active prompt.
///
/// The prompt keeps a single owner regardless of who started it. Loading the same session while
/// that prompt runs adds a follower which receives later events but cannot cancel or restart the
/// turn, matching the behavior of reopening any ordinary running conversation.
pub(super) struct SessionFollowers {
    followers: HashMap<u64, SessionFollower>,
}

struct SessionFollower {
    events: mpsc::Sender<Result<LoadSessionEvent, BackendError>>,
    overflow: mpsc::UnboundedSender<()>,
}

/// Separates live fan-out from the contract queue that may still contain the recorded history.
const FOLLOWER_QUEUE_CAPACITY: usize = 256;

impl SessionFollowers {
    /// Creates an empty follower set for one prompt operation.
    pub(super) fn new() -> Self {
        Self {
            followers: HashMap::new(),
        }
    }

    /// Continues a loaded conversation after its initial recorded history was delivered.
    pub(super) fn insert(
        &mut self,
        operation_id: u64,
        contract_sender: mpsc::Sender<Result<LoadSessionEvent, BackendError>>,
    ) {
        let (events, mut event_receiver) = mpsc::channel(FOLLOWER_QUEUE_CAPACITY);
        // Overflow travels independently from the bounded event queue so a slow view receives an
        // explicit failure instead of an ambiguous end-of-stream after it falls behind.
        let (overflow, mut overflow_receiver) = mpsc::unbounded_channel();
        tokio::spawn(async move {
            let mut overflow_open = true;
            loop {
                tokio::select! {
                    signal = overflow_receiver.recv(), if overflow_open => {
                        match signal {
                            Some(()) => {
                                let _ = contract_sender
                                    .send(Err(runtime_internal(
                                        "session_follower_overflow",
                                        "session load follower fell behind the active prompt",
                                    )))
                                    .await;
                                break;
                            }
                            None => overflow_open = false,
                        }
                    }
                    event = event_receiver.recv() => {
                        let Some(event) = event else {
                            break;
                        };
                        if contract_sender.send(event).await.is_err() {
                            break;
                        }
                    }
                }
            }
        });
        self.followers
            .insert(operation_id, SessionFollower { events, overflow });
    }

    /// Detaches a closed view without affecting the prompt that owns the turn.
    pub(super) fn remove(&mut self, operation_id: u64) -> bool {
        self.followers.remove(&operation_id).is_some()
    }

    /// Mirrors one provider update to every view that still has the session open.
    pub(super) fn send_update(&mut self, update: &SessionUpdate) {
        self.followers.retain(|_, follower| {
            let event = LoadSessionEvent::SessionUpdate {
                update: update.clone(),
            };
            match follower.events.try_send(Ok(event)) {
                Ok(()) => true,
                Err(mpsc::error::TrySendError::Full(_)) => {
                    let _ = follower.overflow.send(());
                    false
                }
                Err(mpsc::error::TrySendError::Closed(_)) => false,
            }
        });
    }

    /// Closes every loaded conversation stream with the active turn's recorded boundary.
    pub(super) fn finish(&mut self, stop_reason: StopReason) {
        for follower in std::mem::take(&mut self.followers).into_values() {
            tokio::spawn(async move {
                if follower
                    .events
                    .send(Ok(LoadSessionEvent::TurnEnded { stop_reason }))
                    .await
                    .is_ok()
                {
                    let _ = follower.events.send(Ok(LoadSessionEvent::Completed)).await;
                }
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{FOLLOWER_QUEUE_CAPACITY, SessionFollowers};
    use agent_client_protocol_schema::v1::{SessionUpdate, StopReason};
    use ora_contracts::LoadSessionEvent;
    use pretty_assertions::assert_eq;
    use serde_json::json;
    use tokio::sync::mpsc;

    /// A loaded session receives live updates and the active turn's finite ending.
    #[tokio::test]
    async fn continues_a_loaded_session_through_prompt_completion() {
        let (sender, mut receiver) = mpsc::channel(4);
        let mut followers = SessionFollowers::new();
        followers.insert(7, sender);
        let parsed: Result<SessionUpdate, _> = serde_json::from_value(json!({
            "sessionUpdate": "agent_message_chunk",
            "content": { "type": "text", "text": "hello" }
        }));
        let update = match parsed {
            Ok(update) => update,
            Err(error) => panic!("valid session update: {error}"),
        };

        followers.send_update(&update);
        followers.finish(StopReason::EndTurn);

        assert_eq!(
            [
                receiver
                    .recv()
                    .await
                    .map(|result| result.map_err(|error| error.to_string())),
                receiver
                    .recv()
                    .await
                    .map(|result| result.map_err(|error| error.to_string())),
                receiver
                    .recv()
                    .await
                    .map(|result| result.map_err(|error| error.to_string())),
            ],
            [
                Some(Ok(LoadSessionEvent::SessionUpdate { update })),
                Some(Ok(LoadSessionEvent::TurnEnded {
                    stop_reason: StopReason::EndTurn,
                })),
                Some(Ok(LoadSessionEvent::Completed)),
            ],
        );
    }

    /// Closing one view removes only that view's session stream.
    #[tokio::test]
    async fn removes_one_loaded_view_without_touching_others() {
        let (first, _first_receiver) = mpsc::channel(1);
        let (second, _second_receiver) = mpsc::channel(1);
        let mut followers = SessionFollowers::new();
        followers.insert(1, first);
        followers.insert(2, second);

        assert!(followers.remove(1));
        assert!(!followers.remove(1));
        assert!(followers.remove(2));
    }

    /// A view that cannot keep up gets a diagnostic terminal error without slowing the prompt.
    #[tokio::test]
    async fn reports_follower_queue_overflow_explicitly() {
        let (sender, mut receiver) = mpsc::channel(1);
        let mut followers = SessionFollowers::new();
        followers.insert(1, sender);
        let parsed: Result<SessionUpdate, _> = serde_json::from_value(json!({
            "sessionUpdate": "agent_message_chunk",
            "content": { "type": "text", "text": "hello" }
        }));
        let update = match parsed {
            Ok(update) => update,
            Err(error) => panic!("valid session update: {error}"),
        };

        for _ in 0..=FOLLOWER_QUEUE_CAPACITY {
            followers.send_update(&update);
        }

        let terminal_error = tokio::time::timeout(std::time::Duration::from_secs(1), async {
            while let Some(event) = receiver.recv().await {
                if let Err(error) = event {
                    return Some(error.to_string());
                }
            }
            None
        })
        .await;

        assert_eq!(
            terminal_error,
            Ok(Some(
                "session load follower fell behind the active prompt".to_string()
            )),
        );
    }
}
