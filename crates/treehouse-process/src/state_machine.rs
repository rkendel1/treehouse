use std::collections::BTreeMap;

use crate::workflow::ProcessWorkflow;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateMachine {
    transitions: BTreeMap<String, Vec<String>>,
}

impl StateMachine {
    pub fn from_workflow(workflow: &ProcessWorkflow) -> Self {
        let mut transitions: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for transition in &workflow.transitions {
            transitions
                .entry(transition.from.clone())
                .or_default()
                .push(transition.to.clone());
        }
        Self { transitions }
    }

    pub fn can_transition(&self, from: &str, to: &str) -> bool {
        self.transitions
            .get(from)
            .is_some_and(|targets| targets.iter().any(|target| target == to))
    }
}
