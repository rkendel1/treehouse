use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct OwnershipDeclaration {
    pub entities: Vec<String>,
    pub workflows: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct ProvidedInterfaces {
    pub apis: Vec<ApiContract>,
    pub events: Vec<EventContract>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FieldContract {
    pub name: String,
    pub field_type: String,
    pub required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct DataContract {
    pub entity: String,
    pub fields: Vec<FieldContract>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct SchemaContract {
    pub fields: Vec<FieldContract>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct ApiContract {
    pub method: String,
    pub path: String,
    pub request: SchemaContract,
    pub response: SchemaContract,
    pub authorization: Option<String>,
    pub lifecycle: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct EventContract {
    pub name: String,
    pub payload: SchemaContract,
    pub ordering_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct ProcessContract {
    pub trigger_event: String,
    pub must_eventually_emit: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct OwnershipContract {
    pub entity_field: String,
    pub owner_subsystem: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct ConsumerDependency {
    pub consumer: String,
    pub dependency: String,
    pub expected_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct SubsystemContract {
    pub subsystem: String,
    pub version: String,
    pub owns: OwnershipDeclaration,
    pub provides: ProvidedInterfaces,
    pub consumes: BTreeMap<String, Vec<String>>,
    pub guarantees: Vec<String>,
    pub data_contracts: Vec<DataContract>,
    pub process_contracts: Vec<ProcessContract>,
    pub ownership_contracts: Vec<OwnershipContract>,
    pub consumer_dependencies: Vec<ConsumerDependency>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ContractTestKind {
    RequestSchema,
    ResponseSchema,
    AuthorizationBehavior,
    LifecycleBehavior,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ContractTest {
    pub name: String,
    pub subsystem: String,
    pub kind: ContractTestKind,
    pub target: String,
}

pub fn generate_contract_tests(contract: &SubsystemContract) -> Vec<ContractTest> {
    let mut tests = Vec::new();

    for api in &contract.provides.apis {
        let target = format!("{} {}", api.method, api.path);
        tests.push(ContractTest {
            name: format!("{} request schema", target),
            subsystem: contract.subsystem.clone(),
            kind: ContractTestKind::RequestSchema,
            target: target.clone(),
        });
        tests.push(ContractTest {
            name: format!("{} response schema", target),
            subsystem: contract.subsystem.clone(),
            kind: ContractTestKind::ResponseSchema,
            target: target.clone(),
        });
        tests.push(ContractTest {
            name: format!("{} authorization behavior", target),
            subsystem: contract.subsystem.clone(),
            kind: ContractTestKind::AuthorizationBehavior,
            target: target.clone(),
        });
        tests.push(ContractTest {
            name: format!("{} lifecycle behavior", target),
            subsystem: contract.subsystem.clone(),
            kind: ContractTestKind::LifecycleBehavior,
            target,
        });
    }

    tests
}
