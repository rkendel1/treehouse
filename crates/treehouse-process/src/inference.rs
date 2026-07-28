use std::collections::BTreeMap;

use crate::workflow::{ProcessTransition, ProcessWorkflow};

pub fn infer_workflows_from_events(events: &[String]) -> Vec<ProcessWorkflow> {
    let mut grouped: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for event in events {
        let upper = event.to_ascii_uppercase();
        let Some((prefix, _)) = upper.split_once('_') else {
            continue;
        };
        grouped
            .entry(prefix.to_string())
            .or_default()
            .push(event.to_string());
    }

    grouped
        .into_iter()
        .map(|(name, event_names)| {
            let states: Vec<String> = event_names
                .iter()
                .map(|event| {
                    event
                        .rsplit('_')
                        .next()
                        .unwrap_or(event)
                        .to_ascii_lowercase()
                })
                .collect();
            let transitions = states
                .windows(2)
                .map(|window| ProcessTransition {
                    from: window[0].clone(),
                    to: window[1].clone(),
                })
                .collect();
            ProcessWorkflow {
                name,
                states,
                transitions,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn infers_order_workflow_from_event_sequence() {
        let events = vec![
            "ORDER_CREATED".to_string(),
            "ORDER_PAID".to_string(),
            "ORDER_SHIPPED".to_string(),
        ];

        let workflows = infer_workflows_from_events(&events);
        assert_eq!(workflows.len(), 1);
        let workflow = &workflows[0];
        assert_eq!(workflow.name, "ORDER");
        assert_eq!(workflow.states, vec!["created", "paid", "shipped"]);
        assert_eq!(workflow.transitions.len(), 2);
    }
}
