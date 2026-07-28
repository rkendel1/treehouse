use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StateMachine {
    pub resource: String,
    pub transitions: BTreeMap<String, BTreeSet<String>>,
}

impl StateMachine {
    pub fn can_transition(&self, from: &str, to: &str) -> bool {
        self.transitions
            .get(from)
            .is_some_and(|targets| targets.contains(to))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct StateTracker {
    states: BTreeMap<(String, String), String>,
}

impl StateTracker {
    pub fn remember(&mut self, resource: &str, id: &str, state: &str) {
        self.states
            .insert((resource.to_string(), id.to_string()), state.to_string());
    }

    pub fn current_state(&self, resource: &str, id: &str) -> Option<&str> {
        self.states
            .get(&(resource.to_string(), id.to_string()))
            .map(String::as_str)
    }
}

pub fn discover_state_machine(resource: &str, events: &[String]) -> StateMachine {
    let mut states = Vec::new();
    for event in events {
        let state = event
            .rsplit('_')
            .next()
            .unwrap_or(event)
            .to_ascii_lowercase();
        if states.last() != Some(&state) {
            states.push(state);
        }
    }

    let mut transitions: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for pair in states.windows(2) {
        transitions
            .entry(pair[0].clone())
            .or_default()
            .insert(pair[1].clone());
    }

    StateMachine {
        resource: resource.to_string(),
        transitions,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovers_order_state_machine_and_tracks_state() {
        let events = vec![
            "ORDER_CREATED".to_string(),
            "PAYMENT_RECEIVED".to_string(),
            "ORDER_FULFILLED".to_string(),
        ];

        let machine = discover_state_machine("Order", &events);
        assert!(machine.can_transition("created", "received"));
        assert!(machine.can_transition("received", "fulfilled"));

        let mut tracker = StateTracker::default();
        tracker.remember("Order", "123", "pending");
        assert_eq!(tracker.current_state("Order", "123"), Some("pending"));
    }
}
