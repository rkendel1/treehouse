use serde::{Deserialize, Serialize};
use treehouse_api_engine::TransactionFlow;

use crate::{assertions::assert_status, environment::ExecutionEnvironment, fixtures::FixtureStore};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StepExecution {
    pub method: String,
    pub path: String,
    pub status: u16,
    pub success: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutionReport {
    pub environment: String,
    pub flow_name: String,
    pub steps: Vec<StepExecution>,
}

pub fn execute_flow(
    environment: &ExecutionEnvironment,
    flow: &TransactionFlow,
    fixtures: &FixtureStore,
) -> ExecutionReport {
    let steps = flow
        .steps
        .iter()
        .map(|step| {
            let expected = if step.method == "POST" { 201 } else { 200 };
            let status = fixtures
                .status_for(&step.method, &step.path)
                .unwrap_or(expected);
            StepExecution {
                method: step.method.clone(),
                path: step.path.clone(),
                status,
                success: assert_status(expected, status),
            }
        })
        .collect();

    ExecutionReport {
        environment: environment.base_url.clone(),
        flow_name: flow.name.clone(),
        steps,
    }
}

#[cfg(test)]
mod tests {
    use treehouse_api_engine::{TransactionFlow, TransactionStep};

    use super::*;

    #[test]
    fn executes_flow_with_fixture_statuses() {
        let env = ExecutionEnvironment::default();
        let flow = TransactionFlow {
            name: "Purchase Flow".to_string(),
            steps: vec![
                TransactionStep {
                    method: "POST".to_string(),
                    path: "/customers".to_string(),
                },
                TransactionStep {
                    method: "POST".to_string(),
                    path: "/orders".to_string(),
                },
            ],
        };

        let mut fixtures = FixtureStore::default();
        fixtures.set_status("POST", "/customers", 201);
        fixtures.set_status("POST", "/orders", 500);

        let report = execute_flow(&env, &flow, &fixtures);
        assert_eq!(report.steps.len(), 2);
        assert!(report.steps[0].success);
        assert!(!report.steps[1].success);
    }
}
