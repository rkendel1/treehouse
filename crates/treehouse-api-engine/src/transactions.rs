use serde::{Deserialize, Serialize};

use crate::openapi::ApiGraph;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransactionStep {
    pub method: String,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransactionFlow {
    pub name: String,
    pub steps: Vec<TransactionStep>,
}

pub fn discover_transaction_flows(graph: &ApiGraph) -> Vec<TransactionFlow> {
    let create_steps: Vec<TransactionStep> = graph
        .operations
        .iter()
        .filter(|operation| operation.method == "POST")
        .map(|operation| TransactionStep {
            method: operation.method.clone(),
            path: operation.path.clone(),
        })
        .collect();

    if create_steps.len() < 2 {
        return Vec::new();
    }

    vec![TransactionFlow {
        name: infer_flow_name(&create_steps),
        steps: create_steps,
    }]
}

fn infer_flow_name(steps: &[TransactionStep]) -> String {
    let entities: Vec<String> = steps
        .iter()
        .map(|step| step.path.trim_start_matches('/').replace('/', " "))
        .collect();
    if entities.is_empty() {
        "Inferred Transaction Flow".to_string()
    } else {
        format!("{} Flow", entities.join(" → "))
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::openapi::import_openapi;

    use super::*;

    #[test]
    fn discovers_alphabetized_flow_from_create_operations() {
        let spec = json!({
            "openapi": "3.0.0",
            "paths": {
                "/orders": {"post": {}},
                "/payments": {"post": {}},
                "/customers": {"post": {}}
            }
        });

        let graph = import_openapi(&spec);
        let flows = discover_transaction_flows(&graph);
        assert_eq!(flows.len(), 1);
        assert_eq!(flows[0].name, "customers → orders → payments Flow");
        assert_eq!(flows[0].steps[0].path, "/customers");
        assert_eq!(flows[0].steps[1].path, "/orders");
        assert_eq!(flows[0].steps[2].path, "/payments");
    }
}
