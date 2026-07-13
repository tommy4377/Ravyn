use std::{collections::VecDeque, sync::Arc};

use serde::Serialize;
use tokio::sync::broadcast;
use uuid::Uuid;

use super::models::{JobStatus, ProgressSnapshot};

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    JobStatus {
        job_id: Uuid,
        status: JobStatus,
        error: Option<String>,
    },
    Progress(ProgressSnapshot),
    /// Managed component provisioning state or download progress changed.
    Component {
        component: crate::services::components::ComponentId,
        state: crate::services::components::ComponentState,
        #[serde(skip_serializing_if = "Option::is_none")]
        progress_pct: Option<u8>,
        #[serde(skip_serializing_if = "Option::is_none")]
        bytes_downloaded: Option<u64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        bytes_total: Option<u64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        message: Option<String>,
    },
    QueueChanged,
    ResyncRequired {
        oldest_available: u64,
        newest_available: u64,
    },
}

#[derive(Debug, Clone, Serialize)]
pub struct SequencedEvent {
    pub sequence: u64,
    #[serde(flatten)]
    pub event: Event,
}

pub struct EventSubscription {
    pub replay: Vec<SequencedEvent>,
    pub receiver: broadcast::Receiver<SequencedEvent>,
}

#[derive(Debug, Clone, Copy)]
pub struct EventStats {
    pub replay_buffer_events: usize,
    pub replayed_events_total: u64,
    pub resync_required_total: u64,
    pub receiver_count: usize,
    pub sequence_span: u64,
}

struct EventState {
    next_sequence: u64,
    replay: VecDeque<SequencedEvent>,
    replay_capacity: usize,
    replayed_events_total: u64,
    resync_required_total: u64,
}

#[derive(Clone)]
pub struct EventBus {
    sender: broadcast::Sender<SequencedEvent>,
    state: Arc<std::sync::Mutex<EventState>>,
}

impl EventBus {
    pub fn new(capacity: usize) -> Self {
        let capacity = capacity.max(1);
        let (sender, _) = broadcast::channel(capacity);
        Self {
            sender,
            state: Arc::new(std::sync::Mutex::new(EventState {
                next_sequence: 1,
                replay: VecDeque::with_capacity(capacity),
                replay_capacity: capacity,
                replayed_events_total: 0,
                resync_required_total: 0,
            })),
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<SequencedEvent> {
        self.sender.subscribe()
    }

    pub fn subscribe_from(&self, last_sequence: Option<u64>) -> EventSubscription {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let receiver = self.sender.subscribe();
        let newest = state.next_sequence.saturating_sub(1);
        let oldest = state
            .replay
            .front()
            .map_or(state.next_sequence, |event| event.sequence);
        let replay = match last_sequence {
            None => Vec::new(),
            Some(last) if last.saturating_add(1) < oldest => vec![SequencedEvent {
                sequence: newest,
                event: Event::ResyncRequired {
                    oldest_available: oldest,
                    newest_available: newest,
                },
            }],
            Some(last) => state
                .replay
                .iter()
                .filter(|event| event.sequence > last)
                .cloned()
                .collect(),
        };
        state.replayed_events_total = state
            .replayed_events_total
            .saturating_add(u64::try_from(replay.len()).unwrap_or(u64::MAX));
        if replay
            .iter()
            .any(|event| matches!(event.event, Event::ResyncRequired { .. }))
        {
            state.resync_required_total = state.resync_required_total.saturating_add(1);
        }
        EventSubscription { replay, receiver }
    }

    pub fn stats(&self) -> EventStats {
        let state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let oldest = state
            .replay
            .front()
            .map_or(state.next_sequence, |event| event.sequence);
        EventStats {
            replay_buffer_events: state.replay.len(),
            replayed_events_total: state.replayed_events_total,
            resync_required_total: state.resync_required_total,
            receiver_count: self.sender.receiver_count(),
            sequence_span: state.next_sequence.saturating_sub(oldest),
        }
    }

    pub fn publish(&self, event: Event) {
        let sequenced = {
            let mut state = self
                .state
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let sequenced = SequencedEvent {
                sequence: state.next_sequence,
                event,
            };
            state.next_sequence = state.next_sequence.saturating_add(1);
            if state.replay.len() == state.replay_capacity {
                state.replay.pop_front();
            }
            state.replay.push_back(sequenced.clone());
            sequenced
        };
        let _ = self.sender.send(sequenced);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replays_events_after_the_last_seen_sequence() {
        let bus = EventBus::new(4);
        bus.publish(Event::QueueChanged);
        bus.publish(Event::QueueChanged);
        let subscription = bus.subscribe_from(Some(1));
        assert_eq!(subscription.replay.len(), 1);
        assert_eq!(subscription.replay[0].sequence, 2);
    }

    #[test]
    fn requests_resync_when_the_cursor_is_older_than_the_buffer() {
        let bus = EventBus::new(2);
        for _ in 0..4 {
            bus.publish(Event::QueueChanged);
        }
        let subscription = bus.subscribe_from(Some(1));
        assert!(matches!(
            subscription.replay.first().map(|event| &event.event),
            Some(Event::ResyncRequired { .. })
        ));
    }
}
